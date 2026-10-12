//! The reference semantics of the scalar, `char`, text, and bytes rows of the
//! compiler registry (`docs/spec/06-runtime.md`, "M3 compiler intrinsic
//! registry").
//!
//! Every operation is total: a partial one answers with the `option`,
//! `result`, or `ordering` value of its checked result type, which the caller
//! passes in, so no library type's identity is fixed here.

use std::cmp::Ordering;

use vibra_ir::external::{CompilerIntrinsic, FloatOp, IntegerOp, NumericType};
use vibra_ir::{Type, Value};

use crate::RuntimeValue;

/// Applies a scalar or module row to its evaluated operands. `None` for an
/// operand that does not match the checked signature, which the checker rules
/// out.
pub(crate) fn apply(
    intrinsic: CompilerIntrinsic,
    operands: &[RuntimeValue],
    result: &Type,
) -> Option<RuntimeValue> {
    let primitives = operands
        .iter()
        .map(|operand| match operand {
            RuntimeValue::Primitive(value) => Some(value),
            _ => None,
        })
        .collect::<Option<Vec<_>>>();
    match intrinsic {
        CompilerIntrinsic::Integer(numeric, op) => {
            integer(numeric, op, &primitives?, result)
        }
        CompilerIntrinsic::Float(numeric, op) => {
            float(numeric, op, &primitives?, result)
        }
        CompilerIntrinsic::Convert(source, target) => {
            let primitives = primitives?;
            let [value] = primitives.as_slice() else {
                return None;
            };
            let converted = make_integer(target, integer_value(value)?).map(primitive);
            Some(if CompilerIntrinsic::conversion_is_total(source, target) {
                converted?
            } else {
                match converted {
                    Some(value) => ok(result, value),
                    None => err(result, "out-of-range"),
                }
            })
        }
        CompilerIntrinsic::TextToChars
        | CompilerIntrinsic::TextFromChars
        | CompilerIntrinsic::BytesToArray
        | CompilerIntrinsic::BytesFromArray => sequences(intrinsic, operands, result),
        _ => module_row(intrinsic, &primitives?, result),
    }
}

fn primitive(value: Value) -> RuntimeValue {
    RuntimeValue::Primitive(value)
}

/// `some value` or `none` of the checked `option` type.
fn option(result: &Type, value: Option<RuntimeValue>) -> RuntimeValue {
    RuntimeValue::Enum {
        value_type: result.clone(),
        variant: if value.is_some() { "some" } else { "none" }.to_owned(),
        payload: value.and_then(crate::present_payload),
    }
}

/// `ok value` of the checked `result` type.
fn ok(result: &Type, value: RuntimeValue) -> RuntimeValue {
    RuntimeValue::Enum {
        value_type: result.clone(),
        variant: "ok".to_owned(),
        payload: crate::present_payload(value),
    }
}

/// `err error` of the checked `result` type, whose error type is its second
/// argument and whose error variants all have `void` payloads.
fn err(result: &Type, error: &str) -> RuntimeValue {
    let error_type = match result {
        Type::Applied(_, arguments) => arguments.get(1).cloned().unwrap_or(Type::Void),
        _ => Type::Void,
    };
    RuntimeValue::Enum {
        value_type: result.clone(),
        variant: "err".to_owned(),
        payload: Some(crate::value::Inner::new(RuntimeValue::Enum {
            value_type: error_type,
            variant: error.to_owned(),
            payload: None,
        })),
    }
}

/// The checked `ordering` value.
fn ordering(result: &Type, order: Ordering) -> RuntimeValue {
    RuntimeValue::Enum {
        value_type: result.clone(),
        variant: match order {
            Ordering::Less => "less",
            Ordering::Equal => "equal",
            Ordering::Greater => "greater",
        }
        .to_owned(),
        payload: None,
    }
}

// ---------------------------------------------------------------------------
// Integers
// ---------------------------------------------------------------------------

fn integer_value(value: &Value) -> Option<i128> {
    Some(match value {
        Value::I8(value) => i128::from(*value),
        Value::I16(value) => i128::from(*value),
        Value::I32(value) => i128::from(*value),
        Value::I64(value) => i128::from(*value),
        Value::U8(value) => i128::from(*value),
        Value::U16(value) => i128::from(*value),
        Value::U32(value) => i128::from(*value),
        Value::U64(value) => i128::from(*value),
        _ => return None,
    })
}

/// The `numeric` value of `value`, or `None` when it lies outside the type.
fn make_integer(numeric: NumericType, value: i128) -> Option<Value> {
    Some(match numeric {
        NumericType::I8 => Value::I8(i8::try_from(value).ok()?),
        NumericType::I16 => Value::I16(i16::try_from(value).ok()?),
        NumericType::I32 => Value::I32(i32::try_from(value).ok()?),
        NumericType::I64 => Value::I64(i64::try_from(value).ok()?),
        NumericType::U8 => Value::U8(u8::try_from(value).ok()?),
        NumericType::U16 => Value::U16(u16::try_from(value).ok()?),
        NumericType::U32 => Value::U32(u32::try_from(value).ok()?),
        NumericType::U64 => Value::U64(u64::try_from(value).ok()?),
        NumericType::F32 | NumericType::F64 => return None,
    })
}

/// `ok` of an exact value inside the type, else `err overflow`.
fn checked(numeric: NumericType, value: Option<i128>, result: &Type) -> RuntimeValue {
    match value.and_then(|value| make_integer(numeric, value)) {
        Some(value) => ok(result, primitive(value)),
        None => err(result, "overflow"),
    }
}

fn integer(
    numeric: NumericType,
    op: IntegerOp,
    operands: &[&Value],
    result: &Type,
) -> Option<RuntimeValue> {
    let values = operands
        .iter()
        .map(|value| integer_value(value))
        .collect::<Option<Vec<_>>>();
    Some(match (op, operands) {
        (IntegerOp::Parse, [Value::Str(text)]) => parse_integer(numeric, text, result),
        (IntegerOp::ToStr, _) => {
            let [value] = values.as_deref()? else {
                return None;
            };
            primitive(Value::Str(value.to_string()))
        }
        (IntegerOp::NegChecked, _) => {
            let [value] = values.as_deref()? else {
                return None;
            };
            checked(numeric, value.checked_neg(), result)
        }
        (
            IntegerOp::ShiftLeftChecked | IntegerOp::ShiftRight,
            [value, Value::U32(amount)],
        ) => {
            let value = integer_value(value)?;
            if *amount >= numeric.bits() {
                return Some(err(result, "invalid-shift"));
            }
            if op == IntegerOp::ShiftRight {
                // `>>` on `i128` floors, as `floor(left / 2^amount)` does.
                checked(numeric, Some(value >> amount), result)
            } else {
                checked(
                    numeric,
                    1_i128
                        .checked_shl(*amount)
                        .and_then(|factor| value.checked_mul(factor)),
                    result,
                )
            }
        }
        _ => {
            let [left, right] = values.as_deref()? else {
                return None;
            };
            let (left, right) = (*left, *right);
            match op {
                IntegerOp::AddChecked => {
                    checked(numeric, left.checked_add(right), result)
                }
                IntegerOp::SubChecked => {
                    checked(numeric, left.checked_sub(right), result)
                }
                IntegerOp::MulChecked => {
                    checked(numeric, left.checked_mul(right), result)
                }
                // `i128` division truncates toward zero and its remainder
                // takes the dividend's sign; the signed minimum divided by
                // `-1` lies outside the type, and its remainder is `0`.
                IntegerOp::DivChecked if right == 0 => err(result, "division-by-zero"),
                IntegerOp::DivChecked => {
                    checked(numeric, left.checked_div(right), result)
                }
                IntegerOp::RemChecked if right == 0 => err(result, "division-by-zero"),
                IntegerOp::RemChecked => {
                    checked(numeric, left.checked_rem(right), result)
                }
                IntegerOp::Equal => primitive(Value::Bool(left == right)),
                IntegerOp::Compare => ordering(result, left.cmp(&right)),
                _ => return None,
            }
        }
    })
}

/// An optional `-` (signed types only) and one or more ASCII digits.
fn parse_integer(numeric: NumericType, text: &str, result: &Type) -> RuntimeValue {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(digits) if numeric.is_signed() => (true, digits),
        Some(_) => return err(result, "invalid-format"),
        None => (false, text),
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return err(result, "invalid-format");
    }
    // A well-formed value too large even for `i128` is out of range.
    let value = digits.bytes().try_fold(0_i128, |value, digit| {
        value.checked_mul(10)?.checked_add(i128::from(digit - b'0'))
    });
    let value = value.map(|value| if negative { -value } else { value });
    match value.and_then(|value| make_integer(numeric, value)) {
        Some(value) => ok(result, primitive(value)),
        None => err(result, "out-of-range"),
    }
}

// ---------------------------------------------------------------------------
// Floats
// ---------------------------------------------------------------------------

fn float(
    numeric: NumericType,
    op: FloatOp,
    operands: &[&Value],
    result: &Type,
) -> Option<RuntimeValue> {
    if op == FloatOp::Parse {
        let [Value::Str(text)] = operands else {
            return None;
        };
        return Some(parse_float(numeric, text, result));
    }
    let values = operands
        .iter()
        .map(|value| match value {
            Value::F32(bits) => Some(f64::from(f32::from_bits(*bits))),
            Value::F64(bits) => Some(f64::from_bits(*bits)),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    let wide = numeric == NumericType::F64;
    // Binary32 arithmetic runs in `f32` so it rounds once, at its own width.
    let make = |value: f64| {
        primitive(if wide {
            Value::F64(value.to_bits())
        } else {
            #[allow(clippy::cast_possible_truncation)]
            Value::F32((value as f32).to_bits())
        })
    };
    let narrow = |value: f64| {
        #[allow(clippy::cast_possible_truncation)]
        let value = value as f32;
        value
    };
    Some(match (op, values.as_slice()) {
        (FloatOp::Neg, [value]) => make(-value),
        (FloatOp::ToStr, [value]) => primitive(Value::Str(if wide {
            vibra_ir::canonical_f64_text(*value)
        } else {
            vibra_ir::canonical_f32_text(narrow(*value))
        })),
        (FloatOp::Equal, [left, right]) => primitive(Value::Bool(left == right)),
        (FloatOp::CompareTotal, [left, right]) => {
            let order = if wide {
                canonical_nan_f64(*left).total_cmp(&canonical_nan_f64(*right))
            } else {
                canonical_nan_f32(narrow(*left))
                    .total_cmp(&canonical_nan_f32(narrow(*right)))
            };
            ordering(result, order)
        }
        (op, [left, right]) if wide => make(match op {
            FloatOp::Add => left + right,
            FloatOp::Sub => left - right,
            FloatOp::Mul => left * right,
            FloatOp::Div => left / right,
            _ => return None,
        }),
        (op, [left, right]) => {
            let (left, right) = (narrow(*left), narrow(*right));
            make(f64::from(match op {
                FloatOp::Add => left + right,
                FloatOp::Sub => left - right,
                FloatOp::Mul => left * right,
                FloatOp::Div => left / right,
                _ => return None,
            }))
        }
        _ => return None,
    })
}

/// Every NaN as the one quiet NaN of its width, as serialization and
/// equality see it.
fn canonical_nan_f64(value: f64) -> f64 {
    if value.is_nan() { f64::NAN } else { value }
}

fn canonical_nan_f32(value: f32) -> f32 {
    if value.is_nan() { f32::NAN } else { value }
}

/// The unsuffixed decimal float literal grammar, rounded to the nearest value
/// of the type; `out-of-range` when that value is infinite.
fn parse_float(numeric: NumericType, text: &str, result: &Type) -> RuntimeValue {
    if !is_float_literal(text) {
        return err(result, "invalid-format");
    }
    let value = if numeric == NumericType::F64 {
        text.parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .map(|value| Value::F64(value.to_bits()))
    } else {
        text.parse::<f32>()
            .ok()
            .filter(|value| value.is_finite())
            .map(|value| Value::F32(value.to_bits()))
    };
    match value {
        Some(value) => ok(result, primitive(value)),
        None => err(result, "out-of-range"),
    }
}

/// `[-] digits "." digits [exponent]` or `[-] digits exponent`, where
/// `exponent = ("e" | "E") ["+" | "-"] digits`.
fn is_float_literal(text: &str) -> bool {
    let digits =
        |text: &str| -> usize { text.bytes().take_while(u8::is_ascii_digit).count() };
    let body = text.strip_prefix('-').unwrap_or(text);
    let whole = digits(body);
    if whole == 0 {
        return false;
    }
    let mut rest = &body[whole..];
    let mut fraction = false;
    if let Some(after) = rest.strip_prefix('.') {
        let count = digits(after);
        if count == 0 {
            return false;
        }
        fraction = true;
        rest = &after[count..];
    }
    if let Some(after) = rest.strip_prefix(['e', 'E']) {
        let after = after.strip_prefix(['+', '-']).unwrap_or(after);
        let count = digits(after);
        return count > 0 && count == after.len();
    }
    fraction && rest.is_empty()
}

// ---------------------------------------------------------------------------
// Modules
// ---------------------------------------------------------------------------

/// Elements in the half-open range, or `None` when start exceeds end or end
/// exceeds the length.
fn range<T>(items: &[T], start: u64, end: u64) -> Option<&[T]> {
    let start = usize::try_from(start).ok()?;
    let end = usize::try_from(end).ok()?;
    (start <= end).then_some(())?;
    items.get(start..end)
}

fn module_row(
    intrinsic: CompilerIntrinsic,
    operands: &[&Value],
    result: &Type,
) -> Option<RuntimeValue> {
    Some(match (intrinsic, operands) {
        (CompilerIntrinsic::CharToU32, [Value::Char(value)]) => {
            primitive(Value::U32(u32::from(*value)))
        }
        (CompilerIntrinsic::CharFromU32, [Value::U32(scalar)]) => option(
            result,
            char::from_u32(*scalar).map(|value| primitive(Value::Char(value))),
        ),
        (CompilerIntrinsic::TextConcat, [Value::Str(left), Value::Str(right)]) => {
            primitive(Value::Str(format!("{left}{right}")))
        }
        (CompilerIntrinsic::TextLength, [Value::Str(value)]) => {
            primitive(Value::U64(value.chars().count() as u64))
        }
        (CompilerIntrinsic::TextEqual, [Value::Str(left), Value::Str(right)]) => {
            primitive(Value::Bool(left == right))
        }
        // UTF-8 byte order is scalar-value order.
        (CompilerIntrinsic::TextCompare, [Value::Str(left), Value::Str(right)]) => {
            ordering(result, left.cmp(right))
        }
        (
            CompilerIntrinsic::TextSlice,
            [Value::Str(value), Value::U64(start), Value::U64(end)],
        ) => {
            let scalars = value.chars().collect::<Vec<_>>();
            option(
                result,
                range(&scalars, *start, *end)
                    .map(|slice| primitive(Value::Str(slice.iter().collect()))),
            )
        }
        (CompilerIntrinsic::TextToUtf8, [Value::Str(value)]) => {
            primitive(Value::Bytes(value.as_bytes().to_vec()))
        }
        (CompilerIntrinsic::TextFromUtf8, [Value::Bytes(encoded)]) => {
            match String::from_utf8(encoded.clone()) {
                Ok(value) => ok(result, primitive(Value::Str(value))),
                Err(_) => err(result, "invalid-format"),
            }
        }
        (CompilerIntrinsic::BytesLength, [Value::Bytes(value)]) => {
            primitive(Value::U64(value.len() as u64))
        }
        (CompilerIntrinsic::BytesConcat, [Value::Bytes(left), Value::Bytes(right)]) => {
            let mut joined = left.clone();
            joined.extend_from_slice(right);
            primitive(Value::Bytes(joined))
        }
        (CompilerIntrinsic::BytesEqual, [Value::Bytes(left), Value::Bytes(right)]) => {
            primitive(Value::Bool(left == right))
        }
        (
            CompilerIntrinsic::BytesCompare,
            [Value::Bytes(left), Value::Bytes(right)],
        ) => ordering(result, left.cmp(right)),
        (
            CompilerIntrinsic::BytesSlice,
            [Value::Bytes(value), Value::U64(start), Value::U64(end)],
        ) => option(
            result,
            range(value, *start, *end)
                .map(|slice| primitive(Value::Bytes(slice.to_vec()))),
        ),
        _ => return None,
    })
}

/// Conversions between a string or bytes and an array of its items.
fn sequences(
    intrinsic: CompilerIntrinsic,
    operands: &[RuntimeValue],
    result: &Type,
) -> Option<RuntimeValue> {
    let array = |values: Vec<RuntimeValue>| RuntimeValue::Array {
        value_type: result.clone(),
        values: values.into(),
    };
    Some(match (intrinsic, operands) {
        (
            CompilerIntrinsic::TextToChars,
            [RuntimeValue::Primitive(Value::Str(value))],
        ) => array(
            value
                .chars()
                .map(|value| primitive(Value::Char(value)))
                .collect(),
        ),
        (
            CompilerIntrinsic::BytesToArray,
            [RuntimeValue::Primitive(Value::Bytes(value))],
        ) => array(
            value
                .iter()
                .map(|value| primitive(Value::U8(*value)))
                .collect(),
        ),
        (CompilerIntrinsic::TextFromChars, [RuntimeValue::Array { values, .. }]) => {
            primitive(Value::Str(
                values
                    .iter()
                    .map(|value| match value {
                        RuntimeValue::Primitive(Value::Char(value)) => Some(*value),
                        _ => None,
                    })
                    .collect::<Option<String>>()?,
            ))
        }
        (CompilerIntrinsic::BytesFromArray, [RuntimeValue::Array { values, .. }]) => {
            primitive(Value::Bytes(
                values
                    .iter()
                    .map(|value| match value {
                        RuntimeValue::Primitive(Value::U8(value)) => Some(*value),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>()?,
            ))
        }
        _ => return None,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use vibra_ir::external::{CompilerIntrinsic, FloatOp, IntegerOp, NumericType};
    use vibra_ir::{Type, TypeId, Value};

    use super::apply;
    use crate::RuntimeValue;

    fn result_type() -> Type {
        let id = |name: &str| TypeId::new(format!("@test/{name}"), name);
        Type::Applied(id("result"), vec![Type::Void, Type::Declared(id("error"))])
    }

    fn run(intrinsic: CompilerIntrinsic, operands: &[Value]) -> String {
        let operands = operands
            .iter()
            .cloned()
            .map(RuntimeValue::Primitive)
            .collect::<Vec<_>>();
        let value = apply(intrinsic, &operands, &result_type()).unwrap();
        crate::observe(value).unwrap().canonical_vibon()
    }

    fn int(numeric: NumericType, op: IntegerOp, operands: &[Value]) -> String {
        run(CompilerIntrinsic::Integer(numeric, op), operands)
    }

    fn is_err(text: &str, variant: &str) -> bool {
        text.contains("variant: @err") && text.contains(&format!("variant: @{variant}"))
    }

    /// The checked result type of `intrinsic`, with the standard-library roles
    /// bound to test types.
    fn checked_result_type(intrinsic: CompilerIntrinsic) -> Type {
        use vibra_ir::external::RoleTypes;
        let id = |name: &str| TypeId::new(format!("@test/{name}"), name);
        let roles = RoleTypes::new(Some(id("option")))
            .with_result(Some(id("result")))
            .with_core(
                Some(id("ordering")),
                Some(id("arithmetic-error")),
                Some(id("conversion-error")),
            );
        intrinsic.signature(&roles).result()
    }

    /// The reference implementation answers every sample vector of every row
    /// as the table specifies, which the WebAssembly lowering is held to as
    /// well, so the two backends agree on each of them.
    #[test]
    fn every_sample_vector_of_every_row_is_answered_as_specified() {
        let mut checked = 0_usize;
        for intrinsic in CompilerIntrinsic::all() {
            let result = checked_result_type(intrinsic);
            for vector in intrinsic.vectors() {
                let operands = vector
                    .operands
                    .iter()
                    .cloned()
                    .map(RuntimeValue::Primitive)
                    .collect::<Vec<_>>();
                let value = apply(intrinsic, &operands, &result).unwrap();
                let observed = crate::observe(value).unwrap();
                assert_eq!(
                    observed,
                    vector.outcome.observed(&result),
                    "{} {:?}",
                    intrinsic.symbol(),
                    vector.operands
                );
                checked += 1;
            }
        }
        assert!(checked > 10_000, "only {checked} vectors");
    }

    #[test]
    fn integer_boundaries() {
        use IntegerOp::{AddChecked, DivChecked, MulChecked, NegChecked, RemChecked};
        use NumericType::{I8, I64, U8, U64};
        assert!(is_err(
            &int(U64, AddChecked, &[Value::U64(u64::MAX), Value::U64(1)]),
            "overflow"
        ));
        assert!(
            int(U64, MulChecked, &[Value::U64(u64::MAX), Value::U64(1)])
                .contains("payload: 18446744073709551615u64")
        );
        assert!(is_err(
            &int(U8, MulChecked, &[Value::U8(16), Value::U8(16)]),
            "overflow"
        ));
        assert!(is_err(
            &int(I64, DivChecked, &[Value::I64(i64::MIN), Value::I64(-1)]),
            "overflow"
        ));
        assert!(
            int(I64, RemChecked, &[Value::I64(i64::MIN), Value::I64(-1)])
                .contains("payload: 0i64")
        );
        assert!(is_err(
            &int(I8, RemChecked, &[Value::I8(1), Value::I8(0)]),
            "division-by-zero"
        ));
        assert!(is_err(
            &int(I8, NegChecked, &[Value::I8(i8::MIN)]),
            "overflow"
        ));
        assert!(
            int(I8, DivChecked, &[Value::I8(-7), Value::I8(2)])
                .contains("payload: -3i8")
        );
    }

    #[test]
    fn integer_conversions_keep_the_value_or_report_out_of_range() {
        let integers = NumericType::ALL
            .into_iter()
            .filter(|numeric| numeric.is_integer())
            .collect::<Vec<_>>();
        for source in &integers {
            for target in &integers {
                if source == target {
                    continue;
                }
                let total = CompilerIntrinsic::conversion_is_total(*source, *target);
                let (source_low, source_high) = source.range();
                let (target_low, target_high) = target.range();
                // A conversion is total exactly when the target holds both of
                // the source's bounds.
                assert_eq!(
                    total,
                    target_low <= source_low && source_high <= target_high
                );
                for value in [source_low, source_high, 0, 1] {
                    let operand = super::make_integer(*source, value).unwrap();
                    let text =
                        run(CompilerIntrinsic::Convert(*source, *target), &[operand]);
                    let fits = target_low <= value && value <= target_high;
                    let spelled = format!("{value}{}", target.name());
                    if total {
                        assert_eq!(text, spelled, "{source:?} to {target:?}");
                    } else if fits {
                        assert!(
                            text.contains("variant: @ok")
                                && text.contains(&format!("payload: {spelled}")),
                            "{source:?} to {target:?} at {value}: {text}"
                        );
                    } else {
                        assert!(
                            is_err(&text, "out-of-range"),
                            "{source:?} to {target:?} at {value}: {text}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn shifts_reject_amounts_at_or_above_the_width() {
        use IntegerOp::{ShiftLeftChecked, ShiftRight};
        use NumericType::{I16, U8};
        assert!(is_err(
            &int(U8, ShiftLeftChecked, &[Value::U8(1), Value::U32(8)]),
            "invalid-shift"
        ));
        assert!(is_err(
            &int(U8, ShiftRight, &[Value::U8(1), Value::U32(9)]),
            "invalid-shift"
        ));
        assert!(
            int(U8, ShiftLeftChecked, &[Value::U8(1), Value::U32(7)])
                .contains("payload: 128u8")
        );
        assert!(is_err(
            &int(U8, ShiftLeftChecked, &[Value::U8(2), Value::U32(7)]),
            "overflow"
        ));
        assert!(
            int(I16, ShiftRight, &[Value::I16(-1), Value::U32(15)])
                .contains("payload: -1i16")
        );
    }

    #[test]
    fn integer_text_boundaries() {
        use IntegerOp::{Parse, ToStr};
        use NumericType::{I8, U8};
        let parse =
            |numeric, text: &str| int(numeric, Parse, &[Value::Str(text.to_owned())]);
        assert!(parse(I8, "-128").contains("payload: -128i8"));
        assert!(is_err(&parse(I8, "128"), "out-of-range"));
        assert!(is_err(
            &parse(U8, "99999999999999999999999999999999999999999"),
            "out-of-range"
        ));
        for invalid in ["", "-", "+1", "1 ", "0x1", "1.0", "١"] {
            assert!(is_err(&parse(I8, invalid), "invalid-format"), "{invalid:?}");
        }
        assert!(is_err(&parse(U8, "-0"), "invalid-format"));
        assert_eq!(int(I8, ToStr, &[Value::I8(i8::MIN)]), "\"-128\"");
    }

    #[test]
    fn float_nan_and_signed_zeros() {
        let float = |op, operands: &[Value]| {
            run(CompilerIntrinsic::Float(NumericType::F64, op), operands)
        };
        let bits = |value: f64| Value::F64(value.to_bits());
        let nan = bits(f64::NAN);
        assert_eq!(float(FloatOp::Equal, &[nan.clone(), nan.clone()]), "false");
        assert_eq!(float(FloatOp::Equal, &[bits(-0.0), bits(0.0)]), "true");
        assert!(
            float(FloatOp::CompareTotal, &[bits(-0.0), bits(0.0)]).contains("@less")
        );
        // Every NaN payload is the one quiet NaN under totalOrder.
        let negative_nan = Value::F64(f64::NAN.to_bits() | (1 << 63));
        assert!(
            float(FloatOp::CompareTotal, &[negative_nan, nan.clone()])
                .contains("@equal")
        );
        assert_eq!(float(FloatOp::ToStr, &[nan]), "\"nan\"");
        assert_eq!(float(FloatOp::ToStr, &[bits(-0.0)]), "\"-0.0\"");
        assert!(is_err(
            &float(FloatOp::Parse, &[Value::Str("1e309".to_owned())]),
            "out-of-range"
        ));
        for invalid in ["1", ".5", "1.", "nan", "inf", "1e", "1.0f64"] {
            assert!(
                is_err(
                    &float(FloatOp::Parse, &[Value::Str(invalid.to_owned())]),
                    "invalid-format"
                ),
                "{invalid:?}"
            );
        }
    }

    #[test]
    fn text_and_scalar_boundaries() {
        let text = |value: &str| Value::Str(value.to_owned());
        assert!(
            run(CompilerIntrinsic::CharFromU32, &[Value::U32(0xD800)])
                .contains("@none")
        );
        assert!(
            run(CompilerIntrinsic::CharFromU32, &[Value::U32(0x11_0000)])
                .contains("@none")
        );
        assert!(
            run(CompilerIntrinsic::CharFromU32, &[Value::U32(0x10_FFFF)])
                .contains("@some")
        );
        assert_eq!(run(CompilerIntrinsic::TextLength, &[text("")]), "0u64");
        assert_eq!(run(CompilerIntrinsic::TextLength, &[text("𝄞é")]), "2u64");
        assert!(
            run(
                CompilerIntrinsic::TextSlice,
                &[text("𝄞é"), Value::U64(1), Value::U64(2)]
            )
            .contains("payload: \"é\"")
        );
        assert!(
            run(
                CompilerIntrinsic::TextSlice,
                &[text(""), Value::U64(0), Value::U64(1)]
            )
            .contains("@none")
        );
        assert!(is_err(
            &run(
                CompilerIntrinsic::TextFromUtf8,
                &[Value::Bytes(vec![0xED, 0xA0, 0x80])]
            ),
            "invalid-format"
        ));
        // A lone surrogate never decodes, and scalar order is UTF-8 byte order.
        assert!(
            run(
                CompilerIntrinsic::TextCompare,
                &[text("\u{FFFF}"), text("𝄞")]
            )
            .contains("@less")
        );
    }
}
