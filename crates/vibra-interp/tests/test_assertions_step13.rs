//! Focused regression tests for the pure M2 assertion execution path.

#![allow(clippy::expect_used, clippy::indexing_slicing)]

use vibra_diagnostics::ByteSpan;
use vibra_interp::Interpreter;
use vibra_ir::{
    CheckedFunction, CheckedProgram, Expr, FunctionSignature, PrimitiveType,
    SourceOrigin, TestAssertion, Value,
};

fn origin() -> SourceOrigin {
    SourceOrigin::new("tests/assertions.vib", ByteSpan::new(10, 25))
}

fn program(assertion: TestAssertion, operands: Vec<Value>) -> CheckedProgram {
    let assertion_origin =
        SourceOrigin::new("stdlib/m2/src/std/assert.vib", ByteSpan::empty_at(0));
    let assertion_function = CheckedFunction::new_test_assertion(
        assertion.symbol(),
        assertion,
        assertion_origin,
    )
    .expect("closed assertion function");
    let call_origin = origin();
    let arguments = operands
        .into_iter()
        .map(|value| Expr::literal(value, call_origin.clone()))
        .collect();
    let body = Expr::call(0, arguments, PrimitiveType::Void, call_origin.clone());
    let test = CheckedFunction::new(
        "@tests.assertions::\"example\"",
        FunctionSignature::new(Vec::new(), PrimitiveType::Void),
        body,
        call_origin,
    )
    .expect("void test body");
    CheckedProgram::try_new(vec![assertion_function, test], 1)
        .expect("checked test program")
}

#[test]
fn passing_assertion_is_a_void_test_completion() {
    let execution = Interpreter::run_test(&program(
        TestAssertion::EqualI32,
        vec![Value::I32(7), Value::I32(7)],
    ))
    .expect("test execution");

    assert!(execution.assertion_failure().is_none());
    assert!(execution.audit_trace().is_empty());
}

#[test]
fn false_assertion_is_structured_and_carries_the_call_origin() {
    let execution = Interpreter::run_test(&program(
        TestAssertion::EqualStr,
        vec![
            Value::Str("expected".to_owned()),
            Value::Str("actual".to_owned()),
        ],
    ))
    .expect("test execution");
    let failure = execution.assertion_failure().expect("assertion failure");

    assert_eq!(failure.assertion(), "@std.assert.equal-str");
    assert_eq!(failure.expected(), &Value::Str("expected".to_owned()));
    assert_eq!(failure.actual(), &Value::Str("actual".to_owned()));
    assert_eq!(failure.origin().source_id(), "tests/assertions.vib");
    assert_eq!(failure.origin().span(), ByteSpan::new(10, 25));
    assert!(execution.audit_trace().is_empty());
}

#[test]
fn ordinary_run_rejects_invoking_test_assertions() {
    let checked = program(TestAssertion::True, vec![Value::Bool(true)]);

    assert!(matches!(
        Interpreter::run(&checked),
        Err(vibra_interp::RuntimeError::InvalidBody { .. })
    ));
}

#[test]
fn ordinary_run_allows_unused_test_assertion_markers() {
    let assertion = CheckedFunction::new_test_assertion(
        TestAssertion::True.symbol(),
        TestAssertion::True,
        SourceOrigin::new("stdlib/m2/src/std/assert.vib", ByteSpan::empty_at(0)),
    )
    .expect("closed assertion function");
    let origin = origin();
    let entry = CheckedFunction::new(
        "@demo@0.1.0/app.main.execute",
        FunctionSignature::new(Vec::new(), PrimitiveType::Void),
        Expr::literal(Value::Void, origin.clone()),
        origin,
    )
    .expect("void entry");
    let checked = CheckedProgram::try_new(vec![assertion, entry], 1)
        .expect("program with unused assertion member");

    assert!(Interpreter::run(&checked).is_ok());
}

#[test]
fn assertion_failure_stops_nested_helper_arguments_and_later_calls() {
    let assertion_origin =
        SourceOrigin::new("stdlib/m2/src/std/assert.vib", ByteSpan::empty_at(0));
    let assertion = CheckedFunction::new_test_assertion(
        TestAssertion::EqualStr.symbol(),
        TestAssertion::EqualStr,
        assertion_origin,
    )
    .expect("closed assertion function");
    let first_call_origin =
        SourceOrigin::new("tests/assertions.vib", ByteSpan::new(20, 43));
    let first_assertion = Expr::call(
        0,
        vec![
            Expr::literal(
                Value::Str("first-expected".to_owned()),
                first_call_origin.clone(),
            ),
            Expr::literal(
                Value::Str("first-actual".to_owned()),
                first_call_origin.clone(),
            ),
        ],
        PrimitiveType::Void,
        first_call_origin.clone(),
    );
    let helper = CheckedFunction::new(
        "@demo@0.1.0/tests.assertions.helper",
        FunctionSignature::new(Vec::new(), PrimitiveType::Str),
        Expr::sequence(
            vec![
                first_assertion,
                Expr::literal(
                    Value::Str("helper-result".to_owned()),
                    first_call_origin.clone(),
                ),
            ],
            first_call_origin.clone(),
        ),
        first_call_origin.clone(),
    )
    .expect("string helper");
    let sink = CheckedFunction::new(
        "@demo@0.1.0/tests.assertions.sink",
        FunctionSignature::new(vec![PrimitiveType::Str], PrimitiveType::Void),
        Expr::literal(Value::Void, first_call_origin.clone()),
        first_call_origin.clone(),
    )
    .expect("void sink");
    let later_call_origin =
        SourceOrigin::new("tests/assertions.vib", ByteSpan::new(60, 80));
    let sink_call = Expr::call(
        2,
        vec![Expr::call(
            1,
            Vec::new(),
            PrimitiveType::Str,
            first_call_origin.clone(),
        )],
        PrimitiveType::Void,
        first_call_origin,
    );
    let later_assertion = Expr::call(
        0,
        vec![
            Expr::literal(
                Value::Str("later-expected".to_owned()),
                later_call_origin.clone(),
            ),
            Expr::literal(
                Value::Str("later-actual".to_owned()),
                later_call_origin.clone(),
            ),
        ],
        PrimitiveType::Void,
        later_call_origin.clone(),
    );
    let test = CheckedFunction::new(
        "@tests.assertions::\"nested\"",
        FunctionSignature::new(Vec::new(), PrimitiveType::Void),
        Expr::sequence(vec![sink_call, later_assertion], later_call_origin.clone()),
        later_call_origin,
    )
    .expect("void test");
    let program = CheckedProgram::try_new(vec![assertion, helper, sink, test], 3)
        .expect("checked nested test program");

    let execution = Interpreter::run_test(&program).expect("test execution");
    let failure = execution
        .assertion_failure()
        .expect("first assertion failure");

    assert_eq!(failure.expected(), &Value::Str("first-expected".to_owned()));
    assert_eq!(failure.actual(), &Value::Str("first-actual".to_owned()));
    assert_eq!(failure.origin().span(), ByteSpan::new(20, 43));
}
