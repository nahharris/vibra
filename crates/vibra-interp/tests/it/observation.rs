//! Observation, floats, lookups, and dict order (`docs/spec/06-runtime.md`,
//! "Canonical value encoding", "Floating-point operations", and
//! "Evaluation"; `docs/spec/02-type-system.md`, "Dict keys, equality, and
//! order").

use vibra_diagnostics::DiagnosticCode;
use vibra_interp::{Evaluation, MemoryBudget, RuntimeError, evaluate, run};
use vibra_ir::external::{CompilerIntrinsic, FloatOp, FloatType};
use vibra_ir::{
    CANONICAL_NAN_F32, CANONICAL_NAN_F64, Case, Construction, Expr, Node, Type,
    TypeBody, TypeDefinition, Value, Visibility,
};

use crate::{
    BASE, LIMITS, SEED, bytes_type, closure, construct, dict, err_value, eval, fn_type,
    function, id, int, library_set, list_of, lit, node, none_value, program, row,
    set_with, signature, some_value, str_type, text, text_value, tuple_of, u64_lit,
    var, void,
};

fn f64_lit(value: f64) -> Node {
    lit(Type::F64, Value::f64(value))
}

fn f64_row(op: FloatOp, arguments: Vec<Node>) -> Node {
    row(CompilerIntrinsic::Float(FloatType::F64, op), arguments)
}

/// `f64.toStr` writes the canonical text: decimal for zero and a magnitude
/// in [1e-4, 1e16), scientific otherwise, and `nan` for a NaN.
#[test]
fn float_text_is_canonical() {
    let set = library_set();
    let text_of = |value: Node| eval(&set, &f64_row(FloatOp::ToStr, vec![value]));
    assert_eq!(
        text_of(f64_lit(1e20)),
        Evaluation::Value(text_value("1e20"))
    );
    assert_eq!(
        text_of(f64_lit(0.0025)),
        Evaluation::Value(text_value("0.0025"))
    );
    assert_eq!(
        text_of(f64_lit(-0.0)),
        Evaluation::Value(text_value("-0.0"))
    );
    let nan = f64_row(FloatOp::Div, vec![f64_lit(0.0), f64_lit(0.0)]);
    assert_eq!(text_of(nan), Evaluation::Value(text_value("nan")));
}

/// A parse of text in the grammar whose value rounds to infinity is
/// `outOfRange`.
#[test]
fn a_float_parse_that_rounds_to_infinity_is_out_of_range() {
    let set = library_set();
    for (float, text_value) in [(FloatType::F64, "1e400"), (FloatType::F32, "1e39")] {
        let parse = row(
            CompilerIntrinsic::Float(float, FloatOp::Parse),
            vec![text(text_value)],
        );
        assert_eq!(
            eval(&set, &parse),
            Evaluation::Value(err_value("outOfRange"))
        );
    }
}

/// Runs `main`, of result type `result`, and gives its canonical
/// observation.
fn observe(result: Type, body: Node, types: Vec<TypeDefinition>) -> String {
    let set = set_with(
        vec![function("main", signature(Vec::new(), result), body, 0)],
        Vec::new(),
        types,
    );
    run(&program(set, BASE), MemoryBudget::DEFAULT)
        .unwrap()
        .canonical
}

#[test]
fn a_non_finite_float_is_observed_as_the_non_finite_record() {
    let negative = f64_row(FloatOp::Div, vec![f64_lit(-1.0), f64_lit(0.0)]);
    assert_eq!(
        observe(Type::F64, negative, Vec::new()),
        "{format: @value.v2 typeInfo: @f64 value: {kind: @nonFinite value: @negativeInfinity}}"
    );
    let nan = f64_row(FloatOp::Div, vec![f64_lit(0.0), f64_lit(0.0)]);
    assert_eq!(
        observe(Type::F64, nan, Vec::new()),
        "{format: @value.v2 typeInfo: @f64 value: {kind: @nonFinite value: @nan}}"
    );
}

/// The chapter's example: `Any` holding the `U8` 200.
#[test]
fn an_interface_value_is_observed_with_its_held_observation() {
    let held = node(Expr::WidenToInterface {
        value: Box::new(lit(Type::U8, Value::U8(200))),
        value_type: Type::Any,
    });
    assert_eq!(
        observe(Type::Any, held, Vec::new()),
        "{format: @value.v2 typeInfo: @any value: \
         {kind: @interface held: {format: @value.v2 typeInfo: @u8 value: 200u8}}}"
    );
}

/// The NaNs of each width: canonical, negative, and with a payload.
fn nans() -> Vec<(FloatType, Value)> {
    vec![
        (FloatType::F64, Value::F64(CANONICAL_NAN_F64)),
        (FloatType::F64, Value::F64(0xFFF8_0000_0000_0000)),
        (FloatType::F64, Value::F64(0x7FF0_0000_0000_0001)),
        (FloatType::F32, Value::F32(CANONICAL_NAN_F32)),
        (FloatType::F32, Value::F32(0xFFC0_0000)),
        (FloatType::F32, Value::F32(0x7F80_0001)),
    ]
}

const fn float_type(float: FloatType) -> Type {
    match float {
        FloatType::F32 => Type::F32,
        FloatType::F64 => Type::F64,
    }
}

/// `equal` is false for each NaN with itself; `compareTotal` canonicalizes,
/// so any two NaNs of a width compare `equal`; and the observation writes
/// every NaN as the canonical one, agreeing with `compareTotal`.
#[test]
fn nan_is_canonical_for_comparison_and_observation() {
    let set = library_set();
    let apply = |float: FloatType, op: FloatOp, operands: Vec<Value>| {
        let ty = float_type(float);
        let arguments = (0..operands.len())
            .map(|slot| var(slot, ty.clone()))
            .collect();
        evaluate(
            &set,
            &row(CompilerIntrinsic::Float(float, op), arguments).expr,
            &operands,
            LIMITS,
        )
    };
    for (float, nan) in nans() {
        assert_eq!(
            apply(float, FloatOp::Equal, vec![nan.clone(), nan.clone()]),
            Evaluation::Value(Value::bool(false))
        );
        let canonical = match float {
            FloatType::F32 => Value::F32(CANONICAL_NAN_F32),
            FloatType::F64 => Value::F64(CANONICAL_NAN_F64),
        };
        assert_eq!(
            apply(
                float,
                FloatOp::CompareTotal,
                vec![nan.clone(), canonical.clone()]
            ),
            Evaluation::Value(crate::variant_value("equal", Vec::new()))
        );
        let observed = evaluate(&set, &var(0, float_type(float)).expr, &[nan], LIMITS);
        assert_eq!(observed, Evaluation::Value(canonical));
    }
}

#[test]
fn a_lookup_out_of_range_or_of_a_missing_key_is_none() {
    let set = library_set();
    let lookup = |collection: Node, key: Node| {
        node(Expr::Lookup {
            collection: Box::new(collection),
            key: Box::new(key),
        })
    };
    let list = || list_of(Type::I32, vec![int(1), int(2)]);
    assert_eq!(
        eval(&set, &lookup(list(), u64_lit(1))),
        Evaluation::Value(some_value(Value::I32(2)))
    );
    assert_eq!(
        eval(&set, &lookup(list(), u64_lit(2))),
        Evaluation::Value(none_value())
    );
    assert_eq!(
        eval(&set, &lookup(list(), u64_lit(u64::MAX))),
        Evaluation::Value(none_value())
    );
    assert_eq!(
        eval(&set, &lookup(text("aé"), u64_lit(1))),
        Evaluation::Value(some_value(Value::Char('é')))
    );
    assert_eq!(
        eval(&set, &lookup(text("aé"), u64_lit(2))),
        Evaluation::Value(none_value())
    );
    let bytes = lit(bytes_type(), Value::Bytes(vec![7]));
    assert_eq!(
        eval(&set, &lookup(bytes, u64_lit(1))),
        Evaluation::Value(none_value())
    );
    let entries = || {
        construct(
            dict(str_type(), Type::I32),
            Construction::Dict(vec![(text("a"), int(1)), (text("b"), int(2))]),
        )
    };
    assert_eq!(
        eval(&set, &lookup(entries(), text("b"))),
        Evaluation::Value(some_value(Value::I32(2)))
    );
    assert_eq!(
        eval(&set, &lookup(entries(), text("z"))),
        Evaluation::Value(none_value())
    );
}

/// Keys `"b"`, `"a"`, `"a"` give two entries, `"a"` first, holding the later
/// value.
#[test]
fn a_dict_is_in_key_order_with_the_later_duplicate_value() {
    let set = library_set();
    let literal = construct(
        dict(str_type(), Type::I32),
        Construction::Dict(vec![
            (text("b"), int(1)),
            (text("a"), int(2)),
            (text("a"), int(3)),
        ]),
    );
    assert_eq!(
        eval(&set, &literal),
        Evaluation::Value(Value::Dict(vec![
            (text_value("a"), Value::I32(3)),
            (text_value("b"), Value::I32(1)),
        ]))
    );
}

fn atom(name: &str) -> Node {
    lit(Type::Atom, Value::Atom(name.to_owned()))
}

/// Atoms order by their spelling's bytes, and a tuple of an atom and a
/// `Str` component-wise, the `Str` by its own `Ordered` implementation.
#[test]
fn structural_keys_order_component_wise() {
    let set = library_set();
    let atoms = construct(
        dict(Type::Atom, Type::I32),
        Construction::Dict(vec![
            (atom("b"), int(1)),
            (atom("B"), int(2)),
            (atom("a"), int(3)),
        ]),
    );
    let atom_value = |name: &str| Value::Atom(name.to_owned());
    assert_eq!(
        eval(&set, &atoms),
        Evaluation::Value(Value::Dict(vec![
            (atom_value("B"), Value::I32(2)),
            (atom_value("a"), Value::I32(3)),
            (atom_value("b"), Value::I32(1)),
        ]))
    );
    let key_type = Type::Tuple(vec![Type::Atom, str_type()]);
    let key = |name: &str, text_value: &str| {
        tuple_of(
            vec![Type::Atom, str_type()],
            vec![atom(name), text(text_value)],
        )
    };
    let pairs = construct(
        dict(key_type, Type::I32),
        Construction::Dict(vec![
            (key("a", "z"), int(1)),
            (key("a", "b"), int(2)),
            (key("Z", "z"), int(3)),
        ]),
    );
    let pair = |name: &str, text: &str| {
        Value::Tuple(vec![Value::Atom(name.to_owned()), text_value(text)])
    };
    assert_eq!(
        eval(&set, &pairs),
        Evaluation::Value(Value::Dict(vec![
            (pair("Z", "z"), Value::I32(3)),
            (pair("a", "b"), Value::I32(2)),
            (pair("a", "z"), Value::I32(1)),
        ]))
    );
}

fn declared_text(path: &str) -> String {
    format!("{{kind: @declared path: @{path} seed: \"{SEED}\" arguments: []}}")
}

/// A declared record keeps declaration order and carries its `typeInfo`.
#[test]
fn a_declared_record_is_observed_canonically() {
    let point = id("test.point.Point");
    let definition = TypeDefinition::new(
        point.clone(),
        Vec::new(),
        TypeBody::Record(vec![
            ("y".to_owned(), Type::I32),
            ("x".to_owned(), Type::I32),
        ]),
        None,
        Visibility::Public,
    )
    .unwrap();
    let value = construct(
        Type::Declared(point, Vec::new()),
        Construction::Record(vec![("x".to_owned(), int(1)), ("y".to_owned(), int(2))]),
    );
    let ty = declared_text("test.point.Point");
    assert_eq!(
        observe(
            Type::Declared(id("test.point.Point"), Vec::new()),
            value,
            vec![definition]
        ),
        format!(
            "{{format: @value.v2 typeInfo: {ty} value: \
             {{kind: @record typeInfo: {ty} fields: {{y: 2i32 x: 1i32}}}}}}"
        )
    );
}

fn shape() -> TypeDefinition {
    TypeDefinition::new(
        id("test.shape.Shape"),
        Vec::new(),
        TypeBody::Variant(vec![
            Case::new("circle", vec![("radius".to_owned(), Type::F64)]),
            Case::new("empty", Vec::new()),
        ]),
        None,
        Visibility::Public,
    )
    .unwrap()
}

/// A case with a payload, and a case without one.
#[test]
fn a_variant_is_observed_canonically() {
    let shape_type = Type::Declared(id("test.shape.Shape"), Vec::new());
    let circle = construct(
        shape_type.clone(),
        Construction::Case {
            case: "circle".to_owned(),
            fields: vec![("radius".to_owned(), f64_lit(1.5))],
        },
    );
    let ty = declared_text("test.shape.Shape");
    assert_eq!(
        observe(shape_type.clone(), circle, vec![shape()]),
        format!(
            "{{format: @value.v2 typeInfo: {ty} value: \
             {{kind: @enum typeInfo: {ty} variant: @circle payload: {{radius: 1.5f64}}}}}}"
        )
    );
    let empty = construct(
        shape_type.clone(),
        Construction::Case {
            case: "empty".to_owned(),
            fields: Vec::new(),
        },
    );
    assert_eq!(
        observe(shape_type, empty, vec![shape()]),
        format!(
            "{{format: @value.v2 typeInfo: {ty} value: {{kind: @enum typeInfo: {ty} variant: @empty}}}}"
        )
    );
}

#[test]
fn a_wrapper_is_observed_canonically() {
    let meters = id("test.meters.Meters");
    let definition = TypeDefinition::new(
        meters.clone(),
        Vec::new(),
        TypeBody::Wrapper(Type::U32),
        None,
        Visibility::Public,
    )
    .unwrap();
    let value = construct(
        Type::Declared(meters.clone(), Vec::new()),
        Construction::Wrapper(Box::new(lit(Type::U32, Value::U32(5)))),
    );
    let ty = declared_text("test.meters.Meters");
    assert_eq!(
        observe(Type::Declared(meters, Vec::new()), value, vec![definition]),
        format!(
            "{{format: @value.v2 typeInfo: {ty} value: {{kind: @wrapper typeInfo: {ty} value: 5u32}}}}"
        )
    );
}

/// An interface value holding a closure has no observation: the
/// observation error `@tool.unsupportedValue`, never a trap.
#[test]
fn an_interface_value_holding_a_closure_is_an_observation_error() {
    let held = node(Expr::WidenToInterface {
        value: Box::new(closure(
            fn_type(Vec::new(), Type::Void),
            0,
            Vec::new(),
            void(),
        )),
        value_type: Type::Any,
    });
    let set = set_with(
        vec![function("main", signature(Vec::new(), Type::Any), held, 0)],
        Vec::new(),
        Vec::new(),
    );
    let error = run(&program(set, BASE), MemoryBudget::DEFAULT).unwrap_err();
    assert_eq!(
        error,
        RuntimeError::UnsupportedValue {
            value_type: Type::Any
        }
    );
    assert_eq!(error.code(), DiagnosticCode::ToolUnsupportedValue);
    assert_eq!(error.trap(), None);
    assert!(!error.is_host_event());
}
