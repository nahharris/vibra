//! The interpreter's side of the review of `vibra-ir` and `vibra-interp`:
//! memory reserved before it is used, and neutral values validated as they
//! enter.

use std::sync::Arc;

use vibra_interp::{
    Evaluation, Limit, Limits, MemoryBudget, RuntimeError, evaluate, run,
};
use vibra_ir::external::{IntegerOp, IntegerType};
use vibra_ir::{
    CheckedFunction, CheckedModuleSet, Constant, Expr, FnType, Invariant, MatchArm,
    NamedTypes, Node, Pattern, SignatureRecord, Trap, Type, TypeBody, TypeDefinition,
    Value, Visibility,
};

use crate::{
    BASE, bool_type, boolean, dict, function, id, int, integer, lit, node, program,
    set_with, signature, some_value, str_type, text, var, void,
};

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

// -- V4: invariants hold for values coming in ----------------------------------

fn positive() -> Type {
    Type::Declared(id("test.positive.Positive"), Vec::new())
}

/// `Positive` wraps `I32` `where Ordered.less(0i32 n)`, a bare invariant
/// whose predicate is function `predicate`.
fn positive_definition(predicate: usize) -> TypeDefinition {
    TypeDefinition::new(
        id("test.positive.Positive"),
        Vec::new(),
        TypeBody::Wrapper(Type::I32),
        Some(Invariant {
            predicate,
            consequence: None,
        }),
        Visibility::Public,
    )
    .unwrap()
}

/// `aboveZero(n: I32) Bool`.
fn above_zero() -> CheckedFunction {
    let order = integer(
        IntegerType::I32,
        IntegerOp::Compare,
        vec![int(0), var(0, Type::I32)],
    );
    let body = node(Expr::Match {
        subject: Box::new(order),
        arms: vec![
            MatchArm {
                pattern: Pattern::Case {
                    case: "less".to_owned(),
                    fields: Vec::new(),
                },
                body: boolean(true),
            },
            MatchArm {
                pattern: Pattern::Discard,
                body: boolean(false),
            },
        ],
        value_type: bool_type(),
    });
    function(
        "aboveZero",
        signature(vec![Type::I32], bool_type()),
        body,
        1,
    )
}

fn positive_set() -> Arc<CheckedModuleSet> {
    set_with(
        vec![above_zero()],
        Vec::new(),
        vec![positive_definition(BASE)],
    )
}

fn wrapped(value: i32) -> Value {
    Value::Wrapper(Box::new(Value::I32(value)))
}

/// A constant of an invariant-carrying type is admitted only when the
/// predicate holds; one that breaks it is impossible typed IR.
#[test]
fn v4_a_constant_breaking_its_invariant_is_rejected() {
    let set = positive_set();
    let literal = |value: i32| lit(positive(), wrapped(value));
    assert_eq!(
        evaluate(&set, &literal(0).expr, &[], SMALL),
        Evaluation::Defect(Trap::InvalidCheckedProgram)
    );
    assert_eq!(
        evaluate(&set, &literal(1).expr, &[], SMALL),
        Evaluation::Value(wrapped(1))
    );
}

/// An input of an invariant-carrying type, alone or inside a list, that
/// breaks the invariant is an invalid host value.
#[test]
fn v4_an_input_breaking_its_invariant_is_an_invalid_host_value() {
    let set = positive_set();
    let read =
        |ty: Type, input: Value| evaluate(&set, &var(0, ty).expr, &[input], SMALL);
    assert_eq!(
        read(positive(), wrapped(0)),
        Evaluation::Defect(Trap::InvalidHostValue)
    );
    assert_eq!(read(positive(), wrapped(1)), Evaluation::Value(wrapped(1)));
    assert_eq!(
        read(
            Type::list(positive()),
            Value::List(vec![wrapped(1), wrapped(0)])
        ),
        Evaluation::Defect(Trap::InvalidHostValue)
    );
}

/// A default of an invariant-carrying type is admitted before the run, even
/// when no call uses it.
#[test]
fn v4_a_default_breaking_its_invariant_is_rejected() {
    let labelled = Type::function(FnType {
        parameters: Vec::new(),
        labels: NamedTypes::new([("p".to_owned(), positive())]).unwrap(),
        result: Type::Void,
    });
    let with_default = |value: i32| CheckedFunction {
        defaults: vec![(
            "p".to_owned(),
            Constant::new(positive(), wrapped(value)).unwrap(),
        )],
        ..function(
            "take",
            SignatureRecord::new(Vec::new(), labelled.clone()).unwrap(),
            void(),
            1,
        )
    };
    let main = || function("main", signature(Vec::new(), Type::Void), void(), 0);
    let outcome = |value: i32| {
        let set = set_with(
            vec![above_zero(), with_default(value), main()],
            Vec::new(),
            vec![positive_definition(BASE)],
        );
        run(&program(set, BASE + 2), MemoryBudget::DEFAULT)
    };
    assert!(matches!(
        outcome(0),
        Err(RuntimeError::Trap {
            trap: Trap::InvalidCheckedProgram,
            ..
        })
    ));
    assert!(outcome(1).is_ok());
}

/// An input interface value holds a closed type that widens to it, and an
/// input function value has the type it stands at.
#[test]
fn v4_an_input_interface_or_function_value_fits_its_type() {
    let set = positive_set();
    let read =
        |ty: Type, input: Value| evaluate(&set, &var(0, ty).expr, &[input], SMALL);
    let held = |held_type: Type, held: Value| Value::Interface {
        held_type,
        held: Box::new(held),
    };
    assert_eq!(
        read(Type::Any, held(Type::I32, Value::I32(1))),
        Evaluation::Value(held(Type::I32, Value::I32(1)))
    );
    assert_eq!(
        read(Type::Any, held(Type::Binder(0), Value::I32(1))),
        Evaluation::Defect(Trap::InvalidHostValue)
    );
    assert_eq!(
        read(Type::Any, held(Type::Any, held(Type::I32, Value::I32(1)))),
        Evaluation::Defect(Trap::InvalidHostValue)
    );
    let predicate = Type::function(FnType::new(vec![Type::I32], bool_type()));
    let function_value = |function: usize| Value::Function {
        function,
        type_arguments: Vec::new(),
    };
    assert_eq!(
        read(predicate.clone(), function_value(BASE)),
        Evaluation::Value(function_value(BASE))
    );
    assert_eq!(
        read(predicate.clone(), function_value(0)),
        Evaluation::Defect(Trap::InvalidHostValue)
    );
    assert_eq!(
        read(predicate, function_value(usize::MAX)),
        Evaluation::Defect(Trap::InvalidHostValue)
    );
}

// -- V8: dicts come in in key order --------------------------------------------

fn table() -> Type {
    dict(str_type(), Type::U8)
}

fn entries(keys: &[(&str, u8)]) -> Value {
    Value::Dict(
        keys.iter()
            .map(|(key, value)| (Value::Str((*key).to_owned()), Value::U8(*value)))
            .collect(),
    )
}

fn lookup_b(collection: Node) -> Node {
    node(Expr::Lookup {
        collection: Box::new(collection),
        key: Box::new(text("b")),
    })
}

/// An input dict must hold its keys ascending and distinct under the key
/// type's `Ordered.compare`.
#[test]
fn v8_an_input_dict_out_of_key_order_is_an_invalid_host_value() {
    let set = set_with(Vec::new(), Vec::new(), Vec::new());
    let find =
        |input: Value| evaluate(&set, &lookup_b(var(0, table())).expr, &[input], SMALL);
    assert_eq!(
        find(entries(&[("b", 20), ("a", 10)])),
        Evaluation::Defect(Trap::InvalidHostValue)
    );
    assert_eq!(
        find(entries(&[("a", 1), ("a", 2)])),
        Evaluation::Defect(Trap::InvalidHostValue)
    );
    assert_eq!(
        find(entries(&[("a", 10), ("b", 20)])),
        Evaluation::Value(some_value(Value::U8(20)))
    );
}

/// A dict constant out of key order is impossible typed IR.
#[test]
fn v8_a_constant_dict_out_of_key_order_is_rejected() {
    let set = set_with(Vec::new(), Vec::new(), Vec::new());
    let find =
        |value: Value| evaluate(&set, &lookup_b(lit(table(), value)).expr, &[], SMALL);
    assert_eq!(
        find(entries(&[("b", 20), ("a", 10)])),
        Evaluation::Defect(Trap::InvalidCheckedProgram)
    );
    assert_eq!(
        find(entries(&[("a", 10), ("b", 20)])),
        Evaluation::Value(some_value(Value::U8(20)))
    );
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
