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
//! that the row names, it MUST be a `proc`, and its written signature MUST
//! be the row's with `Self` first and no `performs`, so that its complete
//! row is `{owner}` (each `@external.signatureMismatch` at the declaration,
//! the row's signature in a note). A test-world row's signature takes the
//! world as `Self` and its fixture data as the row's labelled parameters,
//! in order, and returns the row's result.
//!
//! The three narrowing rows `grant.restrict`, `grant.sub`, and `grant.limit`
//! are bound in the standard operation interfaces with their generic `G` read
//! as the interface's `Self`: `grant.limit` in each of the 17, the other two
//! in the six `Dir…` interfaces of `@std.fs`; any other owner is
//! `@external.ownerMismatch`.

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::Library;
use vibra_ir::external::host::{
    HostInterface, HostRow, TestWorldRow, host_row, test_world_row,
};
use vibra_ir::external::{
    CompilerIntrinsic, FloatOp, IntegerOp, RowType, spell_generics,
};
use vibra_syntax::SyntaxKind;
use vibra_syntax::source::{
    Callable, CallableKind, Declaration, Module, TypeBody, TypeForm,
};

use super::names::{HostDeclaration, LibraryNames, Scope, Written};

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
            if is_grant_row(row) {
                check_grant(callable, row, &symbol, symbol_span, site, scope, found);
                return;
            }
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
            let operation = host_row(&symbol);
            let (row_owner, member) = if let Some(row) = operation {
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
                return;
            }
            let mismatch = match (operation, test_world_row(&symbol)) {
                (Some(row), _) => (!host_signature_fits(callable, row, scope))
                    .then(|| row.signature()),
                (None, Some(row)) => (!test_world_signature_fits(callable, row, scope))
                    .then(|| row.signature()),
                (None, None) => None,
            };
            if let Some(expected) = mismatch {
                found.push(
                    Diagnostic::new(
                        DiagnosticCode::ExternalSignatureMismatch,
                        declaration_span,
                        format!("the signature differs from the host row `{symbol}`"),
                    )
                    .with_note(format!("expected {expected}")),
                );
            }
        }
        _ => {}
    }
}

/// Whether `row` is one of the three narrowing rows, `grant.restrict`,
/// `grant.sub`, and `grant.limit`.
const fn is_grant_row(row: CompilerIntrinsic) -> bool {
    matches!(
        row,
        CompilerIntrinsic::GrantRestrict
            | CompilerIntrinsic::GrantSub
            | CompilerIntrinsic::GrantLimit
    )
}

/// The check of a narrowing row (`docs/spec/03-effects.md`, "Narrowing";
/// `docs/spec/06-runtime.md`, "Compiler intrinsic registry"). The rows are
/// bound in the standard operation interfaces, generic over the grant type,
/// which in the interface is `Self`: `grant.limit` in each of the 17, and
/// `grant.restrict` and `grant.sub` in the six `Dir…` interfaces of
/// `@std.fs`. Another owner is `@external.ownerMismatch` at the symbol;
/// another name or signature `@external.signatureMismatch` at the
/// declaration.
fn check_grant(
    callable: &Callable<'_>,
    row: CompilerIntrinsic,
    symbol: &str,
    symbol_span: ByteSpan,
    site: &Site<'_>,
    scope: &Scope<'_>,
    found: &mut Vec<Diagnostic>,
) {
    let declaration_span = callable.construct().span();
    let directories_only = row != CompilerIntrinsic::GrantLimit;
    let owner = site
        .interface
        .as_deref()
        .and_then(HostInterface::by_owner_path);
    let owned = owner.is_some_and(|interface| {
        !directories_only || interface.module() == DIRECTORY_MODULE
    });
    if !owned {
        let owners = if directories_only {
            "the six `Dir…` interfaces of `@std.fs`"
        } else {
            "the standard operation interfaces"
        };
        let mut diagnostic = Diagnostic::new(
            DiagnosticCode::ExternalOwnerMismatch,
            symbol_span,
            format!(
                "the row `{symbol}` is bound in {owners}, not by the owner of this declaration"
            ),
        );
        if let Some(span) = site.owner_span {
            diagnostic = diagnostic.with_related(span, "the owner of the declaration");
        }
        found.push(diagnostic);
        return;
    }
    if let Some(diagnostic) =
        name_fault(callable, symbol, &row.member(), declaration_span)
    {
        found.push(diagnostic);
        return;
    }
    let (parameters, result) = row.signature();
    let as_self = |ty: &RowType| grant_written(ty);
    let has_bound = callable
        .given()
        .is_some_and(|given| !given.entries().is_empty());
    let takes = callable.takes();
    let fits = callable.kind() == CallableKind::Fn
        && !has_bound
        && callable.requires().is_none()
        && callable.ensures().is_none()
        && callable.labelled().is_empty()
        && scope.binder_count() == 0
        && takes.len() == parameters.len()
        && takes
            .iter()
            .zip(&parameters)
            .all(|((_, written), expected)| scope.read(written) == as_self(expected))
        && callable
            .result()
            .is_some_and(|(_, written)| scope.read(&written) == as_self(&result));
    if !fits {
        found.push(
            Diagnostic::new(
                DiagnosticCode::ExternalSignatureMismatch,
                declaration_span,
                format!("the signature differs from the row `{symbol}`"),
            )
            .with_note(format!(
                "expected {} with `G` the interface's `Self`",
                expected_signature(row)
            )),
        );
    }
}

/// The module of the six `Dir…` interfaces.
const DIRECTORY_MODULE: &str = "@std.fs";

/// A type of a narrowing row with its generic `G` read as `Self`; `None` for
/// a shape the three rows do not have.
fn grant_written(ty: &RowType) -> Option<Written> {
    match ty {
        RowType::Generic(0) => Some(Written::SelfType),
        RowType::Narrowing(declaration) => Some(Written::Narrowing(*declaration)),
        RowType::Library(library, arguments) => Some(Written::Library(
            *library,
            arguments
                .iter()
                .map(grant_written)
                .collect::<Option<Vec<_>>>()?,
        )),
        _ => None,
    }
}

/// Whether the written signature of `callable` is the signature of the host
/// row `row`: a `proc` with no bound, guard, labelled parameter, type
/// parameter, or `performs`, whose parameters are `Self` and then the row's,
/// and whose result is `Result` of the row's `Ok` type and error type. Its
/// complete row is then `{owner}`, the row's.
fn host_signature_fits(
    callable: &Callable<'_>,
    row: &HostRow,
    scope: &Scope<'_>,
) -> bool {
    let has_bound = callable
        .given()
        .is_some_and(|given| !given.entries().is_empty());
    if has_bound
        || callable.performs().is_some()
        || callable.requires().is_some()
        || callable.ensures().is_some()
        || !callable.labelled().is_empty()
        || scope.binder_count() != 0
    {
        return false;
    }
    let takes = callable.takes();
    let expected = std::iter::once(Some(Written::SelfType))
        .chain(
            row.parameters
                .iter()
                .map(|parameter| Written::of_host(parameter.ty)),
        )
        .collect::<Vec<_>>();
    let result = Written::of_host(row.ok).map(|ok| {
        Written::Library(
            Library::Result,
            vec![ok, Written::Host(HostDeclaration::Error(row.error))],
        )
    });
    takes.len() == expected.len()
        && takes.iter().zip(&expected).all(|((_, written), expected)| {
            expected.is_some() && scope.read(written) == *expected
        })
        && result.is_some()
        && callable
            .result()
            .is_some_and(|(_, written)| scope.read(&written) == result)
}

/// Whether the written signature of `callable` is the signature of the
/// test-world row `row`: a `proc` with no bound, guard, type parameter, or
/// `performs`, whose one parameter is `Self`, whose labelled parameters
/// are the row's in order, and whose result is the row's. Its complete row
/// is then `{TestWorld}`.
fn test_world_signature_fits(
    callable: &Callable<'_>,
    row: &TestWorldRow,
    scope: &Scope<'_>,
) -> bool {
    let has_bound = callable
        .given()
        .is_some_and(|given| !given.entries().is_empty());
    if has_bound
        || callable.performs().is_some()
        || callable.requires().is_some()
        || callable.ensures().is_some()
        || scope.binder_count() != 0
    {
        return false;
    }
    let takes = callable.takes();
    let labelled = callable.labelled();
    let world = matches!(takes.as_slice(), [(_, written)]
        if scope.read(written) == Some(Written::SelfType));
    let fixtures = labelled.len() == row.labelled.len()
        && labelled
            .iter()
            .zip(row.labelled)
            .all(|((name, written, _), expected)| {
                name.text() == expected.name
                    && Written::of_host(expected.ty)
                        .is_some_and(|expected| scope.read(written) == Some(expected))
            });
    let result = Written::of_host(row.result);
    world
        && fixtures
        && result.is_some()
        && callable
            .result()
            .is_some_and(|(_, written)| scope.read(&written) == result)
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
    let generics = callable.params();
    let mut written_bounds = callable
        .given()
        .map(|given| given.entries())
        .unwrap_or_default()
        .iter()
        .map(|(name, bound)| {
            let at = generics
                .iter()
                .position(|generic| generic.text() == name.text());
            (at, scope.read(bound))
        })
        .collect::<Vec<_>>();
    written_bounds.sort_by_key(|(at, _)| *at);
    let expected_bounds = row
        .bounds()
        .iter()
        .map(|(at, bound)| (Some(*at), Some(Written::Library(*bound, Vec::new()))))
        .collect::<Vec<_>>();
    if callable.kind() != CallableKind::Fn
        || written_bounds != expected_bounds
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
        format!("{callable} over {}", spell_generics(generics, row.bounds()))
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

    /// An interface `name` whose one member `member` binds `symbol` with
    /// `external @host`, taking `takes` and returning `returns`.
    fn bound(
        name: &str,
        member: &str,
        symbol: &str,
        takes: &str,
        returns: &str,
    ) -> String {
        format!(
            "interface {name}\n  visibility @public\n  proc {member}\n    \
             takes {{{takes}}}\n    returns outcome: {returns}\n    external @host\n    \
             symbol \"{symbol}\"\n  endproc\nendinterface\n"
        )
    }

    /// An interface `name` whose one member `member` binds `symbol` with
    /// `external @host` and the signature of no host row.
    fn interface(name: &str, member: &str, symbol: &str) -> String {
        bound(name, member, symbol, "grant: Self", "Void")
    }

    /// An interface `name` with the narrowing member `member` bound to
    /// `symbol` with `external @compiler`, taking `takes` and returning
    /// `returns`.
    fn narrowing(
        name: &str,
        member: &str,
        symbol: &str,
        takes: &str,
        returns: &str,
    ) -> String {
        format!(
            "interface {name}\n  visibility @public\n  fn {member}\n    \
             takes {{{takes}}}\n    returns narrowed: {returns}\n    external @compiler\n    \
             symbol \"{symbol}\"\n  endfn\nendinterface\n"
        )
    }

    fn codes(found: Vec<(DiagnosticCode, String)>) -> Vec<DiagnosticCode> {
        found.into_iter().map(|(code, _)| code).collect()
    }

    #[test]
    fn a_host_row_on_its_owner_is_accepted() {
        let source = bound(
            "StdoutText",
            "write",
            "io.stdoutText",
            "grant: Self value: Str",
            "Result(Void IoError)",
        );
        assert_eq!(check("@std.io", &source), Vec::new());
        let source = bound(
            "DirList",
            "entries",
            "fs.readEntries",
            "grant: Self at: RelativePath",
            "Result(List(Str) FsError)",
        );
        assert_eq!(check("@std.fs", &source), Vec::new());
    }

    #[test]
    fn another_signature_than_the_host_row_s_is_a_signature_mismatch() {
        let mismatch = vec![DiagnosticCode::ExternalSignatureMismatch];
        let text = |takes: &str, returns: &str| {
            bound("DirReadText", "read", "fs.readText", takes, returns)
        };
        let at_str = text("grant: Self at: Str", "Result(Str FsError)");
        assert_eq!(codes(check("@std.fs", &at_str)), mismatch);
        let bytes = text("grant: Self at: RelativePath", "Result(Bytes FsError)");
        assert_eq!(codes(check("@std.fs", &bytes)), mismatch);
        let error = text("grant: Self at: RelativePath", "Result(Str ScopeError)");
        let error = format!("use @std.policy.ScopeError enduse\n\n{error}");
        assert_eq!(codes(check("@std.fs", &error)), mismatch);
        let extra = text(
            "grant: Self at: RelativePath max: U64",
            "Result(Str FsError)",
        );
        assert_eq!(codes(check("@std.fs", &extra)), mismatch);
    }

    #[test]
    fn performs_on_a_host_bound_member_is_a_signature_mismatch() {
        let source = "interface StdoutText\n  visibility @public\n  proc write\n    \
                      takes {grant: Self value: Str}\n    \
                      returns outcome: Result(Void IoError)\n    \
                      performs {StdoutText}\n    external @host\n    \
                      symbol \"io.stdoutText\"\n  endproc\nendinterface\n";
        assert_eq!(
            codes(check("@std.io", source)),
            vec![DiagnosticCode::ExternalSignatureMismatch]
        );
    }

    #[test]
    fn a_narrowing_row_on_an_operation_interface_is_accepted() {
        let limit = narrowing(
            "StdinText",
            "limit",
            "grant.limit",
            "grant: Self limits: GrantLimits",
            "Self",
        );
        let limit = format!("use @std.policy.GrantLimits enduse\n\n{limit}");
        assert_eq!(check("@std.io", &limit), Vec::new());
        let sub = narrowing(
            "DirList",
            "sub",
            "grant.sub",
            "grant: Self prefix: RelativePath",
            "Result(Self ScopeError)",
        );
        let sub = format!("use @std.policy.ScopeError enduse\n\n{sub}");
        assert_eq!(check("@std.fs", &sub), Vec::new());
    }

    #[test]
    fn a_narrowing_row_elsewhere_is_an_owner_mismatch_at_the_symbol() {
        let owner = vec![(
            DiagnosticCode::ExternalOwnerMismatch,
            "\"grant.restrict\"".to_owned(),
        )];
        let restrict = narrowing(
            "StdinText",
            "restrict",
            "grant.restrict",
            "grant: Self policy: FsPolicy",
            "Result(Self ScopeError)",
        );
        assert_eq!(check("@std.io", &restrict), owner);
        let local = narrowing(
            "Local",
            "limit",
            "grant.limit",
            "grant: Self limits: GrantLimits",
            "Self",
        );
        assert_eq!(
            codes(check("@std.io", &local)),
            vec![DiagnosticCode::ExternalOwnerMismatch]
        );
    }

    #[test]
    fn another_narrowing_signature_is_a_signature_mismatch() {
        let source = narrowing(
            "StdinText",
            "limit",
            "grant.limit",
            "grant: Self limits: GrantLimits",
            "Result(Self ScopeError)",
        );
        let source = format!(
            "use @std.policy.GrantLimits enduse\n\nuse @std.policy.ScopeError enduse\n\n{source}"
        );
        assert_eq!(
            codes(check("@std.io", &source)),
            vec![DiagnosticCode::ExternalSignatureMismatch]
        );
    }

    /// The `TestWorld` interface whose one member `environment` binds its
    /// row with the labelled section `labelled` and the result `returns`.
    fn environment(labelled: &str, returns: &str) -> String {
        format!(
            "use @std.env.EnvReadVar enduse\n\nuse @std.env.EnvReadArgs enduse\n\n\
             interface TestWorld\n  visibility @public\n  proc environment\n    \
             takes {{world: Self}}\n    labelled {{{labelled}}}\n    \
             returns grants: {returns}\n    external @host\n    \
             symbol \"testWorld.environment\"\n  endproc\nendinterface\n"
        )
    }

    const VARIABLES: &str =
        "variables: Dict(Str Str) be [:] arguments: List(Str) be []";

    #[test]
    fn a_test_world_row_on_test_world_is_accepted() {
        let source =
            environment(VARIABLES, "{arguments: EnvReadArgs variables: EnvReadVar}");
        assert_eq!(check("@std.test", &source), Vec::new());
    }

    #[test]
    fn another_test_world_signature_is_a_signature_mismatch() {
        let mismatch = vec![DiagnosticCode::ExternalSignatureMismatch];
        let grants = "{variables: EnvReadVar arguments: EnvReadArgs}";
        let swapped = environment(
            "arguments: List(Str) be [] variables: Dict(Str Str) be [:]",
            grants,
        );
        assert_eq!(codes(check("@std.test", &swapped)), mismatch);
        let missing = environment("variables: Dict(Str Str) be [:]", grants);
        assert_eq!(codes(check("@std.test", &missing)), mismatch);
        let result = environment(VARIABLES, "{variables: EnvReadVar}");
        assert_eq!(codes(check("@std.test", &result)), mismatch);
        let grant = interface("TestWorld", "directory", "testWorld.directory");
        assert_eq!(codes(check("@std.test", &grant)), mismatch);
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
        assert_eq!(
            codes(check("@std.io", &source)),
            vec![DiagnosticCode::ExternalSignatureMismatch]
        );
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
