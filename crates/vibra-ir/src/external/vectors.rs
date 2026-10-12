//! The sample vectors of the integer and `char` primitive rows.
//!
//! One table holds the operands and the specified outcome of every sample of
//! every row that a backend lowers inline. The reference interpreter's registry
//! and the WebAssembly lowering are each held to the table, so the two cannot
//! drift apart on a boundary: overflow, division by zero, the signed minimum
//! divided by `-1`, the sign of a remainder, a shift at or past the bit width,
//! a conversion at the edge of its target, or a scalar at the edge of the
//! surrogate range.
//!
//! The expected outcome is not read from either implementation. It is computed
//! here, once, by the language's own integer types: each sample is run through
//! the checked operation of the Rust type it names (`checked_add`,
//! `checked_div`, `try_from`, and `char::from_u32`), with the registry's two
//! deviations from them written out where the specification states them (a
//! remainder of the signed minimum and `-1` is `0`, and a shift is checked by
//! the exact product `left * 2^amount`). A short list of hand-written
//! examples, taken from the specification's prose, holds the table itself.

#![allow(clippy::useless_conversion, clippy::unnecessary_fallible_conversions)]

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::OnceLock;

use super::{CompilerIntrinsic, IntegerOp, NumericType};
use crate::{ObservedValue, Type, Value};

/// What the specification says a row does on one sample.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// A value the row returns as it is: a `bool`, the integer of a conversion
    /// that cannot fail, or the `u32` of a `char`.
    Value(Value),
    /// `ok value`, of `(result t arithmetic-error)` or
    /// `(result t conversion-error)`.
    Ok(Value),
    /// `err variant`, naming the variant of `arithmetic-error` or
    /// `conversion-error`, whose payload is `void`.
    Err(&'static str),
    /// `some value`, of `(option t)`.
    Some(Value),
    /// `none`.
    None,
    /// The variant of `ordering`: `less`, `equal`, or `greater`.
    Order(&'static str),
}

impl Outcome {
    /// The observable value of this outcome at the checked result type of the
    /// row: the standard library's `result`, `option`, or `ordering`, whose
    /// error type is the second type argument of a `result`.
    #[must_use]
    pub fn observed(&self, result: &Type) -> ObservedValue {
        let declared = |ty: &Type| match ty {
            Type::Declared(id) | Type::Applied(id, _) => Some(id.clone()),
            _ => None,
        };
        let variant = |ty: &Type, name: &str, payload: Option<ObservedValue>| {
            ObservedValue::Enum {
                type_id: declared(ty),
                variant: name.to_owned(),
                payload: payload.map(Box::new),
            }
        };
        match self {
            Self::Value(value) => ObservedValue::Primitive(value.clone()),
            Self::Ok(value) => {
                variant(result, "ok", Some(ObservedValue::Primitive(value.clone())))
            }
            Self::Err(error) => {
                let error_type = match result {
                    Type::Applied(_, arguments) => {
                        arguments.get(1).cloned().unwrap_or(Type::Void)
                    }
                    _ => Type::Void,
                };
                variant(result, "err", Some(variant(&error_type, error, None)))
            }
            Self::Some(value) => variant(
                result,
                "some",
                Some(ObservedValue::Primitive(value.clone())),
            ),
            Self::None => variant(result, "none", None),
            Self::Order(order) => variant(result, order, None),
        }
    }
}

/// One sample: the operands a row is applied to and what it returns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Vector {
    /// The row.
    pub intrinsic: CompilerIntrinsic,
    /// The operands, in the order of the row's signature.
    pub operands: Vec<Value>,
    /// The specified result.
    pub outcome: Outcome,
}

/// Every sample of `intrinsic`, or none for a row that has none yet.
pub(super) fn of(intrinsic: CompilerIntrinsic) -> &'static [Vector] {
    table().get(&intrinsic).map_or(&[], Vec::as_slice)
}

fn table() -> &'static BTreeMap<CompilerIntrinsic, Vec<Vector>> {
    static TABLE: OnceLock<BTreeMap<CompilerIntrinsic, Vec<Vector>>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut all = Vec::new();
        integers(&mut all);
        conversions(&mut all);
        characters(&mut all);
        let mut table: BTreeMap<CompilerIntrinsic, Vec<Vector>> = BTreeMap::new();
        for vector in all {
            table.entry(vector.intrinsic).or_default().push(vector);
        }
        table
    })
}

// -- samples ------------------------------------------------------------------

/// The shift amounts every width is tried at: none, one, the widest legal, the
/// first illegal, one past it, and the largest `u32`.
fn amounts(numeric: NumericType) -> Vec<u32> {
    let bits = numeric.bits();
    let mut amounts = vec![0, 1, bits - 1, bits, bits + 1, u32::MAX];
    amounts.sort_unstable();
    amounts.dedup();
    amounts
}

/// The operands of a binary row: the type's boundaries, the values around zero,
/// and the points where a sum, a product, and a double leave the type.
fn pair_samples(numeric: NumericType) -> Vec<i128> {
    let (min, max) = numeric.range();
    let root = max.isqrt();
    let mut samples = vec![
        min,
        min + 1,
        -7,
        -2,
        -1,
        0,
        1,
        2,
        7,
        root,
        root + 1,
        max / 2,
        max / 2 + 1,
        max - 1,
        max,
    ];
    samples.retain(|value| min <= *value && *value <= max);
    samples.sort_unstable();
    samples.dedup();
    samples
}

/// The operands of a conversion from `numeric`: its boundaries, the values
/// around zero, and the edge of every integer type on each side.
fn conversion_samples(numeric: NumericType) -> Vec<i128> {
    let (min, max) = numeric.range();
    let mut samples = vec![min, min + 1, -1, 0, 1, max - 1, max];
    for other in NumericType::ALL
        .into_iter()
        .filter(|other| other.is_integer())
    {
        let (low, high) = other.range();
        samples.extend([low - 1, low, low + 1, high - 1, high, high + 1]);
    }
    samples.retain(|value| min <= *value && *value <= max);
    samples.sort_unstable();
    samples.dedup();
    samples
}

// -- integers -----------------------------------------------------------------

fn push(
    out: &mut Vec<Vector>,
    intrinsic: CompilerIntrinsic,
    operands: Vec<Value>,
    outcome: Outcome,
) {
    out.push(Vector {
        intrinsic,
        operands,
        outcome,
    });
}

/// `ok` of an exact value, or `overflow`.
fn checked<T>(value: Option<T>, make: fn(T) -> Value) -> Outcome {
    value.map_or(Outcome::Err("overflow"), |value| Outcome::Ok(make(value)))
}

fn order(order: Ordering) -> &'static str {
    match order {
        Ordering::Less => "less",
        Ordering::Equal => "equal",
        Ordering::Greater => "greater",
    }
}

/// The rows of one integer type, from the operations of the Rust type itself.
macro_rules! integer_rows {
    ($name:ident, $t:ty, $numeric:expr, $make:path) => {
        fn $name(out: &mut Vec<Vector>) {
            let numeric: NumericType = $numeric;
            let row = |op: IntegerOp| CompilerIntrinsic::Integer(numeric, op);
            let samples = pair_samples(numeric)
                .into_iter()
                .filter_map(|value| <$t>::try_from(value).ok())
                .collect::<Vec<$t>>();
            for &left in &samples {
                for &right in &samples {
                    let operands = || vec![$make(left), $make(right)];
                    push(
                        out,
                        row(IntegerOp::AddChecked),
                        operands(),
                        checked(left.checked_add(right), $make),
                    );
                    push(
                        out,
                        row(IntegerOp::SubChecked),
                        operands(),
                        checked(left.checked_sub(right), $make),
                    );
                    push(
                        out,
                        row(IntegerOp::MulChecked),
                        operands(),
                        checked(left.checked_mul(right), $make),
                    );
                    // `checked_div` is `None` for a zero divisor and for the
                    // signed minimum divided by `-1`; the registry tells the
                    // two apart.
                    push(
                        out,
                        row(IntegerOp::DivChecked),
                        operands(),
                        if right == 0 {
                            Outcome::Err("division-by-zero")
                        } else {
                            checked(left.checked_div(right), $make)
                        },
                    );
                    // The signed minimum and `-1` have the remainder `0`.
                    push(
                        out,
                        row(IntegerOp::RemChecked),
                        operands(),
                        if right == 0 {
                            Outcome::Err("division-by-zero")
                        } else {
                            Outcome::Ok($make(left.wrapping_rem(right)))
                        },
                    );
                    push(
                        out,
                        row(IntegerOp::Equal),
                        operands(),
                        Outcome::Value(Value::Bool(left == right)),
                    );
                    push(
                        out,
                        row(IntegerOp::Compare),
                        operands(),
                        Outcome::Order(order(left.cmp(&right))),
                    );
                }
                if numeric.is_signed() {
                    push(
                        out,
                        row(IntegerOp::NegChecked),
                        vec![$make(left)],
                        checked(left.checked_neg(), $make),
                    );
                }
                for amount in amounts(numeric) {
                    let operands = || vec![$make(left), Value::U32(amount)];
                    if amount >= numeric.bits() {
                        push(
                            out,
                            row(IntegerOp::ShiftLeftChecked),
                            operands(),
                            Outcome::Err("invalid-shift"),
                        );
                        push(
                            out,
                            row(IntegerOp::ShiftRight),
                            operands(),
                            Outcome::Err("invalid-shift"),
                        );
                        continue;
                    }
                    // `left * 2^amount`, exactly; a shift of a `u64` by up to
                    // 63 stays inside `i128`.
                    let product = i128::from(left) * (1_i128 << amount);
                    push(
                        out,
                        row(IntegerOp::ShiftLeftChecked),
                        operands(),
                        checked(<$t>::try_from(product).ok(), $make),
                    );
                    // The native `>>` is arithmetic for a signed type, so it
                    // is `floor(left / 2^amount)`, and logical for an unsigned
                    // one, where the two agree.
                    push(
                        out,
                        row(IntegerOp::ShiftRight),
                        operands(),
                        Outcome::Ok($make(left >> amount)),
                    );
                }
            }
        }
    };
}

integer_rows!(rows_i8, i8, NumericType::I8, Value::I8);
integer_rows!(rows_i16, i16, NumericType::I16, Value::I16);
integer_rows!(rows_i32, i32, NumericType::I32, Value::I32);
integer_rows!(rows_i64, i64, NumericType::I64, Value::I64);
integer_rows!(rows_u8, u8, NumericType::U8, Value::U8);
integer_rows!(rows_u16, u16, NumericType::U16, Value::U16);
integer_rows!(rows_u32, u32, NumericType::U32, Value::U32);
integer_rows!(rows_u64, u64, NumericType::U64, Value::U64);

fn integers(out: &mut Vec<Vector>) {
    rows_i8(out);
    rows_i16(out);
    rows_i32(out);
    rows_i64(out);
    rows_u8(out);
    rows_u16(out);
    rows_u32(out);
    rows_u64(out);
}

// -- conversions --------------------------------------------------------------

/// The conversion of `$operand` (a `$source`) to `$target`, by the Rust
/// conversion between the two.
macro_rules! convert_to {
    ($out:ident, $source:expr, $make_source:path, $operand:expr,
     $target:ty, $target_numeric:expr, $make_target:path) => {
        let target: NumericType = $target_numeric;
        if $source != target {
            let outcome = match <$target>::try_from($operand) {
                Ok(value) => {
                    if CompilerIntrinsic::conversion_is_total($source, target) {
                        Outcome::Value($make_target(value))
                    } else {
                        Outcome::Ok($make_target(value))
                    }
                }
                Err(_) => Outcome::Err("out-of-range"),
            };
            push(
                $out,
                CompilerIntrinsic::Convert($source, target),
                vec![$make_source($operand)],
                outcome,
            );
        }
    };
}

macro_rules! conversions_from {
    ($name:ident, $t:ty, $numeric:expr, $make:path) => {
        fn $name(out: &mut Vec<Vector>) {
            let source: NumericType = $numeric;
            for sample in conversion_samples(source) {
                let Ok(operand) = <$t>::try_from(sample) else {
                    continue;
                };
                convert_to!(
                    out,
                    source,
                    $make,
                    operand,
                    i8,
                    NumericType::I8,
                    Value::I8
                );
                convert_to!(
                    out,
                    source,
                    $make,
                    operand,
                    i16,
                    NumericType::I16,
                    Value::I16
                );
                convert_to!(
                    out,
                    source,
                    $make,
                    operand,
                    i32,
                    NumericType::I32,
                    Value::I32
                );
                convert_to!(
                    out,
                    source,
                    $make,
                    operand,
                    i64,
                    NumericType::I64,
                    Value::I64
                );
                convert_to!(
                    out,
                    source,
                    $make,
                    operand,
                    u8,
                    NumericType::U8,
                    Value::U8
                );
                convert_to!(
                    out,
                    source,
                    $make,
                    operand,
                    u16,
                    NumericType::U16,
                    Value::U16
                );
                convert_to!(
                    out,
                    source,
                    $make,
                    operand,
                    u32,
                    NumericType::U32,
                    Value::U32
                );
                convert_to!(
                    out,
                    source,
                    $make,
                    operand,
                    u64,
                    NumericType::U64,
                    Value::U64
                );
            }
        }
    };
}

conversions_from!(from_i8, i8, NumericType::I8, Value::I8);
conversions_from!(from_i16, i16, NumericType::I16, Value::I16);
conversions_from!(from_i32, i32, NumericType::I32, Value::I32);
conversions_from!(from_i64, i64, NumericType::I64, Value::I64);
conversions_from!(from_u8, u8, NumericType::U8, Value::U8);
conversions_from!(from_u16, u16, NumericType::U16, Value::U16);
conversions_from!(from_u32, u32, NumericType::U32, Value::U32);
conversions_from!(from_u64, u64, NumericType::U64, Value::U64);

fn conversions(out: &mut Vec<Vector>) {
    from_i8(out);
    from_i16(out);
    from_i32(out);
    from_i64(out);
    from_u8(out);
    from_u16(out);
    from_u32(out);
    from_u64(out);
}

// -- char ---------------------------------------------------------------------

/// The `char` operands: the edges of every UTF-8 length and of the surrogate
/// gap.
pub const CHARS: [char; 14] = [
    '\u{0}',
    'a',
    ' ',
    '\u{7F}',
    '\u{80}',
    '\u{7FF}',
    '\u{800}',
    '\u{D7FF}',
    '\u{E000}',
    '\u{FFFD}',
    '\u{FFFF}',
    '\u{10000}',
    '\u{1D11E}',
    '\u{10FFFF}',
];

/// The `u32` operands of `char.from-u32`: scalars, the surrogate range and
/// its edges, the first value past U+10FFFF, and the largest `u32`.
pub const SCALARS: [u32; 18] = [
    0,
    0x41,
    0xD7FF,
    0xD800,
    0xD801,
    0xDBFF,
    0xDC00,
    0xDFFF,
    0xE000,
    0xFFFF,
    0x1_0000,
    0x10_FFFE,
    0x10_FFFF,
    0x11_0000,
    0x11_0001,
    0x7FFF_FFFF,
    0x8000_0000,
    u32::MAX,
];

fn characters(out: &mut Vec<Vector>) {
    for value in CHARS {
        push(
            out,
            CompilerIntrinsic::CharToU32,
            vec![Value::Char(value)],
            Outcome::Value(Value::U32(u32::from(value))),
        );
    }
    for scalar in SCALARS {
        push(
            out,
            CompilerIntrinsic::CharFromU32,
            vec![Value::U32(scalar)],
            char::from_u32(scalar)
                .map_or(Outcome::None, |value| Outcome::Some(Value::Char(value))),
        );
    }
}

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;

    /// A vector written by hand from the specification's prose, to be found in
    /// the generated table.
    fn example(
        intrinsic: CompilerIntrinsic,
        operands: Vec<Value>,
        outcome: Outcome,
    ) -> Vector {
        Vector {
            intrinsic,
            operands,
            outcome,
        }
    }

    fn row(numeric: NumericType, op: IntegerOp) -> CompilerIntrinsic {
        CompilerIntrinsic::Integer(numeric, op)
    }

    #[test]
    fn the_table_holds_the_examples_the_specification_gives() {
        use IntegerOp::{
            AddChecked, DivChecked, MulChecked, NegChecked, RemChecked,
            ShiftLeftChecked, ShiftRight, SubChecked,
        };
        use NumericType::{I8, I16, I32, I64, U8, U32, U64};
        let examples = vec![
            // Exact result, or `overflow`.
            example(
                row(U64, AddChecked),
                vec![Value::U64(u64::MAX), Value::U64(1)],
                Outcome::Err("overflow"),
            ),
            example(
                row(U64, MulChecked),
                vec![Value::U64(u64::MAX), Value::U64(1)],
                Outcome::Ok(Value::U64(u64::MAX)),
            ),
            example(
                row(U8, SubChecked),
                vec![Value::U8(0), Value::U8(1)],
                Outcome::Err("overflow"),
            ),
            example(
                row(I32, AddChecked),
                vec![Value::I32(i32::MAX - 1), Value::I32(1)],
                Outcome::Ok(Value::I32(i32::MAX)),
            ),
            // Quotient truncated toward zero; the remainder has the dividend's
            // sign; the signed minimum divided by `-1` overflows and has the
            // remainder `0`.
            example(
                row(I8, DivChecked),
                vec![Value::I8(-7), Value::I8(2)],
                Outcome::Ok(Value::I8(-3)),
            ),
            example(
                row(I8, RemChecked),
                vec![Value::I8(-7), Value::I8(2)],
                Outcome::Ok(Value::I8(-1)),
            ),
            example(
                row(I8, RemChecked),
                vec![Value::I8(7), Value::I8(-2)],
                Outcome::Ok(Value::I8(1)),
            ),
            example(
                row(I64, DivChecked),
                vec![Value::I64(i64::MIN), Value::I64(-1)],
                Outcome::Err("overflow"),
            ),
            example(
                row(I64, RemChecked),
                vec![Value::I64(i64::MIN), Value::I64(-1)],
                Outcome::Ok(Value::I64(0)),
            ),
            example(
                row(U8, DivChecked),
                vec![Value::U8(1), Value::U8(0)],
                Outcome::Err("division-by-zero"),
            ),
            example(
                row(I16, NegChecked),
                vec![Value::I16(i16::MIN)],
                Outcome::Err("overflow"),
            ),
            // `invalid-shift` at the width, `overflow` when the product
            // leaves the type, and a flooring right shift.
            example(
                row(U32, ShiftLeftChecked),
                vec![Value::U32(1), Value::U32(32)],
                Outcome::Err("invalid-shift"),
            ),
            example(
                row(U8, ShiftLeftChecked),
                vec![Value::U8(1), Value::U32(7)],
                Outcome::Ok(Value::U8(128)),
            ),
            example(
                row(U8, ShiftLeftChecked),
                vec![Value::U8(2), Value::U32(7)],
                Outcome::Err("overflow"),
            ),
            example(
                row(I8, ShiftLeftChecked),
                vec![Value::I8(-1), Value::U32(7)],
                Outcome::Ok(Value::I8(i8::MIN)),
            ),
            example(
                row(I8, ShiftLeftChecked),
                vec![Value::I8(1), Value::U32(7)],
                Outcome::Err("overflow"),
            ),
            example(
                row(I64, ShiftRight),
                vec![Value::I64(-7), Value::U32(1)],
                Outcome::Ok(Value::I64(-4)),
            ),
            example(
                row(I16, ShiftRight),
                vec![Value::I16(-1), Value::U32(15)],
                Outcome::Ok(Value::I16(-1)),
            ),
            example(
                row(U64, ShiftRight),
                vec![Value::U64(1), Value::U32(64)],
                Outcome::Err("invalid-shift"),
            ),
            // Order is numeric.
            example(
                row(I32, IntegerOp::Compare),
                vec![Value::I32(-1), Value::I32(1)],
                Outcome::Order("less"),
            ),
            example(
                row(U64, IntegerOp::Compare),
                vec![Value::U64(u64::MAX), Value::U64(0)],
                Outcome::Order("greater"),
            ),
            // A conversion keeps the value or reports `out-of-range`.
            example(
                CompilerIntrinsic::Convert(I8, U8),
                vec![Value::I8(-1)],
                Outcome::Err("out-of-range"),
            ),
            example(
                CompilerIntrinsic::Convert(U8, I8),
                vec![Value::U8(128)],
                Outcome::Err("out-of-range"),
            ),
            example(
                CompilerIntrinsic::Convert(U8, I64),
                vec![Value::U8(255)],
                Outcome::Value(Value::I64(255)),
            ),
            example(
                CompilerIntrinsic::Convert(U64, I64),
                vec![Value::U64(1 << 63)],
                Outcome::Err("out-of-range"),
            ),
            example(
                CompilerIntrinsic::Convert(I64, U64),
                vec![Value::I64(i64::MAX)],
                Outcome::Ok(Value::U64(i64::MAX.unsigned_abs())),
            ),
            // `char` is the Unicode scalar, without the surrogates.
            example(
                CompilerIntrinsic::CharFromU32,
                vec![Value::U32(0xD800)],
                Outcome::None,
            ),
            example(
                CompilerIntrinsic::CharFromU32,
                vec![Value::U32(0x11_0000)],
                Outcome::None,
            ),
            example(
                CompilerIntrinsic::CharFromU32,
                vec![Value::U32(0x10_FFFF)],
                Outcome::Some(Value::Char('\u{10FFFF}')),
            ),
            example(
                CompilerIntrinsic::CharToU32,
                vec![Value::Char('\u{E000}')],
                Outcome::Value(Value::U32(0xE000)),
            ),
        ];
        for expected in examples {
            let found = of(expected.intrinsic)
                .iter()
                .find(|vector| vector.operands == expected.operands)
                .unwrap_or_else(|| panic!("no vector for {expected:?}"));
            assert_eq!(found.outcome, expected.outcome, "{expected:?}");
        }
    }

    #[test]
    fn every_integer_type_is_sampled_at_each_of_its_boundaries() {
        let widened = |value: &Value| match value {
            Value::I8(value) => i128::from(*value),
            Value::I16(value) => i128::from(*value),
            Value::I32(value) => i128::from(*value),
            Value::I64(value) => i128::from(*value),
            Value::U8(value) => i128::from(*value),
            Value::U16(value) => i128::from(*value),
            Value::U32(value) => i128::from(*value),
            Value::U64(value) => i128::from(*value),
            other => panic!("not an integer: {other:?}"),
        };
        for numeric in NumericType::ALL.into_iter().filter(|n| n.is_integer()) {
            let (min, max) = numeric.range();
            let operands = of(row(numeric, IntegerOp::AddChecked))
                .iter()
                .map(|vector| vector.operands.clone())
                .collect::<Vec<_>>();
            for boundary in [min, min + 1, 0, 1, max - 1, max] {
                assert!(
                    operands.iter().any(|pair| pair
                        .iter()
                        .all(|value| widened(value) == boundary)),
                    "{numeric:?} is not sampled at {boundary}"
                );
            }
        }
    }

    #[test]
    fn a_row_has_vectors_exactly_when_the_integer_and_char_rows_own_it() {
        for intrinsic in CompilerIntrinsic::all() {
            let vectored = match intrinsic {
                CompilerIntrinsic::Convert(..)
                | CompilerIntrinsic::CharToU32
                | CompilerIntrinsic::CharFromU32 => true,
                CompilerIntrinsic::Integer(_, op) => {
                    !matches!(op, IntegerOp::ToStr | IntegerOp::Parse)
                }
                _ => false,
            };
            assert_eq!(
                !of(intrinsic).is_empty(),
                vectored,
                "{}",
                intrinsic.symbol()
            );
        }
    }
}
