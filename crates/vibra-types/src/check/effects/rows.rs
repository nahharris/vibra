//! Rows as written: the entries of a `performs` section, resolved to the
//! operation interfaces they name.
//!
//! `docs/spec/03-effects.md`, "Operation interfaces" and "Effect ceilings":
//! an interface that declares at least one `proc` member, abstract or
//! default, is an operation interface, and its identity is its
//! declaration's, never an import name. A row entry is a path that MUST
//! resolve to one (`@effect.invalidReference` at the entry otherwise, a path
//! that resolves to nothing included); a row that names one interface
//! twice, under any spelling, is `@effect.duplicateRoot` at the later
//! entry, related to the first; and the owner of a member written in the
//! member's own `performs` is `@effect.duplicateRoot` at that entry. A
//! resolved row is a [`Row`], sorted by the UTF-8 bytes of the paths.
//!
//! The row of a written `Proc` or `Intent` type resolves the same way, an
//! omitted row being the empty row ([`super::values`]).

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::Row;
use vibra_syntax::SyntaxKind;
use vibra_syntax::source::{CallableKind, Row as Written};

use crate::check::declare::{Entity, NoType, Program};

/// Whether the interface `index` of `program` is an operation interface: it
/// declares at least one `proc` member, abstract or with a body.
pub(crate) fn is_operation_interface(program: &Program<'_>, index: usize) -> bool {
    program.interfaces.get(index).is_some_and(|data| {
        data.view
            .members()
            .iter()
            .any(|member| member.kind() == CallableKind::Proc)
    })
}

/// The row `written` of a declaration of `module`, resolved, with `owner`
/// joined to it: the interface whose member the declaration is, if any.
/// No row written is the empty row. The first entry that names no
/// operation interface, or one already named, is the fault.
pub(crate) fn resolve(
    program: &Program<'_>,
    module: usize,
    written: Option<Written<'_>>,
    owner: Option<usize>,
) -> Result<Row, NoType> {
    let mut seen: Vec<(usize, Option<ByteSpan>)> = Vec::new();
    if let Some(owner) = owner {
        seen.push((owner, None));
    }
    for path in written.iter().flat_map(Written::paths) {
        let span = path.node().span();
        let found = match program.path_entity(module, &path) {
            Some(Entity::Interface(index))
                if is_operation_interface(program, index) =>
            {
                index
            }
            Some(Entity::Interface(_)) => {
                return Err(fault(Diagnostic::new(
                    DiagnosticCode::EffectInvalidReference,
                    span,
                    format!(
                        "`{}` declares no `proc` member, so it is no operation interface and no row names it",
                        path.dotted()
                    ),
                )));
            }
            Some(_) => {
                return Err(fault(Diagnostic::new(
                    DiagnosticCode::EffectInvalidReference,
                    span,
                    format!(
                        "`{}` is not an interface; a row names operation interfaces",
                        path.dotted()
                    ),
                )));
            }
            None => {
                return Err(fault(Diagnostic::new(
                    DiagnosticCode::EffectInvalidReference,
                    span,
                    format!(
                        "`{}` names nothing here; a row names operation interfaces",
                        path.dotted()
                    ),
                )));
            }
        };
        if let Some((_, first)) = seen.iter().find(|(index, _)| *index == found) {
            let diagnostic = Diagnostic::new(
                DiagnosticCode::EffectDuplicateRoot,
                span,
                format!("`{}` is named twice in this row", path.dotted()),
            );
            let diagnostic = match first {
                Some(first) => program.related(diagnostic, module, *first, "the first"),
                None => owner_related(program, found, diagnostic),
            };
            return Err(fault(diagnostic));
        }
        seen.push((found, Some(span)));
    }
    let mut roots = Vec::new();
    for (index, _) in seen {
        let id = program
            .interfaces
            .get(index)
            .and_then(|data| data.id.clone())
            .ok_or(NoType::Unsupported)?;
        roots.push(id.entity().clone());
    }
    Row::new(roots).map_err(|_| NoType::Unresolved)
}

/// The `performs` section whose row is `written`: from its keyword through
/// the end of the row, among the children of `node`.
pub(crate) fn section(node: &vibra_syntax::CstNode, written: Written<'_>) -> ByteSpan {
    node.children()
        .iter()
        .find(|child| child.kind() == SyntaxKind::Performs)
        .map_or(written.node().span(), |section| {
            let start = section
                .children()
                .first()
                .map_or(section.span().start(), |keyword| keyword.span().start());
            ByteSpan::new(start, written.node().span().end())
        })
}

/// `diagnostic` of an owner written in its own member's row, related to the
/// owner's name.
fn owner_related(
    program: &Program<'_>,
    owner: usize,
    diagnostic: Diagnostic,
) -> Diagnostic {
    let Some(data) = program.interfaces.get(owner) else {
        return diagnostic.with_note(
            "the owner of a member is in its row already; a member writes only what it adds",
        );
    };
    let diagnostic = diagnostic.with_note(format!(
        "`{}` owns this member, so it is in the member's row already; a member writes only what it adds",
        data.name
    ));
    match data.view.name() {
        Some(name) => {
            program.related(diagnostic, data.module, name.span(), "the owner")
        }
        None => diagnostic,
    }
}

/// A fault of a written row, still to be reported.
fn fault(diagnostic: Diagnostic) -> NoType {
    NoType::Fault(Box::new(diagnostic))
}
