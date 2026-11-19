//! Activations: their slots, their captured environment, and the type
//! arguments they run at (`docs/spec/06-runtime.md`, "Generic
//! instantiation").

use std::rc::Rc;

use vibra_ir::{ExitGuard, NamedTypes, Substitution, Type, Value};

use crate::machine::Halt;
use crate::value::{ClosureVal, Val};

/// The closed substitution an activation runs under: its binders, and for
/// a default member, the receiver and the selected implementation's
/// choices.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TypeEnv {
    pub(crate) binders: Vec<Type>,
    pub(crate) self_type: Option<Type>,
    pub(crate) associated: Option<NamedTypes>,
}

impl TypeEnv {
    /// An environment of `binders` alone.
    pub(crate) const fn of(binders: Vec<Type>) -> Self {
        Self {
            binders,
            self_type: None,
            associated: None,
        }
    }

    fn is_empty(&self) -> bool {
        self.binders.is_empty() && self.self_type.is_none() && self.associated.is_none()
    }

    /// `ty` at this activation's type arguments.
    pub(crate) fn apply(&self, ty: &Type) -> Type {
        if self.is_empty() {
            return ty.clone();
        }
        ty.substitute(&Substitution {
            binders: &self.binders,
            self_type: self.self_type.as_ref(),
            associated: self.associated.as_ref(),
        })
    }

    /// Each of `types` at this activation's type arguments.
    pub(crate) fn apply_all(&self, types: &[Type]) -> Vec<Type> {
        types.iter().map(|ty| self.apply(ty)).collect()
    }
}

/// One activation.
#[derive(Debug)]
pub(crate) struct Frame<'a> {
    pub(crate) slots: Vec<Option<Val<'a>>>,
    /// The closure being run, whose captures `Captured` reads.
    pub(crate) closure: Option<Rc<ClosureVal<'a>>>,
    pub(crate) env: Rc<TypeEnv>,
    /// The `ensures … else` still to run on the candidate, once the body
    /// has run under a satisfied or absent `requires`.
    pub(crate) exit: Option<&'a ExitGuard>,
    /// The neutral values an evaluation starts with, converted on first
    /// read at the type the read names.
    pub(crate) inputs: &'a [Value],
}

impl<'a> Frame<'a> {
    /// A frame of `slot_count` unset slots, `None` when the host cannot hold
    /// them. The machine reserves the frame's bytes against its budget
    /// first.
    pub(crate) fn try_new(slot_count: usize, env: Rc<TypeEnv>) -> Option<Self> {
        let mut slots = Vec::new();
        slots.try_reserve_exact(slot_count).ok()?;
        slots.resize(slot_count, None);
        Some(Self {
            slots,
            closure: None,
            env,
            exit: None,
            inputs: &[],
        })
    }

    /// Sets `slot`, which the frame has.
    pub(crate) fn set(&mut self, slot: usize, value: Val<'a>) -> Result<(), Halt> {
        let count = self.slots.len();
        let place = self.slots.get_mut(slot).ok_or_else(|| {
            Halt::Invalid(format!("slot {slot} of an activation with {count}"))
        })?;
        *place = Some(value);
        Ok(())
    }
}
