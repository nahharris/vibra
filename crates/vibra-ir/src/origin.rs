//! Source origins carried by checked nodes, and the canonical origin table
//! of a program (`docs/spec/06-runtime.md`, "Audit trace", "Origins").

use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::sync::Arc;

use vibra_diagnostics::ByteSpan;

use crate::{Callable, CheckedModuleSet, CheckedProgram, FunctionBody, Node};

/// A source identity and a span: where a checked node came from.
///
/// A trap keeps the origin of the node that raised it, as the runtime
/// chapter's "Traps" requires.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceOrigin {
    /// Shared, so that cloning an origin, which every node carries, never
    /// copies the source identity.
    source_id: Arc<str>,
    span: ByteSpan,
}

impl SourceOrigin {
    /// The origin of `span` in the document `source_id`.
    #[must_use]
    pub fn new(source_id: impl Into<Arc<str>>, span: ByteSpan) -> Self {
        Self {
            source_id: source_id.into(),
            span,
        }
    }

    /// The source document identity.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// The half-open byte span in that document.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }
}

/// The chapter's order of origins: by the bytes of the source identity, then
/// by start, then by end.
fn canonical(left: &SourceOrigin, right: &SourceOrigin) -> Ordering {
    left.source_id
        .as_bytes()
        .cmp(right.source_id.as_bytes())
        .then(left.span.start().cmp(&right.span.start()))
        .then(left.span.end().cmp(&right.span.end()))
}

/// The canonical origin table of a program: every distinct source origin of
/// the bodies reachable from its entry, in the chapter's order, numbered
/// from 1. The interpreter, the lowering, and the trace of a trap write
/// ordinals of this one table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OriginTable {
    origins: Vec<SourceOrigin>,
}

/// A body whose origins the table holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Owner {
    Global(usize),
    Function(usize),
    Closure(usize),
}

impl OriginTable {
    /// The table of `program`. A body is reachable from the entry function
    /// through the module values it reads, the functions it enters, and the
    /// callables it creates as values; each reachable function and module
    /// value gives its own origin and the origin of every node of its body,
    /// guards, and closures. The bodies are walked with explicit work lists.
    #[must_use]
    pub fn of(program: &CheckedProgram) -> Self {
        let set = program.module_set();
        let mut origins = Vec::new();
        let mut visited = BTreeSet::new();
        let mut pending = vec![Owner::Function(program.entry())];
        while let Some(owner) = pending.pop() {
            if !visited.insert(owner) {
                continue;
            }
            let flow = set.flow();
            let facts = match owner {
                Owner::Global(index) => flow.globals.get(index),
                Owner::Function(index) => flow.functions.get(index),
                Owner::Closure(index) => {
                    flow.closures.get(index).map(|(_, facts)| facts)
                }
            };
            if let Some(facts) = facts {
                pending.extend(facts.reads.iter().map(|read| Owner::Global(*read)));
                pending.extend(facts.calls.iter().map(|call| Owner::Function(*call)));
                pending.extend(facts.values.iter().map(
                    |(callable, _)| match callable {
                        Callable::Function(index) => Owner::Function(*index),
                        Callable::Closure(index) => Owner::Closure(*index),
                    },
                ));
            }
            collect(set, owner, &mut origins);
        }
        origins.sort_by(canonical);
        origins.dedup();
        Self { origins }
    }

    /// The ordinal of `origin`, from 1; zero for an origin outside the
    /// table, which the trace writes as "no source origin".
    #[must_use]
    pub fn ordinal(&self, origin: &SourceOrigin) -> u64 {
        self.origins
            .binary_search_by(|held| canonical(held, origin))
            .ok()
            .and_then(|index| u64::try_from(index.saturating_add(1)).ok())
            .unwrap_or(0)
    }

    /// The origins in ordinal order: the origin at position `n` has the
    /// ordinal `n + 1`.
    #[must_use]
    pub fn origins(&self) -> &[SourceOrigin] {
        &self.origins
    }
}

/// Adds the origins of `owner`'s declaration and of every node of its
/// bodies. A closure's nodes are in the body that creates it.
fn collect(set: &CheckedModuleSet, owner: Owner, origins: &mut Vec<SourceOrigin>) {
    let mut roots: Vec<&Node> = Vec::new();
    match owner {
        Owner::Global(index) => {
            if let Some(global) = set.globals().get(index) {
                origins.push(global.origin.clone());
                roots.push(&global.initializer);
            }
        }
        Owner::Function(index) => {
            if let Some(function) = set.functions().get(index) {
                origins.push(function.origin.clone());
                if let FunctionBody::Expr(body) = &function.body {
                    roots.push(body);
                }
                for guard in function
                    .entry_guard
                    .iter()
                    .chain(function.exit_guard.iter().map(|exit| &exit.guard))
                {
                    roots.extend([&guard.predicate, &guard.consequence]);
                }
            }
        }
        Owner::Closure(_) => {}
    }
    while let Some(node) = roots.pop() {
        origins.push(node.origin.clone());
        roots.extend(node.expr.operands());
    }
}
