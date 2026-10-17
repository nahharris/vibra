//! The test matrix of Step 9: the positive, boundary, negative, and balance
//! cases of interface values and contract dispatch in the WebAssembly backend.
//! The reference interpreter is the oracle.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    missing_docs
)]

use vibra_ir::{CheckedProgram, Type};
use vibra_types::check_source;
use vibra_wasm_run::{LiveSizes, MemoryLimit, Observed, Outcome, Runner};

const MIB: usize = 1024 * 1024;

fn checked(source: &str) -> CheckedProgram {
    let result = check_source("input.vib", source);
    assert!(result.accepted(), "{:?}\n{source}", result.diagnostics());
    result.program().expect("an accepted program").clone()
}

fn entry_type(program: &CheckedProgram) -> Type {
    program.entry().signature().result()
}

fn run_limited(source: &str, limit: usize) -> Observed {
    let program = checked(source);
    let ty = entry_type(&program);
    let module = vibra_wasm::emit(&program).expect("the program lowers");
    let heads = |head: u32| module.descriptors().type_of(head).cloned();
    Runner::new(MemoryLimit::new(limit))
        .expect("the engine configures")
        .run_observed_with(module.bytes(), &ty, program.types(), &heads)
        .expect("the module runs")
}

/// Runs `source` in both backends and asserts they agree.
fn agree(source: &str) -> (String, LiveSizes) {
    let program = checked(source);
    let expected = vibra_interp::run(&program)
        .expect("the interpreter runs the program")
        .canonical_result();
    let ty = entry_type(&program);
    match run_limited(source, 64 * MIB) {
        Observed::Completed { value, live } => {
            let observed = value.canonical_observation(&ty);
            assert_eq!(observed, expected, "the backends disagree on:\n{source}");
            (observed, live)
        }
        Observed::Stopped(outcome) => panic!("the entry did not complete: {outcome:?}"),
    }
}

const SHAPES: &str = "(defint shape (defn name (value self) str) (defn twice (value self) str (shape.name value)) (impl i32 (defn name (value self) str \"integer\")))\n\
(deftype point (record x i32) (impl shape (defn name (value self) str \"point\")))\n\
(deftype box (record item t) where: (t any) (impl shape (defn name (value self) str \"box\")))\n";

#[test]
fn a_receiver_implementing_a_generic_interface_twice_answers_at_the_call_arguments() {
    let (result, _) = agree(
        "(defn main () (tuple str str) (tupleof (tagged.label (as (tagged str) (both n: 1i32))) (tagged.twice (as (tagged i32) (both n: 1i32)))))\n\
(defint tagged where: (item any) (defn label (value self) str) (defn twice (value self) str (tagged.label value)))\n\
(deftype both (record n i32) (impl (tagged i32) (defn label (value self) str \"int\")) (impl (tagged str) (defn label (value self) str \"str\")))",
    );
    assert!(result.contains("\"str\" \"int\""), "{result}");
}

#[test]
fn a_default_member_calls_a_contract_member_at_the_receiver_type() {
    let (result, _) = agree(&format!(
        "(defn main () (tuple str str str) (tupleof (shape.twice (point x: 1i32)) (shape.twice 3i32) (shape.twice (box item: \"x\"))))\n{SHAPES}"
    ));
    assert!(result.contains("\"point\" \"integer\" \"box\""), "{result}");
}

#[test]
fn an_interface_value_is_stored_in_a_record_an_array_and_a_dict() {
    agree(&format!(
        "(defn main () (tuple str str str) (tupleof (shape.name ((holder item: 3i32) @item)) (pick ((array.of (as shape (point x: 1i32)) (as shape 2i32)) 1u64)) (pick ((dict.of \"k\" (as shape (point x: 0i32))) \"k\"))))\n(defn pick (found (option shape)) str (match found (option.some inner) (shape.name inner) (option.none) \"none\"))\n(deftype holder (record item shape))\n{SHAPES}"
    ));
}

#[test]
fn a_receiver_of_a_generic_type_dispatches() {
    agree(&format!(
        "(defn main () str (name-of (box item: \"x\")))\n(defn name-of (value (box t)) str where: (t any) (shape.name value))\n{SHAPES}"
    ));
}

#[test]
fn iter_over_an_empty_collection_and_over_many_items_agrees() {
    let (result, _) = agree(
        "(defn main () (tuple (array i32) (array u64)) (tupleof (iter.collect (as (array i32) (array.of))) (iter.collect (iter.take (iter.skip (naturals from: 0u64) 3000u64) 3u64))))\n(import iter @std.iter.iter)\n\
(deftype naturals (record from u64) (impl (iter u64) (defn next (value self) (option (tuple u64 self)) (option.some (tupleof (value @from) (naturals from: (match (u64.add-checked (value @from) 1u64) (result.ok h) h (result.err -) 0u64)))))))",
    );
    assert!(result.contains("3000u64"), "{result}");
}

fn tail_source(n: u32, tail: bool) -> String {
    let call = "(walker.walk (as walker (counter left: (lower (value @left)))))";
    let step = if tail {
        call.to_owned()
    } else {
        format!("(do {call} 7u64)")
    };
    format!(
        "(defn main () u64 (walker.walk (as walker (counter left: {n}u64))))\n\
(defint walker (defn walk (value self) u64))\n\
(deftype counter (record left u64) (impl walker (defn walk (value self) u64 (if (u64.equal (value @left) 0u64) 7u64 {step}))))\n\
(defn lower (c u64) u64 (match (u64.sub-checked c 1u64) (result.ok v) v (result.err -) 0u64))"
    )
}

#[test]
fn a_tail_contract_call_reuses_the_frame_and_a_non_tail_one_does_not() {
    let limit = 256 * 1024;
    let done = run_limited(&tail_source(100_000, true), limit);
    assert!(matches!(done, Observed::Completed { .. }), "{done:?}");
    let control = run_limited(&tail_source(100_000, false), limit);
    assert!(
        matches!(control, Observed::Stopped(Outcome::MemoryExhausted)),
        "{control:?}"
    );
}

#[test]
fn dispatch_through_a_loop_returns_the_live_size_to_its_baseline() {
    let (_, small) = agree(&tail_source(100, true));
    let (_, large) = agree(&tail_source(5_000, true));
    assert_eq!(small.start, 0);
    assert_eq!(large.end, small.end, "live size returns to its baseline");
}

#[test]
fn a_function_hidden_behind_an_interface_that_reaches_the_result_is_the_trap() {
    let outcome = run_limited(
        "(defn main () any (as any (lambda (x i32) i32 x)))",
        64 * MIB,
    );
    assert!(
        matches!(
            outcome,
            Observed::Stopped(Outcome::Trapped {
                code: vibra_ir::boundary::TrapCode::UnobservableFunction,
                ..
            })
        ),
        "{outcome:?}"
    );
}
