//! The code generator: verified execution IR in, a native object file out,
//! with Cranelift (decisions N2, N11, N12).
//!
//! `emit(program, target, inputs)` writes one object. The program must have
//! passed [`vibra_exec::verify`]: the parameter is a
//! [`Verified`](vibra_exec::Verified), which only a successful verification
//! can build. The object follows the contract of `vibra-nexus-bridge`, the
//! only description of the boundary (decision N32):
//!
//! - one native function per execution-IR function, `vibra_fn_<n>`, local to
//!   the object, with the step signature `(ctx, resume) -> status`;
//! - a step is a switch on `resume` over the function's blocks. An instruction
//!   is one call of a `nexus_*` routine, the status is checked after every
//!   call, and a status other than `STATUS_CONTINUE` ends the step with it. A
//!   `Jump` is a jump; `Branch` and `Switch` call `nexus_branch` and
//!   `nexus_switch` and continue at the block they answer; `Call`,
//!   `TailCall`, `Host`, `Return`, and `Trap` call their routine and return its
//!   status; `Global` carries on at its resume block when the routine answers
//!   `STATUS_PROCEED` and returns the status otherwise;
//! - no generated function calls another one, and none touches memory through
//!   the context pointer: the only calls are to the routines, and operand
//!   lists are `u32` slot indices in read-only data;
//! - one exported data symbol, `vibra_program`, a `VibraProgram` with a
//!   relocation for every pointer, and the constant, symbol, and origin data it
//!   points to.
//!
//! Emission is deterministic: the bytes depend on the program, the [`Target`],
//! and the [`Inputs`], and on nothing else. It is all or nothing: a program
//! with something the generator does not implement is an [`EmitError`], and no
//! part of it is emitted. It implements every instruction and terminator the
//! reference stepper executes, and refuses `Inst::Intrinsic` as the stepper
//! does.
//!
//! This crate links nothing and runs nothing.

mod descriptor;
mod emit;
mod error;
mod function;
mod services;
mod target;

pub use descriptor::Inputs;
pub use emit::{Listing, Object, emit, emit_with_listing};
pub use error::EmitError;
pub use target::{SUPPORTED_TRIPLES, Target};
