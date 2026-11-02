//! A native that refuses its operands (milestone 4 Step 10).
//!
//! `docs/spec/06-runtime.md`, "Native implementations": a native import reads the
//! instance's memory through the toolchain's runtime only. A module the toolchain
//! emitted never gives a native a bad operand, so a native that meets one means
//! the toolchain is at fault. The call stops with no recorded status, and the
//! runner reports the defect `@runtime.invalid-checked-program` with no origin,
//! never a program result, never a trap with an origin, and never a read outside
//! the arena.
//!
//! The modules here are forged: each calls the real import, by its registry name
//! and signature, with an operand a checked program cannot produce.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

mod common;

use common::{Spec, module};
use vibra_ir::arena::{header, kind};
use vibra_ir::boundary::TrapCode;
use vibra_wasm_run::{MemoryLimit, Outcome, ResultSlot, Runner};
use wasm_encoder::{Instruction, MemArg, ValType};

const PAGE: i32 = 65_536;

fn runner() -> Runner {
    Runner::new(MemoryLimit::new(64 * 1024 * 1024)).expect("the engine configures")
}

fn store8(at: i32, value: i32) -> Vec<Instruction<'static>> {
    vec![
        Instruction::I32Const(at),
        Instruction::I32Const(value),
        Instruction::I32Store8(MemArg {
            offset: 0,
            align: 0,
            memory_index: 0,
        }),
    ]
}

fn store32(at: i32, value: i32) -> Vec<Instruction<'static>> {
    vec![
        Instruction::I32Const(at),
        Instruction::I32Const(value),
        Instruction::I32Store(MemArg {
            offset: 0,
            align: 2,
            memory_index: 0,
        }),
    ]
}

/// The header of an object at `at`: its kind, stride, block size, and length.
fn object(
    at: i32,
    kind: u8,
    stride: u8,
    block: i32,
    len: i32,
) -> Vec<Instruction<'static>> {
    let place = |field: u32| at + i32::try_from(field).expect("a small offset");
    [
        store32(place(header::BLOCK_SIZE), block),
        store8(place(header::KIND), i32::from(kind)),
        store8(place(header::STRIDE), i32::from(stride)),
        store32(place(header::LEN), len),
    ]
    .concat()
}

/// A module whose entry builds `setup` and calls the native `name` with
/// `operands`, then drops its answer.
fn forged(
    name: &'static str,
    params: &[ValType],
    setup: Vec<Instruction<'static>>,
    operands: &[i32],
) -> Vec<u8> {
    let mut entry = setup;
    entry.extend(
        operands
            .iter()
            .map(|operand| Instruction::I32Const(*operand)),
    );
    entry.push(Instruction::Call(0));
    entry.push(Instruction::Drop);
    module(&Spec {
        entry,
        typed: vec![(name, params.to_vec(), ValType::I64)],
        ..Spec::default()
    })
}

fn length_of(setup: Vec<Instruction<'static>>, operand: i32) -> Outcome {
    runner()
        .run_entry(&forged("text.length", &[ValType::I32], setup, &[operand]))
        .expect("a runnable module")
}

fn defect(outcome: &Outcome, fragment: &str) {
    let Outcome::Defect { cause } = outcome else {
        panic!("a native fault must end as a defect, got {outcome:?}");
    };
    assert!(cause.contains("native `"), "{cause}");
    assert!(
        cause.contains(fragment),
        "{cause} does not mention {fragment}"
    );
    // The defect is reported as the registered trap, with no origin.
    assert_eq!(Outcome::defect_trap(), TrapCode::InvalidCheckedProgram);
    assert_eq!(
        Outcome::defect_trap().diagnostic_code().as_atom(),
        "@runtime.invalid-checked-program"
    );
}

#[test]
fn a_well_formed_operand_completes() {
    let str_object = object(1024, kind::STR, 4, 64, 0);
    assert_eq!(
        length_of(str_object, 1024),
        Outcome::Completed {
            result: ResultSlot::from_bits(0),
            live_size: 0
        }
    );
}

#[test]
fn an_offset_outside_the_memory_is_a_defect() {
    defect(&length_of(Vec::new(), 0x7fff_fff0), "outside");
    defect(&length_of(Vec::new(), -1), "outside");
    // A header that starts inside the memory and ends past it.
    defect(&length_of(Vec::new(), PAGE - 8), "outside");
}

#[test]
fn an_object_of_another_kind_is_a_defect() {
    // `bytes` where a `str` is taken.
    defect(
        &length_of(object(1024, kind::BYTES, 1, 64, 0), 1024),
        "kind",
    );
    // A tuple where a `str` is taken.
    defect(
        &length_of(object(1024, kind::TUPLE, 8, 64, 0), 1024),
        "kind",
    );
}

#[test]
fn a_wrong_stride_is_a_defect() {
    for stride in [0, 1, 2, 8, 255] {
        defect(
            &length_of(object(1024, kind::STR, stride, 64, 0), 1024),
            "kind",
        );
    }
}

#[test]
fn a_length_that_reaches_past_its_block_or_the_memory_is_a_defect() {
    // The payload of 1000 scalars does not fit a 64-byte block.
    defect(
        &length_of(object(1024, kind::STR, 4, 64, 1000), 1024),
        "outside",
    );
    // A block that claims room the memory does not have.
    defect(
        &length_of(object(PAGE - 64, kind::STR, 4, 1 << 20, 1000), PAGE - 64),
        "outside",
    );
    // A length whose payload size overflows 32 bits.
    defect(
        &length_of(object(1024, kind::STR, 4, i32::MAX, -1), 1024),
        "outside",
    );
}

#[test]
fn a_destination_that_is_not_sized_to_the_result_is_a_defect() {
    // `text.concat` of two empty strings builds a string of no scalar, and the
    // destination the module allocated holds five.
    let mut setup = object(1024, kind::STR, 4, 64, 0);
    setup.extend(object(2048, kind::STR, 4, 64, 0));
    setup.extend(object(3072, kind::STR, 4, 64, 5));
    let bytes = forged(
        "text.concat",
        &[ValType::I32, ValType::I32, ValType::I32],
        setup,
        &[1024, 2048, 3072],
    );
    let outcome = runner().run_entry(&bytes).expect("a runnable module");
    defect(&outcome, "destination");
    // The same call, with a destination of the right size, completes.
    let mut setup = object(1024, kind::STR, 4, 64, 0);
    setup.extend(object(2048, kind::STR, 4, 64, 0));
    setup.extend(object(3072, kind::STR, 4, 64, 0));
    let bytes = forged(
        "text.concat",
        &[ValType::I32, ValType::I32, ValType::I32],
        setup,
        &[1024, 2048, 3072],
    );
    assert!(matches!(
        runner().run_entry(&bytes).expect("a runnable module"),
        Outcome::Completed { .. }
    ));
}

#[test]
fn a_destination_outside_the_memory_is_a_defect() {
    let mut setup = object(1024, kind::STR, 4, 64, 0);
    setup.extend(object(2048, kind::STR, 4, 64, 0));
    let bytes = forged(
        "text.concat",
        &[ValType::I32, ValType::I32, ValType::I32],
        setup,
        &[1024, 2048, 0x7fff_fff0],
    );
    defect(
        &runner().run_entry(&bytes).expect("a runnable module"),
        "outside",
    );
}

#[test]
fn a_cell_of_the_wrong_class_or_a_row_given_a_wrong_object_is_a_defect() {
    // `array.concat` on a `str` and on `bytes`.
    let mut setup = object(1024, kind::STR, 4, 64, 0);
    setup.extend(object(2048, kind::BYTES, 1, 64, 0));
    let bytes = forged(
        "array.concat",
        &[ValType::I32, ValType::I32, ValType::I32],
        setup,
        &[1024, 2048, 0],
    );
    defect(
        &runner().run_entry(&bytes).expect("a runnable module"),
        "kind",
    );
    // A `dict` is read as cells, and a `str` is not.
    let setup = object(1024, kind::STR, 4, 64, 0);
    let bytes = forged(
        "dict.entries",
        &[ValType::I32, ValType::I32],
        setup,
        &[1024, 0],
    );
    defect(
        &runner().run_entry(&bytes).expect("a runnable module"),
        "kind",
    );
}
