//! The integer and `char` primitive rows (milestone 4 Step 8a), in
//! WebAssembly.
//!
//! One table of sample vectors (`CompilerIntrinsic::vectors`, in `vibra-ir`)
//! holds the operands and the specified outcome of every sample of every row
//! that the emitter lowers inline. Each row is run as one program, a tuple of
//! its samples, by the reference interpreter and by the WebAssembly module, and
//! both are held to the table. The expected outcomes come from the language's
//! own integer types, not from either backend, so the two backends cannot
//! drift apart on a boundary: overflow, division by zero, the signed minimum
//! divided by `-1`, the sign of a remainder, a shift at or past the width, a
//! conversion at the edge of its target, and the surrogate range of `char`.
//!
//! The tests also hold the lowering to what the runtime chapter asks of it:
//! every path through a row, the error results included, leaves the arena
//! balanced; a primitive application creates no activation; and emission is
//! deterministic.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    missing_docs
)]

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use vibra_ir::external::{CompilerIntrinsic, IntegerOp, NumericType, Vector};
use vibra_ir::{CheckedProgram, ObservedValue, Type, Value};
use vibra_types::check_source;
use vibra_wasm::layout::{self, header, state};
use vibra_wasm::{Form, NotLowered};
use vibra_wasm_run::{Instance, MemoryLimit, Observed, Outcome, Runner, Started};

const MIB: usize = 1024 * 1024;

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

/// The interpreter's observed result.
fn interpreted(program: &CheckedProgram) -> String {
    vibra_interp::run(program)
        .expect("the interpreter runs the program")
        .canonical_result()
}

/// What the module observes, as the canonical result of the entry.
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

/// A character literal that spells `value` for the reader.
fn character(value: char) -> String {
    match value {
        ' ' => "\\space".to_owned(),
        '\u{0}'..='\u{FFFF}' if !value.is_ascii_graphic() => {
            format!("\\u{:04X}", u32::from(value))
        }
        other => format!("\\{other}"),
    }
}

/// A source literal of a vector operand.
fn literal(value: &Value) -> String {
    match value {
        Value::I8(value) => format!("{value}i8"),
        Value::I16(value) => format!("{value}i16"),
        Value::I32(value) => format!("{value}i32"),
        Value::I64(value) => format!("{value}i64"),
        Value::U8(value) => format!("{value}u8"),
        Value::U16(value) => format!("{value}u16"),
        Value::U32(value) => format!("{value}u32"),
        Value::U64(value) => format!("{value}u64"),
        Value::Char(value) => character(*value),
        other => panic!("not a vector operand: {other:?}"),
    }
}

/// The type of the result of a row, written from the registry table of the
/// specification and not from `signature`, so a wrong signature is rejected by
/// the checker.
fn result_type(intrinsic: CompilerIntrinsic) -> String {
    match intrinsic {
        CompilerIntrinsic::Integer(_, IntegerOp::Equal) => "bool".to_owned(),
        CompilerIntrinsic::Integer(_, IntegerOp::Compare) => "core.ordering".to_owned(),
        CompilerIntrinsic::Integer(numeric, _) => {
            format!("(result {} core.arithmetic-error)", numeric.name())
        }
        CompilerIntrinsic::Convert(source, target) => {
            if CompilerIntrinsic::conversion_is_total(source, target) {
                target.name().to_owned()
            } else {
                format!("(result {} core.conversion-error)", target.name())
            }
        }
        CompilerIntrinsic::CharToU32 => "u32".to_owned(),
        CompilerIntrinsic::CharFromU32 => "(option char)".to_owned(),
        other => panic!("no vectors for {}", other.symbol()),
    }
}

/// One program whose entry is a tuple of every sample of `intrinsic`.
fn row_program(intrinsic: CompilerIntrinsic, vectors: &[Vector]) -> String {
    let ty = result_type(intrinsic);
    let types = vec![ty.as_str(); vectors.len()].join(" ");
    let calls = vectors
        .iter()
        .map(|vector| {
            let operands = vector
                .operands
                .iter()
                .map(literal)
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

/// How many vectors a row's program ran, and how long its checking, the
/// interpreter, and the module took.
#[derive(Default)]
struct Run {
    vectors: usize,
}

/// Runs the program of `intrinsic` in both backends and holds each to the table.
fn run_row(runner: &Runner, intrinsic: CompilerIntrinsic) -> Run {
    let vectors = intrinsic.vectors();
    let source = row_program(intrinsic, vectors);
    let program = checked(&source);
    let want = expected(&program, vectors).canonical_vibon();
    let ty = entry_type(&program);

    let interpreter = vibra_interp::run(&program)
        .expect("the interpreter runs the program")
        .canonical_result();
    let interpreter = result_value(&interpreter);
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
    Run {
        vectors: vectors.len(),
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

// -- the table and the rows -----------------------------------------------------

/// Whether the integer and `char` rows own `intrinsic`: the rows Step 8a lowers.
fn owned_by_step_8a(intrinsic: CompilerIntrinsic) -> bool {
    match intrinsic {
        CompilerIntrinsic::Convert(..)
        | CompilerIntrinsic::CharToU32
        | CompilerIntrinsic::CharFromU32 => true,
        CompilerIntrinsic::Integer(_, op) => {
            !matches!(op, IntegerOp::ToStr | IntegerOp::Parse)
        }
        _ => false,
    }
}

#[test]
fn the_rows_the_emitter_lowers_are_exactly_the_rows_that_have_vectors() {
    let mut lowered = 0_usize;
    for intrinsic in CompilerIntrinsic::all() {
        assert_eq!(
            vibra_wasm::lowers_primitive(intrinsic),
            owned_by_step_8a(intrinsic),
            "{}",
            intrinsic.symbol()
        );
        assert_eq!(
            vibra_wasm::lowers_primitive(intrinsic),
            !intrinsic.vectors().is_empty(),
            "{} has vectors exactly when it is lowered",
            intrinsic.symbol()
        );
        lowered += usize::from(vibra_wasm::lowers_primitive(intrinsic));
    }
    // Eight integer types of ten rows, `neg-checked` for the four signed ones,
    // fifty-six conversions, and the two `char` rows.
    assert_eq!(lowered, 8 * 9 + 4 + 56 + 2);
}

/// The rows, grouped so a few threads share the work, with each thread its own
/// engine.
fn run_all(rows: &[CompilerIntrinsic]) -> (usize, usize) {
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
                        .map(|row| run_row(&runner, *row).vectors)
                        .sum::<usize>()
                })
            })
            .collect::<Vec<_>>();
        for handle in handles {
            vectors += handle.join().expect("a row's thread finishes");
        }
    });
    (rows.len(), vectors)
}

#[test]
fn every_vector_of_every_row_is_the_same_in_both_backends_and_in_the_table() {
    let started = Instant::now();
    let rows = CompilerIntrinsic::all()
        .into_iter()
        .filter(|row| owned_by_step_8a(*row))
        .collect::<Vec<_>>();
    let (count, vectors) = run_all(&rows);
    let by_type = rows
        .iter()
        .map(|row| match row {
            CompilerIntrinsic::Integer(numeric, _) => numeric.name().to_owned(),
            CompilerIntrinsic::Convert(source, _) => format!("{}.to-*", source.name()),
            _ => "char".to_owned(),
        })
        .collect::<std::collections::BTreeSet<_>>();
    eprintln!(
        "{count} rows over {} types, {vectors} vectors, {:?}",
        by_type.len(),
        started.elapsed()
    );
    assert_eq!(count, rows.len());
    assert!(vectors > 10_000);
}

#[test]
fn the_manifest_lists_exactly_the_rows_that_have_vectors_and_more() {
    // Every row with vectors is a symbol of the standard library's manifest, and
    // each is still declared `external: @compiler` there.
    let manifest = include_str!("../../../stdlib/manifest.vibon");
    for intrinsic in CompilerIntrinsic::all() {
        let quoted = format!("\"{}\"", intrinsic.symbol());
        assert!(manifest.contains(&quoted), "{}", intrinsic.symbol());
        if !intrinsic.vectors().is_empty() {
            assert!(
                !intrinsic.is_native(),
                "{} is a primitive row",
                intrinsic.symbol()
            );
        }
    }
}

#[test]
fn every_integer_type_has_every_row_and_every_boundary_in_the_table() {
    let mut per_type: BTreeMap<&str, usize> = BTreeMap::new();
    for numeric in NumericType::ALL.into_iter().filter(|n| n.is_integer()) {
        for op in [
            IntegerOp::AddChecked,
            IntegerOp::SubChecked,
            IntegerOp::MulChecked,
            IntegerOp::DivChecked,
            IntegerOp::RemChecked,
            IntegerOp::ShiftLeftChecked,
            IntegerOp::ShiftRight,
            IntegerOp::Equal,
            IntegerOp::Compare,
        ] {
            let vectors = CompilerIntrinsic::Integer(numeric, op).vectors();
            assert!(!vectors.is_empty(), "{}.{}", numeric.name(), op.name());
            *per_type.entry(numeric.name()).or_default() += vectors.len();
        }
        let neg = CompilerIntrinsic::Integer(numeric, IntegerOp::NegChecked);
        assert_eq!(neg.vectors().is_empty(), !numeric.is_signed());
    }
    assert_eq!(per_type.len(), 8);
}

// -- balance, determinism, and composition ----------------------------------------

#[test]
fn a_program_of_primitive_rows_is_emitted_to_the_same_bytes_twice() {
    let program = checked(&row_program(
        CompilerIntrinsic::Integer(NumericType::I8, IntegerOp::DivChecked),
        CompilerIntrinsic::Integer(NumericType::I8, IntegerOp::DivChecked).vectors(),
    ));
    assert_eq!(emitted(&program), emitted(&program));
}

#[test]
fn a_row_that_is_not_lowered_is_still_named_with_its_owning_step() {
    for (source, symbol) in [
        (
            "(defn main () str (i32.to-str 1i32))\n",
            "external:i32.to-str",
        ),
        ("(defn main () f64 (f64.add 1.0 2.0))\n", "external:f64.add"),
    ] {
        let program = checked(source);
        let error: NotLowered =
            vibra_wasm::emit(&program).expect_err("not lowered yet");
        assert!(
            error
                .forms()
                .iter()
                .any(|form| form.form() == Form::External
                    && form.detail() == Some(symbol.trim_start_matches("external:"))),
            "{error}"
        );
    }
}

#[test]
fn operations_compose_in_a_program_with_matches_try_and_tail_calls() {
    // gcd by the remainder, the number of set bits by shifts, and a checked sum
    // that stops at the first overflow through `try`.
    let source = "(import core @std.core)\n\
(defn main () (tuple u64 u32 (result i32 core.arithmetic-error) (result i32 core.arithmetic-error) bool core.ordering u8 (option char))\n\
  (tupleof (gcd 1071u64 462u64) (bits 255u32 0u32)\n\
           (sum 1i32 2i32 3i32) (sum 2147483647i32 1i32 0i32)\n\
           (u16.equal 7u16 7u16) (i64.compare -1i64 1i64)\n\
           (widen 200u8) (char.from-u32 128512u32)))\n\
(defn gcd (a u64 b u64) u64\n\
  (if (u64.equal b 0u64) a\n\
    (match (u64.rem-checked a b) (result.ok r) (gcd b r) (result.err -) 0u64)))\n\
(defn bits (n u32 count u32) u32\n\
  (if (u32.equal n 0u32) count\n\
    (match (u32.shift-right n 1u32)\n\
      (result.ok half) (bits half (add-one count))\n\
      (result.err -) count)))\n\
(defn add-one (n u32) u32 (match (u32.add-checked n 1u32) (result.ok v) v (result.err -) n))\n\
(defn sum (a i32 b i32 c i32) (result i32 core.arithmetic-error)\n\
  (let ab (try (i32.add-checked a b)))\n\
  (i32.add-checked ab c))\n\
(defn widen (n u8) u8 (match (u8.to-i8 n) (result.ok v) (match (i8.to-u8 v) (result.ok w) w (result.err -) 0u8) (result.err -) 1u8))\n";
    let program = checked(source);
    let expected = interpreted(&program);
    assert_eq!(observed_by_wasm(&runner(), &program), expected);
    assert!(
        expected.contains("8u32"),
        "255 has eight set bits: {expected}"
    );
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

/// What a run left, after the host released the result.
struct Footprint {
    live: u64,
    kept: u64,
    arena_used: u32,
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
    let live = instance.live_size().expect("live size");
    let kept = module_values_bytes(&mut instance);
    Footprint {
        live,
        kept,
        arena_used: word(&mut instance, state::ARENA_USED),
        frame_depth: word(&mut instance, state::FRAME_DEPTH),
    }
}

#[test]
fn every_path_through_every_family_of_rows_leaves_the_arena_balanced() {
    // One row of each outcome, run over all its vectors, so each of its ok and
    // error paths is taken many times, in a frame that holds the results: after
    // the host released the result nothing is live but what the module values
    // keep, and no frame is left.
    use NumericType::{I8, I64, U8, U16, U64};
    let rows = [
        CompilerIntrinsic::Integer(I8, IntegerOp::AddChecked),
        CompilerIntrinsic::Integer(I64, IntegerOp::MulChecked),
        CompilerIntrinsic::Integer(U64, IntegerOp::DivChecked),
        CompilerIntrinsic::Integer(I8, IntegerOp::ShiftLeftChecked),
        CompilerIntrinsic::Integer(I64, IntegerOp::NegChecked),
        CompilerIntrinsic::Integer(U16, IntegerOp::Equal),
        CompilerIntrinsic::Integer(U8, IntegerOp::Compare),
        CompilerIntrinsic::Convert(I64, U8),
        CompilerIntrinsic::Convert(U8, I64),
        CompilerIntrinsic::CharToU32,
        CompilerIntrinsic::CharFromU32,
    ];
    for row in rows {
        let program = checked(&row_program(row, row.vectors()));
        let after = footprint(&program, 64 * MIB);
        assert_eq!(after.live, after.kept, "{} is not balanced", row.symbol());
        assert_eq!(after.frame_depth, 0, "{} left a frame", row.symbol());
    }
}

#[test]
fn a_primitive_application_creates_no_activation_so_a_long_loop_of_them_holds_a_bounded_arena()
 {
    // Each round applies four rows, and the loop is a tail call: a hundred
    // thousand rounds hold what ten thousand do, in 256 KiB.
    let source = |rounds: u32| {
        format!(
            "(defn main () u64 (spin {rounds}u64 0u64))\n\
(defn spin (n u64 acc u64) u64\n\
  (if (u64.equal n 0u64)\n\
    acc\n\
    (match (u64.sub-checked n 1u64)\n\
      (result.ok next)\n\
        (match (u64.shift-left-checked 3u64 2u32)\n\
          (result.ok step)\n\
            (match (u64.add-checked acc step)\n\
              (result.ok total) (spin next total)\n\
              (result.err -) acc)\n\
          (result.err -) acc)\n\
      (result.err -) acc)))\n"
        )
    };
    let small = footprint(&checked(&source(10_000)), 256 * 1024);
    let large = footprint(&checked(&source(100_000)), 256 * 1024);
    assert_eq!(small.live, small.kept);
    assert_eq!(large.live, large.kept);
    assert_eq!(
        small.arena_used, large.arena_used,
        "the arena grew with the loop"
    );
    assert_eq!(large.frame_depth, 0);
    let program = checked(&source(100_000));
    assert_eq!(
        observed_by_wasm(&runner(), &program),
        interpreted(&program),
        "the loop's result"
    );
}
