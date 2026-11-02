//! Number text: the integer and float `to-str` and `parse` rows.
//!
//! `docs/spec/06-runtime.md`, "M3 compiler intrinsic registry": these are the
//! primitive rows whose algorithm loops, so they have no Vibra body and are the
//! toolchain's own code, written once. The reference interpreter calls these
//! functions over its values and a module reaches them through an import, so the
//! two backends print and read numbers by the same code.
//!
//! - `to-str` of an integer is its shortest decimal digits with a leading `-`
//!   only for a negative value; of a float it is the canonical float
//!   serialization of the runtime chapter, `nan`, `inf`, `-inf`, or the shortest
//!   digits that round-trip, which `vibra_ir::canonical_f64_text` writes once for
//!   this row and for the canonical value encoding.
//! - `parse` of an integer is an optional `-` (signed types only) and one or
//!   more ASCII decimal digits and nothing else; `invalid-format` otherwise, and
//!   `out-of-range` for a well-formed value outside the type.
//! - `parse` of a float is the unsuffixed decimal float grammar, rounded to the
//!   nearest value of the type with ties to even; `invalid-format` otherwise, and
//!   `out-of-range` when the rounded value is infinite.
//!
//! Neither direction keeps a NaN's sign or payload: every NaN is written as
//! `nan`, and `parse` has no text for one.

use vibra_ir::external::NumericType;
use vibra_ir::{canonical_f32_text, canonical_f64_text};

use crate::{Sink, Source};

/// What reading text as a number answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Parsed<T> {
    /// The number the text spells.
    Value(T),
    /// The text is not in the grammar: `invalid-format`.
    InvalidFormat,
    /// The text is well formed and the number lies outside the type:
    /// `out-of-range`.
    OutOfRange,
}

/// `T.to-str` of an integer type: the shortest decimal digits of `value`.
pub fn integer_to_str<K: Sink<Unit = u32> + ?Sized>(value: i128, out: &mut K) {
    put_text(&value.to_string(), out);
}

/// `T.to-str` of `f32` or `f64`: the canonical float serialization, without a
/// suffix, of the value whose IEEE bits are `bits` (the low 32 bits for `f32`).
/// Every NaN is `nan` whatever its sign and payload.
pub fn float_to_str<K: Sink<Unit = u32> + ?Sized>(
    numeric: NumericType,
    bits: u64,
    out: &mut K,
) {
    let text = if numeric == NumericType::F32 {
        canonical_f32_text(f32::from_bits(
            u32::try_from(bits & 0xFFFF_FFFF).unwrap_or(0),
        ))
    } else {
        canonical_f64_text(f64::from_bits(bits))
    };
    put_text(&text, out);
}

fn put_text<K: Sink<Unit = u32> + ?Sized>(text: &str, out: &mut K) {
    for scalar in text.chars() {
        out.put(u32::from(scalar));
    }
}

/// The text of `source` when every scalar is ASCII, which is all any number's
/// text holds.
fn ascii<S: Source<Unit = u32>>(source: &S) -> Option<String> {
    source
        .range(0, source.len())
        .map(|scalar| u8::try_from(scalar).ok().filter(u8::is_ascii))
        .map(|byte| byte.map(char::from))
        .collect()
}

/// `T.parse` of an integer type: an optional `-` (signed types only) followed by
/// one or more ASCII decimal digits and nothing else.
pub fn parse_integer<S: Source<Unit = u32>>(
    numeric: NumericType,
    text: &S,
) -> Parsed<i128> {
    let Some(text) = ascii(text) else {
        return Parsed::InvalidFormat;
    };
    let (negative, digits) = match text.strip_prefix('-') {
        Some(digits) if numeric.is_signed() => (true, digits),
        Some(_) => return Parsed::InvalidFormat,
        None => (false, text.as_str()),
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Parsed::InvalidFormat;
    }
    // A well-formed value too large even for `i128` is out of range.
    let value = digits.bytes().try_fold(0_i128, |value, digit| {
        value.checked_mul(10)?.checked_add(i128::from(digit - b'0'))
    });
    let (low, high) = numeric.range();
    match value.map(|value| if negative { -value } else { value }) {
        Some(value) if low <= value && value <= high => Parsed::Value(value),
        _ => Parsed::OutOfRange,
    }
}

/// `T.parse` of `f32` or `f64`: the unsuffixed decimal float literal grammar,
/// rounded to nearest with ties to even. The value is the IEEE bits of the
/// result, in the low 32 bits for `f32`.
pub fn parse_float<S: Source<Unit = u32>>(
    numeric: NumericType,
    text: &S,
) -> Parsed<u64> {
    let Some(text) = ascii(text) else {
        return Parsed::InvalidFormat;
    };
    if !is_float_literal(&text) {
        return Parsed::InvalidFormat;
    }
    let bits = if numeric == NumericType::F32 {
        text.parse::<f32>()
            .ok()
            .filter(|value| value.is_finite())
            .map(|value| u64::from(value.to_bits()))
    } else {
        text.parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .map(f64::to_bits)
    };
    bits.map_or(Parsed::OutOfRange, Parsed::Value)
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
    let mut rest = body.get(whole..).unwrap_or("");
    let mut fraction = false;
    if let Some(after) = rest.strip_prefix('.') {
        let count = digits(after);
        if count == 0 {
            return false;
        }
        fraction = true;
        rest = after.get(count..).unwrap_or("");
    }
    if let Some(after) = rest.strip_prefix(['e', 'E']) {
        let after = after.strip_prefix(['+', '-']).unwrap_or(after);
        let count = digits(after);
        return count > 0 && count == after.len();
    }
    fraction && rest.is_empty()
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::indexing_slicing, clippy::unwrap_used)]
mod tests {
    use super::*;

    struct Text(Vec<u32>);

    impl Text {
        fn of(text: &str) -> Self {
            Self(text.chars().map(u32::from).collect())
        }
    }

    impl Source for Text {
        type Unit = u32;
        type Iter<'a>
            = std::iter::Copied<std::slice::Iter<'a, u32>>
        where
            Self: 'a;

        fn len(&self) -> u64 {
            self.0.len() as u64
        }

        fn range(&self, from: u64, to: u64) -> Self::Iter<'_> {
            let (from, to) =
                (usize::try_from(from).unwrap(), usize::try_from(to).unwrap());
            self.0.get(from..to).unwrap_or(&[]).iter().copied()
        }
    }

    struct Collect(String);

    impl Sink for Collect {
        type Unit = u32;
        fn put(&mut self, unit: u32) {
            self.0.push(char::from_u32(unit).unwrap());
        }
    }

    fn float_text(numeric: NumericType, bits: u64) -> String {
        let mut out = Collect(String::new());
        float_to_str(numeric, bits, &mut out);
        out.0
    }

    #[test]
    fn every_nan_is_written_as_nan_whatever_its_sign_and_payload() {
        for payload in [0_u64, 1, 0x7_FFFF_FFFF_FFFF, 0x4_0000_0000_0000] {
            for sign in [0_u64, 1 << 63] {
                let bits = sign | 0x7FF0_0000_0000_0000 | payload.max(1);
                assert_eq!(float_text(NumericType::F64, bits), "nan", "{bits:#x}");
            }
        }
        for payload in [1_u32, 0x40_0000, 0x7F_FFFF, 0x12_3456] {
            for sign in [0_u32, 1 << 31] {
                let bits = sign | 0x7F80_0000 | payload;
                assert_eq!(
                    float_text(NumericType::F32, u64::from(bits)),
                    "nan",
                    "{bits:#x}"
                );
            }
        }
    }

    #[test]
    fn the_float_grammar_has_a_digit_before_the_point_and_a_fraction_or_exponent() {
        for valid in [
            "0.0", "-0.0", "1.5", "1e5", "1E5", "1e+5", "-1.5e-7", "007.50",
        ] {
            assert!(is_float_literal(valid), "{valid:?}");
        }
        for invalid in [
            "", "-", "1", ".5", "1.", "1.e5", "e5", "1e", "1e+", "+1.0", "1.0f64",
            " 1.0",
        ] {
            assert!(!is_float_literal(invalid), "{invalid:?}");
        }
    }

    #[test]
    fn integer_text_follows_the_grammar_and_the_type_range() {
        let parse = |numeric, text: &str| parse_integer(numeric, &Text::of(text));
        assert_eq!(parse(NumericType::I8, "-128"), Parsed::Value(-128));
        assert_eq!(parse(NumericType::I8, "-129"), Parsed::OutOfRange);
        assert_eq!(parse(NumericType::U8, "255"), Parsed::Value(255));
        assert_eq!(parse(NumericType::U8, "256"), Parsed::OutOfRange);
        assert_eq!(parse(NumericType::U8, "-0"), Parsed::InvalidFormat);
        assert_eq!(parse(NumericType::I8, "-0"), Parsed::Value(0));
        assert_eq!(parse(NumericType::I8, "-"), Parsed::InvalidFormat);
        assert_eq!(parse(NumericType::I8, "\u{661}"), Parsed::InvalidFormat);
        assert_eq!(
            parse(NumericType::U64, &"9".repeat(60)),
            Parsed::OutOfRange,
            "beyond even i128 is out of range"
        );
        assert_eq!(
            parse(NumericType::U64, &format!("{}x", "9".repeat(60))),
            Parsed::InvalidFormat,
            "the format is checked before the range"
        );
    }
}
