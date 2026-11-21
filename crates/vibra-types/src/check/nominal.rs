//! The finite-size check of declared types.
//!
//! `docs/spec/02-type-system.md`, "Recursive types": a reference to a type
//! is symbolic, and recursion through `List`, `Dict`, or a callable type is
//! finite. A type whose direct expansion repeats has an infinite size, which
//! is `@type.infiniteSize` at the declaration whose expansion first repeats:
//! one diagnostic for each cycle of types, at its first declaration in
//! declaration order, related to the other declarations of the cycle.
//!
//! A type parameter is expanded in place when the body holds it directly, so
//! `type A of {b: Box(A)}` with `type Box(T) of {item: T}` repeats; which
//! parameters a body holds directly is found first, to a fixpoint.

use std::collections::BTreeSet;

use vibra_diagnostics::{Diagnostic, DiagnosticCode};
use vibra_ir::Type;

use super::declare::{Program, TypeKind};

/// The types a body is directly built from.
fn body_components(kind: &TypeKind) -> Vec<&Type> {
    match kind {
        TypeKind::Wrapper(representation) => vec![representation],
        TypeKind::Tuple(components) => components.iter().collect(),
        TypeKind::Record(fields) => fields.iter().map(|(_, ty)| ty).collect(),
        TypeKind::Variant(cases) => cases
            .iter()
            .flat_map(|case| case.fields.iter().flatten().map(|(_, ty)| ty))
            .collect(),
        _ => Vec::new(),
    }
}

/// What the body of a type holds directly: the declared types, by program
/// index, and its own parameters, by position.
struct Inline {
    types: BTreeSet<usize>,
    params: BTreeSet<usize>,
}

/// Walks the body of `kind` through the positions that are stored in place,
/// given which parameters of every type are stored in place.
fn inline(program: &Program<'_>, kind: &TypeKind, held: &[BTreeSet<usize>]) -> Inline {
    let mut found = Inline {
        types: BTreeSet::new(),
        params: BTreeSet::new(),
    };
    let mut pending = body_components(kind);
    while let Some(ty) = pending.pop() {
        match ty {
            Type::Binder(at) => {
                found.params.insert(*at);
            }
            Type::Tuple(_) | Type::Record(_) => pending.extend(ty.components()),
            Type::Declared(id, arguments) => {
                let Some(index) = program.type_index_of(id) else {
                    continue;
                };
                found.types.insert(index);
                for (at, argument) in arguments.iter().enumerate() {
                    if held.get(index).is_some_and(|params| params.contains(&at)) {
                        pending.push(argument);
                    }
                }
            }
            _ => {}
        }
    }
    found
}

/// Every type reachable from `start` along `edges`.
fn reach(start: usize, edges: &[BTreeSet<usize>]) -> BTreeSet<usize> {
    let mut seen = BTreeSet::new();
    let mut pending = vec![start];
    while let Some(next) = pending.pop() {
        for target in edges.get(next).into_iter().flatten() {
            if seen.insert(*target) {
                pending.push(*target);
            }
        }
    }
    seen
}

/// Reports every cycle of types whose expansion repeats.
pub(crate) fn infinite_sizes(program: &mut Program<'_>) {
    let count = program.types.len();
    let mut held = vec![BTreeSet::new(); count];
    loop {
        let mut grew = false;
        for index in 0..count {
            let Some(data) = program.types.get(index) else {
                continue;
            };
            let found = inline(program, &data.kind, &held);
            if let Some(params) = held.get_mut(index)
                && !found.params.is_subset(params)
            {
                params.extend(found.params);
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    let edges = (0..count)
        .map(|index| {
            program
                .types
                .get(index)
                .map(|data| inline(program, &data.kind, &held).types)
                .unwrap_or_default()
        })
        .collect::<Vec<_>>();
    let mut reported = BTreeSet::new();
    let mut found = Vec::new();
    for index in 0..count {
        if reported.contains(&index) {
            continue;
        }
        let reachable = reach(index, &edges);
        if !reachable.contains(&index) {
            continue;
        }
        let cycle = reachable
            .iter()
            .copied()
            .filter(|other| reach(*other, &edges).contains(&index))
            .collect::<BTreeSet<_>>();
        reported.extend(cycle.iter().copied());
        found.push((index, cycle));
    }
    for (first, cycle) in found {
        let Some(data) = program.types.get(first) else {
            continue;
        };
        let module = data.module;
        let mut diagnostic = Diagnostic::new(
            DiagnosticCode::TypeInfiniteSize,
            data.view.construct().span(),
            format!(
                "`{}` holds itself directly, so its expansion never ends; hold the recursion in a `List`, a `Dict`, or a callable",
                data.name
            ),
        );
        for other in cycle.iter().filter(|other| **other != first) {
            let Some(other) = program.types.get(*other) else {
                continue;
            };
            let message = format!("`{}` is on the same cycle", other.name);
            let span = other.view.construct().span();
            diagnostic = if other.module == module {
                diagnostic.with_related(span, message)
            } else {
                match program.modules.get(other.module) {
                    Some(source) => diagnostic.with_related_source(
                        source.source_id.as_ref(),
                        span,
                        message,
                    ),
                    None => diagnostic,
                }
            };
        }
        program.fault(module, diagnostic);
        for index in cycle {
            if let Some(data) = program.types.get_mut(index) {
                data.kind = TypeKind::Faulty;
                data.id = None;
            }
        }
    }
}
