//! Scaffolding that builds a transition module around a hand-written entry
//! body.
//!
//! The runtime routines of a module (the allocator, the reference counts, the
//! release, the handle table, and the accessors) are the same whether a lowered
//! program or a host test calls them. A test of the memory layer or of the
//! runner builds a state that no checked program reaches, such as a recorded
//! defect trap, by calling the routines from a body it writes itself, in a
//! module that is otherwise exactly an emitted one: [`module_with_entry`] is
//! that module. The body is a plain WebAssembly function with no frame, since
//! it is not a language function. It is a fault-injection hook for host tests,
//! and it is not a way to emit a program.

pub use wasm_encoder::{BlockType, Instruction, MemArg, ValType};

pub use crate::runtime::{Routines, ValueClass};

use crate::encode::{self, Entry, FunctionCode, Parts};

/// An evaluation module whose `vibra_v1_eval` runs the body `body` builds,
/// which leaves a value of `class`, with `locals` declared after no parameter.
/// The body calls the routines it is given, and its `End` is added.
#[must_use]
pub fn module_with_entry(
    class: ValueClass,
    locals: &[ValType],
    body: impl FnOnce(&Routines) -> Vec<Instruction<'static>>,
) -> Vec<u8> {
    let fns = encode::routines_for(1);
    let mut instructions = body(&fns);
    instructions.push(Instruction::End);
    let parts = Parts {
        functions: vec![FunctionCode {
            params: Vec::new(),
            results: class.val_type().into_iter().collect(),
            locals: locals.to_vec(),
            body: instructions,
        }],
        table: Vec::new(),
        entry: Entry::Raw { function: 0, class },
        segments: Vec::new(),
        imports: Vec::new(),
        extra: Vec::new(),
    };
    // The one function is the entry, so the module always encodes.
    encode::module(&parts).unwrap_or_default()
}
