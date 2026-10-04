//! The module an emitted literal program is (milestone 4 Step 5a): validated
//! under the baseline, deterministic, balanced, and observed through the
//! accessors a host has.
//!
//! The programs are built from IR constructors, because no checked source
//! program lowers before the prelude's module values do (Step 5b).

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use vibra_diagnostics::ByteSpan;
use vibra_ir::boundary::MEMORY_EXPORT;
use vibra_ir::{
    CheckedFunction, CheckedProgram, Expr, FunctionSignature, SourceOrigin, Type, Value,
};
use vibra_wasm_run::{
    Instance, MemoryLimit, Observed, Outcome, ResultSlot, Runner, Started, validate,
};

fn origin() -> SourceOrigin {
    SourceOrigin::new("input.vib", ByteSpan::new(0, 1))
}

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

fn literal(value: Value) -> CheckedProgram {
    let ty = value.ty();
    returning(ty, Expr::literal(value, origin()))
}

fn literals() -> Vec<Value> {
    vec![
        Value::Void,
        Value::Char('x'),
        Value::I8(-128),
        Value::I16(-300),
        Value::I32(i32::MIN),
        Value::I64(i64::MIN),
        Value::U8(255),
        Value::U16(65_535),
        Value::U32(u32::MAX),
        Value::U64(u64::MAX),
        Value::F32(1.5_f32.to_bits()),
        Value::F64(2.25_f64.to_bits()),
        Value::Bool(true),
        Value::Bool(false),
        Value::Str("hello".to_owned()),
        Value::Str(String::new()),
        Value::Atom("ok".to_owned()),
        Value::Bytes(vec![0, 255]),
        Value::Bytes(Vec::new()),
    ]
}

fn emitted(program: &CheckedProgram) -> Vec<u8> {
    vibra_wasm::emit(program)
        .expect("the program lowers")
        .into_bytes()
}

fn runner() -> Runner {
    Runner::new(MemoryLimit::new(64 * 1024 * 1024)).expect("the engine configures")
}

fn interpreter_result(program: &CheckedProgram) -> String {
    vibra_interp::run(program)
        .expect("the interpreter runs the program")
        .canonical_result()
}

fn observed_result(program: &CheckedProgram, ty: &Type) -> String {
    match runner()
        .run_observed(&emitted(program), ty)
        .expect("the module runs")
    {
        Observed::Completed { value, .. } => value.canonical_observation(ty),
        Observed::Stopped(outcome) => panic!("the entry did not complete: {outcome:?}"),
    }
}

fn instance_of(program: &CheckedProgram) -> (Instance, ResultSlot) {
    let bytes = emitted(program);
    let Started::Ready(mut instance) = runner().start(&bytes).expect("a v1 module")
    else {
        panic!("the module's own memory exceeds the limit");
    };
    let Outcome::Completed { result, .. } = instance.call_entry().expect("runs") else {
        panic!("the entry did not complete");
    };
    (instance, result)
}

// -- the module -------------------------------------------------------------

#[test]
fn every_literal_module_validates_under_the_baseline_and_exports_the_full_table() {
    for value in literals() {
        let summary = validate(&emitted(&literal(value.clone()))).expect("a v1 module");
        assert!(summary.imports.is_empty(), "{value:?}");
        assert_eq!(
            summary.exports.len(),
            15,
            "the memory and fourteen functions"
        );
        assert_eq!(summary.exports[0], MEMORY_EXPORT);
        assert!(summary.exports.contains(&"vibra_v1_release".to_owned()));
        assert!(summary.exports.contains(&"vibra_v1_read_id".to_owned()));
        assert_eq!(summary.initial_pages, 1);
    }
}

#[test]
fn a_module_includes_the_constructor_only_when_the_program_builds_an_object() {
    let scalar = validate(&emitted(&literal(Value::I64(9)))).expect("valid");
    let object =
        validate(&emitted(&literal(Value::Str("x".to_owned())))).expect("valid");
    assert_eq!(scalar.functions + 1, object.functions);
    assert_eq!(scalar.data_segments, 0);
    assert_eq!(object.data_segments, 1);
}

#[test]
fn equal_literal_contents_share_one_data_segment() {
    let body = Expr::Sequence {
        expressions: vec![
            Expr::literal(Value::Str("ab".to_owned()), origin()),
            Expr::literal(Value::Str("ab".to_owned()), origin()),
            Expr::literal(Value::Str("ab ".to_owned()), origin()),
        ],
        origin: origin(),
    };
    let summary = validate(&emitted(&returning(Type::Str, body))).expect("valid");
    assert_eq!(summary.data_segments, 2);
}

#[test]
fn emission_of_a_literal_program_is_byte_identical() {
    for value in literals() {
        let a = emitted(&literal(value.clone()));
        let b = emitted(&literal(value));
        assert_eq!(a, b);
    }
}

// -- what the host observes -------------------------------------------------

#[test]
fn bool_is_an_enum_read_through_variant_in_declaration_order() {
    for (value, variant) in [(false, 0), (true, 1)] {
        let (mut instance, slot) = instance_of(&literal(Value::Bool(value)));
        let id = instance.result_id(slot);
        assert_eq!(instance.variant(&id), Ok(variant), "{value}");
        // An enum has a variant and a payload, and no length.
        assert!(instance.length(&id).is_err());
        assert!(
            instance.read_i32(&id, 0).is_err(),
            "`void` payloads have no cell"
        );
        instance.release(id).expect("released");
        assert_eq!(instance.live_size(), Ok(0));
    }
}

#[test]
fn str_atom_and_bytes_read_through_length_and_their_components() {
    let (mut instance, slot) =
        instance_of(&literal(Value::Str("a\u{1F600}b".to_owned())));
    let text = instance.result_id(slot);
    assert_eq!(
        instance.length(&text),
        Ok(3),
        "a scalar count, not a byte count"
    );
    assert_eq!(instance.read_i32(&text, 1), Ok(0x1F600));
    assert!(instance.variant(&text).is_err());

    let (mut instance, slot) = instance_of(&literal(Value::Atom("ok".to_owned())));
    let atom = instance.result_id(slot);
    assert_eq!(instance.length(&atom), Ok(2));
    assert_eq!(instance.read_i32(&atom, 0), Ok(i32::from(b'o')));

    let (mut instance, slot) = instance_of(&literal(Value::Bytes(vec![7, 200])));
    let bytes = instance.result_id(slot);
    assert_eq!(instance.length(&bytes), Ok(2), "a byte count");
    assert_eq!(instance.read_i32(&bytes, 1), Ok(200));
    assert!(instance.read_i32(&bytes, 2).is_err(), "past the end");
}

#[test]
fn a_literal_program_is_balanced_and_only_its_arena_result_is_held() {
    let runner = runner();
    for value in literals() {
        let ty = value.ty();
        let Observed::Completed { live, .. } = runner
            .run_observed(&emitted(&literal(value.clone())), &ty)
            .expect("runs")
        else {
            panic!("{value:?} did not complete");
        };
        assert_eq!(live.start, 0, "{value:?}");
        assert_eq!(live.end, live.start, "{value:?}: dup and drop balance");
        let is_arena = matches!(
            value,
            Value::Bool(_) | Value::Str(_) | Value::Bytes(_) | Value::Atom(_)
        );
        assert_eq!(
            live.with_result > live.start,
            is_arena,
            "{value:?}: only an arena result is held, and with the handle table"
        );
    }
}

#[test]
fn a_sequence_drops_every_value_it_does_not_return() {
    // `(do "a" 1u8 @b)` as a `void` body: three values are discarded, two of
    // them arena values.
    let body = Expr::Sequence {
        expressions: vec![
            Expr::literal(Value::Str("a".to_owned()), origin()),
            Expr::literal(Value::U8(1), origin()),
            Expr::literal(Value::Atom("b".to_owned()), origin()),
            Expr::literal(Value::Void, origin()),
        ],
        origin: origin(),
    };
    let (mut instance, _) = instance_of(&returning(Type::Void, body));
    assert_eq!(
        instance.live_size(),
        Ok(0),
        "both arena values were dropped"
    );

    // A sequence has the value of its last expression.
    let body = Expr::Sequence {
        expressions: vec![
            Expr::literal(Value::Str("dropped".to_owned()), origin()),
            Expr::literal(Value::Bytes(vec![1, 2, 3]), origin()),
        ],
        origin: origin(),
    };
    let program = returning(Type::Bytes, body);
    assert_eq!(
        observed_result(&program, &Type::Bytes),
        interpreter_result(&program)
    );
}

#[test]
fn the_entry_need_not_be_the_first_function_and_other_functions_lower_too() {
    let helper = CheckedFunction::new(
        "helper",
        FunctionSignature::new(Vec::new(), Type::Str),
        Expr::literal(Value::Str("never called".to_owned()), origin()),
        origin(),
    )
    .expect("a function");
    let main = CheckedFunction::new(
        "main",
        FunctionSignature::new(Vec::new(), Type::U16),
        Expr::literal(Value::U16(513), origin()),
        origin(),
    )
    .expect("a function");
    let program = CheckedProgram::try_new(vec![helper, main], 1).expect("a program");
    assert_eq!(
        observed_result(&program, &Type::U16),
        interpreter_result(&program)
    );
    assert_eq!(
        observed_result(&program, &Type::U16),
        "(record type: @u16 value: 513u16)\n"
    );
}

// -- exhaustion through an emitted program ----------------------------------

#[test]
fn a_literal_larger_than_the_limit_is_the_host_event_with_no_partial_result() {
    // 600 KiB of bytes is a 1 MiB block.
    let bytes = emitted(&literal(Value::Bytes(vec![0xAB; 600 * 1024])));
    let small = Runner::new(MemoryLimit::new(512 * 1024)).expect("engine");
    assert_eq!(
        small.run_observed(&bytes, &Type::Bytes).expect("runs"),
        Observed::Stopped(Outcome::MemoryExhausted)
    );
    // The limit the harness applies runs it.
    assert!(matches!(
        runner().run_observed(&bytes, &Type::Bytes).expect("runs"),
        Observed::Completed { .. }
    ));
}

#[test]
fn the_limit_at_exactly_the_programs_need_runs_and_one_page_under_does_not() {
    let bytes = emitted(&literal(Value::Bytes(vec![1; 100 * 1024])));
    let runs = |pages: usize| {
        Runner::new(MemoryLimit::new(pages * 65_536))
            .expect("engine")
            .run_observed(&bytes, &Type::Bytes)
            .expect("runs")
    };
    let need = (1..=16)
        .find(|pages| matches!(runs(*pages), Observed::Completed { .. }))
        .expect("some limit admits the program");
    assert_eq!(runs(need - 1), Observed::Stopped(Outcome::MemoryExhausted));
    assert!(matches!(runs(need), Observed::Completed { .. }));
    println!("a 100 KiB bytes literal needs {need} pages of 64 KiB");
}
