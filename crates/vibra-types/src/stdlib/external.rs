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
//! An `external @host` declaration is checked here for its symbol, against the
//! host registry, and for its kind: the host rows are `proc` operations.

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::external::CompilerIntrinsic;
use vibra_ir::external::host::host_row;
use vibra_syntax::SyntaxKind;
use vibra_syntax::source::{Callable, CallableKind, Declaration, Module, TypeBody};

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
                check(&callable, Some(module_owner), None, &scope, &mut found);
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
                let owner_span = Some(name.span());
                for member in decl.members() {
                    let scope = base.clone().with_binders(binder_names(&member));
                    check(&member, owner, owner_span, &scope, &mut found);
                }
                for block in decl.impls() {
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
                        check(&member, owner, owner_span, &scope, &mut found);
                    }
                }
            }
            Declaration::Interface(decl) => {
                let owner_span = decl.name().map(|name| name.span());
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
                    check(&member, None, owner_span, &scope, &mut found);
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

fn check(
    callable: &Callable<'_>,
    owner: Option<&str>,
    owner_span: Option<ByteSpan>,
    scope: &Scope<'_>,
    found: &mut Vec<Diagnostic>,
) {
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
            if owner != Some(row.owner()) {
                let mut diagnostic = Diagnostic::new(
                    DiagnosticCode::ExternalOwnerMismatch,
                    symbol_span,
                    format!(
                        "the row `{symbol}` is owned by `{}`, not by the owner of this declaration",
                        row.owner()
                    ),
                );
                if let Some(span) = owner_span {
                    diagnostic =
                        diagnostic.with_related(span, "the owner of the declaration");
                }
                found.push(diagnostic);
                return;
            }
            if let Some(diagnostic) = name_fault(callable, &symbol, declaration_span) {
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
            if host_row(&symbol).is_none() {
                found.push(Diagnostic::new(
                    DiagnosticCode::ExternalUnknownSymbol,
                    symbol_span,
                    format!("`{symbol}` is not a row of the host registry"),
                ));
                return;
            }
            if let Some(diagnostic) = name_fault(callable, &symbol, declaration_span) {
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

/// `@external.signatureMismatch` at the declaration when its name is not the
/// member part of the symbol it binds: `fn addChecked` binds a symbol that
/// ends in `.addChecked`.
fn name_fault(
    callable: &Callable<'_>,
    symbol: &str,
    declaration_span: ByteSpan,
) -> Option<Diagnostic> {
    let member = symbol.split_once('.').map_or(symbol, |(_, member)| member);
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
