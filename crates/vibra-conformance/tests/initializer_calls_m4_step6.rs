//! A module value's initializer is an activation like any other, so it may call
//! a function with operands (`docs/spec/06-runtime.md`, "Evaluation",
//! "Module-value initialization"). Found by Step 5b: the reference interpreter
//! reported `InvalidBody` for such an initializer.

#![allow(clippy::expect_used, missing_docs)]

use vibra_ir::Value;
use vibra_types::check_source;

fn run(source: &str) -> Value {
    let checked = check_source("input.vib", source);
    assert!(checked.accepted(), "{:?}\n{source}", checked.diagnostics());
    vibra_interp::run(checked.program().expect("a program"))
        .expect("the interpreter runs the program")
        .value()
        .cloned()
        .expect("a primitive result")
}

#[test]
fn an_initializer_calls_a_function_with_an_operand() {
    let source =
        "(def v i32 (one 1i32))\n(defn main () i32 v)\n(defn one (n i32) i32 n)\n";
    assert_eq!(run(source), Value::I32(1));
}

#[test]
fn an_initializer_calls_through_a_binding() {
    let source = "(def v i32 (do (let t (one 1i32)) t))\n(defn main () i32 v)\n(defn one (n i32) i32 n)\n";
    assert_eq!(run(source), Value::I32(1));
}
