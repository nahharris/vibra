//! An executable program: a module set and its entry, with no
//! initializer cycle (`docs/spec/06-runtime.md`, "Module-value
//! initialization").

use std::collections::BTreeSet;
use std::sync::Arc;

use crate::module_set::may_match;
use crate::{Callable, CheckedModuleSet, Facts, IrError, Type};

/// A module set and the function a run starts in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckedProgram {
    module_set: Arc<CheckedModuleSet>,
    entry: usize,
}

/// A body whose facts the analysis follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Owner {
    Global(usize),
    Function(usize),
    Closure(usize),
}

impl From<Callable> for Owner {
    fn from(callable: Callable) -> Self {
        match callable {
            Callable::Function(index) => Self::Function(index),
            Callable::Closure(index) => Self::Closure(index),
        }
    }
}

fn facts_of(module_set: &CheckedModuleSet, owner: Owner) -> Option<&Facts> {
    let flow = module_set.flow();
    match owner {
        Owner::Global(index) => flow.globals.get(index),
        Owner::Function(index) => flow.functions.get(index),
        Owner::Closure(index) => flow.closures.get(index).map(|(_, facts)| facts),
    }
}

/// Whether initializing `root` may read `root` again: the bodies its
/// initializer reaches, through reads of other module values, direct and
/// contract calls, and calls of callable values that those bodies created,
/// include a read of it.
fn reaches_itself(module_set: &CheckedModuleSet, root: usize) -> bool {
    let mut visited = BTreeSet::new();
    let mut pending = vec![Owner::Global(root)];
    let mut escaped: Vec<(Callable, Type)> = Vec::new();
    let mut indirect: Vec<Type> = Vec::new();
    loop {
        while let Some(owner) = pending.pop() {
            if !visited.insert(owner) {
                continue;
            }
            let Some(facts) = facts_of(module_set, owner) else {
                continue;
            };
            if facts.reads.contains(&root) {
                return true;
            }
            pending.extend(facts.reads.iter().map(|read| Owner::Global(*read)));
            pending.extend(facts.calls.iter().map(|call| Owner::Function(*call)));
            escaped.extend(facts.values.iter().cloned());
            indirect.extend(facts.indirect.iter().cloned());
        }
        for (callable, created) in &escaped {
            let owner = Owner::from(*callable);
            if !visited.contains(&owner)
                && indirect.iter().any(|callee| may_match(created, callee))
            {
                pending.push(owner);
            }
        }
        if pending.is_empty() {
            return false;
        }
    }
}

impl CheckedProgram {
    /// The program of `module_set` that starts in the function `entry`.
    ///
    /// # Errors
    ///
    /// The entry is not a function of the set, is generic or a contract
    /// member, or takes inputs; or the set has an initializer cycle, which
    /// names the first module value on one.
    pub fn for_entry(
        module_set: Arc<CheckedModuleSet>,
        entry: usize,
    ) -> Result<Self, IrError> {
        let function = module_set
            .functions()
            .get(entry)
            .ok_or(IrError::UnknownFunction(entry))?;
        let signature = &function.signature;
        if !signature.binders().is_empty() || signature.associated().is_some() {
            return Err(IrError::InvalidEntry(format!(
                "`{}` is generic or an interface member",
                function.name
            )));
        }
        let inputs = match signature.type_info() {
            Type::Fn(callable) => callable.slot_count(),
            Type::Proc(callable) => callable
                .parameters
                .len()
                .saturating_add(callable.labels.len()),
            _ => usize::MAX,
        };
        if inputs != 0 {
            return Err(IrError::InvalidEntry(format!(
                "`{}` takes inputs",
                function.name
            )));
        }
        for (index, global) in module_set.globals().iter().enumerate() {
            if reaches_itself(&module_set, index) {
                return Err(IrError::InitializerCycle {
                    global: index,
                    name: global.name.clone(),
                });
            }
        }
        Ok(Self { module_set, entry })
    }

    /// The module set.
    #[must_use]
    pub const fn module_set(&self) -> &Arc<CheckedModuleSet> {
        &self.module_set
    }

    /// The entry function's index.
    #[must_use]
    pub const fn entry(&self) -> usize {
        self.entry
    }
}
