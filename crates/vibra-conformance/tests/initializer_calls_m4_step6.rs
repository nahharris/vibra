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
fn an_initializer_calls_with_nested_labelled_and_variadic_operands() {
    let source = "(def nested i32 (one (one 2i32)))\n\
(def defaulted i32 (pick 4i32))\n\
(def labelled i32 (pick 5i32 extra: 10i32))\n\
(def counted u64 (count 1i32 2i32 3i32))\n\
(defn main () (tuple i32 i32 i32 u64) (tupleof nested defaulted labelled counted))\n\
(defn one (n i32) i32 n)\n\
(defn pick (n i32) i32\n  labelled: (extra i32 7i32)\n  extra)\n\
(defn count () u64\n  variadic: (items (array i32))\n  (array.length items))\n";
    let checked = check_source("input.vib", source);
    assert!(checked.accepted(), "{:?}", checked.diagnostics());
    let execution = vibra_interp::run(checked.program().expect("a program"))
        .expect("the interpreter runs the program");
    assert_eq!(
        execution.canonical_result(),
        "(record type: (record type: @tuple arguments: (array @i32 @i32 @i32 @u64)) value: (record kind: @tuple values: (array 2i32 7i32 10i32 3u64)))\n"
    );
}

#[test]
fn an_initializer_calls_a_lambda_a_function_value_and_a_method() {
    let source = "(def by-lambda i32 ((lambda (n i32) i32 n) 1i32))\n\
(def by-value i32 (apply one 2i32))\n\
(def by-method i32 (box.get (box n: 3i32)))\n\
(defn main () (tuple i32 i32 i32) (tupleof by-lambda by-value by-method))\n\
(defn one (n i32) i32 n)\n\
(defn apply (f (fn (i32) i32) value i32) i32 (f value))\n\
(deftype box (record n i32)\n  (defn get (value self) i32 (value @n)))\n";
    let checked = check_source("input.vib", source);
    assert!(checked.accepted(), "{:?}", checked.diagnostics());
    let execution = vibra_interp::run(checked.program().expect("a program"))
        .expect("the interpreter runs the program");
    assert_eq!(
        execution.canonical_result(),
        "(record type: (record type: @tuple arguments: (array @i32 @i32 @i32)) value: (record kind: @tuple values: (array 1i32 2i32 3i32)))\n"
    );
}

#[test]
fn the_first_function_of_a_source_is_its_entry_and_an_entry_takes_no_operands() {
    // The `InvalidBody` Step 5b saw: the entry here is `one`, which has an
    // operand, so there is nothing to call it with. It is not the initializer.
    let source =
        "(def v i32 (one 1i32))\n(defn one (n i32) i32 n)\n(defn main () i32 v)\n";
    let checked = check_source("input.vib", source);
    assert!(checked.accepted(), "{:?}", checked.diagnostics());
    let program = checked.program().expect("a program");
    assert_eq!(program.entry().name(), "one");
    assert!(matches!(
        vibra_interp::run(program),
        Err(vibra_interp::RuntimeError::InvalidBody { .. })
    ));
}

#[test]
fn an_initializer_calls_through_a_binding() {
    let source = "(def v i32 (do (let t (one 1i32)) t))\n(defn main () i32 v)\n(defn one (n i32) i32 n)\n";
    assert_eq!(run(source), Value::I32(1));
}
