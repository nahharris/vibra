//! One host test per row of the compiler intrinsic registry, each named
//! after its wire symbol (`docs/spec/06-runtime.md`, "Compiler intrinsic
//! registry").
//!
//! A row with vectors runs every vector through `evaluate`: the vectors are
//! the registry's hand-written samples. The fifteen rows without vectors
//! have tests written here by hand from the chapter.

use vibra_diagnostics::DiagnosticCode;
use vibra_interp::{
    Evaluation, FailureKind, MemoryBudget, RuntimeError, evaluate, run, run_test,
};
use vibra_ir::external::{CompilerIntrinsic, IntegerOp, IntegerType};
use vibra_ir::{CANONICAL_NAN_F32, CANONICAL_NAN_F64, Expr, Trap, Type, Value};

use crate::{
    BASE, LIMITS, bytes_type, closure, dict, fn_type, function, generic_row, int,
    integer, library_set, list_of, node, none_value, origin_at, program, row, set_with,
    signature, some_value, str_type, text_value, var, void,
};

/// `value` with every NaN replaced by the canonical NaN of its width, as
/// every observation writes it.
fn canonical_nan(value: Value) -> Value {
    match value {
        Value::F32(bits) if f32::from_bits(bits).is_nan() => {
            Value::F32(CANONICAL_NAN_F32)
        }
        Value::F64(bits) if f64::from_bits(bits).is_nan() => {
            Value::F64(CANONICAL_NAN_F64)
        }
        Value::Variant { case, fields } => Value::Variant {
            case,
            fields: fields
                .into_iter()
                .map(|(name, value)| (name, canonical_nan(value)))
                .collect(),
        },
        other => other,
    }
}

fn check_vectors(symbol: &str) {
    let intrinsic = CompilerIntrinsic::from_symbol(symbol).expect("a registry row");
    let vectors = intrinsic.vectors();
    assert!(!vectors.is_empty(), "`{symbol}` has vectors");
    let set = library_set();
    let parameters = intrinsic
        .row()
        .parameters
        .iter()
        .map(|parameter| parameter.instantiate(set.library(), &[]).unwrap())
        .collect::<Vec<_>>();
    let arguments = parameters
        .iter()
        .enumerate()
        .map(|(slot, ty)| var(slot, ty.clone()))
        .collect();
    let expr = row(intrinsic, arguments).expr;
    for vector in vectors {
        assert_eq!(
            evaluate(&set, &expr, &vector.operands, LIMITS),
            Evaluation::Value(canonical_nan(vector.outcome.value())),
            "`{symbol}` on {:?}",
            vector.operands
        );
    }
}

macro_rules! vector_rows {
    ($($name:ident => $symbol:literal,)*) => {
        $(
            #[test]
            fn $name() {
                check_vectors($symbol);
            }
        )*

        /// The rows tested by their vectors.
        const VECTOR_ROWS: &[&str] = &[$($symbol),*];
    };
}

vector_rows! {
    bytes_compare => "bytes.compare",
    bytes_concat => "bytes.concat",
    bytes_equal => "bytes.equal",
    bytes_length => "bytes.length",
    bytes_slice => "bytes.slice",
    char_from_u32 => "char.fromU32",
    char_to_u32 => "char.toU32",
    f32_add => "f32.add",
    f32_compare_total => "f32.compareTotal",
    f32_div => "f32.div",
    f32_equal => "f32.equal",
    f32_mul => "f32.mul",
    f32_neg => "f32.neg",
    f32_parse => "f32.parse",
    f32_sub => "f32.sub",
    f32_to_str => "f32.toStr",
    f64_add => "f64.add",
    f64_compare_total => "f64.compareTotal",
    f64_div => "f64.div",
    f64_equal => "f64.equal",
    f64_mul => "f64.mul",
    f64_neg => "f64.neg",
    f64_parse => "f64.parse",
    f64_sub => "f64.sub",
    f64_to_str => "f64.toStr",
    i16_add => "i16.add",
    i16_add_checked => "i16.addChecked",
    i16_compare => "i16.compare",
    i16_div_checked => "i16.divChecked",
    i16_equal => "i16.equal",
    i16_mul => "i16.mul",
    i16_mul_checked => "i16.mulChecked",
    i16_neg => "i16.neg",
    i16_neg_checked => "i16.negChecked",
    i16_parse => "i16.parse",
    i16_rem_checked => "i16.remChecked",
    i16_shift_left => "i16.shiftLeft",
    i16_shift_left_checked => "i16.shiftLeftChecked",
    i16_shift_right => "i16.shiftRight",
    i16_shift_right_checked => "i16.shiftRightChecked",
    i16_sub => "i16.sub",
    i16_sub_checked => "i16.subChecked",
    i16_to_i32 => "i16.toI32",
    i16_to_i64 => "i16.toI64",
    i16_to_i8 => "i16.toI8",
    i16_to_str => "i16.toStr",
    i16_to_u16 => "i16.toU16",
    i16_to_u32 => "i16.toU32",
    i16_to_u64 => "i16.toU64",
    i16_to_u8 => "i16.toU8",
    i32_add => "i32.add",
    i32_add_checked => "i32.addChecked",
    i32_compare => "i32.compare",
    i32_div_checked => "i32.divChecked",
    i32_equal => "i32.equal",
    i32_mul => "i32.mul",
    i32_mul_checked => "i32.mulChecked",
    i32_neg => "i32.neg",
    i32_neg_checked => "i32.negChecked",
    i32_parse => "i32.parse",
    i32_rem_checked => "i32.remChecked",
    i32_shift_left => "i32.shiftLeft",
    i32_shift_left_checked => "i32.shiftLeftChecked",
    i32_shift_right => "i32.shiftRight",
    i32_shift_right_checked => "i32.shiftRightChecked",
    i32_sub => "i32.sub",
    i32_sub_checked => "i32.subChecked",
    i32_to_i16 => "i32.toI16",
    i32_to_i64 => "i32.toI64",
    i32_to_i8 => "i32.toI8",
    i32_to_str => "i32.toStr",
    i32_to_u16 => "i32.toU16",
    i32_to_u32 => "i32.toU32",
    i32_to_u64 => "i32.toU64",
    i32_to_u8 => "i32.toU8",
    i64_add => "i64.add",
    i64_add_checked => "i64.addChecked",
    i64_compare => "i64.compare",
    i64_div_checked => "i64.divChecked",
    i64_equal => "i64.equal",
    i64_mul => "i64.mul",
    i64_mul_checked => "i64.mulChecked",
    i64_neg => "i64.neg",
    i64_neg_checked => "i64.negChecked",
    i64_parse => "i64.parse",
    i64_rem_checked => "i64.remChecked",
    i64_shift_left => "i64.shiftLeft",
    i64_shift_left_checked => "i64.shiftLeftChecked",
    i64_shift_right => "i64.shiftRight",
    i64_shift_right_checked => "i64.shiftRightChecked",
    i64_sub => "i64.sub",
    i64_sub_checked => "i64.subChecked",
    i64_to_i16 => "i64.toI16",
    i64_to_i32 => "i64.toI32",
    i64_to_i8 => "i64.toI8",
    i64_to_str => "i64.toStr",
    i64_to_u16 => "i64.toU16",
    i64_to_u32 => "i64.toU32",
    i64_to_u64 => "i64.toU64",
    i64_to_u8 => "i64.toU8",
    i8_add => "i8.add",
    i8_add_checked => "i8.addChecked",
    i8_compare => "i8.compare",
    i8_div_checked => "i8.divChecked",
    i8_equal => "i8.equal",
    i8_mul => "i8.mul",
    i8_mul_checked => "i8.mulChecked",
    i8_neg => "i8.neg",
    i8_neg_checked => "i8.negChecked",
    i8_parse => "i8.parse",
    i8_rem_checked => "i8.remChecked",
    i8_shift_left => "i8.shiftLeft",
    i8_shift_left_checked => "i8.shiftLeftChecked",
    i8_shift_right => "i8.shiftRight",
    i8_shift_right_checked => "i8.shiftRightChecked",
    i8_sub => "i8.sub",
    i8_sub_checked => "i8.subChecked",
    i8_to_i16 => "i8.toI16",
    i8_to_i32 => "i8.toI32",
    i8_to_i64 => "i8.toI64",
    i8_to_str => "i8.toStr",
    i8_to_u16 => "i8.toU16",
    i8_to_u32 => "i8.toU32",
    i8_to_u64 => "i8.toU64",
    i8_to_u8 => "i8.toU8",
    str_compare => "str.compare",
    str_concat => "str.concat",
    str_equal => "str.equal",
    str_from_utf8 => "str.fromUtf8",
    str_length => "str.length",
    str_slice => "str.slice",
    str_to_utf8 => "str.toUtf8",
    u16_add => "u16.add",
    u16_add_checked => "u16.addChecked",
    u16_compare => "u16.compare",
    u16_div_checked => "u16.divChecked",
    u16_equal => "u16.equal",
    u16_mul => "u16.mul",
    u16_mul_checked => "u16.mulChecked",
    u16_parse => "u16.parse",
    u16_rem_checked => "u16.remChecked",
    u16_shift_left => "u16.shiftLeft",
    u16_shift_left_checked => "u16.shiftLeftChecked",
    u16_shift_right => "u16.shiftRight",
    u16_shift_right_checked => "u16.shiftRightChecked",
    u16_sub => "u16.sub",
    u16_sub_checked => "u16.subChecked",
    u16_to_i16 => "u16.toI16",
    u16_to_i32 => "u16.toI32",
    u16_to_i64 => "u16.toI64",
    u16_to_i8 => "u16.toI8",
    u16_to_str => "u16.toStr",
    u16_to_u32 => "u16.toU32",
    u16_to_u64 => "u16.toU64",
    u16_to_u8 => "u16.toU8",
    u32_add => "u32.add",
    u32_add_checked => "u32.addChecked",
    u32_compare => "u32.compare",
    u32_div_checked => "u32.divChecked",
    u32_equal => "u32.equal",
    u32_mul => "u32.mul",
    u32_mul_checked => "u32.mulChecked",
    u32_parse => "u32.parse",
    u32_rem_checked => "u32.remChecked",
    u32_shift_left => "u32.shiftLeft",
    u32_shift_left_checked => "u32.shiftLeftChecked",
    u32_shift_right => "u32.shiftRight",
    u32_shift_right_checked => "u32.shiftRightChecked",
    u32_sub => "u32.sub",
    u32_sub_checked => "u32.subChecked",
    u32_to_i16 => "u32.toI16",
    u32_to_i32 => "u32.toI32",
    u32_to_i64 => "u32.toI64",
    u32_to_i8 => "u32.toI8",
    u32_to_str => "u32.toStr",
    u32_to_u16 => "u32.toU16",
    u32_to_u64 => "u32.toU64",
    u32_to_u8 => "u32.toU8",
    u64_add => "u64.add",
    u64_add_checked => "u64.addChecked",
    u64_compare => "u64.compare",
    u64_div_checked => "u64.divChecked",
    u64_equal => "u64.equal",
    u64_mul => "u64.mul",
    u64_mul_checked => "u64.mulChecked",
    u64_parse => "u64.parse",
    u64_rem_checked => "u64.remChecked",
    u64_shift_left => "u64.shiftLeft",
    u64_shift_left_checked => "u64.shiftLeftChecked",
    u64_shift_right => "u64.shiftRight",
    u64_shift_right_checked => "u64.shiftRightChecked",
    u64_sub => "u64.sub",
    u64_sub_checked => "u64.subChecked",
    u64_to_i16 => "u64.toI16",
    u64_to_i32 => "u64.toI32",
    u64_to_i64 => "u64.toI64",
    u64_to_i8 => "u64.toI8",
    u64_to_str => "u64.toStr",
    u64_to_u16 => "u64.toU16",
    u64_to_u32 => "u64.toU32",
    u64_to_u8 => "u64.toU8",
    u8_add => "u8.add",
    u8_add_checked => "u8.addChecked",
    u8_compare => "u8.compare",
    u8_div_checked => "u8.divChecked",
    u8_equal => "u8.equal",
    u8_mul => "u8.mul",
    u8_mul_checked => "u8.mulChecked",
    u8_parse => "u8.parse",
    u8_rem_checked => "u8.remChecked",
    u8_shift_left => "u8.shiftLeft",
    u8_shift_left_checked => "u8.shiftLeftChecked",
    u8_shift_right => "u8.shiftRight",
    u8_shift_right_checked => "u8.shiftRightChecked",
    u8_sub => "u8.sub",
    u8_sub_checked => "u8.subChecked",
    u8_to_i16 => "u8.toI16",
    u8_to_i32 => "u8.toI32",
    u8_to_i64 => "u8.toI64",
    u8_to_i8 => "u8.toI8",
    u8_to_str => "u8.toStr",
    u8_to_u16 => "u8.toU16",
    u8_to_u32 => "u8.toU32",
    u8_to_u64 => "u8.toU64",
}

/// The rows tested by hand below.
const HAND_ROWS: &[&str] = &[
    "str.toChars",
    "str.fromChars",
    "bytes.toList",
    "bytes.fromList",
    "dict.entries",
    "dict.next",
    "list.length",
    "list.append",
    "list.concat",
    "list.slice",
    "list.fold",
    "list.next",
    "assert.true",
    "assert.false",
    "assert.equal",
];

#[test]
fn every_registry_row_has_a_test_named_after_its_wire_symbol() {
    let mut tested = VECTOR_ROWS
        .iter()
        .chain(HAND_ROWS)
        .map(|symbol| (*symbol).to_owned())
        .collect::<Vec<_>>();
    tested.sort();
    let mut registry = CompilerIntrinsic::all()
        .into_iter()
        .map(CompilerIntrinsic::symbol)
        .collect::<Vec<_>>();
    registry.sort();
    assert_eq!(tested, registry);
    for symbol in HAND_ROWS {
        let intrinsic = CompilerIntrinsic::from_symbol(symbol).unwrap();
        assert!(
            intrinsic.vectors().is_empty(),
            "`{symbol}` is tested by hand"
        );
    }
}

/// Applies `intrinsic` at `type_arguments` to the inputs `operands`.
fn apply(
    intrinsic: CompilerIntrinsic,
    type_arguments: Vec<Type>,
    operands: Vec<(Type, Value)>,
) -> Evaluation {
    let set = library_set();
    let (types, values): (Vec<Type>, Vec<Value>) = operands.into_iter().unzip();
    let arguments = types
        .into_iter()
        .enumerate()
        .map(|(slot, ty)| var(slot, ty))
        .collect();
    let expr = generic_row(intrinsic, type_arguments, arguments).expr;
    evaluate(&set, &expr, &values, LIMITS)
}

fn chars(text: &str) -> Value {
    Value::List(text.chars().map(Value::Char).collect())
}

fn i32s(values: &[i32]) -> Value {
    Value::List(values.iter().copied().map(Value::I32).collect())
}

fn list_i32() -> Type {
    Type::list(Type::I32)
}

#[test]
fn str_to_chars() {
    assert_eq!(
        apply(
            CompilerIntrinsic::StrToChars,
            Vec::new(),
            vec![(str_type(), text_value("aé"))]
        ),
        Evaluation::Value(chars("aé"))
    );
    assert_eq!(
        apply(
            CompilerIntrinsic::StrToChars,
            Vec::new(),
            vec![(str_type(), text_value(""))]
        ),
        Evaluation::Value(Value::List(Vec::new()))
    );
}

#[test]
fn str_from_chars() {
    assert_eq!(
        apply(
            CompilerIntrinsic::StrFromChars,
            Vec::new(),
            vec![(Type::list(Type::Char), chars("aé"))]
        ),
        Evaluation::Value(text_value("aé"))
    );
}

#[test]
fn bytes_to_list() {
    assert_eq!(
        apply(
            CompilerIntrinsic::BytesToList,
            Vec::new(),
            vec![(bytes_type(), Value::Bytes(vec![0, 255]))]
        ),
        Evaluation::Value(Value::List(vec![Value::U8(0), Value::U8(255)]))
    );
}

#[test]
fn bytes_from_list() {
    assert_eq!(
        apply(
            CompilerIntrinsic::BytesFromList,
            Vec::new(),
            vec![(
                Type::list(Type::U8),
                Value::List(vec![Value::U8(0), Value::U8(255)])
            )]
        ),
        Evaluation::Value(Value::Bytes(vec![0, 255]))
    );
}

fn two_entries() -> Value {
    Value::Dict(vec![
        (text_value("a"), Value::U8(1)),
        (text_value("b"), Value::U8(2)),
    ])
}

#[test]
fn dict_entries() {
    assert_eq!(
        apply(
            CompilerIntrinsic::DictEntries,
            vec![str_type(), Type::U8],
            vec![(dict(str_type(), Type::U8), two_entries())]
        ),
        Evaluation::Value(Value::List(vec![
            Value::Tuple(vec![text_value("a"), Value::U8(1)]),
            Value::Tuple(vec![text_value("b"), Value::U8(2)]),
        ]))
    );
}

#[test]
fn dict_next() {
    assert_eq!(
        apply(
            CompilerIntrinsic::DictNext,
            vec![str_type(), Type::U8],
            vec![(dict(str_type(), Type::U8), two_entries())]
        ),
        Evaluation::Value(some_value(Value::Tuple(vec![
            Value::Tuple(vec![text_value("a"), Value::U8(1)]),
            Value::Dict(vec![(text_value("b"), Value::U8(2))]),
        ])))
    );
    assert_eq!(
        apply(
            CompilerIntrinsic::DictNext,
            vec![str_type(), Type::U8],
            vec![(dict(str_type(), Type::U8), Value::Dict(Vec::new()))]
        ),
        Evaluation::Value(none_value())
    );
}

#[test]
fn list_length() {
    assert_eq!(
        apply(
            CompilerIntrinsic::ListLength,
            vec![Type::I32],
            vec![(list_i32(), i32s(&[1, 2, 3]))]
        ),
        Evaluation::Value(Value::U64(3))
    );
}

#[test]
fn list_append() {
    assert_eq!(
        apply(
            CompilerIntrinsic::ListAppend,
            vec![Type::I32],
            vec![(list_i32(), i32s(&[1, 2])), (Type::I32, Value::I32(3))]
        ),
        Evaluation::Value(i32s(&[1, 2, 3]))
    );
}

#[test]
fn list_concat() {
    assert_eq!(
        apply(
            CompilerIntrinsic::ListConcat,
            vec![Type::I32],
            vec![(list_i32(), i32s(&[1])), (list_i32(), i32s(&[2, 3]))]
        ),
        Evaluation::Value(i32s(&[1, 2, 3]))
    );
}

#[test]
fn list_slice() {
    let slice = |start: u64, end: u64| {
        apply(
            CompilerIntrinsic::ListSlice,
            vec![Type::I32],
            vec![
                (list_i32(), i32s(&[1, 2, 3])),
                (Type::U64, Value::U64(start)),
                (Type::U64, Value::U64(end)),
            ],
        )
    };
    assert_eq!(slice(1, 3), Evaluation::Value(some_value(i32s(&[2, 3]))));
    assert_eq!(slice(3, 3), Evaluation::Value(some_value(i32s(&[]))));
    assert_eq!(slice(2, 1), Evaluation::Value(none_value()));
    assert_eq!(slice(0, 4), Evaluation::Value(none_value()));
}

/// `list.fold` goes left: the step receives the accumulator, then the item.
#[test]
fn list_fold() {
    let set = library_set();
    let step = closure(
        fn_type(vec![Type::I32, Type::I32], Type::I32),
        2,
        Vec::new(),
        integer(
            IntegerType::I32,
            IntegerOp::Sub,
            vec![var(0, Type::I32), var(1, Type::I32)],
        ),
    );
    let fold = |items: Vec<i32>| {
        generic_row(
            CompilerIntrinsic::ListFold,
            vec![Type::I32, Type::I32],
            vec![
                list_of(Type::I32, items.into_iter().map(int).collect()),
                int(10),
                step.clone(),
            ],
        )
    };
    assert_eq!(
        crate::eval(&set, &fold(vec![1, 2, 3])),
        Evaluation::Value(Value::I32(4))
    );
    assert_eq!(
        crate::eval(&set, &fold(Vec::new())),
        Evaluation::Value(Value::I32(10))
    );
}

#[test]
fn list_next() {
    let next = |items: &[i32]| {
        apply(
            CompilerIntrinsic::ListNext,
            vec![Type::I32],
            vec![(list_i32(), i32s(items))],
        )
    };
    assert_eq!(
        next(&[1, 2]),
        Evaluation::Value(some_value(Value::Tuple(vec![Value::I32(1), i32s(&[2])])))
    );
    assert_eq!(next(&[]), Evaluation::Value(none_value()));
}

/// A test body of `statements`, run by `run_test`.
fn test_body(statements: Vec<vibra_ir::Node>) -> vibra_ir::CheckedProgram {
    let body = node(Expr::Sequence(statements));
    let set = set_with(
        vec![function("test", signature(Vec::new(), Type::Void), body, 0)],
        Vec::new(),
        Vec::new(),
    );
    program(set, BASE)
}

fn assertion(
    intrinsic: CompilerIntrinsic,
    arguments: Vec<vibra_ir::Node>,
) -> vibra_ir::Node {
    let mut assertion = row(intrinsic, arguments);
    assertion.origin = origin_at(10, 20);
    assertion
}

fn equal_at(
    ty: Type,
    expected: vibra_ir::Node,
    actual: vibra_ir::Node,
) -> vibra_ir::Node {
    let mut assertion = generic_row(
        CompilerIntrinsic::AssertEqual,
        vec![ty],
        vec![expected, actual],
    );
    assertion.origin = origin_at(10, 20);
    assertion
}

#[test]
fn assert_true() {
    let passed = run_test(
        &test_body(vec![assertion(
            CompilerIntrinsic::AssertTrue,
            vec![crate::boolean(true)],
        )]),
        MemoryBudget::DEFAULT,
    )
    .unwrap();
    assert_eq!(passed.failure, None);
    // The failure stops the body: the later assertion never runs.
    let failed = run_test(
        &test_body(vec![
            assertion(CompilerIntrinsic::AssertTrue, vec![crate::boolean(false)]),
            equal_at(Type::I32, int(1), int(2)),
        ]),
        MemoryBudget::DEFAULT,
    )
    .unwrap()
    .failure
    .unwrap();
    assert_eq!(failed.kind, FailureKind::Assertion);
    assert_eq!(failed.assertion, "assert.true");
    assert_eq!(failed.origin, origin_at(10, 20));
}

#[test]
fn assert_false() {
    let passed = run_test(
        &test_body(vec![assertion(
            CompilerIntrinsic::AssertFalse,
            vec![crate::boolean(false)],
        )]),
        MemoryBudget::DEFAULT,
    )
    .unwrap();
    assert_eq!(passed.failure, None);
    let failed = run_test(
        &test_body(vec![assertion(
            CompilerIntrinsic::AssertFalse,
            vec![crate::boolean(true)],
        )]),
        MemoryBudget::DEFAULT,
    )
    .unwrap()
    .failure
    .unwrap();
    assert_eq!(failed.kind, FailureKind::Assertion);
    assert_eq!(failed.assertion, "assert.false");
}

/// `assert.equal` compares canonical value encodings, expected then actual.
#[test]
fn assert_equal() {
    let passed = run_test(
        &test_body(vec![equal_at(Type::I32, int(1), int(1)), void()]),
        MemoryBudget::DEFAULT,
    )
    .unwrap();
    assert_eq!(passed.failure, None);
    let failed = run_test(
        &test_body(vec![equal_at(Type::I32, int(1), int(2))]),
        MemoryBudget::DEFAULT,
    )
    .unwrap()
    .failure
    .unwrap();
    assert_eq!(failed.kind, FailureKind::Assertion);
    assert_eq!(failed.assertion, "assert.equal");
    assert_eq!(failed.expected.as_deref(), Some("1i32"));
    assert_eq!(failed.actual.as_deref(), Some("2i32"));
    assert_eq!(failed.origin, origin_at(10, 20));
}

/// An operand that cannot be observed is the observation error, not a
/// trap.
#[test]
fn assert_equal_of_an_unobservable_operand_is_an_observation_failure() {
    let held = || {
        node(Expr::WidenToInterface {
            value: Box::new(closure(
                fn_type(Vec::new(), Type::Void),
                0,
                Vec::new(),
                void(),
            )),
            value_type: Type::Any,
        })
    };
    let failed = run_test(
        &test_body(vec![equal_at(Type::Any, held(), held())]),
        MemoryBudget::DEFAULT,
    )
    .unwrap()
    .failure
    .unwrap();
    assert_eq!(
        failed.kind,
        FailureKind::Observation {
            code: DiagnosticCode::ToolUnsupportedValue,
            type_info: Type::Any,
        }
    );
    assert_eq!(failed.assertion, "assert.equal");
    assert_eq!((failed.expected, failed.actual), (None, None));
}

/// An assertion reached outside a test is an invalid checked program.
#[test]
fn an_assertion_in_a_run_or_an_evaluation_is_an_invalid_checked_program() {
    let body = test_body(vec![assertion(
        CompilerIntrinsic::AssertTrue,
        vec![crate::boolean(true)],
    )]);
    let error = run(&body, MemoryBudget::DEFAULT).unwrap_err();
    assert_eq!(error.trap(), Some(Trap::InvalidCheckedProgram));
    assert!(matches!(error, RuntimeError::Trap { .. }));
    let set = library_set();
    assert_eq!(
        crate::eval(
            &set,
            &assertion(CompilerIntrinsic::AssertTrue, vec![crate::boolean(true)])
        ),
        Evaluation::Defect(Trap::InvalidCheckedProgram)
    );
}
