//! The bridge between generated machine code and Nexus: the C ABI, the
//! launcher, and the static archive. **The only crate of the workspace that
//! contains `unsafe`** (decision N10); `crates/vibra-conformance`'s
//! `architecture_boundary` tests enforce that, and that this crate's lint
//! tables are the workspace's with `unsafe_code` allowed and every `unsafe`
//! block required to carry a `// SAFETY:` comment.
//!
//! This is the contract the code generator is written against. It is
//! PROVISIONAL until the native-execution chapter of the runtime specification
//! is approved, and nothing here is advertised by any command.
//!
//! # How a program runs
//!
//! The archive contains `main` (the launcher). `main` reads the data symbol
//! [`PROGRAM_SYMBOL`] (`vibra_program`), a [`VibraProgram`], and
//! [`admit`]s the whole descriptor: nothing of the program runs, and no host
//! operation is performed, unless it is admitted. The dispatcher
//! [`vibra_nexus::run`], the same one that drives the reference stepper, then
//! calls the step function of the function on top of the context's frames:
//!
//! ```c
//! uint32_t step(NexusContext *ctx, uint32_t resume_block);
//! ```
//!
//! A step runs the function from block `resume_block` up to and including its
//! next checkpoint (call, tail call, return, host operation, or the first read
//! of a module value), and returns a status: [`STATUS_CONTINUE`] `0` (the
//! context has a frame to continue), [`STATUS_DONE`] `1` (the entry returned),
//! [`STATUS_TRAP`] `2` (a trap is recorded), [`STATUS_EXHAUSTED`] `3` (the host
//! event), and `4`, [`STATUS_WAIT`], reserved for v0.0.4 and never answered
//! today. A step keeps nothing outside the context. Quota is a runtime control
//! that generated code never sees.
//!
//! A step's body is a switch on `resume_block` over its blocks. Instructions
//! are routine calls; `Jump` is a jump; `Branch` and `Switch` call
//! [`nexus_branch`] and [`nexus_switch`] and jump to the block they answer; a
//! checkpoint calls the transfer routine and returns its status. A routine
//! that answers anything but [`STATUS_CONTINUE`] (and, for
//! [`nexus_global`], [`STATUS_PROCEED`]) ends the step with that status.
//! Slots are named by index; values never leave the context.
//!
//! # Layout of the descriptor
//!
//! Little-endian, 64-bit targets only (every field below is naturally
//! aligned; a pointer is 8 bytes). Strings are UTF-8, length-prefixed by their
//! table, never NUL-terminated. All reserved fields are zero; a nonzero one is
//! refused. `T[len @ ptr]` is a [`VibraTable`]: `ptr` (8 bytes) then `len`
//! (8 bytes, a count of elements); `ptr` may be null only when `len` is zero.
//!
//! `VibraProgram`, 96 bytes, alignment 8, found under the symbol
//! `vibra_program`:
//!
//! | Offset | Size | Field | Meaning |
//! | --- | --- | --- | --- |
//! | 0 | 4 | `abi` | must equal [`ABI_VERSION`]; checked before anything else is read |
//! | 4 | 4 | `registry_minor` | at most the runtime's registry minor |
//! | 8 | 4 | `entry` | function index of the entry; it takes no operand and no environment |
//! | 12 | 4 | `entry_outcome` | `0`: the entry's result is not a `result`, and the launcher never reports `program-failed`; `n > 0`: the entry returns a `result` whose `err` variant index is `n - 1` |
//! | 16 | 16 | `functions` | `VibraFunction[len @ ptr]`, at least one, at most 2^24 |
//! | 32 | 16 | `globals` | `u32[len @ ptr]`: for module value *i*, the index of its initializer function (no operand, no environment) |
//! | 48 | 16 | `constants` | `VibraConstant[len @ ptr]` |
//! | 64 | 16 | `symbols` | `VibraTable<u8>[len @ ptr]`: host registry symbols, each a UTF-8 string, all rows of the runtime's registry; `nexus_host` names one by index |
//! | 80 | 16 | `origins` | `VibraOrigin[len @ ptr]`: ordinal *n* is entry *n - 1*, ordinal 0 is no origin |
//!
//! `VibraFunction`, 24 bytes, alignment 8: offset 0, 8 bytes, `step` (a
//! non-null [`VibraStep`]); 8, 4, `slots` (at most [`MAX_FRAME_SLOTS`], 65 536);
//! 12, 4, `params`; 16, 4, `env` (`0` or `1`; `1` means the function value is
//! passed in the slot after the parameters); 20, 4, `blocks` (the
//! number of resume blocks, at least 1). Required: `params + env <= slots`.
//!
//! `VibraConstant`, 24 bytes, alignment 8: offset 0, 4, `kind`
//! ([`CONSTANT_STR`] 0, [`CONSTANT_BYTES`] 1, [`CONSTANT_ATOM`] 2); 4, 4,
//! `reserved`; 8, 16, `data` (`u8[len @ ptr]`, UTF-8 for `str` and atom).
//!
//! `VibraOrigin`, 24 bytes, alignment 8: offset 0, 16, `source`
//! (`u8[len @ ptr]`, UTF-8); 16, 4, `start`; 20, 4, `end` (`end >= start`).
//!
//! A frame is `[parameters][environment][everything else]`; slot indices are
//! within `slots`. Operand lists cross as a pointer to `u32` slot indices and a
//! count that may not exceed the running function's `slots`.
//!
//! # Admission
//!
//! [`admit`] refuses, with `@runtime.incompatible-artifact` and before any code
//! runs, a descriptor whose `abi` is not [`ABI_VERSION`], whose
//! `registry_minor` exceeds the runtime's, or that names a symbol that is not a
//! registry row, and also one that is malformed ([`Malformed`]): a nonzero
//! reserved field of a constant, a function with no blocks, a null or empty function table, a table that is null,
//! misaligned, or longer than [`MAX_TABLE_LEN`], a function without a step, an
//! `env` other than `0` or `1`, a frame larger than [`MAX_FRAME_SLOTS`] or too
//! small for its operands, an entry or initializer that is out of range or
//! takes operands, a constant of an undefined kind or that is not UTF-8, and
//! an origin that ends before it starts.
//!
//! A block named to a routine (`resume`, a branch or switch target) must be
//! below the `blocks` of the running function, and the dispatcher checks the
//! `resume` it reads from the context the same way; otherwise the toolchain
//! defect is recorded.
//!
//! # Open for the specification
//!
//! Provisional choices pending the maintainer's rulings:
//!
//! - the exit code of an admission refusal ([EXIT_ADMISSION_REFUSED], the
//!   operational-failure code `3`; the draft names only the diagnostic
//!   `@runtime.incompatible-artifact`);
//! - the memory limit of the `system` profile ([SYSTEM_MEMORY_LIMIT],
//!   256 MiB, the budget `vibra run` gives the interpreter);
//! - the text written to standard error for a trap (`<atom> at
//!   <source>:<start>..<end>`, or the bare atom) and for a refusal
//!   (`@runtime.incompatible-artifact: <reason>`); and
//! - `program-failed` writes only the code line `@command.program-failed`,
//!   because the canonical encoding of the `err` value needs the observation
//!   of package X3-18, which does not exist yet.
//!
//! # The harness profile
//!
//! With the cargo feature `harness` the launcher takes a host world and a quota
//! and writes one observation document instead of using the process: see the
//! documentation of the `harness` module for the document and the grammar of the
//! dump of the entry's result. The document carries the outcome, the audit
//! trace, the context's counters (yields, peak frames, tail transfers, peak and
//! final live size), and the result as a structural dump that names no type: the
//! bridge reads the value store through the `vibra_nexus` read API and never
//! learns the type system, and the conformance runner applies the entry's
//! result type. The shipped `system` launcher does not contain any of it, and
//! the context does not tell what its module values keep, so that counter is
//! not reported.
//!
//! # Safety argument
//!
//! `unsafe` is used for four things only, each a single operation with a
//! `// SAFETY:` comment: (a) turning the handle generated code passes back
//! into the session, (b) reading the descriptor's tables and an operand list
//! with explicit lengths, (c) calling a step function through its pointer, and
//! (d) declaring `#[unsafe(no_mangle)] extern "C"` items and the one extern
//! static. No pointer into the value store or the cell stack crosses the
//! boundary, and no routine panics: a routine that is misused records the
//! toolchain-defect trap and answers a status. The archive is built with the
//! workspace profile, in which a panic would unwind; Cargo cannot set
//! `panic = "abort"` for one package, but a panic in an `extern "C"` function
//! aborts the process rather than unwinding into generated code, and the
//! workspace lints deny the constructs that panic.

mod abi;
mod admission;
// The crate's own unit-test harness would define a second `main` beside the
// launcher's, so the entry point is not part of a `cfg(test)` build. The
// tests are integration tests, which link the library.
#[cfg(not(test))]
pub mod entry;
mod generated;
mod launcher;
mod routines;

#[cfg(feature = "harness")]
pub mod harness;

pub use abi::{
    ABI_VERSION, CLASS_F32, CLASS_F64, CLASS_I32, CLASS_I64, CONSTANT_ATOM,
    CONSTANT_BYTES, CONSTANT_STR, KIND_ARRAY, KIND_ENUM, KIND_RECORD, KIND_TUPLE,
    KIND_UNION, KIND_WRAPPER, NexusContext, OP_I64_ADD, OP_I64_EQ, OP_I64_LT_S,
    OP_I64_SUB, PROGRAM_SYMBOL, STATUS_CONTINUE, STATUS_DONE, STATUS_EXHAUSTED,
    STATUS_PROCEED, STATUS_TRAP, STATUS_WAIT, VibraConstant, VibraFunction,
    VibraOrigin, VibraProgram, VibraStep, VibraTable,
};
pub use admission::{
    Admitted, MAX_DATA_LEN, MAX_TABLE_LEN, Malformed, Origin, Refusal, admit,
};
pub use generated::Generated;
pub use launcher::{
    EXIT_ADMISSION_REFUSED, EXIT_COMPLETED, EXIT_HOST_EVENT, EXIT_PROGRAM_FAILED,
    EXIT_TRAP, Execution, Finished, SYSTEM_MEMORY_LIMIT, execute, launch,
    launch_system, report, trap_atom,
};
pub use routines::{
    MAX_SWITCH_TARGETS, nexus_branch, nexus_build, nexus_call, nexus_call_value,
    nexus_cell, nexus_constant, nexus_copy, nexus_drop, nexus_function, nexus_global,
    nexus_host, nexus_move, nexus_op, nexus_ret, nexus_scalar, nexus_switch,
    nexus_tail_call, nexus_tail_call_value, nexus_trap, nexus_variant,
};
pub use vibra_nexus::MAX_FRAME_SLOTS;

/// An exported routine and its C signature: the code generator's import list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Routine {
    /// The symbol.
    pub name: &'static str,
    /// The C prototype, with `NexusContext *` the handle.
    pub signature: &'static str,
}

/// Every routine this crate exports, with its C signature. The single source
/// of truth for the code generator; a test compares it with the
/// `#[unsafe(no_mangle)]` items of the sources. The launcher's `main` is not a
/// routine.
pub const ROUTINES: &[Routine] = &[
    Routine {
        name: "nexus_call",
        signature: "uint32_t nexus_call(NexusContext *, uint32_t function, const uint32_t *args, uint32_t argc, uint32_t dst, uint32_t resume)",
    },
    Routine {
        name: "nexus_call_value",
        signature: "uint32_t nexus_call_value(NexusContext *, uint32_t callee, const uint32_t *args, uint32_t argc, uint32_t dst, uint32_t resume)",
    },
    Routine {
        name: "nexus_tail_call",
        signature: "uint32_t nexus_tail_call(NexusContext *, uint32_t function, const uint32_t *args, uint32_t argc)",
    },
    Routine {
        name: "nexus_tail_call_value",
        signature: "uint32_t nexus_tail_call_value(NexusContext *, uint32_t callee, const uint32_t *args, uint32_t argc)",
    },
    Routine {
        name: "nexus_ret",
        signature: "uint32_t nexus_ret(NexusContext *, uint32_t slot)",
    },
    Routine {
        name: "nexus_host",
        signature: "uint32_t nexus_host(NexusContext *, uint32_t symbol, const uint32_t *args, uint32_t argc, uint32_t dst, uint32_t resume, uint32_t origin)",
    },
    Routine {
        name: "nexus_global",
        signature: "uint32_t nexus_global(NexusContext *, uint32_t global, uint32_t dst, uint32_t resume)",
    },
    Routine {
        name: "nexus_trap",
        signature: "uint32_t nexus_trap(NexusContext *, uint32_t code, uint32_t origin)",
    },
    Routine {
        name: "nexus_scalar",
        signature: "uint32_t nexus_scalar(NexusContext *, uint32_t dst, uint32_t class, uint64_t bits)",
    },
    Routine {
        name: "nexus_constant",
        signature: "uint32_t nexus_constant(NexusContext *, uint32_t dst, uint32_t constant)",
    },
    Routine {
        name: "nexus_copy",
        signature: "uint32_t nexus_copy(NexusContext *, uint32_t dst, uint32_t src)",
    },
    Routine {
        name: "nexus_move",
        signature: "uint32_t nexus_move(NexusContext *, uint32_t dst, uint32_t src)",
    },
    Routine {
        name: "nexus_drop",
        signature: "uint32_t nexus_drop(NexusContext *, uint32_t slot)",
    },
    Routine {
        name: "nexus_op",
        signature: "uint32_t nexus_op(NexusContext *, uint32_t dst, uint32_t op, uint32_t left, uint32_t right)",
    },
    Routine {
        name: "nexus_build",
        signature: "uint32_t nexus_build(NexusContext *, uint32_t dst, uint32_t kind, uint32_t variant, const uint32_t *cells, uint32_t count)",
    },
    Routine {
        name: "nexus_cell",
        signature: "uint32_t nexus_cell(NexusContext *, uint32_t dst, uint32_t object, uint32_t index)",
    },
    Routine {
        name: "nexus_variant",
        signature: "uint32_t nexus_variant(NexusContext *, uint32_t dst, uint32_t object)",
    },
    Routine {
        name: "nexus_function",
        signature: "uint32_t nexus_function(NexusContext *, uint32_t dst, uint32_t function, const uint32_t *captures, uint32_t count)",
    },
    Routine {
        name: "nexus_branch",
        signature: "uint64_t nexus_branch(NexusContext *, uint32_t cond, uint32_t then_block, uint32_t else_block)",
    },
    Routine {
        name: "nexus_switch",
        signature: "uint64_t nexus_switch(NexusContext *, uint32_t value, const uint32_t *targets, uint32_t count, uint32_t default_block)",
    },
];
