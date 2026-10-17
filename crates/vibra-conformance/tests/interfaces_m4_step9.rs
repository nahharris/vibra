//! Interfaces in the WebAssembly backend (milestone 4 Step 9): interface
//! values, contract dispatch from run-time types, default members, members
//! selected by their destination, the closed conformances, `iter`, and dicts
//! keyed by a type that needs run-time dispatch.
//!
//! The reference interpreter is the oracle. A program is checked from source,
//! run by both backends, and held to one result; a program that runs also
//! returns the live size to its baseline.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    missing_docs
)]

use vibra_ir::{CheckedProgram, Type};
use vibra_types::check_source;
use vibra_wasm_run::{LiveSizes, MemoryLimit, Observed, Runner};

const MIB: usize = 1024 * 1024;

fn checked(source: &str) -> CheckedProgram {
    let result = check_source("input.vib", source);
    assert!(result.accepted(), "{:?}\n{source}", result.diagnostics());
    result.program().expect("an accepted program").clone()
}

fn entry_type(program: &CheckedProgram) -> Type {
    program.entry().signature().result()
}

fn oracle(program: &CheckedProgram) -> String {
    vibra_interp::run(program)
        .expect("the interpreter runs the program")
        .canonical_result()
}

fn run_with(runner: &Runner, program: &CheckedProgram) -> (String, LiveSizes) {
    let ty = entry_type(program);
    let module = vibra_wasm::emit(program)
        .unwrap_or_else(|error| panic!("the program lowers: {error}"));
    let heads = |head: u32| module.descriptors().type_of(head).cloned();
    match runner
        .run_observed_with(module.bytes(), &ty, program.types(), &heads)
        .expect("the module runs")
    {
        Observed::Completed { value, live } => (value.canonical_observation(&ty), live),
        Observed::Stopped(outcome) => panic!("the entry did not complete: {outcome:?}"),
    }
}

/// Runs `source` in both backends and asserts they agree.
fn agree(source: &str) -> (String, LiveSizes) {
    let program = checked(source);
    let expected = oracle(&program);
    let runner =
        Runner::new(MemoryLimit::new(64 * MIB)).expect("the engine configures");
    let (observed, live) = run_with(&runner, &program);
    assert_eq!(observed, expected, "the backends disagree on:\n{source}");
    (observed, live)
}

#[test]
fn a_value_widened_to_an_interface_is_passed_along() {
    let (result, _) =
        agree("(defn main () i32 (keep 7i32))\n(defn keep (value any) i32 1i32)");
    assert_eq!(result, "(record type: @i32 value: 1i32)\n");
}

#[test]
fn a_contract_member_is_selected_by_the_static_receiver_type() {
    let (result, _) = agree(
        "(defn main () str (shape.name 7i32))\n\
         (defint shape (defn name (value self) str) (impl i32 (defn name (value self) str \"integer\")))",
    );
    assert_eq!(result, "(record type: @str value: \"integer\")\n");
}

#[test]
fn a_contract_member_is_selected_through_an_interface_value() {
    let (result, _) = agree(
        "(defn main () str (shape.name (as shape 7i32)))\n\
         (defint shape (defn name (value self) str) (impl i32 (defn name (value self) str \"integer\")))",
    );
    assert_eq!(result, "(record type: @str value: \"integer\")\n");
}
