//! [`Generated`]: an admitted program as [`vibra_nexus::Code`], so the one
//! dispatcher [`vibra_nexus::run`] drives generated code exactly as it drives
//! the reference stepper.

use vibra_ir::boundary::TrapCode;
use vibra_nexus::{Code, Context, Halt, Step};

use crate::abi::{
    NexusContext, STATUS_CONTINUE, STATUS_DONE, STATUS_EXHAUSTED, STATUS_TRAP,
};
use crate::admission::Admitted;
use crate::routines::Session;

/// An admitted program, ready to step.
#[derive(Debug)]
pub struct Generated {
    admitted: Admitted,
}

impl Generated {
    /// Wraps a program that passed admission.
    #[must_use]
    pub const fn new(admitted: Admitted) -> Self {
        Self { admitted }
    }

    /// The admitted program.
    #[must_use]
    pub const fn admitted(&self) -> &Admitted {
        &self.admitted
    }

    /// The number of module values a context for this program holds.
    #[must_use]
    pub fn globals(&self) -> usize {
        self.admitted.globals()
    }

    /// Pushes the entry frame.
    ///
    /// # Errors
    ///
    /// The host event when the context's limit does not admit the frame.
    pub fn start(&self, ctx: &mut Context<'_>) -> Result<(), Halt> {
        let slots = usize::try_from(self.admitted.entry)
            .ok()
            .and_then(|entry| self.admitted.functions.get(entry))
            .map_or(0, |entry| entry.slots);
        ctx.enter(self.admitted.entry, slots)
    }
}

impl Code for Generated {
    fn step(&self, ctx: &mut Context<'_>) -> Step {
        let (function, resume) = match (ctx.function(), ctx.resume()) {
            (Ok(function), Ok(resume)) => (function, resume),
            _ => return Step::Trap,
        };
        let step = usize::try_from(function)
            .ok()
            .and_then(|index| self.admitted.functions.get(index))
            .map(|info| info.step);
        let Some(step) = step else {
            ctx.trap(TrapCode::InvalidCheckedProgram, 0);
            return Step::Trap;
        };
        let mut session = Session {
            ctx,
            code: &self.admitted,
        };
        let handle = std::ptr::from_mut(&mut session).cast::<NexusContext>();
        // SAFETY: `step` is a step function of the admitted program, which the
        // code generator guarantees follows the contract of
        // `crate::VibraStep` and the routines. `handle` addresses `session`,
        // which stays alive and unused by this function until the call
        // returns, and the contract forbids the step from keeping the handle.
        let status = unsafe { step(handle, resume) };
        match status {
            STATUS_CONTINUE => Step::Continue,
            STATUS_DONE => Step::Done,
            STATUS_TRAP => Step::Trap,
            STATUS_EXHAUSTED => Step::Exhausted,
            // `STATUS_WAIT` is reserved for v0.0.4, and no other value is a
            // status: a step that answers one is a defect of the generator.
            _ => {
                session.ctx.trap(TrapCode::InvalidCheckedProgram, 0);
                Step::Trap
            }
        }
    }
}
