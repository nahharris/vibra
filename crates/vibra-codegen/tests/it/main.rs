//! Tests of the code generator: determinism, the symbols and relocations of
//! the object, the descriptor decoded back to the program, the shape of the
//! Cranelift IR, and totality over the instruction and terminator enums.
//!
//! No generated code is executed here: running it needs a linker and is the
//! work of the package after this one. Run these tests with
//! `cargo test --locked --offline -p vibra-codegen --test it`.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    missing_docs,
    unreachable_pub
)]

use vibra_codegen::{Inputs, Object, Target, emit};
use vibra_exec::{Program, verified};

mod clif;
mod determinism;
mod image;
mod object_shape;
mod programs;
mod totality;

/// Both targets the generator emits for.
pub const TARGETS: [Target; 2] = [Target::WINDOWS_X64, Target::LINUX_X64];

/// The inputs a caller derives from a checked program; the tests state them.
pub fn inputs(program: &Program) -> Inputs {
    Inputs {
        entry_outcome: 2,
        registry_minor: program.registry_minor(),
    }
}

/// Emits `program` for `target`, after verifying it.
pub fn emitted(program: &Program, target: &Target) -> Object {
    let proof = verified(program).expect("a verified program");
    emit(proof, target, &inputs(program)).expect("an object")
}
