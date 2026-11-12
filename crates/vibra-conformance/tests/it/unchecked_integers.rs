//! The unchecked integer operations through the whole native pipeline (package
//! X3-10a): lowering, the reference stepper, and, on Windows, linked
//! executables.
//!
//! The operations are `add`, `sub`, `mul`, `div`, `rem`, `neg`, `shift-left`,
//! `shift-right`, `equal`, and `compare` of the eight integer types. Their
//! meaning is written here a second time, with exact `i128` arithmetic and the
//! saturating rules of the specification, and never taken from the code under
//! test. The differential test runs one program per type that holds every point
//! of every operation: the interpreter, the stepper, and a linked executable
//! each evaluate every point, and the program answers the number of points
//! whose result is not the one written here.

use std::collections::BTreeSet;

use vibra_exec::{OriginId, Program, ScalarOp, Terminator, verify};
use vibra_interp::{Interpreter, RuntimeError};
use vibra_ir::Value;
use vibra_ir::boundary::TrapCode;
use vibra_lower::{LowerError, form, lower};
use vibra_nexus::{Outcome, Trap, UNBOUNDED};
use vibra_types::check_source;

use crate::lowering_slice::{MIB, Seen, Want, checked, differential, prim, stepped};

/// An integer type: its name, its range, and whether it is signed.
#[derive(Clone, Copy, Debug)]
struct Int {
    name: &'static str,
    lo: i128,
    hi: i128,
}

impl Int {
    const fn signed(self) -> bool {
        self.lo < 0
    }

    fn literal(self, value: i128) -> String {
        format!("{value}{}", self.name)
    }

    /// The values worth trying: the bounds and their neighbours, the signs,
    /// and small numbers.
    fn edges(self) -> Vec<i128> {
        let mut values = BTreeSet::new();
        for value in [
            self.lo,
            self.lo + 1,
            -1,
            0,
            1,
            2,
            3,
            self.hi / 2,
            self.hi - 1,
            self.hi,
        ] {
            if (self.lo..=self.hi).contains(&value) {
                values.insert(value);
            }
        }
        values.into_iter().collect()
    }

    fn clamp(self, value: i128) -> i128 {
        value.clamp(self.lo, self.hi)
    }
}

const TYPES: [Int; 8] = [
    Int {
        name: "i8",
        lo: i8::MIN as i128,
        hi: i8::MAX as i128,
    },
    Int {
        name: "i16",
        lo: i16::MIN as i128,
        hi: i16::MAX as i128,
    },
    Int {
        name: "i32",
        lo: i32::MIN as i128,
        hi: i32::MAX as i128,
    },
    Int {
        name: "i64",
        lo: i64::MIN as i128,
        hi: i64::MAX as i128,
    },
    Int {
        name: "u8",
        lo: 0,
        hi: u8::MAX as i128,
    },
    Int {
        name: "u16",
        lo: 0,
        hi: u16::MAX as i128,
    },
    Int {
        name: "u32",
        lo: 0,
        hi: u32::MAX as i128,
    },
    Int {
        name: "u64",
        lo: 0,
        hi: u64::MAX as i128,
    },
];

/// The shift amounts, a `u32`: inside the width, at it, beyond it, and the
/// largest.
const AMOUNTS: [u32; 15] =
    [0, 1, 2, 7, 8, 15, 16, 31, 32, 33, 63, 64, 65, 127, u32::MAX];

// -- the meaning, written again -----------------------------------------------------

/// The result a point must have.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Expect {
    Int(i128),
    Bool(bool),
}

/// What `left op right` is, with `right` the shift amount for a shift. `None`
/// for a zero divisor, which traps.
fn oracle(ty: Int, op: &str, left: i128, right: i128) -> Option<Expect> {
    let saturated = |negative: bool| if negative { ty.lo } else { ty.hi };
    let value = match op {
        "add" => ty.clamp(left + right),
        "sub" => ty.clamp(left - right),
        "mul" => left.checked_mul(right).map_or_else(
            || saturated((left < 0) != (right < 0)),
            |value| ty.clamp(value),
        ),
        "div" => {
            if right == 0 {
                return None;
            }
            ty.clamp(left / right)
        }
        "rem" => {
            if right == 0 {
                return None;
            }
            left % right
        }
        "neg" => ty.clamp(-left),
        "shift-left" => {
            if left == 0 {
                0
            } else if right > 64 {
                saturated(left < 0)
            } else {
                left.checked_mul(1_i128 << right)
                    .map_or_else(|| saturated(left < 0), |value| ty.clamp(value))
            }
        }
        "shift-right" => {
            if right > 100 {
                if left < 0 { -1 } else { 0 }
            } else {
                left >> right
            }
        }
        "equal" => return Some(Expect::Bool(left == right)),
        other => panic!("{other} has no oracle"),
    };
    Some(Expect::Int(value))
}

/// One point: the call in source and its expected result.
struct Point {
    call: String,
    expect: Expect,
}

/// Every point of `op` at `ty`. A zero divisor is a trap, not a point.
fn points(ty: Int, op: &str) -> Vec<Point> {
    let edges = ty.edges();
    let mut points = Vec::new();
    let mut push = |call: String, expect: Option<Expect>| {
        if let Some(expect) = expect {
            points.push(Point { call, expect });
        }
    };
    match op {
        "neg" => {
            for left in &edges {
                push(
                    format!("({}.neg {})", ty.name, ty.literal(*left)),
                    oracle(ty, op, *left, 0),
                );
            }
        }
        "shift-left" | "shift-right" => {
            for left in &edges {
                for amount in AMOUNTS {
                    push(
                        format!("({}.{op} {} {amount}u32)", ty.name, ty.literal(*left)),
                        oracle(ty, op, *left, i128::from(amount)),
                    );
                }
            }
        }
        _ => {
            for left in &edges {
                for right in &edges {
                    // `equal` is a member of the contract `equatable`, which the
                    // checker resolves to the type's implementation.
                    let callee = if op == "equal" {
                        "equatable.equal".to_owned()
                    } else {
                        format!("{}.{op}", ty.name)
                    };
                    push(
                        format!(
                            "({callee} {} {})",
                            ty.literal(*left),
                            ty.literal(*right)
                        ),
                        oracle(ty, op, *left, *right),
                    );
                }
            }
        }
    }
    points
}

/// The operations of one type that the batch holds, in order.
fn operations(ty: Int) -> Vec<&'static str> {
    let mut all = vec![
        "add",
        "sub",
        "mul",
        "div",
        "rem",
        "shift-left",
        "shift-right",
        "equal",
    ];
    if ty.signed() {
        all.push("neg");
    }
    all
}

/// A program that evaluates every point of `ops` and answers, as a `u32`, how
/// many of them are not what `Expect` says.
fn batch(ty: Int, ops: &[&str]) -> (String, usize) {
    let mut source = String::from("(defn main () u32\n  (let s0 0u32)\n");
    let mut count = 0_usize;
    for op in ops {
        for point in points(ty, op) {
            let check = match point.expect {
                Expect::Int(value) => format!(
                    "(if (equatable.equal {} {}) 0u32 1u32)",
                    point.call,
                    ty.literal(value)
                ),
                Expect::Bool(true) => format!("(if {} 0u32 1u32)", point.call),
                Expect::Bool(false) => format!("(if {} 1u32 0u32)", point.call),
            };
            source.push_str(&format!(
                "  (let s{} (u32.add s{count} {check}))\n",
                count + 1
            ));
            count += 1;
        }
    }
    source.push_str(&format!("  s{count})\n"));
    (source, count)
}

// -- the three executors -------------------------------------------------------------

/// How many points each executor evaluated.
#[derive(Default, Debug)]
struct Tally {
    interpreter: usize,
    stepper: usize,
    linked: usize,
}

/// Runs `source` in the interpreter and the stepper, and links and runs it on
/// Windows, and holds each to `Value::U32(0)`: no point failed.
fn run_batch(label: &str, source: &str, points: usize, tally: &mut Tally) {
    let program = checked(source);
    let execution =
        Interpreter::run(&program).unwrap_or_else(|e| panic!("{label}: {e}"));
    assert_eq!(
        execution.value(),
        Some(&Value::U32(0)),
        "{label}: the interpreter disagrees with the written meaning"
    );
    tally.interpreter += points;

    let lowered = lower(&program).unwrap_or_else(|error| panic!("{label}: {error}"));
    verify(&lowered).unwrap_or_else(|e| panic!("{label}: {e}"));
    for quota in [UNBOUNDED, 1] {
        let ran = stepped(&lowered, 64 * MIB, quota);
        assert_eq!(ran.outcome, Outcome::Completed, "{label} (quota {quota})");
        assert_eq!(
            ran.seen,
            Seen::Scalar(0),
            "{label} (quota {quota}): the stepper disagrees with the written meaning"
        );
        assert_eq!(ran.live, 0, "{label} (quota {quota}): balance");
    }
    tally.stepper += points;

    #[cfg(windows)]
    {
        linked_agrees(label, &program, &lowered);
        tally.linked += points;
    }
}

/// Links `program` with the harness launcher and holds its observation to the
/// reference stepper's.
#[cfg(windows)]
fn linked_agrees(label: &str, program: &vibra_ir::CheckedProgram, lowered: &Program) {
    use crate::linked_exe_support::{
        Profile, Scratch, WORLD, archive, options, reference, run_exe,
    };
    let _ = archive(Profile::Harness);
    let scratch = Scratch::new("x3-10a", "");
    let built = vibra_build::build_executable(
        program,
        &options(&scratch, Profile::Harness, "ints.exe"),
    )
    .unwrap_or_else(|error| panic!("{label}: {error}"));
    let expected = reference(lowered, None, UNBOUNDED, false);
    let ran = run_exe(&built.executable, &[], WORLD.as_bytes());
    assert_eq!(ran.code, Some(0), "{label}\nlink: {}", built.command);
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        format!("{}\n", expected.observation),
        "{label}: the linked executable\nlink: {}",
        built.command
    );
}

/// Finds the failing operation when a batch disagrees: each operation alone,
/// in the interpreter.
fn localize(ty: Int) -> String {
    let mut found = Vec::new();
    for op in operations(ty) {
        let (source, _) = batch(ty, &[op]);
        let program = checked(&source);
        let value = Interpreter::run(&program).expect("runs").value().cloned();
        if value != Some(Value::U32(0)) {
            found.push(format!("{}.{op}: {value:?}", ty.name));
        }
    }
    found.join(", ")
}

/// Runs ody on a thread with a stack for the deep nesting of a batch: a
/// program of a thousand bindings is a thousand levels of `let`.
fn deep(body: fn()) {
    std::thread::Builder::new()
        .stack_size(1 << 30)
        .spawn(body)
        .expect("a thread")
        .join()
        .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
}

#[test]
fn every_operation_of_every_integer_type_agrees_across_the_executors() {
    deep(every_operation_of_every_integer_type);
}

fn every_operation_of_every_integer_type() {
    let mut tally = Tally::default();
    let mut total = 0_usize;
    for ty in TYPES {
        let ops = operations(ty);
        let (source, count) = batch(ty, &ops);
        let label = format!("{} ({count} points)", ty.name);
        // A disagreement names its operation before it fails the test.
        let program = checked(&source);
        let value = Interpreter::run(&program).expect("runs").value().cloned();
        assert_eq!(
            value,
            Some(Value::U32(0)),
            "{label}: failing operations: {}",
            localize(ty)
        );
        run_batch(&label, &source, count, &mut tally);
        total += count;
    }
    eprintln!("integer points: {tally:?} of {total}");
    assert_eq!(tally.interpreter, total);
    assert_eq!(tally.stepper, total);
    #[cfg(windows)]
    assert_eq!(tally.linked, total);
    assert!(total > 4000, "{total} points");
}

#[test]
fn the_written_meaning_agrees_with_the_shared_vectors_on_the_edges() {
    // A guard on the oracle: the edges of the specification's examples.
    let i8t = TYPES[0];
    assert_eq!(oracle(i8t, "add", 127, 1), Some(Expect::Int(127)));
    assert_eq!(oracle(i8t, "sub", -128, 1), Some(Expect::Int(-128)));
    assert_eq!(oracle(i8t, "neg", -128, 0), Some(Expect::Int(127)));
    assert_eq!(oracle(i8t, "div", -128, -1), Some(Expect::Int(127)));
    assert_eq!(oracle(i8t, "rem", -128, -1), Some(Expect::Int(0)));
    assert_eq!(oracle(i8t, "rem", -7, 2), Some(Expect::Int(-1)));
    assert_eq!(oracle(i8t, "div", -7, 2), Some(Expect::Int(-3)));
    assert_eq!(oracle(i8t, "div", 1, 0), None);
    assert_eq!(oracle(i8t, "shift-left", 1, 7), Some(Expect::Int(127)));
    assert_eq!(oracle(i8t, "shift-left", 0, 99), Some(Expect::Int(0)));
    assert_eq!(oracle(i8t, "shift-right", -1, 9), Some(Expect::Int(-1)));
    assert_eq!(oracle(i8t, "shift-right", -5, 1), Some(Expect::Int(-3)));
}

// -- compare ----------------------------------------------------------------------------

/// Three points of `compare` per type, one for each answer. The answer is an
/// `ordering`, which source cannot write, so each is its own program.
#[test]
fn compare_answers_less_equal_and_greater_in_every_executor() {
    let mut run = 0_usize;
    for ty in TYPES {
        for (left, right, index, name) in [
            (ty.lo, ty.hi, 0, "less"),
            (ty.hi, ty.hi, 1, "equal"),
            (ty.hi, ty.lo, 2, "greater"),
        ] {
            let source = format!(
                "(import core @std.core)\n(defn main () core.ordering (ordered.compare {} {}))",
                ty.literal(left),
                ty.literal(right)
            );
            let label = format!("{}.compare {left} {right}", ty.name);
            let program =
                differential(&label, &source, &Want::Variant(index, name), &[], 0);
            run += 1;
            #[cfg(windows)]
            linked_agrees(&label, &program, &lower(&program).expect("lowers"));
        }
    }
    eprintln!("compare programs: {run}");
    assert_eq!(run, 24);
}

// -- the forms ---------------------------------------------------------------------------

#[test]
fn each_operation_lowers_balanced_and_answers_in_both_executors() {
    let cases: [(&str, &str, Want); 12] = [
        (
            "add",
            "(defn main () i32 (i32.add 2i32 3i32))",
            prim(Value::I32(5)),
        ),
        (
            "sub",
            "(defn main () u8 (u8.sub 2u8 3u8))",
            prim(Value::U8(0)),
        ),
        (
            "mul",
            "(defn main () i16 (i16.mul 300i16 300i16))",
            prim(Value::I16(i16::MAX)),
        ),
        (
            "div",
            "(defn main () i64 (i64.div -7i64 2i64))",
            prim(Value::I64(-3)),
        ),
        (
            "rem",
            "(defn main () u64 (u64.rem 7u64 4u64))",
            prim(Value::U64(3)),
        ),
        (
            "neg",
            "(defn main () i8 (i8.neg -128i8))",
            prim(Value::I8(i8::MAX)),
        ),
        (
            "shift-left",
            "(defn main () u32 (u32.shift-left 1u32 31u32))",
            prim(Value::U32(1 << 31)),
        ),
        (
            "shift-right",
            "(defn main () i32 (i32.shift-right -8i32 1u32))",
            prim(Value::I32(-4)),
        ),
        (
            "equal true",
            "(defn main () bool (equatable.equal 9u16 9u16))",
            Want::Bool(true),
        ),
        (
            "equal false",
            "(defn main () bool (equatable.equal 9i64 -9i64))",
            Want::Bool(false),
        ),
        (
            "nested",
            "(defn main () i32 (i32.add (i32.mul 3i32 4i32) (i32.neg (i32.div 8i32 2i32))))",
            prim(Value::I32(8)),
        ),
        (
            "a binding and a call",
            "(defn main () u64 (let a (u64.add 1u64 2u64)) (twice a))\n(defn twice (n u64) u64 (u64.mul n 2u64))",
            prim(Value::U64(6)),
        ),
    ];
    for (name, source, want) in &cases {
        // `differential` runs the interpreter, the stepper unbounded and with
        // quota 1, and holds the live size at the end to zero.
        differential(name, source, want, &[], 0);
    }
}

#[test]
fn division_and_remainder_test_the_divisor_and_trap_with_no_origin() {
    for op in ["div", "rem"] {
        let source = format!("(defn main () i32 (i32.{op} 7i32 (i32.sub 5i32 5i32)))");
        let program = checked(&source);
        // The interpreter: the trap has no origin.
        let error = Interpreter::run(&program).expect_err("a zero divisor traps");
        assert_eq!(error, RuntimeError::DivisionByZero);
        assert!(
            error
                .program_trap()
                .is_some_and(|(_, origin)| origin.is_none())
        );

        let lowered = lower(&program).expect("lowers");
        verify(&lowered).expect("verifies");
        // Exactly one block ends in the trap, and it names no origin; `Inst::Op`
        // carries none at all.
        let traps: Vec<_> = lowered
            .functions
            .iter()
            .flat_map(|function| &function.blocks)
            .filter_map(|block| match block.term {
                Terminator::Trap { code, origin } => Some((code, origin)),
                _ => None,
            })
            .collect();
        assert_eq!(traps, [(TrapCode::DivisionByZero, OriginId::NONE)], "{op}");
        // And the operation sits behind the test: the one `Branch` of the
        // function that divides.
        let divides = |function: &vibra_exec::Function| {
            function.blocks.iter().any(|block| {
                block.insts.iter().any(|inst| {
                    matches!(
                        inst,
                        vibra_exec::Inst::Op {
                            op: ScalarOp::Int(
                                vibra_exec::IntOp::Div | vibra_exec::IntOp::Rem,
                                _
                            ),
                            ..
                        }
                    )
                })
            })
        };
        let function = lowered
            .functions
            .iter()
            .find(|f| divides(f))
            .expect("a divide");
        assert!(
            function
                .blocks
                .iter()
                .any(|block| matches!(block.term, Terminator::Branch { .. }))
        );

        for quota in [UNBOUNDED, 1] {
            let ran = stepped(&lowered, 64 * MIB, quota);
            assert_eq!(
                ran.outcome,
                Outcome::Trapped(Trap {
                    code: TrapCode::DivisionByZero,
                    origin: 0
                }),
                "{op} (quota {quota})"
            );
        }
    }
}

#[test]
fn a_zero_divisor_traps_in_every_executor_for_every_type() {
    let mut count = 0_usize;
    for ty in TYPES {
        for op in ["div", "rem"] {
            let source = format!(
                "(defn main () {name} ({name}.{op} {seven} ({name}.sub {five} {five})))",
                name = ty.name,
                seven = ty.literal(7),
                five = ty.literal(5)
            );
            let label = format!("{}.{op} by zero", ty.name);
            let program = checked(&source);
            assert_eq!(
                Interpreter::run(&program).expect_err(&label),
                RuntimeError::DivisionByZero,
                "{label}"
            );
            let lowered = lower(&program).expect("lowers");
            let ran = stepped(&lowered, 64 * MIB, UNBOUNDED);
            assert_eq!(
                ran.outcome,
                Outcome::Trapped(Trap {
                    code: TrapCode::DivisionByZero,
                    origin: 0
                }),
                "{label}"
            );
            #[cfg(windows)]
            {
                linked_agrees(&label, &program, &lowered);
                let observation = crate::linked_exe_support::reference(
                    &lowered, None, UNBOUNDED, false,
                )
                .observation;
                assert!(
                    observation.contains("outcome: @trapped")
                        && observation.contains("trap: @runtime.division-by-zero")
                        && observation.contains("origin: 0"),
                    "{label}: {observation}"
                );
            }
            count += 1;
        }
    }
    assert_eq!(count, 16);
}

// -- the edge of the slice --------------------------------------------------------------

#[test]
fn the_checker_still_rejects_what_has_no_such_operation() {
    for (name, source) in [
        (
            "negation of an unsigned type",
            "(defn main () u8 (u8.neg 1u8))",
        ),
        (
            "operands of two types",
            "(defn main () i32 (i32.add 1i32 2i64))",
        ),
        (
            "a signed shift amount",
            "(defn main () i32 (i32.shift-left 1i32 1i32))",
        ),
    ] {
        let result = check_source("rejected.vib", source);
        assert!(!result.accepted(), "{name} was accepted");
        assert!(!result.diagnostics().is_empty(), "{name}");
    }
}

#[test]
fn the_other_numeric_operations_are_not_lowered_and_name_the_form() {
    for (name, source) in [
        (
            "a checked sum",
            "(defn main () (result i32 core.arithmetic-error) (i32.add-checked 1i32 2i32))",
        ),
        (
            "a checked negation",
            "(defn main () (result i32 core.arithmetic-error) (i32.neg-checked 1i32))",
        ),
        ("a float sum", "(defn main () f64 (f64.add 1.5f64 2.5f64))"),
        (
            "a checked shift",
            "(defn main () (result u8 core.arithmetic-error) (u8.shift-left-checked 1u8 9u32))",
        ),
    ] {
        let source = format!("(import core @std.core)\n{source}");
        let result = check_source("numeric.vib", &source);
        if !result.accepted() {
            panic!("{name}: {:?}", result.diagnostics());
        }
        assert_eq!(
            lower(result.program().expect("program")),
            Err(LowerError::NotLowered(form::EXTERNAL_NUMERIC)),
            "{name}"
        );
    }
}

#[test]
fn the_runtime_names_each_operation_as_the_execution_ir_does() {
    // The stepper's mapping, held to the execution IR's own names; the
    // generator's encoding is held to the same names in its crate.
    let all = ScalarOp::all();
    assert_eq!(all.len(), 4 + 9 * 8);
    for op in all {
        assert_eq!(
            format!("{:?}", vibra_nexus_eval::runtime_op(op)),
            format!("{op:?}")
        );
    }
}

/// The program is not used as a value; this keeps `Program` named for the
/// Windows-only helper above on other hosts.
const _: fn(&Program) = |_| {};
