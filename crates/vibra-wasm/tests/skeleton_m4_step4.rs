//! The emitter's skeleton (milestone 4 Step 4): it lowers the empty `void`
//! entry, names every other form, and never produces a module that omits part
//! of a program.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use vibra_diagnostics::ByteSpan;
use vibra_ir::{
    CallTarget, CheckedFunction, CheckedGlobal, CheckedProgram, Expr,
    FunctionSignature, SourceOrigin, TestAssertion, Type, Value,
};
use vibra_wasm::{NotLowered, emit};

fn origin() -> SourceOrigin {
    SourceOrigin::new("skeleton.vib", ByteSpan::new(0, 1))
}

fn function(
    name: &str,
    parameters: Vec<Type>,
    result: Type,
    body: Expr,
) -> CheckedFunction {
    CheckedFunction::new(
        name,
        FunctionSignature::new(parameters, result),
        body,
        origin(),
    )
    .expect("a checked function")
}

fn void_literal() -> Expr {
    Expr::literal(Value::Void, origin())
}

fn empty_function(name: &str) -> CheckedFunction {
    function(name, Vec::new(), Type::Void, void_literal())
}

fn program(functions: Vec<CheckedFunction>, entry: usize) -> CheckedProgram {
    CheckedProgram::try_new(functions, entry).expect("a checked program")
}

fn not_lowered(program: &CheckedProgram) -> NotLowered {
    emit(program).expect_err("the program is not lowered")
}

fn forms(error: &NotLowered) -> Vec<&'static str> {
    error
        .forms()
        .iter()
        .map(|used| used.form().name())
        .collect()
}

#[test]
fn the_empty_entry_lowers_to_a_module() {
    let module = emit(&program(vec![empty_function("done")], 0)).expect("lowered");
    assert!(
        module.bytes().starts_with(b"\0asm\x01\0\0\0"),
        "a binary module"
    );
    assert!(
        module.origins().is_empty(),
        "an empty entry has no origin to name"
    );
    assert_eq!(module.origins().origin(0), None);
    assert_eq!(module.origins().origin(1), None);
}

#[test]
fn a_sequence_of_void_literals_is_the_empty_entry_too() {
    // `(do)` checks to a sequence whose only expression is the `void` literal.
    for expressions in [
        Vec::new(),
        vec![void_literal()],
        vec![void_literal(), void_literal()],
    ] {
        let body = Expr::Sequence {
            expressions,
            origin: origin(),
        };
        let module = emit(&program(
            vec![function("done", Vec::new(), Type::Void, body)],
            0,
        ));
        assert!(module.is_ok(), "{module:?}");
    }
}

/// An array built from no element, which Step 8b lowers.
fn empty_array() -> Expr {
    Expr::Array {
        value_type: Type::Array(Box::new(Type::U64)),
        elements: Vec::new(),
        origin: origin(),
    }
}

/// A read of a parameter of interface type, which Step 9 lowers.
fn interface_read() -> Expr {
    Expr::variable(0, Type::Any, origin())
}

#[test]
fn an_array_lowers_since_step_8b() {
    let body = Expr::Sequence {
        expressions: vec![empty_array(), void_literal()],
        origin: origin(),
    };
    let f = function("seq", Vec::new(), Type::Void, body);
    assert!(emit(&program(vec![f], 0)).is_ok());
}

#[test]
fn a_sequence_adds_nothing_to_what_its_expressions_need() {
    let body = Expr::Sequence {
        expressions: vec![interface_read(), void_literal()],
        origin: origin(),
    };
    let f = function("seq", vec![Type::Any], Type::Void, body);
    // A read of an interface value lowers since Step 9, so the sequence does.
    assert!(emit(&program(vec![empty_function("entry"), f], 0)).is_ok());
}

#[test]
fn emission_is_byte_identical_across_runs() {
    let a = emit(&program(vec![empty_function("done")], 0)).expect("lowered");
    let b = emit(&program(vec![empty_function("done")], 0)).expect("lowered");
    assert_eq!(a.bytes(), b.bytes());
    assert_eq!(a, b);
}

#[test]
fn the_module_does_not_depend_on_function_names_or_origins() {
    // Names and source origins are not part of the bytes: no custom section, so
    // no `name` section, and no source map before Milestone 7.
    let a = emit(&program(vec![empty_function("one")], 0)).expect("lowered");
    let renamed = function("two", Vec::new(), Type::Void, void_literal());
    let b = emit(&program(vec![renamed], 0)).expect("lowered");
    assert_eq!(a.bytes(), b.bytes());
}

#[test]
fn every_function_is_lowered_and_the_entry_is_the_one_exported() {
    let one = emit(&program(vec![empty_function("a")], 0)).expect("lowered");
    let two = emit(&program(vec![empty_function("a"), empty_function("b")], 1))
        .expect("lowered");
    assert_ne!(
        one.bytes(),
        two.bytes(),
        "one function per source function, and the entry index reaches the module"
    );
    let first = emit(&program(vec![empty_function("a"), empty_function("b")], 0))
        .expect("lowered");
    assert_ne!(first.bytes(), two.bytes(), "the entry names its function");
}

/// A function with a variadic tail, which Step 8b lowers: the tail is one more
/// slot, which a call fills with the array it already packed.
fn variadic_function(name: &str) -> CheckedFunction {
    CheckedFunction::new(
        name,
        FunctionSignature::new(Vec::new(), Type::Void)
            .with_variadic(Type::Array(Box::new(Type::U64))),
        void_literal(),
        origin(),
    )
    .expect("a checked function")
}

/// A function with a parameter of interface type, which Step 9 lowers.
fn interface_function(name: &str) -> CheckedFunction {
    function(name, vec![Type::Any], Type::Void, void_literal())
}

/// A verified test assertion, which Step 11 lowers and nothing before does.
fn assertion_function(name: &str) -> CheckedFunction {
    CheckedFunction::new_test_assertion(name, TestAssertion::True, origin())
        .expect("a closed assertion function")
}

#[test]
fn a_variadic_and_an_interface_parameter_lower_and_a_test_assertion_is_named() {
    assert!(
        emit(&program(
            vec![empty_function("entry"), variadic_function("f")],
            0
        ))
        .is_ok()
    );
    assert!(
        emit(&program(
            vec![empty_function("entry"), interface_function("f")],
            0
        ))
        .is_ok()
    );
    let error = not_lowered(&program(
        vec![empty_function("entry"), assertion_function("f")],
        0,
    ));
    assert_eq!(forms(&error), ["test-assertion"]);
    assert_eq!(
        error.forms()[0].origin().map(SourceOrigin::source_id),
        Some("skeleton.vib")
    );
    let fixed = function("g", vec![Type::U64], Type::Void, void_literal());
    assert!(emit(&program(vec![empty_function("entry"), fixed], 0)).is_ok());
}

#[test]
fn a_literal_result_lowers_since_step_5a() {
    let f = function(
        "answer",
        Vec::new(),
        Type::I32,
        Expr::literal(Value::I32(1), origin()),
    );
    assert!(emit(&program(vec![f], 0)).is_ok());
}

#[test]
fn a_binding_lowers_since_step_5b() {
    let body = Expr::Let {
        slot: None,
        value: Box::new(void_literal()),
        body: Box::new(void_literal()),
        origin: origin(),
    };
    let f = CheckedFunction::with_slots(
        "bind",
        FunctionSignature::new(Vec::new(), Type::Void),
        body,
        origin(),
        0,
    )
    .expect("a checked function");
    assert!(emit(&program(vec![f], 0)).is_ok());
}

#[test]
fn a_direct_call_lowers_in_and_out_of_tail_position() {
    let call = |tail| Expr::Call {
        target: CallTarget::Direct(1),
        arguments: Vec::new(),
        result: Type::Void,
        tail,
        origin: origin(),
    };
    let lowered = function("caller", Vec::new(), Type::Void, call(false));
    assert!(emit(&program(vec![lowered, empty_function("callee")], 0)).is_ok());

    // A tail call replaces the caller's frame (Step 6); nothing names it.
    let caller = function("caller", Vec::new(), Type::Void, call(true));
    assert!(emit(&program(vec![caller, empty_function("callee")], 0)).is_ok());
}

#[test]
fn a_module_value_lowers_since_step_5b() {
    let global = CheckedGlobal::new("value", Type::Void, void_literal(), origin())
        .expect("a checked global");
    let program = CheckedProgram::try_new_with_globals(
        vec![global],
        vec![empty_function("done")],
        0,
    )
    .expect("a checked program");
    assert!(emit(&program).is_ok());
}

#[test]
fn the_error_lists_each_distinct_form_once_in_form_order() {
    // A program with two test assertions names `test-assertion` once.
    let error = not_lowered(&program(
        vec![
            empty_function("entry"),
            assertion_function("a"),
            assertion_function("b"),
        ],
        0,
    ));
    assert_eq!(forms(&error), ["test-assertion"]);
}

#[test]
fn no_module_is_produced_for_a_program_the_emitter_cannot_lower_completely() {
    // The entry is empty, but another function is not: the emitter reports it
    // rather than emitting a module without it.
    let result = emit(&program(
        vec![empty_function("entry"), assertion_function("helper")],
        0,
    ));
    assert!(
        result.is_err(),
        "a program with an unlowered function has no module"
    );
}
