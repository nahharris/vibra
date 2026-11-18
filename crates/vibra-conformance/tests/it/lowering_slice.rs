//! The lowering slice of package X3-03, held against the reference
//! interpreter.
//!
//! Every case is Vibra source text. It is checked once, then run by
//! `vibra_interp::Interpreter` and, after `vibra_lower::lower`,
//! `vibra_exec::verify` and admission, by the reference stepper on Nexus,
//! unbounded and with quota 1. Both executors are compared with an expectation
//! written here from the meaning of the program in the specification, never
//! with each other alone, and the stepper's run must also be balanced: when it
//! ends the context holds only what module values keep.
//!
//! These tests live in the harness because the architecture test allows the
//! lowering crate no dependency but the two IRs, and they need the checker, the
//! interpreter, the host, and the stepper.

use std::fmt::Write as _;
use std::process::Command;

use vibra_exec::{Callee, Inst, Program, Terminator, verify};
use vibra_host::{AuditEvent, HostWorld, WorldProvider};
use vibra_ir::arena::kind;
use vibra_ir::{CheckedProgram, Value};
use vibra_lower::{LowerError, lower};
use vibra_nexus::{Cell, Context, Outcome, Stats, UNBOUNDED, admit, run};
use vibra_nexus_eval::Evaluator;
use vibra_types::check_source;

use crate::support::workspace_root;

pub(crate) const MIB: u64 = 1024 * 1024;

// -- what a program is expected to answer ---------------------------------------

/// The expected answer of the entry, as the specification defines it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Want {
    /// A primitive scalar.
    Prim(Value),
    /// A `bool`, an enum of the variants `false` and `true`.
    Bool(bool),
    /// A `str`.
    Str(&'static str),
    /// A `(result t e)` that is `ok`, the first variant.
    Ok,
    /// An enum value with no payload: its variant index and its name.
    Variant(u32, &'static str),
}

/// The cell a scalar is stored as: the `I32` class holds the value
/// sign-extended to 32 bits for a signed type narrower than that, and the
/// `I64` class holds all 64 bits.
fn scalar_bits(value: &Value) -> u64 {
    let narrow = |value: i32| u64::from(value.cast_unsigned());
    match value {
        Value::Void => 0,
        Value::I8(value) => narrow(i32::from(*value)),
        Value::I16(value) => narrow(i32::from(*value)),
        Value::I32(value) => narrow(*value),
        Value::U8(value) => u64::from(*value),
        Value::U16(value) => u64::from(*value),
        Value::U32(value) => u64::from(*value),
        Value::Char(value) => u64::from(u32::from(*value)),
        Value::I64(value) => value.cast_unsigned(),
        Value::U64(value) => *value,
        Value::F32(bits) => u64::from(*bits),
        Value::F64(bits) => *bits,
        other => panic!("{other:?} is not a scalar"),
    }
}

/// One audit event of `io.stdout-text`.
fn stdout_event(text: &str) -> String {
    format!(
        "(record kind: @host op: \"io.stdout-text\" args: (array \"{text}\") result: \
         (record kind: @enum type: @std.result.result variant: @ok))"
    )
}

// -- the two executors ----------------------------------------------------------

pub(crate) fn checked(source: &str) -> CheckedProgram {
    let result = check_source("lowering-slice.vib", source);
    assert!(result.accepted(), "{:?}\n{source}", result.diagnostics());
    result.program().expect("an accepted program").clone()
}

fn trace(events: &[AuditEvent]) -> Vec<String> {
    events.iter().map(AuditEvent::to_vibon).collect()
}

/// What the interpreter answered, checked against `want`.
fn interpreted(program: &CheckedProgram, want: &Want, audit: &[String], what: &str) {
    let execution =
        vibra_interp::Interpreter::run(program).expect("the interpreter runs");
    match want {
        Want::Prim(value) => assert_eq!(execution.value(), Some(value), "{what}"),
        Want::Bool(value) => {
            assert_eq!(execution.value(), Some(&Value::Bool(*value)), "{what}");
        }
        Want::Str(text) => {
            assert_eq!(
                execution.value(),
                Some(&Value::Str((*text).to_owned())),
                "{what}"
            );
        }
        Want::Variant(_, name) => {
            assert!(
                execution
                    .canonical_result()
                    .contains(&format!("variant: @{name}")),
                "{what}: {}",
                execution.canonical_result()
            );
        }
        Want::Ok => {
            assert!(!execution.program_failed(), "{what}");
            assert!(
                execution.canonical_result().contains("variant: @ok"),
                "{what}: {}",
                execution.canonical_result()
            );
        }
    }
    assert_eq!(
        trace(execution.audit_trace()),
        audit,
        "{what}: interpreter audit"
    );
}

/// What one stepper run left behind.
pub(crate) struct Ran {
    pub(crate) outcome: Outcome,
    pub(crate) audit: Vec<String>,
    pub(crate) stats: Stats,
    pub(crate) peak: u64,
    /// The live size after the result was released.
    pub(crate) live: u64,
    pub(crate) result: Cell,
    /// The result as the store holds it, read before it was released.
    pub(crate) seen: Seen,
}

/// A result cell read through the store.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Seen {
    Scalar(u64),
    Enum(u32),
    Str(String),
}

/// Runs the lowered `program` to its end, continuing after every yield.
pub(crate) fn stepped(program: &Program, limit: u64, quota: u64) -> Ran {
    verify(program).expect("lower answers only programs that verify");
    let code = Evaluator::new(program).expect("the stepper admits the program");
    admit(&code.requirements()).expect("an admissible program");
    let mut host = WorldProvider::new(&HostWorld::default());
    let mut ctx = Context::new(limit, code.globals(), &mut host);
    code.start(&mut ctx).expect("room for the entry frame");
    let outcome = loop {
        match run(&mut ctx, &code, quota) {
            Outcome::Yielded => {}
            other => break other,
        }
    };
    let result = ctx.result();
    let mut seen = Seen::Scalar(0);
    if outcome == Outcome::Completed {
        seen = match result.as_ref() {
            None => Seen::Scalar(result.bits),
            Some(value) => {
                let store = ctx.store();
                match store.kind(value).expect("a live value") {
                    kind::ENUM => {
                        assert_eq!(store.len(value), Ok(0), "no payload cell");
                        Seen::Enum(store.variant(value).expect("a variant"))
                    }
                    kind::STR => Seen::Str(store.text(value).expect("text")),
                    other => panic!("an unexpected result kind {other}"),
                }
            }
        };
        // The caller owns the result.
        let dropped = ctx.store().drop_cell(result);
        dropped.expect("the result is released");
    }
    Ran {
        outcome,
        audit: trace(ctx.audit()),
        stats: ctx.stats(),
        peak: ctx.peak_size(),
        live: ctx.live_size(),
        result,
        seen,
    }
}

fn expect_seen(ran: &Ran, want: &Want, what: &str) {
    let expected = match want {
        Want::Prim(value) => Seen::Scalar(scalar_bits(value)),
        Want::Bool(value) => Seen::Enum(u32::from(*value)),
        Want::Str(text) => Seen::Str((*text).to_owned()),
        Want::Ok => Seen::Enum(0),
        Want::Variant(index, _) => Seen::Enum(*index),
    };
    assert_eq!(ran.seen, expected, "{what}: stepper result");
}

/// The size of the live module values of a program that has one `str`
/// global: a 24-byte header and five 4-byte characters, in a 64-byte block.
const STR_GLOBAL_BYTES: u64 = 64;

/// Checks `source` in both executors against `want` and `audit`, and returns
/// the checked program. `kept` is what module values keep alive.
pub(crate) fn differential(
    name: &str,
    source: &str,
    want: &Want,
    audit: &[String],
    kept: u64,
) -> CheckedProgram {
    let program = checked(source);
    interpreted(&program, want, audit, name);
    let lowered = lower(&program).unwrap_or_else(|error| panic!("{name}: {error}"));
    for quota in [UNBOUNDED, 1] {
        let what = format!("{name} (quota {quota})");
        let ran = stepped(&lowered, 64 * MIB, quota);
        assert_eq!(ran.outcome, Outcome::Completed, "{what}");
        expect_seen(&ran, want, &what);
        assert_eq!(ran.audit, audit, "{what}: stepper audit");
        assert_eq!(ran.live, kept, "{what}: balance, the live size at the end");
    }
    program
}

fn simple(name: &str, source: &str, want: &Want) {
    differential(name, source, want, &[], 0);
}

/// What the standard library's module values `true` and `false` keep: each
/// is a `bool` object of the smallest block, 32 bytes, built on its first
/// read and held for the rest of the execution. Source never writes a `bool`
/// literal as anything else, so a program that reads `true` and `false` keeps
/// 64 bytes however often it reads them.
const BOOL_GLOBAL_BYTES: u64 = 32;

fn kept(name: &str, source: &str, want: &Want, kept: u64) {
    differential(name, source, want, &[], kept);
}

pub(crate) const fn prim(value: Value) -> Want {
    Want::Prim(value)
}

// -- the forms -----------------------------------------------------------------

#[test]
fn literals_are_scalars_constants_and_bool_enums() {
    for (name, source, want) in [
        ("i32", "(defn main () i32 -7i32)", prim(Value::I32(-7))),
        ("i8", "(defn main () i8 -3i8)", prim(Value::I8(-3))),
        ("u8", "(defn main () u8 200u8)", prim(Value::U8(200))),
        (
            "u32",
            "(defn main () u32 4000000000u32)",
            prim(Value::U32(4_000_000_000)),
        ),
        ("i64", "(defn main () i64 -9i64)", prim(Value::I64(-9))),
        (
            "u64",
            "(defn main () u64 18446744073709551615u64)",
            prim(Value::U64(u64::MAX)),
        ),
        (
            "f64",
            "(defn main () f64 1.5f64)",
            prim(Value::F64(1.5_f64.to_bits())),
        ),
        (
            "f32",
            "(defn main () f32 0.25f32)",
            prim(Value::F32(0.25_f32.to_bits())),
        ),
        ("void", "(defn main () void (do))", prim(Value::Void)),
        ("true", "(defn main () bool true)", Want::Bool(true)),
        ("false", "(defn main () bool false)", Want::Bool(false)),
        (
            "str",
            "(defn main () str \"h\u{e9}llo\")",
            Want::Str("h\u{e9}llo"),
        ),
    ] {
        let held = if matches!(want, Want::Bool(_)) {
            BOOL_GLOBAL_BYTES
        } else {
            0
        };
        kept(name, source, &want, held);
    }
}

#[test]
fn a_sequence_evaluates_in_order_and_drops_what_it_discards() {
    kept(
        "sequence",
        "(defn main () i32 (do \"dropped\" true 3i32 9i32))",
        &prim(Value::I32(9)),
        BOOL_GLOBAL_BYTES,
    );
    simple(
        "empty sequence",
        "(defn main () void (do))",
        &prim(Value::Void),
    );
}

#[test]
fn variables_are_read_from_parameters_and_bindings() {
    kept(
        "parameters and bindings",
        "(defn main () str (pick \"first\" \"second\" true))\n\
         (defn pick (a str b str flag bool) str\n\
         \x20 (let chosen (if flag a b))\n\
         \x20 (let again chosen)\n\
         \x20 again)",
        &Want::Str("first"),
        BOOL_GLOBAL_BYTES,
    );
}

#[test]
fn a_let_binds_for_the_rest_of_the_body_and_a_discard_is_dropped() {
    simple(
        "let",
        "(defn main () str\n  (let - \"unused\")\n  (let held \"kept\")\n  (let - \"also unused\")\n  held)",
        &Want::Str("kept"),
    );
}

#[test]
fn an_if_evaluates_only_the_selected_arm() {
    for (condition, expected) in [("true", 1), ("false", 2)] {
        kept(
            "if",
            &format!("(defn main () i32 (if {condition} 1i32 2i32))"),
            &prim(Value::I32(expected)),
            BOOL_GLOBAL_BYTES,
        );
    }
    // The arm that is not selected would call the host if it ran.
    differential(
        "if arms and the host",
        "(import io @std.io)\n\
         (defn main () i32\n\
         \x20 effects: (io.stdout)\n\
         \x20 (if false (do (let - (io.stdout.text \"never\\n\")) 1i32) 2i32))",
        &prim(Value::I32(2)),
        &[],
        BOOL_GLOBAL_BYTES,
    );
    // `true` and `false` are each read, and each is kept once.
    kept(
        "nested if",
        "(defn main () str (if (if true false true) \"a\" (if false \"b\" \"c\")))",
        &Want::Str("c"),
        2 * BOOL_GLOBAL_BYTES,
    );
}

#[test]
fn a_direct_call_passes_its_operands_and_returns_its_result() {
    simple(
        "direct call",
        "(defn main () str (let value (leaf \"x\" 4i32)) value)\n(defn leaf (a str n i32) str a)",
        &Want::Str("x"),
    );
}

#[test]
fn an_indirect_call_evaluates_its_callee_once_before_its_operands() {
    // The callee is built from a call that writes to the host: once, first.
    differential(
        "callee order",
        "(import io @std.io)\n\
         (defn main () i32\n\
         \x20 effects: (io.stdout)\n\
         \x20 (let r ((choose \"first\\n\") (say \"second\\n\")))\n\
         \x20 r)\n\
         (defn choose (text str) (fn (i32) i32)\n\
         \x20 effects: (io.stdout)\n\
         \x20 (let - (io.stdout.text text))\n\
         \x20 id)\n\
         (defn say (text str) i32\n\
         \x20 effects: (io.stdout)\n\
         \x20 (let - (io.stdout.text text))\n\
         \x20 4i32)\n\
         (defn id (n i32) i32 n)",
        &prim(Value::I32(4)),
        &[stdout_event("first\\n"), stdout_event("second\\n")],
        0,
    );
}

#[test]
fn a_module_function_used_as_a_value_is_called_through_its_wrapper() {
    simple(
        "function value",
        "(defn main () i32 (let f twice) (let r (f 21i32)) r)\n(defn twice (n i32) i32 n)",
        &prim(Value::I32(21)),
    );
}

#[test]
fn a_closure_reads_its_captures_and_its_parameters() {
    kept(
        "closure",
        "(defn main () str\n\
         \x20 (let first \"one\")\n\
         \x20 (let second \"two\")\n\
         \x20 (let f (lambda (flag bool) str (if flag first second)))\n\
         \x20 (let a (f true))\n\
         \x20 (let b (f false))\n\
         \x20 b)",
        &Want::Str("two"),
        2 * BOOL_GLOBAL_BYTES,
    );
    simple(
        "closure inside closure",
        "(defn main () i32\n\
         \x20 (let n 7i32)\n\
         \x20 (let outer (lambda () (fn () i32) (lambda () i32 n)))\n\
         \x20 (let inner (outer))\n\
         \x20 (let r (inner))\n\
         \x20 r)",
        &prim(Value::I32(7)),
    );
}

#[test]
fn return_leaves_the_activation_and_releases_what_it_holds() {
    // The `return` is not in tail position, so the checker admits it: the
    // activation ends with `held` and `other` still bound.
    kept(
        "return",
        "(defn main () str\n\
         \x20 (let held \"held\")\n\
         \x20 (let other \"other\")\n\
         \x20 (let - (if true (return held) other))\n\
         \x20 \"after\")",
        &Want::Str("held"),
        BOOL_GLOBAL_BYTES,
    );
    kept(
        "return in the middle",
        "(defn main () i32 (let - (leaf)) 1i32)\n\
         (defn leaf () str (let a \"a\") (let - (if true (return a) \"x\")) \"b\")",
        &prim(Value::I32(1)),
        BOOL_GLOBAL_BYTES,
    );
}

#[test]
fn a_module_value_is_evaluated_once_on_its_first_read() {
    simple(
        "scalar module value",
        "(def answer i32 41i32)\n(defn main () i32 (let a answer) (let b answer) b)",
        &prim(Value::I32(41)),
    );
    differential(
        "module value kept",
        "(def label str \"label\")\n(defn main () str (let a label) (let b label) b)",
        &Want::Str("label"),
        &[],
        STR_GLOBAL_BYTES,
    );
    // A `def` initializer cannot perform an effect (the effects chapter), so
    // "evaluated once" is not visible as a host event from source. It is
    // visible in the store: `true` is a module value of the standard library,
    // and reading it five times keeps one object, not five.
    kept(
        "a module value read often",
        "(defn main () i32\n\
         \x20 (let a (if true 1i32 2i32))\n\
         \x20 (let b (if true 1i32 2i32))\n\
         \x20 (let c (if true 1i32 2i32))\n\
         \x20 (let d (if true 1i32 2i32))\n\
         \x20 (if true 5i32 6i32))",
        &prim(Value::I32(5)),
        BOOL_GLOBAL_BYTES,
    );
    // A value nothing reads is not lowered, so it is never evaluated.
    let program =
        checked("(def used i32 1i32)\n(def unused i32 2i32)\n(defn main () i32 used)");
    let lowered = lower(&program).expect("lowers");
    assert_eq!(lowered.globals.len(), 1);
    assert_eq!(
        lowered
            .functions
            .iter()
            .flat_map(|function| &function.blocks)
            .filter(|block| matches!(block.term, Terminator::Global { .. }))
            .count(),
        1
    );
}

#[test]
fn a_host_operation_happens_once_with_its_operands_borrowed() {
    differential(
        "host",
        "(import io @std.io)\n\
         (defn main () (result void io.io-error)\n\
         \x20 effects: (io.stdout)\n\
         \x20 (let - (io.stdout.text \"one\\n\"))\n\
         \x20 (io.stdout.text \"two\\n\"))",
        &Want::Ok,
        &[stdout_event("one\\n"), stdout_event("two\\n")],
        0,
    );
}

#[test]
fn a_host_call_carries_the_origin_of_its_source_span() {
    let program = checked(
        "(import io @std.io)\n(defn main () (result void io.io-error)\n  effects: (io.stdout)\n  (io.stdout.text \"x\"))",
    );
    let lowered = lower(&program).expect("lowers");
    // The only host operation is the body of the standard library's wrapper
    // for `io.stdout-text`; its checked origin is the one the table must hold.
    let wanted: Vec<_> = program
        .functions()
        .iter()
        .filter_map(|function| match function.body() {
            vibra_ir::Expr::Host { origin, .. } => Some(origin.clone()),
            _ => None,
        })
        .collect();
    let mut found = Vec::new();
    for function in &lowered.functions {
        for block in &function.blocks {
            if let Terminator::Host { origin, .. } = &block.term {
                found.push(lowered.origin(*origin).expect("a numbered origin").clone());
            }
        }
    }
    assert_eq!(found.len(), 1);
    assert!(wanted.contains(&found[0]), "{found:?} not among {wanted:?}");
}

// -- tail calls ------------------------------------------------------------------

/// The `Callee` of every tail call in the program, and whether any function
/// ends a block in a `Call` whose result is returned unchanged.
fn tail_callees(program: &Program) -> Vec<Callee> {
    program
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .filter_map(|block| match &block.term {
            Terminator::TailCall { callee, .. } => Some(*callee),
            _ => None,
        })
        .collect()
}

#[test]
fn a_tail_call_lowers_to_a_tail_transfer_for_every_kind_of_callee() {
    // (source, tail transfers of the entry's chain that must be indirect)
    let kinds = [
        // A module function, called directly.
        (
            "direct",
            "(defn main () i32 (leaf 5i32))\n(defn leaf (n i32) i32 n)",
            0_usize,
        ),
        // A closure.
        ("closure", "(defn main () i32 ((lambda () i32 6i32)))", 1),
        // A function value held in a parameter.
        (
            "function value in a parameter",
            "(defn main () i32 (apply leaf 7i32))\n\
             (defn apply (f (fn (i32) i32) n i32) i32 (f n))\n\
             (defn leaf (n i32) i32 n)",
            1,
        ),
        // A module function as a value.
        (
            "module function as a value",
            "(defn main () i32 ((pick) 8i32))\n\
             (defn pick () (fn (i32) i32) leaf)\n\
             (defn leaf (n i32) i32 n)",
            1,
        ),
    ];
    for (name, source, indirect) in kinds {
        let program = checked(source);
        let lowered = lower(&program).expect("lowers");
        let callees = tail_callees(&lowered);
        let found = callees
            .iter()
            .filter(|callee| matches!(callee, Callee::Indirect(_)))
            .count();
        assert!(found >= indirect, "{name}: {callees:?}");
        // The entry's last act is a transfer, never a call that is then returned.
        let entry = lowered.function(lowered.entry).expect("entry");
        assert!(
            entry
                .blocks
                .iter()
                .any(|block| matches!(block.term, Terminator::TailCall { .. })),
            "{name}: the entry ends in a tail transfer"
        );
        // Every function that ends a block in a call and then returns its result
        // unchanged would have lost the tail position.
        for function in &lowered.functions {
            for block in &function.blocks {
                if let Terminator::Call { dst, resume, .. } = &block.term {
                    let next = function.blocks.get(resume.index()).expect("resume");
                    let passes_on = next.insts.is_empty()
                        && matches!(&next.term, Terminator::Return { value } if value == dst);
                    assert!(
                        !passes_on,
                        "{name}: a call in tail position stayed a call"
                    );
                }
            }
        }
        // And the transfer runs, in the result the interpreter gives.
        let interpreted = vibra_interp::Interpreter::run(&program).expect("runs");
        let ran = stepped(&lowered, 64 * MIB, UNBOUNDED);
        assert_eq!(ran.outcome, Outcome::Completed, "{name}");
        assert_eq!(
            interpreted.value().map(scalar_bits),
            Some(ran.result.bits),
            "{name}"
        );
        assert!(ran.stats.tail_transfers >= 1, "{name}");
        assert!(
            ran.stats.max_depth <= 2,
            "{name}: at most the entry and one helper"
        );
        assert_eq!(ran.live, 0, "{name}");
    }
}

/// A loop of `rounds` tail calls, with its counter held in `bits` bool
/// parameters. A function value, a closure, and a module function in a
/// parameter share the shape in `kind`.
///
/// The counter counts down in binary with only `if` over `bool`: bit `i`
/// flips when every lower bit is clear. The loop ends when every bit is clear.
pub(crate) fn countdown(rounds: u32, bits: usize, indirect: bool) -> String {
    let names: Vec<String> = (0..bits).map(|bit| format!("b{bit}")).collect();
    let mut source = String::new();
    let start: Vec<&str> = (0..bits)
        .map(|bit| {
            if (rounds >> bit) & 1 == 1 {
                "true"
            } else {
                "false"
            }
        })
        .collect();
    let _ = writeln!(source, "(defn main () i32 (count {}))", start.join(" "));
    let parameters: Vec<String> =
        names.iter().map(|name| format!("{name} bool")).collect();
    // `(if b0 false (if b1 false ... true))` is true when every bit is clear.
    let clear = |upto: usize| {
        let mut text = "true".to_owned();
        for name in names.iter().take(upto).rev() {
            text = format!("(if {name} false {text})");
        }
        text
    };
    let next: Vec<String> = names
        .iter()
        .enumerate()
        .map(|(bit, name)| format!("(if {} (if {name} false true) {name})", clear(bit)))
        .collect();
    let again = if indirect { "again" } else { "count" };
    if indirect {
        // The loop calls itself through a module value that holds it.
        let kinds = vec!["bool"; bits].join(" ");
        let _ = writeln!(source, "(def again (fn ({kinds}) i32) count)");
    }
    let _ = writeln!(
        source,
        "(defn count ({}) i32 (if {} 0i32 ({again} {})))",
        parameters.join(" "),
        clear(bits),
        next.join(" ")
    );
    source
}

#[test]
fn a_hundred_thousand_tail_calls_run_in_bounded_frames_and_live_size() {
    let bits = 17;
    let mut peaks = Vec::new();
    for rounds in [1_000_u32, 100_000] {
        let program = checked(&countdown(rounds, bits, false));
        let interpreted = vibra_interp::Interpreter::run(&program).expect("runs");
        assert_eq!(interpreted.value(), Some(&Value::I32(0)));
        let lowered = lower(&program).expect("lowers");
        for quota in [UNBOUNDED, 1] {
            let ran = stepped(&lowered, 64 * MIB, quota);
            assert_eq!(ran.outcome, Outcome::Completed, "{rounds} rounds");
            assert_eq!(ran.seen, Seen::Scalar(0), "{rounds} rounds");
            // The entry frame, replaced by `count`; and the frame of a `bool` module
            // value while it is first read.
            assert_eq!(ran.stats.max_depth, 2, "{rounds} rounds, quota {quota}");
            assert_eq!(
                ran.live,
                2 * BOOL_GLOBAL_BYTES,
                "{rounds} rounds, quota {quota}"
            );
            // One transfer for the entry into `count`, and one for each round.
            assert_eq!(
                ran.stats.tail_transfers,
                u64::from(rounds) + 1,
                "{rounds} rounds, quota {quota}"
            );
            assert_eq!(
                u64::try_from(interpreted.tail_transfer_count()).ok(),
                Some(ran.stats.tail_transfers),
                "the interpreter counts the same transfers"
            );
            if quota == UNBOUNDED {
                peaks.push(ran.peak);
            }
        }
    }
    assert_eq!(
        peaks[0], peaks[1],
        "nothing grows with the number of rounds"
    );
}

#[test]
fn a_hundred_thousand_tail_calls_through_a_function_value_stay_bounded() {
    let bits = 17;
    // Every round goes through a module value that holds the loop as a function
    // value, so each is an indirect transfer through the wrapper.
    let source = countdown(100_000, bits, true);
    let program = checked(&source);
    let interpreted = vibra_interp::Interpreter::run(&program).expect("runs");
    assert_eq!(interpreted.value(), Some(&Value::I32(0)));
    let lowered = lower(&program).expect("lowers");
    let ran = stepped(&lowered, 64 * MIB, UNBOUNDED);
    assert_eq!(ran.outcome, Outcome::Completed);
    assert_eq!(ran.seen, Seen::Scalar(0));
    assert!(ran.stats.max_depth <= 3, "{}", ran.stats.max_depth);
    // `true`, `false`, and `again`: a function object of three cells (a 24-byte
    // header and 24 bytes of cells, in a 64-byte block).
    assert_eq!(ran.live, 2 * BOOL_GLOBAL_BYTES + 64);
}

// -- the slice's edge ----------------------------------------------------------------

#[test]
fn a_form_outside_the_slice_is_not_lowered_and_names_the_form() {
    let tuple = "(defn main () i32 (let pair (tupleof 1i32 2i32)) 3i32)";
    let matched = "(defn main () i32 (match true true 1i32 false 2i32))";
    let external = "(import text @std.str)\n(defn main () u64 (str.length \"x\"))";
    for (name, source, form) in [
        ("tuple", tuple, "Tuple"),
        ("match", matched, "Match"),
        (
            "compiler intrinsic",
            external,
            vibra_lower::form::EXTERNAL_COLLECTION,
        ),
        // Reached only through a callee.
        (
            "tuple in a callee",
            "(defn main () i32 (helper))\n(defn helper () i32 (let pair (tupleof 1i32 2i32)) 3i32)",
            "Tuple",
        ),
        // Reached only through a function value.
        (
            "tuple behind a function value",
            "(defn main () i32 ((pick)))\n(defn pick () (fn () i32) helper)\n\
             (defn helper () i32 (let pair (tupleof 1i32 2i32)) 3i32)",
            "Tuple",
        ),
        // In a module value's initializer.
        (
            "tuple in a module value",
            "(def pair i32 (do (tupleof 1i32 2i32) 3i32))\n(defn main () i32 pair)",
            "Tuple",
        ),
        // In a closure body.
        (
            "tuple in a closure",
            "(defn main () i32 ((lambda () i32 (do (tupleof 1i32 2i32) 3i32))))",
            "Tuple",
        ),
        // A generic function.
        (
            "generic function",
            "(defn main () i32 (first 1i32 2i32))\n(defn first (a t b t) t\n  where: (t any)\n  a)",
            "generic activation",
        ),
    ] {
        let result = check_source("outside.vib", source);
        if !result.accepted() {
            // The source is only a vehicle for the form; a checker that does not
            // take it is reported, not hidden.
            panic!("{name}: {:?}", result.diagnostics());
        }
        let program = result.program().expect("program");
        assert_eq!(lower(program), Err(LowerError::NotLowered(form)), "{name}");
    }
    // A form that nothing reaches does not matter.
    let unreached = checked(
        "(defn main () i32 1i32)\n(defn elsewhere () i32 (let pair (tupleof 1i32 2i32)) 3i32)",
    );
    let lowered = lower(&unreached).expect("an unreached function is not lowered");
    assert_eq!(lowered.functions.len(), 1);
}

// -- verification and determinism ----------------------------------------------------

/// Every source of this file that the slice admits.
pub(crate) fn admitted_sources() -> Vec<(&'static str, String)> {
    vec![
        ("leaf", "(defn main () i32 7i32)".to_owned()),
        (
            "everything",
            "(import io @std.io)\n\
             (def label str \"label\")\n\
             (def count i32 3i32)\n\
             (defn main () (result void io.io-error)\n\
             \x20 effects: (io.stdout)\n\
             \x20 (let n count)\n\
             \x20 (let f (lambda (flag bool) str (if flag label \"other\")))\n\
             \x20 (let g twice)\n\
             \x20 (let a (f true))\n\
             \x20 (let b (g 2i32))\n\
             \x20 (let - (io.stdout.text a))\n\
             \x20 (let - (if (if true false true) (return (io.stdout.text \"early\")) (io.stdout.text \"late\")))\n\
             \x20 (io.stdout.text \"end\"))\n\
             (defn twice (n i32) i32 n)"
                .to_owned(),
        ),
        ("countdown", countdown(25, 6, false)),
        (
            "mutual",
            "(defn main () i32 (first true))\n\
             (defn first (flag bool) i32 (if flag (second false) 1i32))\n\
             (defn second (flag bool) i32 (if flag (first false) 2i32))"
                .to_owned(),
        ),
    ]
}

#[test]
fn every_lowered_program_verifies() {
    for (name, source) in admitted_sources() {
        let program = checked(&source);
        let lowered = lower(&program).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(verify(&lowered), Ok(()), "{name}\n{lowered}");
    }
    // And every single-file case of the conformance corpus: the slice admits
    // some of them, and whatever it admits must verify.
    let root = workspace_root().join("conformance").join("cases");
    let mut directories: Vec<_> = std::fs::read_dir(&root)
        .expect("the corpus")
        .map(|entry| entry.expect("an entry").path())
        .collect();
    directories.sort();
    let (mut lowered_count, mut refused, mut unchecked) = (0_usize, 0_usize, 0_usize);
    for directory in directories {
        let input = directory.join("input.vib");
        let Ok(source) = std::fs::read_to_string(&input) else {
            continue;
        };
        let result = check_source("corpus.vib", &source);
        let Some(program) = result.program().filter(|_| result.accepted()) else {
            unchecked += 1;
            continue;
        };
        match lower(program) {
            Ok(lowered) => {
                lowered_count += 1;
                assert_eq!(verify(&lowered), Ok(()), "{}\n{lowered}", input.display());
            }
            Err(LowerError::NotLowered(_)) => refused += 1,
            Err(other) => panic!("{}: {other}", input.display()),
        }
    }
    eprintln!(
        "corpus single-file cases: {lowered_count} lowered, {refused} refused, {unchecked} not run"
    );
}

/// The text of the lowering of a program that uses every form of the slice.
fn determinism_text() -> String {
    let (_, source) = admitted_sources()
        .into_iter()
        .find(|(name, _)| *name == "everything")
        .expect("the program");
    let program = checked(&source);
    format!("{}", lower(&program).expect("lowers"))
}

#[test]
fn lowering_is_deterministic() {
    const BEGIN: &str = "LOWERED-PROGRAM-BEGIN";
    const END: &str = "LOWERED-PROGRAM-END";
    if std::env::var_os("VIBRA_LOWER_DETERMINISM_CHILD").is_some() {
        println!("{BEGIN}\n{}\n{END}", determinism_text());
        return;
    }
    let source = admitted_sources()
        .into_iter()
        .find(|(name, _)| *name == "everything")
        .expect("the program")
        .1;
    let program = checked(&source);
    assert_eq!(
        lower(&program),
        lower(&program),
        "two lowerings of one program"
    );
    // A second checking of the same text is a second, separate program.
    assert_eq!(lower(&program), lower(&checked(&source)));

    // A second process lowers the same text and prints it.
    let exe = std::env::current_exe().expect("the test binary");
    let output = Command::new(exe)
        .args([
            "--exact",
            "lowering_slice::lowering_is_deterministic",
            "--nocapture",
        ])
        .env("VIBRA_LOWER_DETERMINISM_CHILD", "1")
        .output()
        .expect("a second process");
    assert!(output.status.success(), "{output:?}");
    let printed = String::from_utf8(output.stdout).expect("text");
    let between = printed
        .split_once(BEGIN)
        .and_then(|(_, rest)| rest.split_once(END))
        .map(|(text, _)| text.trim().to_owned())
        .expect("the child printed the program");
    assert_eq!(between, determinism_text().trim());
}

#[test]
fn frames_are_as_small_as_a_simple_allocator_gives() {
    // One temporary for the result.
    let leaf = lower(&checked("(defn main () i32 7i32)")).expect("lowers");
    assert_eq!(
        leaf.functions.iter().map(|f| f.slots).collect::<Vec<_>>(),
        [1]
    );
    // A parameter, and one temporary for the result: the variable is copied.
    let pass = lower(&checked(
        "(defn main () i32 (pass 1i32))\n(defn pass (n i32) i32 n)",
    ))
    .expect("lowers");
    let slots: Vec<u32> = pass.functions.iter().map(|f| f.slots).collect();
    // `main` holds the result temporary and the operand; `pass` its parameter
    // and the result.
    assert_eq!(slots, [2, 2]);
    // The wrapper of a module function used as a value is its parameter and the
    // environment, and nothing more.
    let wrapped = lower(&checked(
        "(defn main () i32 (let f leaf) (let r (f 3i32)) r)\n(defn leaf (n i32) i32 n)",
    ))
    .expect("lowers");
    assert!(
        wrapped
            .functions
            .iter()
            .any(|function| function.env && function.slots == function.params + 1)
    );
    assert!(
        wrapped
            .functions
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.insts)
            .any(|inst| matches!(inst, Inst::Function { .. }))
    );
}

#[test]
fn code_that_cannot_run_is_lowered_and_discarded() {
    use vibra_diagnostics::ByteSpan;
    use vibra_ir::{CheckedFunction, Expr, FunctionSignature, SourceOrigin, Type};
    let origin = || SourceOrigin::new("hand.vib", ByteSpan::new(0, 1));
    let literal = |value| Expr::literal(value, origin());
    // `(do (return 1) <after>)`: the checker refuses source like this, but
    // checked IR may hold it, and what follows can never run.
    let build = |after: Expr| {
        let body = Expr::sequence(
            vec![
                Expr::Return {
                    value: Box::new(literal(Value::Bool(true))),
                    origin: origin(),
                },
                after,
            ],
            origin(),
        );
        let entry = CheckedFunction::new(
            "main",
            FunctionSignature::new(Vec::new(), Type::Bool),
            body,
            origin(),
        )
        .expect("an entry");
        CheckedProgram::try_new(vec![entry], 0).expect("a program")
    };
    let program = build(literal(Value::Bool(false)));
    let lowered = lower(&program).expect("lowers");
    assert_eq!(verify(&lowered), Ok(()), "{lowered}");
    let ran = stepped(&lowered, 64 * MIB, UNBOUNDED);
    assert_eq!(ran.seen, Seen::Enum(1));
    assert_eq!(ran.live, 0);
    // A form outside the slice there is still found.
    let after = Expr::external(
        vibra_ir::external::CompilerIntrinsic::StrEqual,
        vec![
            literal(Value::Str("a".to_owned())),
            literal(Value::Str("b".to_owned())),
        ],
        origin(),
    );
    let program = build(after);
    assert_eq!(
        lower(&program),
        Err(LowerError::NotLowered(
            vibra_lower::form::EXTERNAL_COLLECTION
        ))
    );
}
