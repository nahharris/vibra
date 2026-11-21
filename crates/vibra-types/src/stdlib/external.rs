//! The check of external declarations against their registry
//! (`docs/spec/01-source-language.md`, "External definitions";
//! `docs/spec/06-runtime.md`, "Compiler intrinsic registry" and "External
//! providers").
//!
//! An `external @compiler` declaration binds a row of the `@compiler`
//! registry. Its symbol MUST be a row (`@external.unknownSymbol` at the
//! symbol), the row's owner MUST be the declaration's owner
//! (`@external.ownerMismatch` at the symbol, related to the owner), its name
//! MUST be the member part of the symbol, and its written signature MUST be
//! the row's (each `@external.signatureMismatch` at the declaration, with the
//! expected name or signature in a note). A declaration shows the first of
//! these faults only.
//!
//! The owner of a member of a type is the type's wire tag for a builtin type
//! (`Intrinsic(@i32)` owns `i32`) and its role for a type that plays one
//! (`role @str` owns `str`); the owner of a module-level function is the last
//! part of its module path. The signature is the declaration's binders (the
//! owner's type parameters, then its own), its positional parameter types,
//! and its result type; a row has no labelled parameter, no bound, and no
//! guard, and it is a `fn`.
//!
//! A conversion row is the body of an implementation, never a member a
//! source path names: it is bound by the `convert` member of an
//! `impl From(S)` or `impl TryFrom(S)` nested in the destination type, so
//! its owner there is the destination and its name `convert`. The
//! conversion rows are `<s>.to<T>` (destination `T`), `<t>.toStr`
//! (destination `Str`), and `<t>.parse` (destination `T`).
//!
//! An `external @host` declaration binds a row of the host registry or of
//! the test-world registry. Its symbol MUST be a row of one of them
//! (`@external.unknownSymbol` at the symbol); the interface that encloses it,
//! by module and name, MUST be the row's owner (`@external.ownerMismatch` at
//! the symbol, related to the owner); its name MUST be the owner's member
//! that the row names, and it MUST be a `proc` (each
//! `@external.signatureMismatch` at the declaration).

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::external::host::{host_row, test_world_row};
use vibra_ir::external::{CompilerIntrinsic, FloatOp, IntegerOp};
use vibra_syntax::SyntaxKind;
use vibra_syntax::source::{
    Callable, CallableKind, Declaration, Module, TypeBody, TypeForm,
};

use super::names::{LibraryNames, Scope, Written};

/// The faults of the external declarations of `module`, whose module path
/// atom is `path`: `@external.unknownSymbol`, `@external.ownerMismatch`, and
/// `@external.signatureMismatch`, in written order.
///
/// `names` tells what the standard library's declarations are, so that a
/// written signature can be compared with a registry row. A declaration
/// without `external`, or whose `external` has no `symbol`, is not checked
/// here: the reader reports a missing half.
#[must_use]
pub fn check_externals(
    module: &Module<'_>,
    path: &str,
    names: &LibraryNames,
) -> Vec<Diagnostic> {
    let mut found = Vec::new();
    let top = Scope::module(path, module, names);
    let module_owner = path.rsplit('.').next().unwrap_or(path);
    for declaration in module.declarations() {
        match declaration {
            Declaration::Fn(callable) | Declaration::Proc(callable) => {
                let scope = top.clone().with_binders(binder_names(&callable));
                let site = Site {
                    owner: Some(module_owner),
                    owner_span: None,
                    interface: None,
                };
                check(&callable, &site, &scope, &mut found, false);
            }
            Declaration::Type(decl) => {
                let Some(name) = decl.name() else { continue };
                let parameters = decl.params();
                let owner = match decl.body() {
                    Some(TypeBody::Intrinsic(tag)) => tag.text().strip_prefix('@'),
                    _ => decl
                        .role()
                        .and_then(|role| role.tag())
                        .and_then(|tag| tag.text().strip_prefix('@')),
                };
                let base = top
                    .clone()
                    .with_binders(parameters.iter().map(|parameter| parameter.text()));
                let self_type = base.declared_self(name.text(), parameters.len());
                let base = base.with_self(self_type);
                let site = Site {
                    owner,
                    owner_span: Some(name.span()),
                    interface: None,
                };
                for member in decl.members() {
                    let scope = base.clone().with_binders(binder_names(&member));
                    check(&member, &site, &scope, &mut found, false);
                }
                for block in decl.impls() {
                    let conversion = block.target().is_some_and(|target| {
                        matches!(target.form(), Some(TypeForm::Path { path, .. })
                            if matches!(path.dotted().as_str(), "From" | "TryFrom"))
                    });
                    let mut inside = base.clone();
                    for choice in block.choices() {
                        if let (Some(choice), Some(chosen)) =
                            (choice.name(), choice.chosen())
                            && let Some(chosen) = base.read(&chosen)
                        {
                            inside = inside.with_choice(choice.text(), chosen);
                        }
                    }
                    for member in block.members() {
                        let scope = inside.clone().with_binders(binder_names(&member));
                        check(&member, &site, &scope, &mut found, conversion);
                    }
                }
            }
            Declaration::Interface(decl) => {
                let site = Site {
                    owner: None,
                    owner_span: decl.name().map(|name| name.span()),
                    interface: decl
                        .name()
                        .map(|name| format!("{path}.{}", name.text())),
                };
                let base = top
                    .clone()
                    .with_binders(
                        decl.params().iter().map(|parameter| parameter.text()),
                    )
                    .with_self(Some(Written::SelfType))
                    .with_associated(
                        decl.assoc_types()
                            .iter()
                            .filter_map(|assoc| assoc.name())
                            .map(|name| name.text())
                            .collect::<Vec<_>>(),
                    );
                for member in decl.members() {
                    let scope = base.clone().with_binders(binder_names(&member));
                    check(&member, &site, &scope, &mut found, false);
                }
            }
            Declaration::Use(_) | Declaration::Let(_) | Declaration::Test(_) => {}
        }
    }
    found
}

fn binder_names<'a>(callable: &Callable<'a>) -> Vec<&'a str> {
    callable
        .params()
        .into_iter()
        .map(|parameter| parameter.text())
        .collect()
}

/// The span of the string operand of a `symbol` section.
fn symbol_span(callable: &Callable<'_>) -> Option<ByteSpan> {
    let section = callable.symbol()?;
    section
        .node()
        .children()
        .iter()
        .find(|child| child.kind() == SyntaxKind::String)
        .map(|child| child.span())
}

/// Where a declaration is.
struct Site<'s> {
    /// The owner a `@compiler` row compares: the type's tag or role, or the
    /// last part of the module path; `None` inside an interface.
    owner: Option<&'s str>,
    /// The span of the owner's name, when the owner is a declaration.
    owner_span: Option<ByteSpan>,
    /// The path atom of the enclosing interface, such as
    /// `@std.io.StdoutText`, which a host row compares.
    interface: Option<String>,
}

fn check(
    callable: &Callable<'_>,
    site: &Site<'_>,
    scope: &Scope<'_>,
    found: &mut Vec<Diagnostic>,
    conversion: bool,
) {
    let (owner, owner_span) = (site.owner, site.owner_span);
    let Some(provider) = callable.external().and_then(|section| section.tag()) else {
        return;
    };
    let (Some(symbol), Some(symbol_span)) = (
        callable.symbol().and_then(|section| section.value()),
        symbol_span(callable),
    ) else {
        return;
    };
    let declaration_span = callable.construct().span();
    match provider.text() {
        "@compiler" => {
            let Some(row) = CompilerIntrinsic::from_symbol(&symbol) else {
                found.push(Diagnostic::new(
                    DiagnosticCode::ExternalUnknownSymbol,
                    symbol_span,
                    format!("`{symbol}` is not a row of the `@compiler` registry"),
                ));
                return;
            };
            let destination = conversion.then(|| conversion_destination(row)).flatten();
            let (row_owner, member) = match destination {
                Some(destination) => (destination, "convert".to_owned()),
                None => (
                    row.owner(),
                    symbol
                        .split_once('.')
                        .map_or(symbol.as_str(), |(_, member)| member)
                        .to_owned(),
                ),
            };
            if owner != Some(row_owner) {
                let mut diagnostic = Diagnostic::new(
                    DiagnosticCode::ExternalOwnerMismatch,
                    symbol_span,
                    format!(
                        "the row `{symbol}` is owned by `{row_owner}`, not by the owner of this declaration"
                    ),
                );
                if let Some(span) = owner_span {
                    diagnostic =
                        diagnostic.with_related(span, "the owner of the declaration");
                }
                found.push(diagnostic);
                return;
            }
            if let Some(diagnostic) =
                name_fault(callable, &symbol, &member, declaration_span)
            {
                found.push(diagnostic);
                return;
            }
            if !signature_fits(callable, row, scope) {
                found.push(
                    Diagnostic::new(
                        DiagnosticCode::ExternalSignatureMismatch,
                        declaration_span,
                        format!("the signature differs from the row `{symbol}`"),
                    )
                    .with_note(format!("expected {}", expected_signature(row))),
                );
            }
        }
        "@host" => {
            let (row_owner, member) = if let Some(row) = host_row(&symbol) {
                (row.owner.owner(), row.owner.member())
            } else if let Some(row) = test_world_row(&symbol) {
                (row.owner(), row.op.member())
            } else {
                found.push(Diagnostic::new(
                    DiagnosticCode::ExternalUnknownSymbol,
                    symbol_span,
                    format!(
                        "`{symbol}` is not a row of the host registry or of the \
                         test-world registry"
                    ),
                ));
                return;
            };
            let row_owner = row_owner.path();
            if site.interface.as_deref() != Some(row_owner.as_str()) {
                let mut diagnostic = Diagnostic::new(
                    DiagnosticCode::ExternalOwnerMismatch,
                    symbol_span,
                    format!(
                        "the host row `{symbol}` is owned by the interface `{row_owner}`, \
                         not by the owner of this declaration"
                    ),
                );
                if let Some(span) = owner_span {
                    diagnostic =
                        diagnostic.with_related(span, "the owner of the declaration");
                }
                found.push(diagnostic);
                return;
            }
            if let Some(diagnostic) =
                name_fault(callable, &symbol, member, declaration_span)
            {
                found.push(diagnostic);
                return;
            }
            if callable.kind() != CallableKind::Proc {
                found.push(Diagnostic::new(
                    DiagnosticCode::ExternalSignatureMismatch,
                    declaration_span,
                    format!(
                        "the host row `{symbol}` is an operation, which a `proc` binds"
                    ),
                ));
            }
        }
        _ => {}
    }
}

/// The owner of the destination of a conversion row, the type whose
/// `From` or `TryFrom` implementation binds it; `None` for any other row.
const fn conversion_destination(row: CompilerIntrinsic) -> Option<&'static str> {
    match row {
        CompilerIntrinsic::Convert(_, target) => Some(target.owner()),
        CompilerIntrinsic::Integer(_, IntegerOp::ToStr)
        | CompilerIntrinsic::Float(_, FloatOp::ToStr) => Some("str"),
        CompilerIntrinsic::Integer(integer, IntegerOp::Parse) => Some(integer.owner()),
        CompilerIntrinsic::Float(float, FloatOp::Parse) => Some(float.owner()),
        CompilerIntrinsic::DictFromList => Some("dict"),
        _ => None,
    }
}

/// `@external.signatureMismatch` at the declaration when its name is not
/// `member`, the member part of the symbol it binds: `fn addChecked` binds a
/// symbol that ends in `.addChecked`.
fn name_fault(
    callable: &Callable<'_>,
    symbol: &str,
    member: &str,
    declaration_span: ByteSpan,
) -> Option<Diagnostic> {
    let name = callable.name()?;
    (name.text() != member).then(|| {
        Diagnostic::new(
            DiagnosticCode::ExternalSignatureMismatch,
            declaration_span,
            format!(
                "the declaration `{}` binds `{symbol}`, whose member is `{member}`",
                name.text()
            ),
        )
        .with_note(format!("expected the name `{member}`"))
    })
}

/// Whether the written signature of `callable` is the signature of `row`.
fn signature_fits(
    callable: &Callable<'_>,
    row: CompilerIntrinsic,
    scope: &Scope<'_>,
) -> bool {
    let (parameters, result) = row.signature();
    let has_bound = callable
        .given()
        .is_some_and(|given| !given.entries().is_empty());
    if callable.kind() != CallableKind::Fn
        || has_bound
        || callable.requires().is_some()
        || callable.ensures().is_some()
        || !callable.labelled().is_empty()
        || scope.binder_count() != row.generics().len()
    {
        return false;
    }
    let takes = callable.takes();
    let Some((_, written_result)) = callable.result() else {
        return false;
    };
    takes.len() == parameters.len()
        && takes
            .iter()
            .zip(&parameters)
            .all(|((_, written), expected)| {
                scope.read(written) == Some(Written::of_row(expected))
            })
        && scope.read(&written_result) == Some(Written::of_row(&result))
}

/// The row's signature in source spelling: `Fn({T T} R)`, generics first
/// when there are any.
fn expected_signature(row: CompilerIntrinsic) -> String {
    let generics = row.generics();
    let (parameters, result) = row.signature();
    let parameters = parameters
        .iter()
        .map(|parameter| parameter.spelling(generics))
        .collect::<Vec<_>>()
        .join(" ");
    let callable = format!("Fn({{{parameters}}} {})", result.spelling(generics));
    if generics.is_empty() {
        callable
    } else {
        format!("{callable} over {}", generics.join(" "))
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use vibra_diagnostics::{ByteSpan, DiagnosticCode};
    use vibra_syntax::parse_source_with;
    use vibra_syntax::source::{SourceOptions, module};

    use super::check_externals;
    use crate::load_stdlib;

    /// The codes `check_externals` gives for `source` read as the module
    /// `path` of the standard library, each with the text of its span.
    fn check(path: &str, source: &str) -> Vec<(DiagnosticCode, String)> {
        let stdlib = load_stdlib().expect("the embedded standard library loads");
        let document = parse_source_with(
            Some("fixture.vib"),
            source.as_bytes(),
            SourceOptions {
                standard_library: true,
            },
        );
        assert!(
            document.diagnostics().is_empty(),
            "{:?}",
            document.diagnostics()
        );
        let view = module(&document).expect("the fixture has a source tree");
        let text = |span: ByteSpan| source.get(span.start()..span.end()).unwrap_or("");
        check_externals(&view, path, stdlib.names())
            .iter()
            .map(|found| (found.code(), text(found.primary_span()).to_owned()))
            .collect()
    }

    fn interface(name: &str, member: &str, symbol: &str) -> String {
        format!(
            "interface {name}\n  visibility @public\n  proc {member}\n    \
             takes {{grant: Self}}\n    returns done: Void\n    external @host\n    \
             symbol \"{symbol}\"\n  endproc\nendinterface\n"
        )
    }

    #[test]
    fn a_host_row_on_its_owner_is_accepted() {
        let source = interface("StdoutText", "write", "io.stdoutText");
        assert_eq!(check("@std.io", &source), Vec::new());
        let source = interface("DirList", "entries", "fs.readEntries");
        assert_eq!(check("@std.fs", &source), Vec::new());
    }

    #[test]
    fn a_test_world_row_on_test_world_is_accepted() {
        let source = interface("TestWorld", "directory", "testWorld.directory");
        assert_eq!(check("@std.test", &source), Vec::new());
    }

    #[test]
    fn another_interface_or_module_is_an_owner_mismatch_at_the_symbol() {
        let owner = (
            DiagnosticCode::ExternalOwnerMismatch,
            "\"io.stdoutText\"".to_owned(),
        );
        let other = interface("StdoutBytes", "write", "io.stdoutText");
        assert_eq!(check("@std.io", &other), vec![owner.clone()]);
        let moved = interface("StdoutText", "write", "io.stdoutText");
        assert_eq!(check("@std.fs", &moved), vec![owner.clone()]);
        let top = "proc write\n  takes {grant: Bytes}\n  returns done: Void\n  \
                   visibility @public\n  external @host\n  symbol \"io.stdoutText\"\nendproc\n";
        assert_eq!(check("@std.io", top), vec![owner]);
        let world = interface("StdoutText", "directory", "testWorld.directory");
        assert_eq!(
            check("@std.io", &world),
            vec![(
                DiagnosticCode::ExternalOwnerMismatch,
                "\"testWorld.directory\"".to_owned()
            )]
        );
    }

    #[test]
    fn the_member_must_be_the_owner_s_member() {
        let source = interface("StdoutText", "stdoutText", "io.stdoutText");
        let codes = check("@std.io", &source)
            .into_iter()
            .map(|(code, _)| code)
            .collect::<Vec<_>>();
        assert_eq!(codes, vec![DiagnosticCode::ExternalSignatureMismatch]);
    }

    #[test]
    fn a_major_1_spelling_is_an_unknown_symbol() {
        let source = interface("StdinText", "read", "io.stdin-text");
        assert_eq!(
            check("@std.io", &source),
            vec![(
                DiagnosticCode::ExternalUnknownSymbol,
                "\"io.stdin-text\"".to_owned()
            )]
        );
    }
}
