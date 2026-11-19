//! Imports: locators, derived names, private access, and cycles.
//!
//! `docs/spec/04-programs-and-packages.md`, "Modules and imports": resolving
//! `@unit.c1…cn` walks the parts from the unit's root, descending while a
//! part names a directory and stopping at the first part that names a
//! module; the remaining parts resolve against that module's declarations.
//! A walk that reaches no module is `@module.unknownPath`. An import names a
//! module or one top-level declaration; a path that resolves to anything
//! else, such as a member, a case, or a field, is `@name.wrongEntityKind`
//! at the import. Resolution is total and access is not: an import of a
//! private declaration is `@name.privateAccess`, related to the declaration.
//! An import whose target module imports, directly or through a chain, the
//! importing module is `@module.importCycle` at the later edge, related
//! along the cycle.
//!
//! "Packages, units, and targets": a module names its own unit, `@std`, and
//! its project's dependencies; a module of `@tests` also names every local
//! target. Any other unit is `@module.unknownPath` at the reference.

use std::collections::VecDeque;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};

use crate::graph::{Access, Graph, ImportTarget, atom_part_span};
use crate::identity::DeclarationKind;
use crate::input::UnitKind;
use crate::namespaces::at;
use crate::prelude::{STD_UNIT, std_module};

/// One import edge between two local modules.
#[derive(Clone, Copy, Debug)]
struct Edge {
    from: usize,
    to: usize,
    import: usize,
}

/// Resolves every import, then reports the cycles of the import graph.
pub(crate) fn resolve_imports(graph: &mut Graph<'_>, out: &mut Vec<Diagnostic>) {
    let mut edges = Vec::new();
    for index in 0..graph.imports.len() {
        let target = resolve_import(graph, index, out);
        if let Some(import) = graph.imports.get_mut(index) {
            import.target = target;
        }
        let to = match target {
            ImportTarget::LocalModule(module) => Some(module),
            ImportTarget::Decl(decl) => graph.decls.get(decl).map(|decl| decl.module),
            ImportTarget::Unresolved | ImportTarget::StdModule(_) => None,
        };
        if let (Some(to), Some(import)) = (to, graph.imports.get(index)) {
            edges.push(Edge {
                from: import.module,
                to,
                import: index,
            });
        }
    }
    cycles(graph, edges, out);
}

/// The locator span of import `index`.
fn locator_span(graph: &Graph<'_>, index: usize) -> ByteSpan {
    graph
        .imports
        .get(index)
        .and_then(|import| import.view.locator())
        .map_or_else(|| ByteSpan::empty_at(0), |locator| locator.span())
}

fn resolve_import(
    graph: &Graph<'_>,
    index: usize,
    out: &mut Vec<Diagnostic>,
) -> ImportTarget {
    let Some(import) = graph.imports.get(index) else {
        return ImportTarget::Unresolved;
    };
    let Some(locator) = import.view.locator() else {
        return ImportTarget::Unresolved;
    };
    let Some(module) = graph.modules.get(import.module) else {
        return ImportTarget::Unresolved;
    };
    let source_id = module.source_id.as_str();
    let parts = locator.parts();
    let unknown_path = |out: &mut Vec<Diagnostic>, message: String| {
        out.push(at(
            DiagnosticCode::ModuleUnknownPath,
            source_id,
            locator.span(),
            message,
        ));
        ImportTarget::Unresolved
    };
    let Some(first) = parts.first() else {
        return ImportTarget::Unresolved;
    };
    let unit = format!("@{first}");
    if unit == STD_UNIT {
        let path = parts
            .get(..2)
            .map(|parts| format!("@{}", parts.join(".")))
            .unwrap_or_default();
        let Some(std) = std_module(&path) else {
            return unknown_path(
                out,
                format!("`{}` reaches no module", locator.text()),
            );
        };
        if parts.len() > 2 {
            out.push(at(
                DiagnosticCode::ToolUnavailable,
                source_id,
                locator.span(),
                "the declarations of the standard library are not available to resolution yet",
            ));
            return ImportTarget::Unresolved;
        }
        return ImportTarget::StdModule(std);
    }
    let visible = unit == module.unit
        || (module.unit_kind == UnitKind::Tests
            && graph
                .modules
                .iter()
                .any(|other| other.unit == unit && other.unit_kind != UnitKind::Tests));
    if !visible {
        return unknown_path(
            out,
            format!("the unit `{unit}` is not visible from `{}`", module.path),
        );
    }
    let mut prefix = unit;
    let mut found = None;
    for (position, part) in parts.iter().enumerate().skip(1) {
        prefix.push('.');
        prefix.push_str(part);
        if let Some(target) =
            graph.modules.iter().position(|other| other.path == prefix)
        {
            found = Some((target, position));
            break;
        }
        let directory = format!("{prefix}.");
        if !graph
            .modules
            .iter()
            .any(|other| other.path.starts_with(&directory))
        {
            break;
        }
    }
    let Some((target, position)) = found else {
        return unknown_path(out, format!("`{}` reaches no module", locator.text()));
    };
    let Some(name) = parts.get(position + 1) else {
        return ImportTarget::LocalModule(target);
    };
    let part_span = |at: usize| {
        atom_part_span(locator.text(), locator.span(), at).unwrap_or(locator.span())
    };
    let Some(decl) = top_level(graph, target, name) else {
        out.push(at(
            DiagnosticCode::NameUnknownSymbol,
            source_id,
            part_span(position + 1),
            format!("the module declares no `{name}`"),
        ));
        return ImportTarget::Unresolved;
    };
    let Some(declaration) = graph.decls.get(decl) else {
        return ImportTarget::Unresolved;
    };
    if let Some(member) = parts.get(position + 2) {
        match declaration.members.iter().find(|entry| entry.name == *member) {
            Some(entry) => out.push(at(
                DiagnosticCode::NameWrongEntityKind,
                source_id,
                locator.span(),
                format!(
                    "`{}` is {} of `{name}`; an import names a module or a top-level declaration",
                    locator.text(),
                    entry.kind.noun()
                ),
            )),
            None => out.push(at(
                DiagnosticCode::NameUnknownSymbol,
                source_id,
                part_span(position + 2),
                format!("`{name}` has no member `{member}`"),
            )),
        }
        return ImportTarget::Unresolved;
    }
    if declaration.access == Access::Private && declaration.module != import.module {
        out.push(
            at(
                DiagnosticCode::NamePrivateAccess,
                source_id,
                locator.span(),
                format!("`{name}` is private to its module"),
            )
            .with_related_source(
                graph.source_id(declaration.module),
                declaration.name_span,
                "the private declaration",
            ),
        );
    }
    ImportTarget::Decl(decl)
}

/// The top-level declaration of `module` named `name`. Imports of that
/// module are not its declarations: there are no re-exports. When a name is
/// declared twice, the later declaration binds.
pub(crate) fn top_level(graph: &Graph<'_>, module: usize, name: &str) -> Option<usize> {
    graph
        .modules
        .get(module)?
        .top
        .iter()
        .copied()
        .rev()
        .find(|&index| {
            graph.decls.get(index).is_some_and(|decl| {
                decl.name == name && decl.kind() != DeclarationKind::Test
            })
        })
}

/// Adds the edges in source order; an edge whose target already reaches its
/// source closes a cycle, is reported, and is left out of the graph.
fn cycles(graph: &Graph<'_>, mut edges: Vec<Edge>, out: &mut Vec<Diagnostic>) {
    edges.sort_by(|left, right| {
        (
            graph.source_id(left.from),
            locator_span(graph, left.import).start(),
        )
            .cmp(&(
                graph.source_id(right.from),
                locator_span(graph, right.import).start(),
            ))
    });
    let mut accepted: Vec<Vec<Edge>> = vec![Vec::new(); graph.modules.len()];
    for edge in edges {
        let Some(path) = path(&accepted, edge.to, edge.from) else {
            if let Some(outgoing) = accepted.get_mut(edge.from) {
                outgoing.push(edge);
            }
            continue;
        };
        let mut diagnostic = at(
            DiagnosticCode::ModuleImportCycle,
            graph.source_id(edge.from),
            locator_span(graph, edge.import),
            "this import closes a cycle of imports",
        );
        for earlier in path {
            diagnostic = diagnostic.with_related_source(
                graph.source_id(earlier.from),
                locator_span(graph, earlier.import),
                "an import of the cycle",
            );
        }
        out.push(diagnostic);
    }
}

/// The accepted edges of the shortest path from `from` to `to`, in order;
/// an empty path when the two are one module; `None` when there is none.
fn path(accepted: &[Vec<Edge>], from: usize, to: usize) -> Option<Vec<Edge>> {
    if from == to {
        return Some(Vec::new());
    }
    let mut reached: Vec<Option<Edge>> = vec![None; accepted.len()];
    let mut queue = VecDeque::from([from]);
    while let Some(module) = queue.pop_front() {
        for edge in accepted.get(module).into_iter().flatten() {
            if edge.to == from || reached.get(edge.to).is_some_and(Option::is_some) {
                continue;
            }
            if let Some(slot) = reached.get_mut(edge.to) {
                *slot = Some(*edge);
            }
            if edge.to == to {
                let mut path = Vec::new();
                let mut current = to;
                while current != from {
                    let step = reached.get(current).copied().flatten()?;
                    path.push(step);
                    current = step.from;
                }
                path.reverse();
                return Some(path);
            }
            queue.push_back(edge.to);
        }
    }
    None
}
