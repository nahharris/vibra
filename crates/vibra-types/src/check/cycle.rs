//! The initializer-cycle analysis.
//!
//! `docs/spec/06-runtime.md`, "Module-value initialization": a module value
//! whose initializer can reach itself through module values, function
//! calls, and function values is `@type.initializerCycle`. One diagnostic is
//! reported for each cycle, at the `let` of the cycle's first module value in
//! source order, related to the references along the cycle in order.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use vibra_diagnostics::{Diagnostic, DiagnosticCode};

use super::declare::Program;
use super::expr::{Reference, Target};

fn source_of(program: &Program<'_>, target: Target) -> Option<String> {
    let module = match target {
        Target::Function(index) => program.functions.get(index)?.module,
        Target::Value(index) => program.values.get(index)?.module,
        Target::Test(index) => program.tests.get(index)?.module,
    };
    program
        .modules
        .get(module)
        .map(|data| data.source_id.as_ref().to_owned())
}

fn name_of(program: &Program<'_>, target: Target) -> String {
    match target {
        Target::Function(index) => program
            .functions
            .get(index)
            .map(|data| data.name.clone())
            .unwrap_or_default(),
        Target::Value(index) => program
            .values
            .get(index)
            .map(|data| data.name.clone())
            .unwrap_or_default(),
        Target::Test(index) => program
            .tests
            .get(index)
            .map(|data| data.qualified.clone())
            .unwrap_or_default(),
    }
}

/// Every declaration reachable from `start` along `edges`.
fn reach(start: Target, edges: &BTreeMap<Target, Vec<Target>>) -> BTreeSet<Target> {
    let mut seen = BTreeSet::new();
    let mut pending = vec![start];
    while let Some(next) = pending.pop() {
        for target in edges.get(&next).into_iter().flatten() {
            if seen.insert(*target) {
                pending.push(*target);
            }
        }
    }
    seen
}

/// The references from `start` back to it along the shortest path.
fn shortest_cycle(
    start: Target,
    references: &BTreeMap<Target, Vec<Reference>>,
) -> Option<Vec<(Target, Reference)>> {
    let mut parent: BTreeMap<Target, (Target, Reference)> = BTreeMap::new();
    let mut queue = VecDeque::from([start]);
    let mut visited = BTreeSet::from([start]);
    while let Some(current) = queue.pop_front() {
        for reference in references.get(&current).into_iter().flatten() {
            if reference.target == start {
                let mut path = vec![(current, *reference)];
                let mut at = current;
                while at != start {
                    let (from, edge) = *parent.get(&at)?;
                    path.push((from, edge));
                    at = from;
                }
                path.reverse();
                return Some(path);
            }
            if visited.insert(reference.target) {
                parent.insert(reference.target, (current, *reference));
                queue.push_back(reference.target);
            }
        }
    }
    None
}

/// One `@type.initializerCycle` for each cycle through module values.
pub(crate) fn initializer_cycles(
    program: &Program<'_>,
    references: &BTreeMap<Target, Vec<Reference>>,
) -> Vec<Diagnostic> {
    let edges: BTreeMap<Target, Vec<Target>> = references
        .iter()
        .map(|(from, refs)| {
            (
                *from,
                refs.iter().map(|reference| reference.target).collect(),
            )
        })
        .collect();
    let mut reversed: BTreeMap<Target, Vec<Target>> = BTreeMap::new();
    for (from, targets) in &edges {
        for target in targets {
            reversed.entry(*target).or_default().push(*from);
        }
    }
    let mut reported: BTreeSet<Target> = BTreeSet::new();
    let mut diagnostics = Vec::new();
    for (index, data) in program.values.iter().enumerate() {
        let start = Target::Value(index);
        if reported.contains(&start) || !references.contains_key(&start) {
            continue;
        }
        let Some(path) = shortest_cycle(start, references) else {
            continue;
        };
        let forward = reach(start, &edges);
        let backward = reach(start, &reversed);
        reported.extend(forward.intersection(&backward).copied());
        let Some(source) = source_of(program, start) else {
            continue;
        };
        let mut diagnostic = Diagnostic::new(
            DiagnosticCode::TypeInitializerCycle,
            data.view.construct().span(),
            format!("the initializer of `{}` reaches itself", data.name),
        )
        .with_source_id(source.as_str());
        for (from, reference) in path {
            let message = format!(
                "`{}` refers to `{}` here",
                name_of(program, from),
                name_of(program, reference.target)
            );
            diagnostic = match source_of(program, from) {
                Some(from_source) if from_source != source => {
                    diagnostic.with_related_source(from_source, reference.span, message)
                }
                _ => diagnostic.with_related(reference.span, message),
            };
        }
        diagnostics.push(diagnostic);
    }
    diagnostics
}
