//! Nexus, the Vibra native runtime.
//!
//! `docs/roadmap/nexus/design.md` is the design, and
//! `docs/roadmap/nexus/decisions.md` records why it has this shape. The crate
//! is the part of the runtime that owns execution state and is written once,
//! whatever runs the program's code:
//!
//! - a [`Context`] is one isolated execution: its value [`Store`], its runtime
//!   frames, its module values, its trap record, and its audit trace. Nothing
//!   of one context is visible to another;
//! - [`Code`] is what executes a program one **step** at a time. A step runs
//!   the top frame from its resume point to the next checkpoint and reports a
//!   [`Step`]. The reference stepper of `vibra-nexus-eval` implements it over
//!   execution IR, and generated machine code implements it through the
//!   bridge; both drive the same transfers of [`Context`], so a frame is
//!   pushed, replaced, and popped by one implementation; and
//! - [`run`] is the dispatcher: it steps until the program completes, stops,
//!   or uses up its quota, and a later [`run`] continues exactly where a yield
//!   left off.
//!
//! Frames never live on the native stack: a non-tail call pushes a runtime
//! frame and returns to the dispatcher, a tail transfer replaces the current
//! frame, and depth is bounded only by the context's memory limit, as
//! `docs/spec/06-runtime.md`, "Activations and memory", requires.
//!
//! This crate has no `unsafe` code and no dependency on the execution IR or on
//! a code generator.

pub mod arith;
mod context;
mod host;
pub mod store;

use vibra_ir::boundary::TrapCode;
use vibra_ir::external::host::{HOST_REGISTRY_MINOR, host_row};

pub use arith::IntOp;
pub use context::{Context, Halt, ScalarOp, Stats, Trap};
pub use store::{Cell, Ref, Store, StoreError};

/// The version of the boundary between generated code and this runtime.
///
/// It is independent of the registry version, of the build format, and of the
/// toolchain release. Before the first alpha it changes with any change to a
/// transfer, a layout, or a status code, and a program built for another
/// version is refused by [`admit`].
pub const ABI_VERSION: u32 = 0;

/// The most slots a function's frame may declare. A program that declares a
/// larger frame is refused at admission, before any of it runs. It is a
/// toolchain limit restated here because the runtime does not carry the
/// compiler IR; a host test checks it equals `vibra_exec::MAX_SLOTS`.
pub const MAX_FRAME_SLOTS: u32 = 65_536;

/// A quota that never runs out.
pub const UNBOUNDED: u64 = u64::MAX;

/// What one step did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Step {
    /// The step reached a checkpoint and the context has a frame to continue.
    Continue,
    /// The entry returned. Its result is [`Context::result`].
    Done,
    /// The step recorded a trap in the context.
    Trap,
    /// The context's memory limit was reached: the host event.
    Exhausted,
}

/// What executes a program, one step at a time.
pub trait Code {
    /// Runs the top frame of `ctx` from its resume point up to and including
    /// its next checkpoint.
    ///
    /// A step keeps nothing outside `ctx` between calls: everything it needs
    /// to continue is in the frame.
    fn step(&self, ctx: &mut Context<'_>) -> Step;
}

/// How a call of [`run`] ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Outcome {
    /// The entry returned.
    Completed,
    /// The quota ran out at a checkpoint. Calling [`run`] again continues.
    Yielded,
    /// A registered trap stopped the program.
    Trapped(Trap),
    /// The host event `@runtime.memory-exhausted`.
    MemoryExhausted,
}

/// The dispatcher: steps `code` until the program ends or `quota` checkpoints
/// have passed.
///
/// A checkpoint costs one unit of quota plus whatever the step charged for
/// proportional work. A yield changes no result, trap, or audit event: the
/// quota is not observable by the program.
pub fn run(ctx: &mut Context<'_>, code: &dyn Code, quota: u64) -> Outcome {
    let mut left = quota;
    loop {
        if left == 0 {
            ctx.note_yield();
            return Outcome::Yielded;
        }
        match code.step(ctx) {
            Step::Continue => {
                let cost = ctx.take_charge().saturating_add(1);
                if quota != UNBOUNDED {
                    left = left.saturating_sub(cost);
                }
            }
            Step::Done => return Outcome::Completed,
            Step::Trap => {
                return Outcome::Trapped(ctx.trap_record().unwrap_or(Trap {
                    code: TrapCode::InvalidCheckedProgram,
                    origin: 0,
                }));
            }
            Step::Exhausted => return Outcome::MemoryExhausted,
        }
    }
}

/// What a program requires of the runtime that executes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Requirements<'a> {
    /// The [`ABI_VERSION`] the program was built for.
    pub abi: u32,
    /// The derived registry minor of "Registry versioning".
    pub registry_minor: u32,
    /// Every host registry symbol the program names.
    pub host_symbols: Vec<&'a str>,
}

/// Why a program may not execute on this runtime.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdmitError {
    /// The program was built for another runtime boundary.
    Abi {
        /// The version this runtime implements.
        runtime: u32,
        /// The version the program requires.
        program: u32,
    },
    /// The program needs a later registry minor than this runtime has.
    RegistryMinor {
        /// The minor this runtime implements.
        runtime: u32,
        /// The minor the program requires.
        program: u32,
    },
    /// The program names a symbol outside the host registry.
    UnknownSymbol(String),
}

/// Checks a program's requirements before any of it executes.
///
/// # Errors
///
/// The first requirement this runtime does not meet.
pub fn admit(requirements: &Requirements<'_>) -> Result<(), AdmitError> {
    if requirements.abi != ABI_VERSION {
        return Err(AdmitError::Abi {
            runtime: ABI_VERSION,
            program: requirements.abi,
        });
    }
    if requirements.registry_minor > HOST_REGISTRY_MINOR {
        return Err(AdmitError::RegistryMinor {
            runtime: HOST_REGISTRY_MINOR,
            program: requirements.registry_minor,
        });
    }
    for symbol in &requirements.host_symbols {
        if host_row(symbol).is_none() {
            return Err(AdmitError::UnknownSymbol((*symbol).to_owned()));
        }
    }
    Ok(())
}
