//! Number text, floats, and NaN (milestone 4 Step 8c), in WebAssembly.
//!
//! The integer and float `to-str` and `parse` rows are toolchain Rust that the
//! reference interpreter calls directly and a module imports from
//! `vibra_native_v1`; the float arithmetic, equality, and order rows are
//! instructions the emitter writes inline. One table of sample vectors
//! (`CompilerIntrinsic::vectors`, in `vibra-ir`) holds the operands and the
//! specified outcome of every sample of all of them, and each outcome comes from
//! Rust's own `f32`, `f64`, formatting, and integer parsing, not from either
//! backend. Each row is run as programs, tuples of its samples, by the reference
//! interpreter and by the WebAssembly module, and both are held to the table.
//!
//! A float with no source spelling (an infinity, a NaN of any sign or payload)
//! cannot be a literal in source, so the program of a row is written with
//! placeholder literals and the checked program's literals are then replaced by
//! the exact operands of the vectors. The interpreter and the module run the same
//! rewritten program.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    missing_docs
)]

use std::collections::BTreeSet;
use std::time::Instant;

use vibra_ir::external::{CompilerIntrinsic, FloatOp, IntegerOp, NumericType, Vector};
use vibra_ir::{CheckedFunction, CheckedProgram, Expr, ObservedValue, Type, Value};
use vibra_types::check_source;
use vibra_wasm::layout::{self, header, state};
use vibra_wasm_run::{Instance, MemoryLimit, Observed, Outcome, Runner, Started};

const MIB: usize = 1024 * 1024;

/// The vectors a program holds, so a program stays small enough to check and
/// emit quickly.
const CHUNK: usize = 300;

// -- the harness --------------------------------------------------------------

fn checked(source: &str) -> CheckedProgram {
    let result = check_source("input.vib", source);
    assert!(
        result.accepted(),
        "{:?}\n{}",
        result.diagnostics(),
        &source[..source.len().min(600)]
    );
    result.program().expect("an accepted program").clone()
}

fn entry_type(program: &CheckedProgram) -> Type {
    program.entry().signature().result()
}

fn emitted(program: &CheckedProgram) -> Vec<u8> {
    vibra_wasm::emit(program)
        .unwrap_or_else(|error| panic!("the program lowers: {error}"))
        .into_bytes()
}

fn runner() -> Runner {
    Runner::new(MemoryLimit::new(64 * MIB)).expect("the engine configures")
}

fn interpreted(program: &CheckedProgram) -> String {
    vibra_interp::run(program)
        .expect("the interpreter runs the program")
        .canonical_result()
}

fn observed_by_wasm(runner: &Runner, program: &CheckedProgram) -> String {
    let ty = entry_type(program);
    match runner
        .run_observed(&emitted(program), &ty, program.types())
        .expect("the module runs")
    {
        Observed::Completed { value, .. } => value.canonical_observation(&ty),
        Observed::Stopped(outcome) => panic!("the entry did not complete: {outcome:?}"),
    }
}

// -- the programs of the vectors ------------------------------------------------

/// A placeholder literal of an operand's type, which the exact operand replaces.
fn placeholder(value: &Value) -> String {
    match value {
        Value::F32(_) => "0.0f32".to_owned(),
        Value::F64(_) => "0.0f64".to_owned(),
        Value::Str(_) => "\"\"".to_owned(),
        Value::I8(_) => "0i8".to_owned(),
        Value::I16(_) => "0i16".to_owned(),
        Value::I32(_) => "0i32".to_owned(),
        Value::I64(_) => "0i64".to_owned(),
        Value::U8(_) => "0u8".to_owned(),
        Value::U16(_) => "0u16".to_owned(),
        Value::U32(_) => "0u32".to_owned(),
        Value::U64(_) => "0u64".to_owned(),
        other => panic!("not a vector operand: {other:?}"),
    }
}

/// The type of the result of a row, written from the registry table of the
/// specification and not from `signature`, so a wrong signature is rejected by
/// the checker.
fn result_type(intrinsic: CompilerIntrinsic) -> String {
    match intrinsic {
        CompilerIntrinsic::Integer(_, IntegerOp::ToStr)
        | CompilerIntrinsic::Float(_, FloatOp::ToStr) => "str".to_owned(),
        CompilerIntrinsic::Integer(numeric, IntegerOp::Parse)
        | CompilerIntrinsic::Float(numeric, FloatOp::Parse) => {
            format!("(result {} core.conversion-error)", numeric.name())
        }
        CompilerIntrinsic::Float(_, FloatOp::Equal) => "bool".to_owned(),
        CompilerIntrinsic::Float(_, FloatOp::CompareTotal) => {
            "core.ordering".to_owned()
        }
        CompilerIntrinsic::Float(numeric, _) => numeric.name().to_owned(),
        other => panic!("no vectors for {}", other.symbol()),
    }
}

/// One program whose entry is a tuple of the samples of `intrinsic`, written with
/// placeholders.
fn row_program(intrinsic: CompilerIntrinsic, vectors: &[Vector]) -> String {
    let ty = result_type(intrinsic);
    let types = vec![ty.as_str(); vectors.len()].join(" ");
    let calls = vectors
        .iter()
        .map(|vector| {
            let operands = vector
                .operands
                .iter()
                .map(placeholder)
                .collect::<Vec<_>>()
                .join(" ");
            format!("({} {operands})", intrinsic.symbol())
        })
        .collect::<Vec<_>>()
        .join("\n    ");
    format!(
        "(import core @std.core)\n(defn main () (tuple {types})\n  (tupleof\n    {calls}))\n"
    )
}

/// The checked program with every literal operand replaced by the exact operand
/// of its vector.
fn with_operands(program: &CheckedProgram, vectors: &[Vector]) -> CheckedProgram {
    let mut functions = program.functions().to_vec();
    let entry = functions
        .iter()
        .position(|function| function == program.entry())
        .expect("the entry is one of the functions");
    let function = functions.get(entry).expect("the entry").clone();
    let mut body = function.body().clone();
    // The entry's body is a sequence of the one tuple the source writes.
    let mut tuple = &mut body;
    while let Expr::Sequence { expressions, .. } = tuple {
        tuple = expressions
            .last_mut()
            .expect("a sequence has an expression");
    }
    let Expr::Tuple { components, .. } = tuple else {
        panic!("the body is a tuple: {tuple:?}");
    };
    assert_eq!(components.len(), vectors.len());
    for (component, vector) in components.iter_mut().zip(vectors) {
        // A row is applied through its wrapper function, a direct call.
        let (Expr::External { arguments, .. } | Expr::Call { arguments, .. }) =
            component
        else {
            panic!("a component is a row application: {component:?}");
        };
        assert_eq!(arguments.len(), vector.operands.len());
        for (argument, operand) in arguments.iter_mut().zip(&vector.operands) {
            let Expr::Literal { value, .. } = argument else {
                panic!("an operand is a literal: {argument:?}");
            };
            *value = operand.clone();
        }
    }
    functions[entry] = CheckedFunction::new(
        function.name(),
        function.signature().clone(),
        body,
        function.origin().clone(),
    )
    .expect("the rewritten entry is well formed");
    CheckedProgram::try_new_with_types(
        program.types().to_vec(),
        program.globals().to_vec(),
        functions,
        entry,
    )
    .expect("the rewritten program is well formed")
}

/// The observable value the table specifies for a row's program.
fn expected(program: &CheckedProgram, vectors: &[Vector]) -> ObservedValue {
    let Type::Tuple(components) = entry_type(program) else {
        panic!("the entry is a tuple");
    };
    ObservedValue::Tuple {
        type_id: None,
        values: vectors
            .iter()
            .zip(&components)
            .map(|(vector, ty)| vector.outcome.observed(ty))
            .collect(),
    }
}

/// The value part of a canonical result `(record type: T value: V)\n`.
fn result_value(encoded: &str) -> String {
    let (_, value) = encoded
        .split_once(" value: ")
        .expect("a canonical result has a value");
    value
        .strip_suffix(")\n")
        .expect("a canonical result ends its record")
        .to_owned()
}

/// Runs the samples of `intrinsic` in both backends and holds each to the table.
fn run_row(runner: &Runner, intrinsic: CompilerIntrinsic) -> usize {
    let mut run = 0;
    for vectors in intrinsic.vectors().chunks(CHUNK) {
        let program =
            with_operands(&checked(&row_program(intrinsic, vectors)), vectors);
        let want = expected(&program, vectors).canonical_vibon();
        let ty = entry_type(&program);

        let interpreter = result_value(&interpreted(&program));
        assert_eq!(
            interpreter,
            want,
            "the interpreter disagrees with the table on {}",
            intrinsic.symbol()
        );

        let wasm = match runner
            .run_observed(&emitted(&program), &ty, program.types())
            .expect("the module runs")
        {
            Observed::Completed { value, .. } => value.canonical_observation(&ty),
            Observed::Stopped(outcome) => {
                panic!("{} did not complete: {outcome:?}", intrinsic.symbol())
            }
        };
        let wasm = result_value(&wasm);
        if wasm != want {
            let at = wasm
                .chars()
                .zip(want.chars())
                .position(|(left, right)| left != right)
                .unwrap_or(0);
            let from = at.saturating_sub(200);
            panic!(
                "the module disagrees with the table on {} near byte {at}:\n  module: {}\n  table:  {}",
                intrinsic.symbol(),
                &wasm[from..(at + 200).min(wasm.len())],
                &want[from..(at + 200).min(want.len())]
            );
        }
        run += vectors.len();
    }
    run
}

// -- the rows ---------------------------------------------------------------------

/// Whether Step 8c owns `intrinsic`: the float rows and the integer `to-str` and
/// `parse` rows.
fn owned_by_step_8c(intrinsic: CompilerIntrinsic) -> bool {
    match intrinsic {
        CompilerIntrinsic::Float(..) => true,
        CompilerIntrinsic::Integer(_, op) => {
            matches!(op, IntegerOp::ToStr | IntegerOp::Parse)
        }
        _ => false,
    }
}

fn rows() -> Vec<CompilerIntrinsic> {
    CompilerIntrinsic::all()
        .into_iter()
        .filter(|row| owned_by_step_8c(*row))
        .collect()
}

/// The rows, grouped so a few threads share the work, with each thread its own
/// engine.
fn run_all(rows: &[CompilerIntrinsic]) -> usize {
    let threads = std::thread::available_parallelism()
        .map_or(2, std::num::NonZeroUsize::get)
        .min(8);
    let chunks = rows
        .chunks(rows.len().div_ceil(threads).max(1))
        .collect::<Vec<_>>();
    let mut vectors = 0;
    std::thread::scope(|scope| {
        let handles = chunks
            .into_iter()
            .map(|chunk| {
                scope.spawn(move || {
                    let runner = runner();
                    chunk
                        .iter()
                        .map(|row| run_row(&runner, *row))
                        .sum::<usize>()
                })
            })
            .collect::<Vec<_>>();
        for handle in handles {
            vectors += handle.join().expect("a row's thread finishes");
        }
    });
    vectors
}

#[test]
fn the_rows_are_the_twenty_looping_rows_and_the_fourteen_inline_float_rows() {
    let rows = rows();
    let looping = rows
        .iter()
        .filter(|row| row.native_import().is_some())
        .count();
    let inline = rows
        .iter()
        .filter(|row| vibra_wasm::lowers_primitive(**row))
        .count();
    // Eight integer types and two float types, `to-str` and `parse`.
    assert_eq!(looping, 20);
    // `add`, `sub`, `mul`, `div`, `neg`, `equal`, and `compare-total`, twice.
    assert_eq!(inline, 14);
    assert_eq!(rows.len(), looping + inline);
    for row in &rows {
        assert!(!row.vectors().is_empty(), "{} has no vectors", row.symbol());
        // No row is both an import and inline.
        assert!(
            row.native_import().is_some() != vibra_wasm::lowers_primitive(*row),
            "{}",
            row.symbol()
        );
    }
}

#[test]
fn every_vector_of_every_number_row_is_the_same_in_both_backends_and_in_the_table() {
    let started = Instant::now();
    let rows = rows();
    let vectors = run_all(&rows);
    eprintln!(
        "{} rows, {vectors} vectors, {:?}",
        rows.len(),
        started.elapsed()
    );
    let table = rows.iter().map(|row| row.vectors().len()).sum::<usize>();
    assert_eq!(vectors, table);
    assert!(vectors > 10_000, "only {vectors} vectors");
}

// -- the boundaries ---------------------------------------------------------------

/// The table holds each boundary the guide lists, as vectors of the rows that
/// observe it.
#[test]
fn the_table_holds_every_boundary_of_the_guide() {
    let f64_to_str =
        CompilerIntrinsic::Float(NumericType::F64, FloatOp::ToStr).vectors();
    let f32_to_str =
        CompilerIntrinsic::Float(NumericType::F32, FloatOp::ToStr).vectors();
    let text = |vectors: &[Vector], bits: Value| -> String {
        let found = vectors
            .iter()
            .find(|vector| vector.operands == [bits.clone()])
            .unwrap_or_else(|| panic!("no vector for {bits:?}"));
        match &found.outcome {
            vibra_ir::external::Outcome::Value(Value::Str(text)) => text.clone(),
            other => panic!("not a text: {other:?}"),
        }
    };
    let f64_text = |value: f64| text(f64_to_str, Value::F64(value.to_bits()));
    let f32_text = |value: f32| text(f32_to_str, Value::F32(value.to_bits()));
    // Zero and negative zero.
    assert_eq!(f64_text(0.0), "0.0");
    assert_eq!(f64_text(-0.0), "-0.0");
    // The largest and smallest finite values, and the subnormals.
    assert_eq!(f64_text(f64::MAX), "1.7976931348623157e308");
    assert_eq!(f64_text(f64::MIN), "-1.7976931348623157e308");
    assert_eq!(f64_text(f64::from_bits(1)), "5e-324");
    assert_eq!(f64_text(f64::MIN_POSITIVE), "2.2250738585072014e-308");
    assert_eq!(f32_text(f32::MAX), "3.4028235e38");
    assert_eq!(f32_text(f32::from_bits(1)), "1e-45");
    // The infinities.
    assert_eq!(f64_text(f64::INFINITY), "inf");
    assert_eq!(f64_text(f64::NEG_INFINITY), "-inf");
    assert_eq!(f32_text(f32::NEG_INFINITY), "-inf");
    // The switch of notation at `1e-4` and `1e16`.
    assert_eq!(f64_text(1e-4), "0.0001");
    assert_eq!(
        f64_text(f64::from_bits(1e-4_f64.to_bits() - 1)),
        "9.999999999999999e-5"
    );
    assert_eq!(f64_text(1e16), "1e16");
    assert_eq!(
        f64_text(f64::from_bits(1e16_f64.to_bits() - 1)),
        "9999999999999998.0"
    );
    // A value that needs seventeen digits.
    assert_eq!(f64_text(0.1 + 0.2), "0.30000000000000004");
    // NaN of both signs and several payloads is the one text.
    for bits in [
        0x7FF8_0000_0000_0000_u64,
        0xFFF8_0000_0000_0000,
        0x7FF0_0000_0000_0001,
        0xFFF4_0000_DEAD_BEEF,
    ] {
        assert_eq!(text(f64_to_str, Value::F64(bits)), "nan", "{bits:#x}");
    }
    for bits in [0x7FC0_0000_u32, 0xFFC0_0000, 0x7F80_0001, 0xFFA0_BEEF] {
        assert_eq!(text(f32_to_str, Value::F32(bits)), "nan", "{bits:#x}");
    }
    // Integer parse at each type's minimum and maximum, one past them, a leading
    // `-` on the unsigned types, and non-ASCII digits.
    for numeric in NumericType::ALL.into_iter().filter(|n| n.is_integer()) {
        let (min, max) = numeric.range();
        let vectors = CompilerIntrinsic::Integer(numeric, IntegerOp::Parse).vectors();
        let outcome = |spelled: &str| {
            vectors
                .iter()
                .find(|vector| vector.operands == [Value::Str(spelled.to_owned())])
                .unwrap_or_else(|| {
                    panic!("{} has no vector for {spelled:?}", numeric.name())
                })
                .outcome
                .clone()
        };
        use vibra_ir::external::Outcome::{Err, Ok};
        assert!(matches!(outcome(&max.to_string()), Ok(_)));
        assert_eq!(outcome(&(max + 1).to_string()), Err("out-of-range"));
        if numeric.is_signed() {
            assert!(matches!(outcome(&min.to_string()), Ok(_)));
            assert_eq!(outcome(&(min - 1).to_string()), Err("out-of-range"));
        } else {
            assert_eq!(outcome("-1"), Err("invalid-format"));
            assert_eq!(outcome("-0"), Err("invalid-format"));
        }
        assert_eq!(outcome("\u{661}\u{662}"), Err("invalid-format"));
        assert_eq!(outcome("\u{FF11}\u{FF12}"), Err("invalid-format"));
        assert_eq!(outcome(""), Err("invalid-format"));
    }
    // Float parse: out of range, malformed, and empty.
    let f64_parse =
        CompilerIntrinsic::Float(NumericType::F64, FloatOp::Parse).vectors();
    let parsed = |text: &str| {
        f64_parse
            .iter()
            .find(|vector| vector.operands == [Value::Str(text.to_owned())])
            .unwrap_or_else(|| panic!("no vector for {text:?}"))
            .outcome
            .clone()
    };
    use vibra_ir::external::Outcome::Err;
    assert_eq!(parsed("1e309"), Err("out-of-range"));
    assert_eq!(parsed(""), Err("invalid-format"));
    assert_eq!(parsed("1"), Err("invalid-format"));
    assert_eq!(parsed("nan"), Err("invalid-format"));
}

/// A NaN has no sign and no payload anywhere a float is observed.
#[test]
fn a_nan_is_one_value_wherever_it_is_observed() {
    // `0.0 / 0.0` is a NaN the engine and the host compute; `neg` flips its
    // sign bit. Neither is observable: `to-str`, `equal`, `compare-total`, and
    // the canonical encoding of the result agree in both backends.
    let source = "(import core @std.core)\n\
(defn main ()\n\
  (tuple str str bool bool core.ordering core.ordering f64 f32 f64 f32 (result f64 core.conversion-error))\n\
  (tupleof\n\
    (f64.to-str nan64) (f32.to-str (nan32))\n\
    (f64.equal nan64 nan64) (f32.equal (nan32) (nan32))\n\
    (f64.compare-total nan64 (f64.neg nan64))\n\
    (f32.compare-total (f32.neg (nan32)) (nan32))\n\
    nan64 (nan32) (f64.neg nan64) (f32.neg (nan32))\n\
    (f64.parse \"nan\")))\n\
(def nan64 f64 (f64.div 0.0f64 0.0f64))\n\
(defn nan32 () f32 (f32.div 0.0f32 0.0f32))\n";
    let program = checked(source);
    let interpreter = interpreted(&program);
    let wasm = observed_by_wasm(&runner(), &program);
    assert_eq!(wasm, interpreter);
    let value = result_value(&interpreter);
    assert!(
        value.starts_with("(record kind: @tuple values: (array \"nan\" \"nan\" false false (record kind: @enum type: @std.core.ordering variant: @equal) (record kind: @enum type: @std.core.ordering variant: @equal) nanf64 nanf32 nanf64 nanf32 "),
        "{value}"
    );
}

#[test]
fn emission_of_the_number_rows_is_byte_identical() {
    for row in [
        CompilerIntrinsic::Float(NumericType::F64, FloatOp::CompareTotal),
        CompilerIntrinsic::Float(NumericType::F32, FloatOp::Div),
        CompilerIntrinsic::Float(NumericType::F64, FloatOp::ToStr),
        CompilerIntrinsic::Integer(NumericType::U8, IntegerOp::Parse),
    ] {
        let vectors = &row.vectors()[..row.vectors().len().min(40)];
        let program = with_operands(&checked(&row_program(row, vectors)), vectors);
        assert_eq!(emitted(&program), emitted(&program), "{}", row.symbol());
    }
}

// -- reclamation ------------------------------------------------------------------

fn word(instance: &mut Instance, address: u32) -> u32 {
    let bytes = instance
        .read_memory_for_tests(address, 4)
        .expect("the state is in memory");
    u32::from_le_bytes(bytes.try_into().expect("four bytes"))
}

/// The bytes of every block the instance's module-value table reaches: what an
/// instance legitimately keeps after the host has released everything it held.
fn module_values_bytes(instance: &mut Instance) -> u64 {
    let table = word(instance, state::MODULE_VALUES);
    if table == 0 {
        return 0;
    }
    let mut seen = BTreeSet::new();
    let mut pending = vec![table];
    let mut bytes = 0_u64;
    while let Some(block) = pending.pop() {
        if !seen.insert(block) {
            continue;
        }
        bytes += u64::from(word(instance, block + header::BLOCK_SIZE));
        let stride = (word(instance, block + header::KIND) >> 8) & 0xFF;
        if stride != layout::STRIDE_CELLS {
            continue;
        }
        let len = word(instance, block + header::LEN);
        let classes = instance
            .read_memory_for_tests(block + header::SIZE, len as usize)
            .expect("class bytes");
        let cells = block + layout::cells_offset(len);
        for (position, class) in (0_u32..).zip(classes) {
            if class == layout::CellClass::Ref.code() {
                pending.push(word(instance, cells + 8 * position));
            }
        }
    }
    bytes
}

struct Footprint {
    live: u64,
    kept: u64,
    frame_depth: u32,
}

fn footprint(program: &CheckedProgram, limit: usize) -> Footprint {
    let bytes = emitted(program);
    let runner = Runner::new(MemoryLimit::new(limit)).expect("the engine configures");
    let Started::Ready(mut instance) = runner.start(&bytes).expect("a v1 module")
    else {
        panic!("the module's own memory exceeds the limit");
    };
    let Outcome::Completed { result, .. } = instance.call_entry().expect("runs") else {
        panic!("the entry did not complete");
    };
    instance
        .observe(result, &entry_type(program), program.types())
        .expect("the host reads the result");
    Footprint {
        live: instance.live_size().expect("live size"),
        kept: module_values_bytes(&mut instance),
        frame_depth: word(&mut instance, state::FRAME_DEPTH),
    }
}

#[test]
fn every_path_through_every_number_text_row_leaves_the_arena_balanced() {
    // Each row's samples, the `ok` and the two `err` paths of `parse` and every
    // string `to-str` builds, in a frame that holds the results: after the host
    // released the result nothing is live but what the module values keep.
    use NumericType::{F32, F64, I8, I64, U8, U64};
    let rows = [
        CompilerIntrinsic::Integer(I8, IntegerOp::ToStr),
        CompilerIntrinsic::Integer(U64, IntegerOp::ToStr),
        CompilerIntrinsic::Integer(I64, IntegerOp::Parse),
        CompilerIntrinsic::Integer(U8, IntegerOp::Parse),
        CompilerIntrinsic::Float(F64, FloatOp::ToStr),
        CompilerIntrinsic::Float(F32, FloatOp::ToStr),
        CompilerIntrinsic::Float(F64, FloatOp::Parse),
        CompilerIntrinsic::Float(F32, FloatOp::Parse),
        CompilerIntrinsic::Float(F64, FloatOp::Add),
        CompilerIntrinsic::Float(F32, FloatOp::CompareTotal),
        CompilerIntrinsic::Float(F64, FloatOp::Equal),
    ];
    for row in rows {
        let vectors = &row.vectors()[..row.vectors().len().min(CHUNK)];
        let program = with_operands(&checked(&row_program(row, vectors)), vectors);
        let after = footprint(&program, 64 * MIB);
        assert_eq!(after.live, after.kept, "{} is not balanced", row.symbol());
        assert_eq!(after.frame_depth, 0, "{} left a frame", row.symbol());
    }
}

#[test]
fn a_loop_of_number_text_holds_a_bounded_arena() {
    // Each round prints an integer and a float and parses them back; the loop is
    // a tail call, so a hundred thousand rounds hold what ten thousand do.
    let source = |rounds: u32| {
        format!(
            "(import core @std.core)\n\
(defn main () u64 (spin {rounds}u64 0u64))\n\
(defn spin (n u64 acc u64) u64\n\
  (if (u64.equal n 0u64)\n\
    acc\n\
    (match (u64.sub-checked n 1u64)\n\
      (result.ok next)\n\
        (match (u64.parse (u64.to-str n))\n\
          (result.ok back)\n\
            (match (f64.parse (f64.to-str 1.5e-7f64))\n\
              (result.ok -) (spin next (digits back acc))\n\
              (result.err -) acc)\n\
          (result.err -) acc)\n\
      (result.err -) acc)))\n\
(defn digits (value u64 acc u64) u64\n\
  (match (u64.add-checked acc value) (result.ok total) total (result.err -) acc))\n"
        )
    };
    let small = footprint(&checked(&source(2_000)), 256 * 1024);
    let large = footprint(&checked(&source(20_000)), 256 * 1024);
    assert_eq!(small.live, small.kept);
    assert_eq!(large.live, large.kept);
    assert_eq!(large.frame_depth, 0);
    let program = checked(&source(20_000));
    assert_eq!(
        observed_by_wasm(&runner(), &program),
        interpreted(&program),
        "the loop's result"
    );
}

#[test]
fn a_string_built_at_the_end_of_memory_is_a_host_event_and_never_a_native_fault() {
    // Shrink the memory limit until the strings no longer fit. At every limit the
    // run completes or exhausts its memory; a native that read or wrote past the
    // end of the memory would be a defect.
    let source = "(defn main () (tuple str str str str)\n\
  (tupleof (f64.to-str 1.7976931348623157e308f64) (f64.to-str -5e-324f64)\n\
           (i64.to-str -9223372036854775808i64) (u64.to-str 18446744073709551615u64)))\n";
    let program = checked(source);
    let bytes = emitted(&program);
    let mut completed = 0;
    let mut exhausted = 0;
    for kib in 1..=256 {
        let runner = Runner::new(MemoryLimit::new(kib * 1024)).expect("an engine");
        match runner.start(&bytes).expect("a v1 module") {
            Started::Ready(mut instance) => {
                match instance.call_entry().expect("runs") {
                    Outcome::Completed { .. } => completed += 1,
                    Outcome::MemoryExhausted => exhausted += 1,
                    other => panic!("at {kib} KiB: {other:?}"),
                }
            }
            Started::MemoryExhausted => exhausted += 1,
        }
    }
    assert!(completed > 0, "no limit let the program finish");
    assert!(
        exhausted > 0,
        "no limit was small enough to exhaust the memory"
    );
}
