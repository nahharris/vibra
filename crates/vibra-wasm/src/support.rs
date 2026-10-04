//! Scaffolding that builds a v1 module around a hand-written entry body.
//!
//! The runtime routines of a module (the allocator, the reference counts, the
//! release, the handle table, and the accessors) are the same whether a lowered
//! program or a host test calls them. A test of the memory layer builds a value
//! that no source program builds, such as one nested a hundred thousand deep, by
//! calling the routines from a body it writes itself, in a module that is
//! otherwise exactly an emitted one: [`module_with_entry`] is that module. The
//! body is a plain WebAssembly function with no frame, since it is not a
//! language function. It is for host tests of the memory layer and of the
//! host's reader, and it is not a way to emit a program.

pub use wasm_encoder::{BlockType, Instruction, MemArg, ValType};

pub use crate::runtime::{Routines, ValueClass};

use crate::encode::{self, Entry, FunctionCode, Parts};

/// A v1 module whose entry function leaves a value of `class` and runs the
/// body `body` builds, with `locals` declared after no parameter. The body
/// calls the routines it is given, and its `End` is added.
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
        include_new: true,
    };
    // The one function is the entry, so the module always encodes.
    encode::module(&parts).unwrap_or_default()
}
