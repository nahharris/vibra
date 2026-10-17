//! The value arena and its runtime (milestone 4 Step 5a): literals lowered
//! through the memory layer and observed through the accessors, held to the
//! reference interpreter byte for byte.
//!
//! No checked source program lowers yet, because every one carries the prelude's
//! module values, so the programs here are built from IR constructors, as Step
//! 4's are. The memory layer's own tests, which build values no lowered form
//! builds yet, are in `arena_runtime_m4_step5a.rs`.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use vibra_conformance::{ExecutionObservation, WasmObservation, observe_wasm};
use vibra_diagnostics::ByteSpan;
use vibra_ir::{
    CheckedFunction, CheckedProgram, Expr, FunctionSignature, SourceOrigin, Type, Value,
};

fn origin() -> SourceOrigin {
    SourceOrigin::new("input.vib", ByteSpan::new(0, 1))
}

/// A program whose entry returns `body`, of type `result`.
fn returning(result: Type, body: Expr) -> CheckedProgram {
    let main = CheckedFunction::new(
        "main",
        FunctionSignature::new(Vec::new(), result),
        body,
        origin(),
    )
    .expect("a checked function");
    CheckedProgram::try_new(vec![main], 0).expect("a checked program")
}

/// A program whose entry is the literal `value`.
fn literal(value: Value) -> CheckedProgram {
    let ty = value.ty();
    returning(ty, Expr::literal(value, origin()))
}

/// The canonical result the reference interpreter reports.
fn interpreter_result(program: &CheckedProgram) -> String {
    vibra_interp::run(program)
        .expect("the interpreter runs the program")
        .canonical_result()
}

/// The canonical result the Wasm backend reports through its accessors.
fn wasm_result(program: &CheckedProgram) -> String {
    match observe_wasm(program) {
        WasmObservation::Completed(ExecutionObservation {
            result: Some(result),
            ..
        }) => result,
        other => panic!("the Wasm backend did not complete: {other:?}"),
    }
}

fn literals() -> Vec<Value> {
    vec![
        Value::Void,
        Value::Char('x'),
        Value::Char('\u{0}'),
        Value::Char('\u{10FFFF}'),
        Value::I8(i8::MIN),
        Value::I8(-1),
        Value::I8(i8::MAX),
        Value::I16(i16::MIN),
        Value::I16(-300),
        Value::I32(i32::MIN),
        Value::I32(7),
        Value::I64(i64::MIN),
        Value::I64(i64::MAX),
        Value::U8(255),
        Value::U16(65_535),
        Value::U32(u32::MAX),
        Value::U64(u64::MAX),
        Value::U64(0),
        Value::F32(1.5_f32.to_bits()),
        Value::F32((-0.0_f32).to_bits()),
        Value::F32(f32::INFINITY.to_bits()),
        Value::F32(f32::NAN.to_bits()),
        Value::F64(2.25_f64.to_bits()),
        Value::F64((-0.0_f64).to_bits()),
        Value::F64(f64::NEG_INFINITY.to_bits()),
        Value::F64(f64::NAN.to_bits()),
        Value::Bool(true),
        Value::Bool(false),
        Value::Str("hello".to_owned()),
        Value::Str(String::new()),
        Value::Str("h\u{e9}llo \u{2713} \u{1F600}".to_owned()),
        Value::Atom("ok".to_owned()),
        Value::Bytes(vec![0, 1, 254, 255]),
        Value::Bytes(Vec::new()),
    ]
}

#[test]
fn every_literal_type_round_trips_through_the_accessors_and_matches_the_interpreter() {
    for value in literals() {
        let program = literal(value.clone());
        assert_eq!(
            wasm_result(&program),
            interpreter_result(&program),
            "{value:?}"
        );
    }
}
