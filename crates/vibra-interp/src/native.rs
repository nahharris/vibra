//! The interpreter's side of the native code.
//!
//! The rows of the closed table of native imports (`vibra_ir::external`,
//! `NATIVE_ROWS`) are implemented once, in `vibra-native`, and a WebAssembly
//! module reaches them through an import. The interpreter calls the same
//! functions directly. This module is the interpreter's half of the
//! value-access interface: it reads a `str`, `bytes`, array, or dict value as a
//! [`Source`] of its units, and builds the result from a [`Sink`]. It holds no
//! algorithm of its own.

use vibra_ir::external::CompilerIntrinsic;
use vibra_ir::{Type, Value};
use vibra_native::{Sink, Source};

use crate::RuntimeValue;
use crate::registry::{err, ok, option, ordering, primitive};

/// A slice of units read as a [`Source`].
struct Units<'a, U>(&'a [U]);

impl<U: Copy> Source for Units<'_, U> {
    type Unit = U;
    type Iter<'a>
        = std::iter::Copied<std::slice::Iter<'a, U>>
    where
        Self: 'a;

    fn len(&self) -> u64 {
        self.0.len() as u64
    }

    fn range(&self, from: u64, to: u64) -> Self::Iter<'_> {
        let bounds = usize::try_from(from).ok().zip(usize::try_from(to).ok());
        bounds
            .and_then(|(from, to)| self.0.get(from..to))
            .unwrap_or(&[])
            .iter()
            .copied()
    }
}

/// Units collected into a vector as a [`Sink`].
struct Collect<U>(Vec<U>);

impl<U: Copy> Sink for Collect<U> {
    type Unit = U;

    fn put(&mut self, unit: U) {
        self.0.push(unit);
    }
}

fn scalars(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}

fn text_of(units: Vec<u32>) -> Option<String> {
    units.into_iter().map(char::from_u32).collect()
}

fn text_value(units: Vec<u32>) -> Option<RuntimeValue> {
    text_of(units).map(|text| primitive(Value::Str(text)))
}

fn bytes_value(units: Vec<u8>) -> RuntimeValue {
    primitive(Value::Bytes(units))
}

fn array_value(value_type: &Type, values: Vec<RuntimeValue>) -> RuntimeValue {
    RuntimeValue::Array {
        value_type: value_type.clone(),
        values: values.into(),
    }
}
/// Applies a native row of the closed table to its evaluated operands, which
/// the checker has already held to the row's signature. `None` for an operand
/// that does not match it.
pub(crate) fn apply(
    intrinsic: CompilerIntrinsic,
    operands: &[RuntimeValue],
    result: &Type,
) -> Option<RuntimeValue> {
    use CompilerIntrinsic as C;
    use RuntimeValue::{Array, Dict, Primitive};
    let text = |value: &RuntimeValue| match value {
        Primitive(Value::Str(text)) => Some(scalars(text)),
        _ => None,
    };
    let bytes = |value: &RuntimeValue| match value {
        Primitive(Value::Bytes(bytes)) => Some(bytes.clone()),
        _ => None,
    };
    let index = |value: &RuntimeValue| match value {
        Primitive(Value::U64(index)) => Some(*index),
        _ => None,
    };
    Some(match (intrinsic, operands) {
        (C::TextLength, [value]) => {
            let value = text(value)?;
            primitive(Value::U64(vibra_native::length(&Units(&value))))
        }
        (C::BytesLength, [value]) => {
            let value = bytes(value)?;
            primitive(Value::U64(vibra_native::length(&Units(&value))))
        }
        (C::TextEqual, [first, second]) => {
            let (first, second) = (text(first)?, text(second)?);
            primitive(Value::Bool(vibra_native::same(
                &Units(&first),
                &Units(&second),
            )))
        }
        (C::BytesEqual, [first, second]) => {
            let (first, second) = (bytes(first)?, bytes(second)?);
            primitive(Value::Bool(vibra_native::same(
                &Units(&first),
                &Units(&second),
            )))
        }
        (C::TextCompare, [first, second]) => {
            let (first, second) = (text(first)?, text(second)?);
            ordering(
                result,
                vibra_native::compare(&Units(&first), &Units(&second)),
            )
        }
        (C::BytesCompare, [first, second]) => {
            let (first, second) = (bytes(first)?, bytes(second)?);
            ordering(
                result,
                vibra_native::compare(&Units(&first), &Units(&second)),
            )
        }
        (C::TextConcat, [first, second]) => {
            let (first, second) = (text(first)?, text(second)?);
            let mut out = Collect(Vec::new());
            vibra_native::concat(&Units(&first), &Units(&second), &mut out);
            text_value(out.0)?
        }
        (C::BytesConcat, [first, second]) => {
            let (first, second) = (bytes(first)?, bytes(second)?);
            let mut out = Collect(Vec::new());
            vibra_native::concat(&Units(&first), &Units(&second), &mut out);
            bytes_value(out.0)
        }
        (C::TextSlice, [value, start, end]) => {
            let (value, start, end) = (text(value)?, index(start)?, index(end)?);
            let mut out = Collect(Vec::new());
            let some = vibra_native::slice(&Units(&value), start, end, &mut out);
            option(result, some.then(|| text_value(out.0)).flatten())
        }
        (C::BytesSlice, [value, start, end]) => {
            let (value, start, end) = (bytes(value)?, index(start)?, index(end)?);
            let mut out = Collect(Vec::new());
            let some = vibra_native::slice(&Units(&value), start, end, &mut out);
            option(result, some.then(|| bytes_value(out.0)))
        }
        (C::TextToChars, [value]) => {
            let value = text(value)?;
            let mut out = Collect(Vec::new());
            vibra_native::copy(&Units(&value), &mut out);
            array_value(
                result,
                out.0
                    .into_iter()
                    .map(|scalar| char::from_u32(scalar).map(Value::Char))
                    .map(|value| value.map(primitive))
                    .collect::<Option<Vec<_>>>()?,
            )
        }
        (C::BytesToArray, [value]) => {
            let value = bytes(value)?;
            let mut out = Collect(Vec::new());
            vibra_native::copy(&Units(&value), &mut out);
            array_value(
                result,
                out.0
                    .into_iter()
                    .map(|byte| primitive(Value::U8(byte)))
                    .collect(),
            )
        }
        (C::TextFromChars, [Array { values, .. }]) => {
            let items = values
                .iter()
                .map(|value| match value {
                    Primitive(Value::Char(value)) => Some(u32::from(*value)),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>()?;
            let mut out = Collect(Vec::new());
            vibra_native::copy(&Units(&items), &mut out);
            text_value(out.0)?
        }
        (C::BytesFromArray, [Array { values, .. }]) => {
            let items = values
                .iter()
                .map(|value| match value {
                    Primitive(Value::U8(value)) => Some(*value),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>()?;
            let mut out = Collect(Vec::new());
            vibra_native::copy(&Units(&items), &mut out);
            bytes_value(out.0)
        }
        (C::TextToUtf8, [value]) => {
            let value = text(value)?;
            let mut out = Collect(Vec::new());
            vibra_native::to_utf8(&Units(&value), &mut out);
            bytes_value(out.0)
        }
        (C::TextFromUtf8, [value]) => {
            let value = bytes(value)?;
            let mut out = Collect(Vec::new());
            if vibra_native::from_utf8(&Units(&value), &mut out) {
                ok(result, text_value(out.0)?)
            } else {
                err(result, "invalid-format")
            }
        }
        (C::ArrayAppend, [Array { value_type, values }, item]) => {
            let items = values.iter().collect::<Vec<_>>();
            let mut out = Collect(Vec::new());
            vibra_native::append(&Units(&items), item, &mut out);
            array_value(value_type, out.0.into_iter().cloned().collect())
        }
        (
            C::ArrayConcat,
            [Array { value_type, values }, Array { values: second, .. }],
        ) => {
            let (first, second) = (
                values.iter().collect::<Vec<_>>(),
                second.iter().collect::<Vec<_>>(),
            );
            let mut out = Collect(Vec::new());
            vibra_native::concat(&Units(&first), &Units(&second), &mut out);
            array_value(value_type, out.0.into_iter().cloned().collect())
        }
        (C::ArraySlice, [Array { value_type, values }, start, end]) => {
            let (start, end) = (index(start)?, index(end)?);
            let items = values.iter().collect::<Vec<_>>();
            let mut out = Collect(Vec::new());
            let some = vibra_native::slice(&Units(&items), start, end, &mut out);
            option(
                result,
                some.then(|| {
                    array_value(value_type, out.0.into_iter().cloned().collect())
                }),
            )
        }
        (
            C::DictEntries,
            [
                Dict {
                    value_type: Type::Dict(key, value),
                    entries,
                },
            ],
        ) => {
            let items = entries.iter().collect::<Vec<_>>();
            let mut out = Collect(Vec::new());
            vibra_native::copy(&Units(&items), &mut out);
            let entry_type =
                Type::Tuple(vec![key.as_ref().clone(), value.as_ref().clone()]);
            RuntimeValue::Array {
                value_type: Type::Array(Box::new(entry_type.clone())),
                values: out
                    .0
                    .into_iter()
                    .map(|(key, value)| RuntimeValue::Tuple {
                        value_type: entry_type.clone(),
                        values: vec![key.clone(), value.clone()].into(),
                    })
                    .collect::<Vec<_>>()
                    .into(),
            }
        }
        _ => return None,
    })
}
