//! The pure rows of the compiler intrinsic registry, with the meanings of
//! `docs/spec/06-runtime.md`, "Compiler intrinsic registry".
//!
//! A plain integer operation saturates and never wraps; a checked one
//! returns a `Result`; every partial row returns an `Option` or a `Result`.
//! No row traps on user data: an operand of the wrong representation is an
//! invalid checked program, which the checker rules out. `list.fold` and
//! the assertion rows need the machine and live there. The three grant rows
//! narrow a sealed grant, which this interpreter does not hold yet: each is
//! [`Halt::Unavailable`].

use std::cmp::Ordering;
use std::rc::Rc;

use vibra_ir::external::{
    CompilerIntrinsic, FloatOp, FloatType, IntegerOp, IntegerType,
};
use vibra_ir::{CANONICAL_NAN_F32, CANONICAL_NAN_F64};

use crate::machine::Halt;
use crate::value::{Repr, Val};

fn operand_error(intrinsic: CompilerIntrinsic) -> Halt {
    Halt::Invalid(format!(
        "the operands of `{}` do not fit it",
        intrinsic.symbol()
    ))
}

/// The `Ordering` case of `order`.
pub(crate) const fn ordering<'a>(order: Ordering) -> Val<'a> {
    Val::tag(match order {
        Ordering::Less => "less",
        Ordering::Equal => "equal",
        Ordering::Greater => "greater",
    })
}

/// The order an `Ordering` value names.
pub(crate) fn order_of(value: &Val<'_>) -> Option<Ordering> {
    match value.case_name()? {
        "less" => Some(Ordering::Less),
        "equal" => Some(Ordering::Equal),
        "greater" => Some(Ordering::Greater),
        _ => None,
    }
}

/// `Result.err(error: case)` of a core error type.
fn failure<'a>(case: &'static str) -> Val<'a> {
    Val::err(Val::tag(case))
}

fn integer_of(value: &Val<'_>) -> Option<i128> {
    Some(match value.0 {
        Repr::I8(n) => n.into(),
        Repr::I16(n) => n.into(),
        Repr::I32(n) => n.into(),
        Repr::I64(n) => n.into(),
        Repr::U8(n) => n.into(),
        Repr::U16(n) => n.into(),
        Repr::U32(n) => n.into(),
        Repr::U64(n) => n.into(),
        _ => return None,
    })
}

/// The value `n` of `integer`, `None` when it does not fit.
fn make_integer<'a>(integer: IntegerType, n: i128) -> Option<Val<'a>> {
    Some(Val(match integer {
        IntegerType::I8 => Repr::I8(i8::try_from(n).ok()?),
        IntegerType::I16 => Repr::I16(i16::try_from(n).ok()?),
        IntegerType::I32 => Repr::I32(i32::try_from(n).ok()?),
        IntegerType::I64 => Repr::I64(i64::try_from(n).ok()?),
        IntegerType::U8 => Repr::U8(u8::try_from(n).ok()?),
        IntegerType::U16 => Repr::U16(u16::try_from(n).ok()?),
        IntegerType::U32 => Repr::U32(u32::try_from(n).ok()?),
        IntegerType::U64 => Repr::U64(u64::try_from(n).ok()?),
    }))
}

/// An exact result, or `None` when it is outside every integer type, with
/// the sign it would have had.
enum Exact {
    Value(i128),
    Beyond { negative: bool },
}

/// `n` clamped to the bounds of `integer`.
fn clamped<'a>(integer: IntegerType, exact: Exact) -> Option<Val<'a>> {
    let (min, max) = integer.range();
    let n = match exact {
        Exact::Value(n) => n.clamp(min, max),
        Exact::Beyond { negative: true } => min,
        Exact::Beyond { negative: false } => max,
    };
    make_integer(integer, n)
}

/// `Result.ok` of an exact result that fits, or `overflow`.
fn checked<'a>(integer: IntegerType, exact: Exact) -> Option<Val<'a>> {
    Some(match exact {
        Exact::Value(n) => {
            make_integer(integer, n).map_or_else(|| failure("overflow"), Val::ok)
        }
        Exact::Beyond { .. } => failure("overflow"),
    })
}

fn product(left: i128, right: i128) -> Exact {
    left.checked_mul(right).map_or(
        Exact::Beyond {
            negative: (left < 0) != (right < 0),
        },
        Exact::Value,
    )
}

/// `value * 2^amount`, exactly.
fn shifted_left(value: i128, amount: u32) -> Exact {
    if value == 0 {
        return Exact::Value(0);
    }
    if amount >= 126 {
        return Exact::Beyond {
            negative: value < 0,
        };
    }
    product(value, 1_i128 << amount)
}

/// Floor division by `2^amount`, for every amount.
fn shifted_right(integer: IntegerType, value: i128, amount: u32) -> i128 {
    if amount >= integer.bits() {
        if value < 0 { -1 } else { 0 }
    } else {
        value >> amount
    }
}

/// The text grammar of `TryFrom(Str)` on an integer type.
fn parse_integer<'a>(integer: IntegerType, text: &str) -> Val<'a> {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(digits) if integer.is_signed() => (true, digits),
        Some(_) => return failure("invalidFormat"),
        None => (false, text),
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return failure("invalidFormat");
    }
    let mut n: i128 = 0;
    for byte in digits.bytes() {
        let digit = i128::from(byte - b'0');
        match n.checked_mul(10).and_then(|n| n.checked_add(digit)) {
            Some(next) => n = next,
            None => return failure("outOfRange"),
        }
    }
    let n = if negative { -n } else { n };
    make_integer(integer, n).map_or_else(|| failure("outOfRange"), Val::ok)
}

#[allow(clippy::too_many_lines)]
fn integer_row<'a>(
    integer: IntegerType,
    op: IntegerOp,
    args: &[Val<'a>],
) -> Option<Val<'a>> {
    let ints = || args.iter().map(integer_of).collect::<Option<Vec<_>>>();
    let amount = || args.get(1).and_then(Val::as_u32);
    Some(match op {
        IntegerOp::Add | IntegerOp::Sub | IntegerOp::Mul => {
            let [left, right] = ints()?[..] else {
                return None;
            };
            let exact = match op {
                IntegerOp::Add => Exact::Value(left + right),
                IntegerOp::Sub => Exact::Value(left - right),
                _ => product(left, right),
            };
            clamped(integer, exact)?
        }
        IntegerOp::AddChecked | IntegerOp::SubChecked | IntegerOp::MulChecked => {
            let [left, right] = ints()?[..] else {
                return None;
            };
            let exact = match op {
                IntegerOp::AddChecked => Exact::Value(left + right),
                IntegerOp::SubChecked => Exact::Value(left - right),
                _ => product(left, right),
            };
            checked(integer, exact)?
        }
        IntegerOp::DivChecked => {
            let [left, right] = ints()?[..] else {
                return None;
            };
            if right == 0 {
                failure("divisionByZero")
            } else {
                checked(integer, Exact::Value(left / right))?
            }
        }
        IntegerOp::RemChecked => {
            let [left, right] = ints()?[..] else {
                return None;
            };
            if right == 0 {
                failure("divisionByZero")
            } else {
                Val::ok(make_integer(integer, left % right)?)
            }
        }
        IntegerOp::Neg => {
            let [value] = ints()?[..] else {
                return None;
            };
            clamped(integer, Exact::Value(-value))?
        }
        IntegerOp::NegChecked => {
            let [value] = ints()?[..] else {
                return None;
            };
            checked(integer, Exact::Value(-value))?
        }
        IntegerOp::ShiftLeft => {
            let value = integer_of(args.first()?)?;
            clamped(integer, shifted_left(value, amount()?))?
        }
        IntegerOp::ShiftRight => {
            let value = integer_of(args.first()?)?;
            make_integer(integer, shifted_right(integer, value, amount()?))?
        }
        IntegerOp::ShiftLeftChecked => {
            let value = integer_of(args.first()?)?;
            let amount = amount()?;
            if amount >= integer.bits() {
                failure("invalidShift")
            } else {
                checked(integer, shifted_left(value, amount))?
            }
        }
        IntegerOp::ShiftRightChecked => {
            let value = integer_of(args.first()?)?;
            let amount = amount()?;
            if amount >= integer.bits() {
                failure("invalidShift")
            } else {
                Val::ok(make_integer(
                    integer,
                    shifted_right(integer, value, amount),
                )?)
            }
        }
        IntegerOp::Equal => {
            let [left, right] = ints()?[..] else {
                return None;
            };
            Val::boolean(left == right)
        }
        IntegerOp::Compare => {
            let [left, right] = ints()?[..] else {
                return None;
            };
            ordering(left.cmp(&right))
        }
        IntegerOp::ToStr => {
            let [value] = ints()?[..] else {
                return None;
            };
            Val::string(&value.to_string())
        }
        IntegerOp::Parse => match args {
            [Val(Repr::Str(text))] => parse_integer(integer, text),
            _ => return None,
        },
    })
}

fn convert<'a>(
    source: IntegerType,
    target: IntegerType,
    args: &[Val<'a>],
) -> Option<Val<'a>> {
    let value = integer_of(args.first()?)?;
    let (low, high) = source.range();
    if !(low..=high).contains(&value) {
        return None;
    }
    Some(if CompilerIntrinsic::conversion_is_total(source, target) {
        make_integer(target, value)?
    } else {
        make_integer(target, value).map_or_else(|| failure("outOfRange"), Val::ok)
    })
}

/// Whether `text` is the unsuffixed decimal float-literal grammar: an
/// optional `-`, digits, then a fraction, an exponent, or both.
fn in_float_grammar(text: &str) -> bool {
    let bytes = text.as_bytes();
    let digit = |at: usize| bytes.get(at).is_some_and(u8::is_ascii_digit);
    let skip = |mut at: usize| {
        while digit(at) {
            at += 1;
        }
        at
    };
    let start = usize::from(bytes.first() == Some(&b'-'));
    let mut at = skip(start);
    if at == start {
        return false;
    }
    let mut fraction = false;
    if bytes.get(at) == Some(&b'.') {
        let first = at + 1;
        at = skip(first);
        if at == first {
            return false;
        }
        fraction = true;
    }
    let mut exponent = false;
    if matches!(bytes.get(at), Some(b'e' | b'E')) {
        at += 1;
        if matches!(bytes.get(at), Some(b'+' | b'-')) {
            at += 1;
        }
        let first = at;
        at = skip(first);
        if at == first {
            return false;
        }
        exponent = true;
    }
    at == bytes.len() && (fraction || exponent)
}

/// The canonical float text: `nan`, `inf`, `-inf`, or the shortest digits
/// that round-trip, decimal for zero and a magnitude in [1e-4, 1e16) and
/// scientific otherwise, which is Rust's debug format.
fn float_text(
    finite: impl std::fmt::Debug,
    nan: bool,
    infinite: bool,
    negative: bool,
) -> String {
    if nan {
        "nan".to_owned()
    } else if infinite {
        if negative { "-inf" } else { "inf" }.to_owned()
    } else {
        format!("{finite:?}")
    }
}

macro_rules! float_row {
    ($t:ty, $bits:ty, $nan:expr, $repr:path, $op:expr, $args:expr) => {{
        let floats = $args
            .iter()
            .map(|value: &Val<'_>| match value.0 {
                $repr(bits) => Some(<$t>::from_bits(bits)),
                _ => None,
            })
            .collect::<Option<Vec<$t>>>();
        let canonical = |value: $t| -> $bits {
            if value.is_nan() {
                $nan
            } else {
                value.to_bits()
            }
        };
        match $op {
            FloatOp::Add | FloatOp::Sub | FloatOp::Mul | FloatOp::Div => {
                let [left, right] = floats?[..] else {
                    return None;
                };
                let result = match $op {
                    FloatOp::Add => left + right,
                    FloatOp::Sub => left - right,
                    FloatOp::Mul => left * right,
                    _ => left / right,
                };
                Val($repr(result.to_bits()))
            }
            FloatOp::Neg => {
                let [value] = floats?[..] else {
                    return None;
                };
                Val($repr((-value).to_bits()))
            }
            FloatOp::Equal => {
                let [left, right] = floats?[..] else {
                    return None;
                };
                Val::boolean(left == right)
            }
            FloatOp::CompareTotal => {
                let [left, right] = floats?[..] else {
                    return None;
                };
                let left = <$t>::from_bits(canonical(left));
                let right = <$t>::from_bits(canonical(right));
                ordering(left.total_cmp(&right))
            }
            FloatOp::ToStr => {
                let [value] = floats?[..] else {
                    return None;
                };
                Val::string(&float_text(
                    value,
                    value.is_nan(),
                    value.is_infinite(),
                    value.is_sign_negative(),
                ))
            }
            FloatOp::Parse => match $args {
                [Val(Repr::Str(text))] => {
                    if !in_float_grammar(text) {
                        failure("invalidFormat")
                    } else {
                        match text.parse::<$t>() {
                            Ok(value) if value.is_finite() => {
                                Val::ok(Val($repr(value.to_bits())))
                            }
                            _ => failure("outOfRange"),
                        }
                    }
                }
                _ => return None,
            },
        }
    }};
}

fn float_row<'a>(float: FloatType, op: FloatOp, args: &[Val<'a>]) -> Option<Val<'a>> {
    Some(match float {
        FloatType::F32 => float_row!(f32, u32, CANONICAL_NAN_F32, Repr::F32, op, args),
        FloatType::F64 => float_row!(f64, u64, CANONICAL_NAN_F64, Repr::F64, op, args),
    })
}

fn index(value: &Val<'_>) -> Option<usize> {
    value.as_u64().and_then(|n| usize::try_from(n).ok())
}

/// The half-open range `start..end` of `length` items: `None` when the
/// start exceeds the end or the end exceeds the length.
fn range(
    length: usize,
    start: &Val<'_>,
    end: &Val<'_>,
) -> Option<Option<(usize, usize)>> {
    let (Some(start_n), Some(end_n)) = (start.as_u64(), end.as_u64()) else {
        return None;
    };
    let length = u64::try_from(length).ok()?;
    if start_n > end_n || end_n > length {
        return Some(None);
    }
    Some(Some((
        usize::try_from(start_n).ok()?,
        usize::try_from(end_n).ok()?,
    )))
}

fn count<'a>(length: usize) -> Option<Val<'a>> {
    u64::try_from(length).ok().map(|n| Val(Repr::U64(n)))
}

#[allow(clippy::too_many_lines)]
fn sequence_row<'a>(intrinsic: CompilerIntrinsic, args: &[Val<'a>]) -> Option<Val<'a>> {
    use CompilerIntrinsic as C;
    Some(match (intrinsic, args) {
        (C::CharToU32, [Val(Repr::Char(c))]) => Val(Repr::U32(u32::from(*c))),
        (C::CharFromU32, [Val(Repr::U32(n))]) => {
            char::from_u32(*n).map_or_else(Val::none, |c| Val::some(Val(Repr::Char(c))))
        }
        (C::StrConcat, [Val(Repr::Str(a)), Val(Repr::Str(b))]) => {
            Val::string(&[a.as_ref(), b.as_ref()].concat())
        }
        (C::StrLength, [Val(Repr::Str(a))]) => count(a.chars().count())?,
        (C::StrEqual, [Val(Repr::Str(a)), Val(Repr::Str(b))]) => Val::boolean(a == b),
        (C::StrSlice, [Val(Repr::Str(a)), start, end]) => {
            match range(a.chars().count(), start, end)? {
                None => Val::none(),
                Some((start, end)) => Val::some(Val::string(
                    &a.chars().skip(start).take(end - start).collect::<String>(),
                )),
            }
        }
        (C::StrToChars, [Val(Repr::Str(a))]) => {
            Val::list(a.chars().map(|c| Val(Repr::Char(c))).collect())
        }
        (C::StrFromChars, [list]) => {
            let chars = list
                .as_list()?
                .iter()
                .map(|value| match value.0 {
                    Repr::Char(c) => Some(c),
                    _ => None,
                })
                .collect::<Option<String>>()?;
            Val::string(&chars)
        }
        (C::StrToUtf8, [Val(Repr::Str(a))]) => Val::bytes(a.as_bytes().to_vec()),
        (C::StrFromUtf8, [Val(Repr::Bytes(bytes))]) => match std::str::from_utf8(bytes)
        {
            Ok(text) => Val::ok(Val::string(text)),
            Err(_) => failure("invalidFormat"),
        },
        (C::BytesLength, [Val(Repr::Bytes(a))]) => count(a.len())?,
        (C::BytesConcat, [Val(Repr::Bytes(a)), Val(Repr::Bytes(b))]) => {
            Val::bytes([a.as_slice(), b.as_slice()].concat())
        }
        (C::BytesEqual, [Val(Repr::Bytes(a)), Val(Repr::Bytes(b))]) => {
            Val::boolean(a == b)
        }

        (C::BytesSlice, [Val(Repr::Bytes(a)), start, end]) => {
            match range(a.len(), start, end)? {
                None => Val::none(),
                Some((start, end)) => {
                    Val::some(Val::bytes(a.get(start..end)?.to_vec()))
                }
            }
        }
        (C::BytesToList, [Val(Repr::Bytes(a))]) => {
            Val::list(a.iter().map(|byte| Val(Repr::U8(*byte))).collect())
        }
        (C::BytesFromList, [list]) => Val::bytes(
            list.as_list()?
                .iter()
                .map(|value| match value.0 {
                    Repr::U8(byte) => Some(byte),
                    _ => None,
                })
                .collect::<Option<Vec<u8>>>()?,
        ),
        (C::DictEntries, [Val(Repr::Dict(dict))]) => Val::list(
            dict.entries
                .iter()
                .map(|(key, value)| Val::tuple(vec![key.clone(), value.clone()]))
                .collect(),
        ),
        (C::DictNext, [Val(Repr::Dict(dict))]) => match dict.entries.split_first() {
            None => Val::none(),
            Some(((key, value), rest)) => Val::some(Val::tuple(vec![
                Val::tuple(vec![key.clone(), value.clone()]),
                Val::dict(dict.key.clone(), rest.to_vec()),
            ])),
        },
        (C::ListLength, [list]) => count(list.as_list()?.len())?,
        (C::ListAppend, [list, item]) => {
            let mut items = Rc::clone(list.as_list()?);
            Rc::make_mut(&mut items).push(item.clone());
            Val(Repr::List(items))
        }
        (C::ListConcat, [left, right]) => {
            let mut items = Rc::clone(left.as_list()?);
            Rc::make_mut(&mut items).extend(right.as_list()?.iter().cloned());
            Val(Repr::List(items))
        }
        (C::ListSlice, [list, start, end]) => {
            let items = list.as_list()?;
            match range(items.len(), start, end)? {
                None => Val::none(),
                Some((start, end)) => {
                    Val::some(Val::list(items.get(start..end)?.to_vec()))
                }
            }
        }
        (C::ListNext, [list]) => match list.as_list()?.split_first() {
            None => Val::none(),
            Some((first, rest)) => {
                Val::some(Val::tuple(vec![first.clone(), Val::list(rest.to_vec())]))
            }
        },
        _ => return None,
    })
}

/// The value of a pure row on `args`, in the row's parameter order.
pub(crate) fn apply<'a>(
    intrinsic: CompilerIntrinsic,
    args: &[Val<'a>],
) -> Result<Val<'a>, Halt> {
    let result = match intrinsic {
        CompilerIntrinsic::GrantRestrict
        | CompilerIntrinsic::GrantSub
        | CompilerIntrinsic::GrantLimit => {
            return Err(Halt::Unavailable(format!(
                "`{}` narrows a sealed grant, which this interpreter does not hold",
                intrinsic.symbol()
            )));
        }
        CompilerIntrinsic::Integer(integer, op) => integer_row(integer, op, args),
        CompilerIntrinsic::Float(float, op) => float_row(float, op, args),
        CompilerIntrinsic::Convert(source, target) => convert(source, target, args),
        other => sequence_row(other, args),
    };
    result.ok_or_else(|| operand_error(intrinsic))
}

/// The lookup of `key` in a `List`, `Str`, or `Bytes`: `some` of the item,
/// or `none` for an index out of range. `None` for any other collection.
pub(crate) fn lookup<'a>(collection: &Val<'a>, key: &Val<'_>) -> Option<Val<'a>> {
    let position = index(key);
    Some(match &collection.0 {
        Repr::List(items) => position
            .and_then(|at| items.get(at))
            .map_or_else(Val::none, |item| Val::some(item.clone())),
        Repr::Str(text) => position
            .and_then(|at| text.chars().nth(at))
            .map_or_else(Val::none, |c| Val::some(Val(Repr::Char(c)))),
        Repr::Bytes(bytes) => position
            .and_then(|at| bytes.get(at))
            .map_or_else(Val::none, |byte| Val::some(Val(Repr::U8(*byte)))),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use vibra_ir::external::CompilerIntrinsic;

    use super::apply;
    use crate::machine::Halt;

    #[test]
    fn the_grant_rows_are_unavailable() {
        for intrinsic in [
            CompilerIntrinsic::GrantRestrict,
            CompilerIntrinsic::GrantSub,
            CompilerIntrinsic::GrantLimit,
        ] {
            assert!(
                matches!(apply(intrinsic, &[]), Err(Halt::Unavailable(_))),
                "{}",
                intrinsic.symbol()
            );
        }
    }
}
