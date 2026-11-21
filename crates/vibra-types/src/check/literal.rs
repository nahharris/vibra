//! Literals: their types and values.
//!
//! `docs/spec/02-type-system.md`, "Scalars", and
//! `docs/spec/01-source-language.md`, "Literals". A suffixed literal fixes
//! its type and MUST fit it; an unsuffixed literal takes its type from the
//! expected type and is `@type.ambiguousInference` without one. A float body
//! is rounded to nearest, ties to even, at the width of its type, and a finite
//! literal that rounds to infinity is out of range.

use vibra_ir::{Type, Value};
use vibra_syntax::{
    CstNode, FloatSuffix, IntegerSuffix, SyntaxKind, decode_character, decode_string,
};

/// What a literal is, before its type is fixed.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Literal {
    /// A numeric literal: its body and the type its suffix fixes.
    Number {
        body: String,
        float_body: bool,
        suffix: Option<Type>,
    },
    /// A string literal.
    Str(String),
    /// A character literal.
    Char(char),
    /// An atom literal, without its `@`.
    Atom(String),
    /// `void`.
    Void,
}

/// Why a literal has no value at a type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LiteralFault {
    /// No type follows for an unsuffixed literal.
    Ambiguous,
    /// The value does not fit the type.
    OutOfRange(Type),
    /// The literal cannot have the expected type at all.
    Mismatch(Type),
    /// The token did not decode; the reader already reported it.
    Malformed,
}

/// Reads the literal leaf `node` of the document `text`.
pub(crate) fn read(node: &CstNode, text: &str) -> Option<Literal> {
    let token = node.text(text);
    match node.kind() {
        SyntaxKind::Integer | SyntaxKind::Float => {
            let (body, suffix) = split_number(token);
            let float_body = body.contains(['.', 'e', 'E']);
            let suffix = if suffix.is_empty() {
                None
            } else if let Some(suffix) = IntegerSuffix::parse(suffix) {
                if float_body {
                    return None;
                }
                Some(integer_type(suffix))
            } else {
                Some(float_type(FloatSuffix::parse(suffix)?))
            };
            Some(Literal::Number {
                body: body.to_owned(),
                float_body,
                suffix,
            })
        }
        SyntaxKind::String => decode_string(token).map(Literal::Str),
        SyntaxKind::Character => decode_character(token).map(Literal::Char),
        SyntaxKind::Atom => Some(Literal::Atom(
            token.strip_prefix('@').unwrap_or(token).to_owned(),
        )),
        SyntaxKind::ReservedWord if token == "void" => Some(Literal::Void),
        _ => None,
    }
}

/// Splits a numeric token into its body and its suffix.
fn split_number(token: &str) -> (&str, &str) {
    let bytes = token.as_bytes();
    let mut at = usize::from(bytes.first() == Some(&b'-'));
    while bytes
        .get(at)
        .is_some_and(|byte| byte.is_ascii_digit() || matches!(byte, b'.' | b'+' | b'-'))
        || (matches!(bytes.get(at), Some(b'e' | b'E'))
            && bytes.get(at + 1).is_some_and(|next| {
                next.is_ascii_digit() || matches!(next, b'+' | b'-')
            }))
    {
        at += 1;
    }
    (
        token.get(..at).unwrap_or(token),
        token.get(at..).unwrap_or_default(),
    )
}

const fn integer_type(suffix: IntegerSuffix) -> Type {
    match suffix {
        IntegerSuffix::I8 => Type::I8,
        IntegerSuffix::I16 => Type::I16,
        IntegerSuffix::I32 => Type::I32,
        IntegerSuffix::I64 => Type::I64,
        IntegerSuffix::U8 => Type::U8,
        IntegerSuffix::U16 => Type::U16,
        IntegerSuffix::U32 => Type::U32,
        IntegerSuffix::U64 => Type::U64,
    }
}

const fn float_type(suffix: FloatSuffix) -> Type {
    match suffix {
        FloatSuffix::F32 => Type::F32,
        FloatSuffix::F64 => Type::F64,
    }
}

const fn is_float(ty: &Type) -> bool {
    matches!(ty, Type::F32 | Type::F64)
}

/// The type and value of `literal` where `expected` is expected; `str_type`
/// is the type playing `@str`.
pub(crate) fn evaluate(
    literal: &Literal,
    expected: Option<&Type>,
    str_type: Option<&Type>,
) -> Result<(Type, Value), LiteralFault> {
    match literal {
        Literal::Number {
            body,
            float_body,
            suffix,
        } => {
            let ty = match (suffix, expected) {
                (Some(ty), _) => ty.clone(),
                (None, Some(ty))
                    if is_float(ty) || (!float_body && ty.is_integer()) =>
                {
                    ty.clone()
                }
                (None, Some(ty)) => return Err(LiteralFault::Mismatch(ty.clone())),
                (None, None) => return Err(LiteralFault::Ambiguous),
            };
            number(body, &ty).map(|value| (ty.clone(), value))
        }
        Literal::Str(text) => str_type
            .map(|ty| (ty.clone(), Value::Str(text.clone())))
            .ok_or(LiteralFault::Malformed),
        Literal::Char(ch) => Ok((Type::Char, Value::Char(*ch))),
        Literal::Atom(name) => {
            if expected == Some(&Type::Atom) {
                Ok((Type::Atom, Value::Atom(name.clone())))
            } else {
                Ok((Type::AtomSingleton(name.clone()), Value::Atom(name.clone())))
            }
        }
        Literal::Void => Ok((Type::Void, Value::Void)),
    }
}

/// The value of the numeric body `body` at the numeric type `ty`.
fn number(body: &str, ty: &Type) -> Result<Value, LiteralFault> {
    let out_of_range = || LiteralFault::OutOfRange(ty.clone());
    if is_float(ty) {
        return if *ty == Type::F32 {
            let value: f32 = body.parse().map_err(|_| LiteralFault::Malformed)?;
            if value.is_finite() {
                Ok(Value::f32(value))
            } else {
                Err(out_of_range())
            }
        } else {
            let value: f64 = body.parse().map_err(|_| LiteralFault::Malformed)?;
            if value.is_finite() {
                Ok(Value::f64(value))
            } else {
                Err(out_of_range())
            }
        };
    }
    let wide: i128 = body.parse().map_err(|_| out_of_range())?;
    let value = match ty {
        Type::I8 => i8::try_from(wide).map(Value::I8).ok(),
        Type::I16 => i16::try_from(wide).map(Value::I16).ok(),
        Type::I32 => i32::try_from(wide).map(Value::I32).ok(),
        Type::I64 => i64::try_from(wide).map(Value::I64).ok(),
        Type::U8 => u8::try_from(wide).map(Value::U8).ok(),
        Type::U16 => u16::try_from(wide).map(Value::U16).ok(),
        Type::U32 => u32::try_from(wide).map(Value::U32).ok(),
        Type::U64 => u64::try_from(wide).map(Value::U64).ok(),
        _ => None,
    };
    value.ok_or_else(out_of_range)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn number_literal(token: &str) -> Literal {
        let (body, suffix) = split_number(token);
        let float_body = body.contains(['.', 'e', 'E']);
        let suffix = IntegerSuffix::parse(suffix)
            .map(integer_type)
            .or_else(|| FloatSuffix::parse(suffix).map(float_type));
        Literal::Number {
            body: body.to_owned(),
            float_body,
            suffix,
        }
    }

    #[test]
    fn suffix_fixes_type_and_must_fit() {
        assert_eq!(
            evaluate(&number_literal("7u8"), None, None),
            Ok((Type::U8, Value::U8(7)))
        );
        assert_eq!(
            evaluate(&number_literal("-1u8"), None, None),
            Err(LiteralFault::OutOfRange(Type::U8))
        );
        assert_eq!(
            evaluate(&number_literal("300u8"), None, None),
            Err(LiteralFault::OutOfRange(Type::U8))
        );
    }

    #[test]
    fn unsuffixed_takes_expected_type() {
        assert_eq!(
            evaluate(&number_literal("-5"), Some(&Type::I64), None),
            Ok((Type::I64, Value::I64(-5)))
        );
        assert_eq!(
            evaluate(&number_literal("1"), None, None),
            Err(LiteralFault::Ambiguous)
        );
        assert_eq!(
            evaluate(&number_literal("1.5"), Some(&Type::I32), None),
            Err(LiteralFault::Mismatch(Type::I32))
        );
        assert_eq!(
            evaluate(&number_literal("2"), Some(&Type::F64), None),
            Ok((Type::F64, Value::f64(2.0)))
        );
    }

    #[test]
    fn float_rounding_to_infinity_is_out_of_range() {
        assert_eq!(
            evaluate(&number_literal("1e39f32"), None, None),
            Err(LiteralFault::OutOfRange(Type::F32))
        );
        assert_eq!(
            evaluate(&number_literal("1e39f64"), None, None),
            Ok((Type::F64, Value::f64(1e39)))
        );
        assert_eq!(
            evaluate(&number_literal("0.1f32"), None, None),
            Ok((Type::F32, Value::f32(0.1)))
        );
    }
}
