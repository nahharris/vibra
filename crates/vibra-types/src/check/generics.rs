//! The expanding-instantiation check.
//!
//! `docs/spec/02-type-system.md`, "Generics": each call of, and reference
//! to, a generic function inside the body of a generic function is an
//! instantiation. Its type argument for a parameter `j` of the callee carries
//! a parameter `i` of the caller when it mentions it, and expands it when it
//! mentions it and is not that parameter alone. The edges `(caller, i)` to
//! `(callee, j)` form a graph; an expanding edge on a cycle is
//! `@type.expandingInstantiation`. One diagnostic is reported for each cycle,
//! at its first expanding instantiation in source order, related to the
//! other instantiations of the cycle.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::Type;

use super::declare::Program;

/// One type parameter: a function and the parameter's position.
type Param = (usize, usize);

/// One instantiation inside a generic body: the function whose body holds
/// it, the function it instantiates, its type arguments, and where.
pub(crate) struct Instantiation {
    pub(crate) caller: usize,
    pub(crate) callee: usize,
    pub(crate) arguments: Vec<Type>,
    pub(crate) span: ByteSpan,
}

struct Edge {
    from: Param,
    to: Param,
    expanding: bool,
    /// The index of the instantiation the edge comes from.
    site: usize,
}

fn mentions(ty: &Type, binder: usize) -> bool {
    let mut found = false;
    ty.visit(&mut |inner| found |= *inner == Type::Binder(binder));
    found
}

/// The shortest path of edges from `start` to `goal`.
fn path(start: Param, goal: Param, edges: &[Edge]) -> Option<Vec<usize>> {
    let mut parent: BTreeMap<Param, usize> = BTreeMap::new();
    let mut seen = BTreeSet::from([start]);
    let mut queue = VecDeque::from([start]);
    while let Some(node) = queue.pop_front() {
        if node == goal {
            let mut found = Vec::new();
            let mut at = goal;
            while at != start {
                let edge = *parent.get(&at)?;
                found.push(edge);
                at = edges.get(edge)?.from;
            }
            found.reverse();
            return Some(found);
        }
        for (index, edge) in edges.iter().enumerate() {
            if edge.from == node && seen.insert(edge.to) {
                parent.insert(edge.to, index);
                queue.push_back(edge.to);
            }
        }
    }
    None
}

/// The diagnostics of every cycle of instantiations that expands a type
/// parameter.
pub(crate) fn expanding_instantiations(
    program: &Program<'_>,
    sites: &[Instantiation],
) -> Vec<Diagnostic> {
    let generic = |function: usize| {
        program
            .functions
            .get(function)
            .map_or(0, |data| data.binders.len())
    };
    let mut edges = Vec::new();
    for (site, instantiation) in sites.iter().enumerate() {
        for binder in 0..generic(instantiation.caller) {
            for (at, argument) in instantiation.arguments.iter().enumerate() {
                if mentions(argument, binder) {
                    edges.push(Edge {
                        from: (instantiation.caller, binder),
                        to: (instantiation.callee, at),
                        expanding: *argument != Type::Binder(binder),
                        site,
                    });
                }
            }
        }
    }
    let module_of = |site: usize| {
        sites
            .get(site)
            .and_then(|found| program.functions.get(found.caller))
            .map_or(usize::MAX, |data| data.module)
    };
    let order = |edge: &Edge| {
        let span = sites
            .get(edge.site)
            .map_or(ByteSpan::empty_at(0), |found| found.span);
        (module_of(edge.site), span.start(), span.end())
    };
    let mut cyclic = edges
        .iter()
        .enumerate()
        .filter(|(_, edge)| edge.expanding)
        .filter_map(|(index, edge)| {
            path(edge.to, edge.from, &edges).map(|back| (index, back))
        })
        .collect::<Vec<_>>();
    cyclic.sort_by_key(|(index, _)| edges.get(*index).map(order));
    let mut covered: BTreeSet<usize> = BTreeSet::new();
    let mut diagnostics = Vec::new();
    for (index, back) in cyclic {
        if covered.contains(&index) {
            continue;
        }
        let Some(edge) = edges.get(index) else {
            continue;
        };
        covered.insert(index);
        covered.extend(back.iter().copied());
        let (Some(site), Some(caller)) = (
            sites.get(edge.site),
            program.functions.get(edge.caller_of(sites)),
        ) else {
            continue;
        };
        let Some(module) = program.modules.get(caller.module) else {
            continue;
        };
        if module.embedded || !module.in_scope {
            continue;
        }
        let callee = program
            .functions
            .get(site.callee)
            .map(|data| data.name.clone())
            .unwrap_or_default();
        let mut diagnostic = Diagnostic::new(
            DiagnosticCode::TypeExpandingInstantiation,
            site.span,
            format!(
                "this instantiation of `{callee}` builds a larger type around a type parameter that comes back to it, so its instantiations never end"
            ),
        )
        .with_source_id(module.source_id.as_ref());
        for step in back {
            let Some(other) = edges.get(step).and_then(|edge| sites.get(edge.site))
            else {
                continue;
            };
            if other.span == site.span && other.caller == site.caller {
                continue;
            }
            let name = program
                .functions
                .get(other.callee)
                .map(|data| data.name.clone())
                .unwrap_or_default();
            let message = format!("an instantiation of `{name}` on the same cycle");
            let other_module = program
                .functions
                .get(other.caller)
                .and_then(|data| program.modules.get(data.module));
            diagnostic = match other_module {
                Some(other_module) if other_module.source_id != module.source_id => {
                    diagnostic.with_related_source(
                        other_module.source_id.as_ref(),
                        other.span,
                        message,
                    )
                }
                _ => diagnostic.with_related(other.span, message),
            };
        }
        diagnostics.push(diagnostic);
    }
    diagnostics
}

impl Edge {
    fn caller_of(&self, sites: &[Instantiation]) -> usize {
        sites.get(self.site).map_or(self.from.0, |site| site.caller)
    }
}
