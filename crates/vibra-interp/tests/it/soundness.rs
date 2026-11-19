//! The interpreter's side of the review of `vibra-ir` and `vibra-interp`:
//! memory reserved before it is used, and neutral values validated as they
//! enter.

use vibra_interp::{
    Evaluation, Limit, Limits, MemoryBudget, RuntimeError, evaluate, run,
};
use vibra_ir::{Type, Value};

use crate::{BASE, function, program, set_with, signature, str_type, text, var, void};

/// A budget of 64 KiB.
const SMALL: Limits = Limits {
    reductions: 1_000_000,
    memory: MemoryBudget::new(64 * 1024),
};

/// A string of 1 MiB.
fn large_text() -> String {
    "x".repeat(1 << 20)
}

// -- V2: the new value counts --------------------------------------------------

/// A literal larger than the budget is not dropped from the memory check
/// when the machine measures its roots again.
#[test]
fn v2_a_literal_larger_than_the_budget_exhausts_it() {
    let set = set_with(Vec::new(), Vec::new(), Vec::new());
    let outcome = evaluate(&set, &text(&large_text()).expr, &[], SMALL);
    assert_eq!(outcome, Evaluation::LimitReached(Limit::Memory));
    let fits = evaluate(&set, &text("small").expr, &[], SMALL);
    assert_eq!(fits, Evaluation::Value(Value::Str("small".to_owned())));
}

/// An input read lazily is accounted before it is converted.
#[test]
fn v2_an_input_larger_than_the_budget_exhausts_it() {
    let set = set_with(Vec::new(), Vec::new(), Vec::new());
    let input = Value::Str(large_text());
    let outcome = evaluate(&set, &var(0, str_type()).expr, &[input], SMALL);
    assert_eq!(outcome, Evaluation::LimitReached(Limit::Memory));
    let small = Value::Str("small".to_owned());
    let fits = evaluate(
        &set,
        &var(0, str_type()).expr,
        std::slice::from_ref(&small),
        SMALL,
    );
    assert_eq!(fits, Evaluation::Value(small));
}

/// A compound input is reserved as a whole before any part is built.
#[test]
fn v2_a_compound_input_larger_than_the_budget_exhausts_it() {
    let set = set_with(Vec::new(), Vec::new(), Vec::new());
    let input = Value::List(vec![Value::Str("y".repeat(1024)); 1024]);
    let outcome = evaluate(&set, &var(0, Type::list(str_type())).expr, &[input], SMALL);
    assert_eq!(outcome, Evaluation::LimitReached(Limit::Memory));
}

// -- V10: slots are reserved before they exist ---------------------------------

/// A function with `slot_count` slots and a `Void` body.
fn wide(slot_count: usize) -> vibra_ir::CheckedFunction {
    function(
        "wide",
        signature(Vec::new(), Type::Void),
        void(),
        slot_count,
    )
}

/// The largest slot count validates without allocating, and the run that
/// would materialize it exhausts its budget instead of the host.
#[test]
fn v10_the_largest_slot_count_validates_and_exhausts_the_budget() {
    let set = set_with(vec![wide(usize::MAX)], Vec::new(), Vec::new());
    let error = run(&program(set, BASE), MemoryBudget::new(1 << 20)).unwrap_err();
    assert!(matches!(error, RuntimeError::MemoryExhausted { .. }));
}

/// A representable slot count is reserved against the budget before the
/// frame is built.
#[test]
fn v10_a_frame_larger_than_the_budget_is_not_built() {
    let set = set_with(vec![wide(1 << 20)], Vec::new(), Vec::new());
    let error = run(&program(set, BASE), MemoryBudget::new(1 << 20)).unwrap_err();
    assert!(matches!(error, RuntimeError::MemoryExhausted { .. }));
    let set = set_with(vec![wide(16)], Vec::new(), Vec::new());
    assert!(run(&program(set, BASE), MemoryBudget::new(1 << 20)).is_ok());
}
