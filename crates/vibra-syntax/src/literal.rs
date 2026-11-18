//! Literal validation and decoding.
//!
//! The literal rules are those of the source chapter's "Literals"
//! (`docs/spec/01-source-language.md`): decimal integers and floats with an
//! optional exact suffix, double-quoted strings with five simple escapes and
//! `\u{…}`, doc strings, and single-quoted character literals. The lexer uses
//! the scanners here to find a token's extent and validity; the data grammar
//! uses the decoders to turn a valid token into its decoded value.
//!
//! A malformed literal and an out-of-range literal are different errors.
//! Nothing here checks range: `300u8` is a well-formed `U8` literal.

use std::fmt;
use std::str::Chars;

/// The exact type an integer suffix fixes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IntegerSuffix {
    /// `i8`.
    I8,
    /// `i16`.
    I16,
    /// `i32`.
    I32,
    /// `i64`.
    I64,
    /// `u8`.
    U8,
    /// `u16`.
    U16,
    /// `u32`.
    U32,
    /// `u64`.
    U64,
}

impl IntegerSuffix {
    /// Every integer suffix, in the order of the chapter's EBNF.
    pub const ALL: &'static [Self] = &[
        Self::I8,
        Self::I16,
        Self::I32,
        Self::I64,
        Self::U8,
        Self::U16,
        Self::U32,
        Self::U64,
    ];

    /// The suffix as written.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
        }
    }

    /// The suffix spelled `text`, if any.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|suffix| suffix.as_str() == text)
    }
}

impl fmt::Display for IntegerSuffix {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// The exact type a float suffix fixes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FloatSuffix {
    /// `f32`.
    F32,
    /// `f64`.
    F64,
}

impl FloatSuffix {
    /// The suffix as written.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::F32 => "f32",
            Self::F64 => "f64",
        }
    }

    /// The suffix spelled `text`, if any.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "f32" => Some(Self::F32),
            "f64" => Some(Self::F64),
            _ => None,
        }
    }
}

impl fmt::Display for FloatSuffix {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Whether a number token is an integer or a float.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NumberKind {
    Integer,
    Float,
}

/// The extent and validity of a number token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NumberScan {
    /// The token's length in bytes.
    pub(crate) len: usize,
    /// Integer or float, by its body and suffix.
    pub(crate) kind: NumberKind,
    /// Whether the token is a well-formed literal.
    pub(crate) valid: bool,
}

/// Whether `character` continues a word: an identifier, an atom part, or the
/// trailing text of a number.
pub(crate) fn continues_word(character: char) -> bool {
    character.is_ascii_alphanumeric()
        || character == '_'
        || (!character.is_ascii() && character.is_alphanumeric())
}

fn skip_digits(bytes: &[u8], mut at: usize) -> usize {
    while bytes.get(at).is_some_and(u8::is_ascii_digit) {
        at += 1;
    }
    at
}

/// Scans the number token at the start of `rest`, which starts with a digit
/// or with `-` directly before a digit.
///
/// The body is `digits`, an optional `.` followed by digits, and an optional
/// exponent. Every word character directly after the body is part of the
/// token: a known suffix, or trailing text that makes it malformed.
pub(crate) fn scan_number(rest: &str) -> NumberScan {
    let bytes = rest.as_bytes();
    let mut at = usize::from(bytes.first() == Some(&b'-'));
    at = skip_digits(bytes, at);
    let mut float = false;
    if bytes.get(at) == Some(&b'.') && bytes.get(at + 1).is_some_and(u8::is_ascii_digit)
    {
        at = skip_digits(bytes, at + 1);
        float = true;
    }
    if matches!(bytes.get(at), Some(b'e' | b'E')) {
        let digits_from = match bytes.get(at + 1) {
            Some(b'+' | b'-') => at + 2,
            _ => at + 1,
        };
        if bytes.get(digits_from).is_some_and(u8::is_ascii_digit) {
            at = skip_digits(bytes, digits_from);
            float = true;
        }
    }
    let body = at;
    let mut end = body;
    for character in rest.get(body..).unwrap_or_default().chars() {
        if !continues_word(character) {
            break;
        }
        end += character.len_utf8();
    }
    let suffix = rest.get(body..end).unwrap_or_default();
    let body_kind = if float {
        NumberKind::Float
    } else {
        NumberKind::Integer
    };
    let (kind, valid) = if suffix.is_empty() {
        (body_kind, true)
    } else if IntegerSuffix::parse(suffix).is_some() {
        (body_kind, !float)
    } else if FloatSuffix::parse(suffix).is_some() {
        (NumberKind::Float, true)
    } else {
        (body_kind, false)
    };
    NumberScan {
        len: end,
        kind,
        valid,
    }
}

/// Splits a well-formed number token into its body and its suffix text.
pub(crate) fn split_number(token: &str) -> (&str, &str) {
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

/// The extent of a string or doc string token at the start of `rest`, which
/// starts with `"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct QuotedScan {
    /// The token's length in bytes.
    pub(crate) len: usize,
    /// Whether the closing delimiter was found before the end of the file.
    pub(crate) terminated: bool,
    /// Whether the token is a doc string, delimited by `"""`.
    pub(crate) doc: bool,
}

/// Scans a string or a doc string. An unterminated one runs to the end of the
/// file and keeps its bytes.
pub(crate) fn scan_quoted(rest: &str) -> QuotedScan {
    const DOC: &str = "\"\"\"";
    if let Some(after) = rest.strip_prefix(DOC) {
        return match after.find(DOC) {
            Some(position) => QuotedScan {
                len: DOC.len() + position + DOC.len(),
                terminated: true,
                doc: true,
            },
            None => QuotedScan {
                len: rest.len(),
                terminated: false,
                doc: true,
            },
        };
    }
    let mut characters = rest.char_indices().skip(1);
    while let Some((at, character)) = characters.next() {
        match character {
            '"' => {
                return QuotedScan {
                    len: at + 1,
                    terminated: true,
                    doc: false,
                };
            }
            '\\' => {
                characters.next();
            }
            _ => {}
        }
    }
    QuotedScan {
        len: rest.len(),
        terminated: false,
        doc: false,
    }
}

/// Scans a character literal at the start of `rest`, which starts with `'`.
///
/// It ends after the next unescaped `'` on the same line. One that meets a
/// line break or the end of the file first ends before it and is malformed.
/// Returns the length and whether the closing quote was found.
pub(crate) fn scan_character(rest: &str) -> (usize, bool) {
    let mut characters = rest.char_indices().skip(1);
    while let Some((at, character)) = characters.next() {
        match character {
            '\'' => return (at + 1, true),
            '\n' | '\r' => return (at, false),
            '\\' => match characters.next() {
                Some((at, '\n' | '\r')) => return (at, false),
                Some(_) => {}
                None => return (rest.len(), false),
            },
            _ => {}
        }
    }
    (rest.len(), false)
}

/// Decodes the escape after a `\`. `\'` is an escape only in a character
/// literal.
fn decode_escape(characters: &mut Chars<'_>, in_character: bool) -> Option<char> {
    match characters.next()? {
        '\\' => Some('\\'),
        '"' => Some('"'),
        'n' => Some('\n'),
        'r' => Some('\r'),
        't' => Some('\t'),
        '\'' if in_character => Some('\''),
        'u' => {
            if characters.next()? != '{' {
                return None;
            }
            let mut value = 0_u32;
            let mut digits = 0_usize;
            loop {
                let character = characters.next()?;
                if character == '}' {
                    break;
                }
                digits += 1;
                if digits > 6 {
                    return None;
                }
                value = value * 16 + character.to_digit(16)?;
            }
            if digits == 0 {
                return None;
            }
            // Rejects a surrogate and a value above U+10FFFF.
            char::from_u32(value)
        }
        _ => None,
    }
}

/// Decodes a terminated string token to its scalar sequence, or `None` when
/// an escape is malformed or unsupported.
#[must_use]
pub fn decode_string(token: &str) -> Option<String> {
    if token.starts_with("\"\"\"") {
        return None;
    }
    let interior = token.strip_prefix('"')?.strip_suffix('"')?;
    let mut decoded = String::with_capacity(interior.len());
    let mut characters = interior.chars();
    while let Some(character) = characters.next() {
        match character {
            '\\' => decoded.push(decode_escape(&mut characters, false)?),
            '"' => return None,
            other => decoded.push(other),
        }
    }
    Some(decoded)
}

/// Decodes a character literal token to its one scalar, or `None` when it is
/// empty, holds more than one scalar, or has a malformed escape.
#[must_use]
pub fn decode_character(token: &str) -> Option<char> {
    let interior = token.strip_prefix('\'')?.strip_suffix('\'')?;
    let mut characters = interior.chars();
    let scalar = match characters.next()? {
        '\\' => decode_escape(&mut characters, true)?,
        '\'' => return None,
        other => other,
    };
    characters.next().is_none().then_some(scalar)
}

/// The canonical spelling of a string: `\"`, `\\`, `\n`, `\r`, and `\t` for
/// those scalars, `\u{H}` with the fewest upper-case digits for every other
/// scalar below U+0020 and for U+007F, and every other scalar written
/// directly (`docs/spec/04-programs-and-packages.md`, "Decoded values and
/// canonical bytes").
#[must_use]
pub fn canonical_string(value: &str) -> String {
    let mut spelled = String::with_capacity(value.len() + 2);
    spelled.push('"');
    for character in value.chars() {
        push_canonical_scalar(&mut spelled, character, '"');
    }
    spelled.push('"');
    spelled
}

/// The canonical spelling of a character literal, with `\'` for the quote.
#[must_use]
pub fn canonical_character(value: char) -> String {
    let mut spelled = String::with_capacity(8);
    spelled.push('\'');
    push_canonical_scalar(&mut spelled, value, '\'');
    spelled.push('\'');
    spelled
}

fn push_canonical_scalar(spelled: &mut String, character: char, quote: char) {
    match character {
        '\\' => spelled.push_str("\\\\"),
        '\n' => spelled.push_str("\\n"),
        '\r' => spelled.push_str("\\r"),
        '\t' => spelled.push_str("\\t"),
        other if other == quote => {
            spelled.push('\\');
            spelled.push(other);
        }
        other if u32::from(other) < 0x20 || other == '\u{7f}' => {
            spelled.push_str(&format!("\\u{{{:X}}}", u32::from(other)));
        }
        other => spelled.push(other),
    }
}

/// The decoded value of an integer body as a sign and minimal digits, so that
/// `007` and `7` are one value and `-0` is `0`.
pub(crate) fn integer_value(body: &str) -> String {
    let (negative, digits) = match body.strip_prefix('-') {
        Some(digits) => (true, digits),
        None => (false, body),
    };
    let digits = digits.trim_start_matches('0');
    if digits.is_empty() {
        "0".to_owned()
    } else if negative {
        format!("-{digits}")
    } else {
        digits.to_owned()
    }
}

/// The exact decimal value of an unsuffixed float body, as a sign, minimal
/// significant digits, and a power of ten. The sign of zero is kept.
pub(crate) fn decimal_value(body: &str) -> String {
    let (negative, unsigned) = match body.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, body),
    };
    let sign = if negative { "-" } else { "" };
    let (mantissa_text, exponent_text) = match unsigned.find(['e', 'E']) {
        Some(position) => (
            unsigned.get(..position).unwrap_or_default(),
            unsigned.get(position + 1..).unwrap_or_default(),
        ),
        None => (unsigned, "0"),
    };
    let (whole, fraction) =
        mantissa_text.split_once('.').unwrap_or((mantissa_text, ""));
    let Ok(exponent) = exponent_text.trim_start_matches('+').parse::<i64>() else {
        return format!("{sign}{unsigned}");
    };
    let digits = format!("{whole}{fraction}");
    let significant = digits.trim_start_matches('0');
    if significant.is_empty() {
        return format!("{sign}0");
    }
    let trimmed = significant.trim_end_matches('0');
    let trailing = significant.len() - trimmed.len();
    let (Ok(fraction_len), Ok(trailing)) =
        (i64::try_from(fraction.len()), i64::try_from(trailing))
    else {
        return format!("{sign}{unsigned}");
    };
    let power = exponent
        .saturating_sub(fraction_len)
        .saturating_add(trailing);
    format!("{sign}{trimmed}e{power}")
}

#[cfg(test)]
mod tests {
    use super::{
        NumberKind, canonical_character, canonical_string, decimal_value,
        decode_character, decode_string, integer_value, scan_character, scan_number,
        scan_quoted, split_number,
    };

    #[test]
    fn numbers_take_their_suffix_and_trailing_text() {
        let scan = scan_number("1i128 ");
        assert_eq!(
            (scan.len, scan.kind, scan.valid),
            (5, NumberKind::Integer, false)
        );
        let scan = scan_number("2.5i32}");
        assert_eq!(
            (scan.len, scan.kind, scan.valid),
            (6, NumberKind::Float, false)
        );
        let scan = scan_number("3u8x");
        assert_eq!((scan.len, scan.valid), (4, false));
        let scan = scan_number("-7i64 ");
        assert_eq!(
            (scan.len, scan.kind, scan.valid),
            (5, NumberKind::Integer, true)
        );
        let scan = scan_number("1e3");
        assert_eq!(
            (scan.len, scan.kind, scan.valid),
            (3, NumberKind::Float, true)
        );
        let scan = scan_number("2f64");
        assert_eq!(
            (scan.len, scan.kind, scan.valid),
            (4, NumberKind::Float, true)
        );
        let scan = scan_number("1u8\"x\"");
        assert_eq!((scan.len, scan.valid), (3, true));
        let scan = scan_number("1.foo");
        assert_eq!(
            (scan.len, scan.kind, scan.valid),
            (1, NumberKind::Integer, true)
        );
    }

    #[test]
    fn a_number_splits_into_body_and_suffix() {
        assert_eq!(split_number("-7i64"), ("-7", "i64"));
        assert_eq!(split_number("2.5e-3f32"), ("2.5e-3", "f32"));
        assert_eq!(split_number("1e3"), ("1e3", ""));
        assert_eq!(split_number("2f64"), ("2", "f64"));
    }

    #[test]
    fn strings_decode_their_escapes() {
        assert_eq!(decode_string("\"a\\n\"").as_deref(), Some("a\n"));
        assert_eq!(decode_string("\"\\u{61}\"").as_deref(), Some("a"));
        assert_eq!(
            decode_string("\"\\u{10FFFF}\"").as_deref(),
            Some("\u{10FFFF}")
        );
        assert_eq!(decode_string("\"\\q\""), None);
        assert_eq!(decode_string("\"\\u{d800}\""), None);
        assert_eq!(decode_string("\"\\u{110000}\""), None);
        assert_eq!(decode_string("\"\\u{}\""), None);
        assert_eq!(decode_string("\"\\u{0000061}\""), None);
        assert_eq!(decode_string("\"\\'\""), None);
    }

    #[test]
    fn quoted_tokens_find_their_end() {
        assert_eq!(scan_quoted("\"open").len, 5);
        assert!(!scan_quoted("\"open").terminated);
        assert_eq!(scan_quoted("\"a\\\"b\" x").len, 6);
        let doc = scan_quoted("\"\"\"x\"\"\"}");
        assert_eq!((doc.len, doc.terminated, doc.doc), (7, true, true));
        assert_eq!(scan_quoted("\"\" x").len, 2);
    }

    #[test]
    fn characters_hold_exactly_one_scalar() {
        assert_eq!(decode_character("'x'"), Some('x'));
        assert_eq!(decode_character("'\\''"), Some('\''));
        assert_eq!(decode_character("'\\u{41}'"), Some('A'));
        assert_eq!(decode_character("''"), None);
        assert_eq!(decode_character("'ab'"), None);
        assert_eq!(decode_character("'\\q'"), None);
        assert_eq!(decode_character("'\\u{d800}'"), None);
        assert_eq!(scan_character("'' b"), (2, true));
        assert_eq!(scan_character("'ab' "), (4, true));
        assert_eq!(scan_character("'a\nb'"), (2, false));
    }

    #[test]
    fn canonical_spellings_escape_the_control_scalars() {
        assert_eq!(
            canonical_string("a\"\\\n\u{1}\u{7f}é"),
            "\"a\\\"\\\\\\n\\u{1}\\u{7F}é\""
        );
        assert_eq!(canonical_character('\''), "'\\''");
    }

    #[test]
    fn decoded_numbers_ignore_spelling_only() {
        assert_eq!(integer_value("007"), "7");
        assert_eq!(integer_value("-0"), "0");
        assert_eq!(decimal_value("1.50"), decimal_value("15e-1"));
        assert_eq!(decimal_value("0.0"), "0");
        assert_eq!(decimal_value("-0.0"), "-0");
        assert_ne!(decimal_value("1.5"), decimal_value("1.05"));
    }
}
