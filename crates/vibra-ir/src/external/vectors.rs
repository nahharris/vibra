//! The sample vectors of the number, character, text, and bytes rows.
//!
//! One table holds the operands and the specified outcome of every sample.
//! Every executor is held to it, so none can drift on a boundary: overflow,
//! division by zero, the signed minimum divided by -1, the sign of a
//! remainder, a shift at or past the width, a conversion at the edge of its
//! target, a scalar at the edge of the surrogate range, a NaN of any sign or
//! payload, or the switch between decimal and scientific float text.
//!
//! No outcome is read from an executor. Each is computed here by Rust's own
//! types: the checked and saturating integer operations, `try_from`,
//! `char::from_u32`, the `str` and slice operations, IEEE 754 float
//! arithmetic, the standard parsers behind hand-written recognizers of the
//! chapter's grammars, and a float text assembled by hand from Rust's
//! shortest scientific digits. The registry's own rules are written out
//! where they differ from Rust's: the remainder of the signed minimum and
//! -1 is zero, a checked shift is checked by the exact product
//! `left * 2^amount`, and every NaN is observed as the canonical one.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::OnceLock;

use super::compiler::{CompilerIntrinsic, FloatOp, FloatType, IntegerOp, IntegerType};
use crate::Value;
use crate::encode::{CANONICAL_NAN_F32, CANONICAL_NAN_F64};

/// What the specification says a row does on one sample.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The value the row returns: a number, a `Bool` (as
    /// [`Value::bool`]), a `Str`, or `Bytes`.
    Value(Value),
    /// `Result.ok(value: …)`.
    Ok(Value),
    /// `Result.err(error: …)`, naming the case of `ArithmeticError` or
    /// `ConversionError`.
    Err(&'static str),
    /// `Option.some(value: …)`.
    Some(Value),
    /// `Option.none()`.
    None,
    /// The case of `Ordering`: `less`, `equal`, or `greater`.
    Order(&'static str),
}

fn case(name: &str, fields: Vec<(String, Value)>) -> Value {
    Value::Variant {
        case: name.to_owned(),
        fields,
    }
}

impl Outcome {
    /// The value of this outcome at the row's result type.
    #[must_use]
    pub fn value(&self) -> Value {
        let field = |name: &str, value: Value| vec![(name.to_owned(), value)];
        match self {
            Self::Value(value) => value.clone(),
            Self::Ok(value) => case("ok", field("value", value.clone())),
            Self::Err(error) => case("err", field("error", case(error, Vec::new()))),
            Self::Some(value) => case("some", field("value", value.clone())),
            Self::None => case("none", Vec::new()),
            Self::Order(order) => case(order, Vec::new()),
        }
    }
}

/// One sample: the operands a row is applied to and what it returns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Vector {
    /// The row.
    pub intrinsic: CompilerIntrinsic,
    /// The operands, in the order of the row's parameters.
    pub operands: Vec<Value>,
    /// The specified result.
    pub outcome: Outcome,
}

/// Every sample of `intrinsic`, none for a row that has none.
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
        sequences(&mut all);
        integer_texts(&mut all);
        floats(&mut all);
        float_text(&mut all);
        let mut table: BTreeMap<CompilerIntrinsic, Vec<Vector>> = BTreeMap::new();
        for vector in all {
            table.entry(vector.intrinsic).or_default().push(vector);
        }
        table
    })
}

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

const fn order(order: Ordering) -> &'static str {
    match order {
        Ordering::Less => "less",
        Ordering::Equal => "equal",
        Ordering::Greater => "greater",
    }
}

// -- samples ------------------------------------------------------------------

/// The shift amounts every width is tried at: none, one, the widest legal,
/// the first illegal, one past it, and the largest `U32`.
fn amounts(integer: IntegerType) -> Vec<u32> {
    let bits = integer.bits();
    let mut amounts = vec![
        0,
        1,
        bits.saturating_sub(1),
        bits,
        bits.saturating_add(1),
        u32::MAX,
    ];
    amounts.sort_unstable();
    amounts.dedup();
    amounts
}

/// The operands of a binary row: the bounds, the values around zero, and
/// the points where a sum, a product, and a double leave the type.
fn pair_samples(integer: IntegerType) -> Vec<i128> {
    let (min, max) = integer.range();
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

/// The operands of a conversion from `integer`: its bounds, the values
/// around zero, and the edges of every integer type.
fn conversion_samples(integer: IntegerType) -> Vec<i128> {
    let (min, max) = integer.range();
    let mut samples = vec![min, min + 1, -1, 0, 1, max - 1, max];
    for other in IntegerType::ALL {
        let (low, high) = other.range();
        samples.extend([low - 1, low, low + 1, high - 1, high, high + 1]);
    }
    samples.retain(|value| min <= *value && *value <= max);
    samples.sort_unstable();
    samples.dedup();
    samples
}

// -- integers -----------------------------------------------------------------

/// `ok` of an exact value, or `overflow`.
fn checked<T>(value: Option<T>, make: fn(T) -> Value) -> Outcome {
    value.map_or(Outcome::Err("overflow"), |value| Outcome::Ok(make(value)))
}

/// The rows of one integer type, from the operations of the Rust type.
macro_rules! integer_rows {
    ($name:ident, $t:ty, $integer:expr, $make:path) => {
        fn $name(out: &mut Vec<Vector>) {
            let integer: IntegerType = $integer;
            let row = |op: IntegerOp| CompilerIntrinsic::Integer(integer, op);
            let samples = pair_samples(integer)
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
                    // signed minimum divided by -1; the registry tells them
                    // apart.
                    push(
                        out,
                        row(IntegerOp::DivChecked),
                        operands(),
                        if right == 0 {
                            Outcome::Err("divisionByZero")
                        } else {
                            checked(left.checked_div(right), $make)
                        },
                    );
                    // The signed minimum and -1 have the remainder zero.
                    push(
                        out,
                        row(IntegerOp::RemChecked),
                        operands(),
                        if right == 0 {
                            Outcome::Err("divisionByZero")
                        } else {
                            Outcome::Ok($make(left.wrapping_rem(right)))
                        },
                    );
                    push(
                        out,
                        row(IntegerOp::Add),
                        operands(),
                        Outcome::Value($make(left.saturating_add(right))),
                    );
                    push(
                        out,
                        row(IntegerOp::Sub),
                        operands(),
                        Outcome::Value($make(left.saturating_sub(right))),
                    );
                    push(
                        out,
                        row(IntegerOp::Mul),
                        operands(),
                        Outcome::Value($make(left.saturating_mul(right))),
                    );
                    push(
                        out,
                        row(IntegerOp::Equal),
                        operands(),
                        Outcome::Value(Value::bool(left == right)),
                    );
                    push(
                        out,
                        row(IntegerOp::Compare),
                        operands(),
                        Outcome::Order(order(left.cmp(&right))),
                    );
                }
                if integer.is_signed() {
                    push(
                        out,
                        row(IntegerOp::NegChecked),
                        vec![$make(left)],
                        checked(left.checked_neg(), $make),
                    );
                    push(
                        out,
                        row(IntegerOp::Neg),
                        vec![$make(left)],
                        Outcome::Value($make(<$t>::default().saturating_sub(left))),
                    );
                }
                for amount in amounts(integer) {
                    let operands = || vec![$make(left), Value::U32(amount)];
                    // `left * 2^amount` clamped to the bounds, and
                    // `floor(left / 2^amount)`, for every amount.
                    let (low, high) = integer.range();
                    let wide = i128::from(left);
                    let product = if wide == 0 {
                        0
                    } else {
                        wide.saturating_mul(
                            1_i128
                                .checked_shl(amount)
                                .filter(|factor| *factor > 0)
                                .unwrap_or(i128::MAX),
                        )
                    };
                    if let (Ok(shifted), Ok(floored)) = (
                        <$t>::try_from(product.clamp(low, high)),
                        <$t>::try_from(wide >> amount.min(127)),
                    ) {
                        push(
                            out,
                            row(IntegerOp::ShiftLeft),
                            operands(),
                            Outcome::Value($make(shifted)),
                        );
                        push(
                            out,
                            row(IntegerOp::ShiftRight),
                            operands(),
                            Outcome::Value($make(floored)),
                        );
                    }
                    if amount >= integer.bits() {
                        push(
                            out,
                            row(IntegerOp::ShiftLeftChecked),
                            operands(),
                            Outcome::Err("invalidShift"),
                        );
                        push(
                            out,
                            row(IntegerOp::ShiftRightChecked),
                            operands(),
                            Outcome::Err("invalidShift"),
                        );
                        continue;
                    }
                    // `left * 2^amount`, exactly: a shift of a `U64` by up
                    // to 63 stays inside `i128`.
                    let exact = i128::from(left) * (1_i128 << amount);
                    push(
                        out,
                        row(IntegerOp::ShiftLeftChecked),
                        operands(),
                        checked(<$t>::try_from(exact).ok(), $make),
                    );
                    // Rust's `>>` is arithmetic for a signed type, so it is
                    // `floor(left / 2^amount)`, and logical for an unsigned
                    // one, where the two agree.
                    push(
                        out,
                        row(IntegerOp::ShiftRightChecked),
                        operands(),
                        Outcome::Ok($make(left >> amount)),
                    );
                }
            }
        }
    };
}

integer_rows!(rows_i8, i8, IntegerType::I8, Value::I8);
integer_rows!(rows_i16, i16, IntegerType::I16, Value::I16);
integer_rows!(rows_i32, i32, IntegerType::I32, Value::I32);
integer_rows!(rows_i64, i64, IntegerType::I64, Value::I64);
integer_rows!(rows_u8, u8, IntegerType::U8, Value::U8);
integer_rows!(rows_u16, u16, IntegerType::U16, Value::U16);
integer_rows!(rows_u32, u32, IntegerType::U32, Value::U32);
integer_rows!(rows_u64, u64, IntegerType::U64, Value::U64);

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

/// The conversion of `$operand` (a `$source`) to `$target`, by Rust's
/// conversion between the two.
macro_rules! convert_to {
    ($out:ident, $source:expr, $make_source:path, $operand:expr,
     $target:ty, $target_type:expr, $make_target:path) => {
        let target: IntegerType = $target_type;
        if $source != target {
            #[allow(
                clippy::useless_conversion,
                clippy::unnecessary_fallible_conversions
            )]
            let converted = <$target>::try_from($operand);
            let outcome = match converted {
                Ok(value)
                    if CompilerIntrinsic::conversion_is_total($source, target) =>
                {
                    Outcome::Value($make_target(value))
                }
                Ok(value) => Outcome::Ok($make_target(value)),
                Err(_) => Outcome::Err("outOfRange"),
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
    ($name:ident, $t:ty, $integer:expr, $make:path) => {
        fn $name(out: &mut Vec<Vector>) {
            let source: IntegerType = $integer;
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
                    IntegerType::I8,
                    Value::I8
                );
                convert_to!(
                    out,
                    source,
                    $make,
                    operand,
                    i16,
                    IntegerType::I16,
                    Value::I16
                );
                convert_to!(
                    out,
                    source,
                    $make,
                    operand,
                    i32,
                    IntegerType::I32,
                    Value::I32
                );
                convert_to!(
                    out,
                    source,
                    $make,
                    operand,
                    i64,
                    IntegerType::I64,
                    Value::I64
                );
                convert_to!(
                    out,
                    source,
                    $make,
                    operand,
                    u8,
                    IntegerType::U8,
                    Value::U8
                );
                convert_to!(
                    out,
                    source,
                    $make,
                    operand,
                    u16,
                    IntegerType::U16,
                    Value::U16
                );
                convert_to!(
                    out,
                    source,
                    $make,
                    operand,
                    u32,
                    IntegerType::U32,
                    Value::U32
                );
                convert_to!(
                    out,
                    source,
                    $make,
                    operand,
                    u64,
                    IntegerType::U64,
                    Value::U64
                );
            }
        }
    };
}

conversions_from!(from_i8, i8, IntegerType::I8, Value::I8);
conversions_from!(from_i16, i16, IntegerType::I16, Value::I16);
conversions_from!(from_i32, i32, IntegerType::I32, Value::I32);
conversions_from!(from_i64, i64, IntegerType::I64, Value::I64);
conversions_from!(from_u8, u8, IntegerType::U8, Value::U8);
conversions_from!(from_u16, u16, IntegerType::U16, Value::U16);
conversions_from!(from_u32, u32, IntegerType::U32, Value::U32);
conversions_from!(from_u64, u64, IntegerType::U64, Value::U64);

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

// -- characters and sequences -------------------------------------------------

/// The `Char` operands: the edges of every UTF-8 length and of the
/// surrogate gap.
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

/// The `U32` operands of `char.fromU32`: scalars, the surrogate range and
/// its edges, the first value past U+10FFFF, and the largest `U32`.
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

/// The strings the text rows are tried on: empty, one scalar of each UTF-8
/// width, scalars at the edges of their widths, and strings that share a
/// prefix.
const TEXTS: [&str; 11] = [
    "",
    "a",
    "ab",
    "b",
    "\u{E9}",
    "h\u{E9}",
    "\u{FFFF}",
    "\u{1D11E}",
    "h\u{1D11E}llo",
    "\u{0}\u{7F}",
    "\u{10FFFF}",
];

/// The byte strings the bytes rows and the UTF-8 decoder are tried on:
/// well-formed sequences of each width, and ill-formed ones (a stray
/// continuation byte, an overlong form, a surrogate, a value above
/// U+10FFFF, and a truncated sequence).
const BYTE_STRINGS: [&[u8]; 17] = [
    &[],
    &[0],
    &[1, 2],
    &[2],
    &[1, 2, 3],
    &[0x61, 0xC3, 0xA9],
    &[0xE2, 0x82, 0xAC],
    &[0xF0, 0x9D, 0x84, 0x9E],
    &[0xFF],
    &[0x80],
    &[0xC0, 0x80],
    &[0xC3],
    &[0xE0, 0x80, 0x80],
    &[0xED, 0xA0, 0x80],
    &[0xF4, 0x90, 0x80, 0x80],
    &[0xE2, 0x82],
    &[0xF0, 0x9D, 0x84],
];

/// The ranges `slice` is tried on: inside, empty at each end, reversed, and
/// past the end by one and by a lot.
const RANGES: [(u64, u64); 9] = [
    (0, 0),
    (0, 1),
    (1, 2),
    (0, 3),
    (2, 1),
    (3, 3),
    (4, 4),
    (0, 4),
    (0, u64::MAX),
];

/// The piece `start..end` of `all`, when the range is within it.
fn piece<T>(all: &[T], start: u64, end: u64) -> Option<&[T]> {
    let start = usize::try_from(start).ok()?;
    let end = usize::try_from(end).ok()?;
    if start <= end {
        all.get(start..end)
    } else {
        None
    }
}

fn count(length: usize) -> Value {
    Value::U64(u64::try_from(length).unwrap_or(u64::MAX))
}

fn sequences(out: &mut Vec<Vector>) {
    use CompilerIntrinsic as C;
    let text = |value: &str| Value::Str(value.to_owned());
    let bytes = |value: &[u8]| Value::Bytes(value.to_vec());
    for first in TEXTS {
        let scalars = first.chars().collect::<Vec<_>>();
        push(
            out,
            C::StrLength,
            vec![text(first)],
            Outcome::Value(count(scalars.len())),
        );
        push(
            out,
            C::StrToUtf8,
            vec![text(first)],
            Outcome::Value(bytes(first.as_bytes())),
        );
        for (start, end) in RANGES {
            push(
                out,
                C::StrSlice,
                vec![text(first), Value::U64(start), Value::U64(end)],
                piece(&scalars, start, end).map_or(Outcome::None, |piece| {
                    Outcome::Some(text(&piece.iter().collect::<String>()))
                }),
            );
        }
        for second in TEXTS {
            let pair = || vec![text(first), text(second)];
            push(
                out,
                C::StrConcat,
                pair(),
                Outcome::Value(text(&format!("{first}{second}"))),
            );
            push(
                out,
                C::StrEqual,
                pair(),
                Outcome::Value(Value::bool(first == second)),
            );
        }
    }
    for first in BYTE_STRINGS {
        push(
            out,
            C::StrFromUtf8,
            vec![bytes(first)],
            std::str::from_utf8(first).map_or(Outcome::Err("invalidFormat"), |valid| {
                Outcome::Ok(text(valid))
            }),
        );
        push(
            out,
            C::BytesLength,
            vec![bytes(first)],
            Outcome::Value(count(first.len())),
        );
        for (start, end) in RANGES {
            push(
                out,
                C::BytesSlice,
                vec![bytes(first), Value::U64(start), Value::U64(end)],
                piece(first, start, end)
                    .map_or(Outcome::None, |piece| Outcome::Some(bytes(piece))),
            );
        }
        for second in BYTE_STRINGS {
            let pair = || vec![bytes(first), bytes(second)];
            push(
                out,
                C::BytesConcat,
                pair(),
                Outcome::Value(bytes(&[first, second].concat())),
            );
            push(
                out,
                C::BytesEqual,
                pair(),
                Outcome::Value(Value::bool(first == second)),
            );
        }
    }
}

// -- the generator ------------------------------------------------------------

/// A xorshift generator with a fixed seed, so the samples never change.
struct Random(u64);

impl Random {
    const fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn digit(&mut self) -> char {
        char::from(b'0' + u8::try_from(self.next() % 10).unwrap_or(0))
    }
}

// -- integer text -------------------------------------------------------------

/// What `parse` answers for `text`: the grammar first (an optional `-` for
/// a signed type, then one or more ASCII digits, and nothing else), then
/// Rust's parser, whose only failure once the grammar holds is a value
/// outside the type.
fn parse_integer_expected<T: std::str::FromStr>(
    integer: IntegerType,
    text: &str,
    make: fn(T) -> Value,
) -> Outcome {
    let digits = match text.strip_prefix('-') {
        Some(digits) if integer.is_signed() => digits,
        Some(_) => return Outcome::Err("invalidFormat"),
        None => text,
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Outcome::Err("invalidFormat");
    }
    text.parse::<T>()
        .map_or(Outcome::Err("outOfRange"), |value| Outcome::Ok(make(value)))
}

/// Malformed integer text of every kind.
const MALFORMED_INTEGERS: [&str; 36] = [
    "",
    "-",
    "--1",
    "-+1",
    "+",
    "+0",
    "+1",
    "-0",
    "-00",
    "00",
    "007",
    "0x10",
    "1_000",
    "1.0",
    "1e3",
    " 1",
    "1 ",
    "1\n",
    "\u{0}",
    "\u{661}\u{662}",
    "-\u{661}",
    "\u{FF11}\u{FF12}",
    "\u{FF0D}1",
    "\u{2212}1",
    "\u{661}\u{662}\u{663}",
    "12a",
    "a12",
    "1-",
    "- 1",
    "0.",
    ".0",
    "999999999999999999999999999999999999999",
    "-999999999999999999999999999999999999999",
    "99999999999999999999999999999999999999999999999999999999999999999999",
    "300x",
    "-300x",
];

/// The text rows of one integer type: `toStr` is `to_string`, and `parse`
/// is Rust's parser behind the chapter's grammar.
macro_rules! integer_text {
    ($name:ident, $t:ty, $integer:expr, $make:path) => {
        fn $name(out: &mut Vec<Vector>) {
            let integer: IntegerType = $integer;
            let (min, max) = integer.range();
            let mut values: Vec<i128> = vec![
                min,
                min + 1,
                -100,
                -99,
                -10,
                -9,
                -1,
                0,
                1,
                9,
                10,
                99,
                100,
                127,
                128,
                255,
                256,
                9_999,
                10_000,
                32_767,
                32_768,
                65_535,
                65_536,
                2_147_483_647,
                2_147_483_648,
                4_294_967_295,
                4_294_967_296,
                9_223_372_036_854_775_807,
                9_223_372_036_854_775_808,
                max - 1,
                max,
            ];
            values.retain(|value| min <= *value && *value <= max);
            let mut random = Random(0xA076_1D64_78BD_642F ^ u64::from(integer.bits()));
            for _ in 0..40 {
                let span = u128::try_from(max - min).unwrap_or(0) + 1;
                let draw = (u128::from(random.next()) << 64
                    | u128::from(random.next()))
                    % span;
                values.push(min + i128::try_from(draw).unwrap_or(0));
            }
            for value in values {
                let Ok(value) = <$t>::try_from(value) else {
                    continue;
                };
                push(
                    out,
                    CompilerIntrinsic::Integer(integer, IntegerOp::ToStr),
                    vec![$make(value)],
                    Outcome::Value(Value::Str(value.to_string())),
                );
            }
            let mut texts: Vec<String> = Vec::new();
            for value in [min - 1, min, min + 1, -1, 0, 1, max - 1, max, max + 1] {
                texts.push(value.to_string());
            }
            for value in [min, max] {
                texts.push(format!("{value}0"));
                texts.push(format!("0{value}"));
            }
            texts.extend(MALFORMED_INTEGERS.iter().map(|text| (*text).to_owned()));
            for text in texts {
                let outcome = parse_integer_expected::<$t>(integer, &text, $make);
                push(
                    out,
                    CompilerIntrinsic::Integer(integer, IntegerOp::Parse),
                    vec![Value::Str(text)],
                    outcome,
                );
            }
        }
    };
}

integer_text!(text_i8, i8, IntegerType::I8, Value::I8);
integer_text!(text_i16, i16, IntegerType::I16, Value::I16);
integer_text!(text_i32, i32, IntegerType::I32, Value::I32);
integer_text!(text_i64, i64, IntegerType::I64, Value::I64);
integer_text!(text_u8, u8, IntegerType::U8, Value::U8);
integer_text!(text_u16, u16, IntegerType::U16, Value::U16);
integer_text!(text_u32, u32, IntegerType::U32, Value::U32);
integer_text!(text_u64, u64, IntegerType::U64, Value::U64);

fn integer_texts(out: &mut Vec<Vector>) {
    text_i8(out);
    text_i16(out);
    text_i32(out);
    text_i64(out);
    text_u8(out);
    text_u16(out);
    text_u32(out);
    text_u64(out);
}

// -- floats -------------------------------------------------------------------

/// IEEE `totalOrder` over canonicalized values, from the sign and the
/// unsigned magnitude of the bits: negative before positive, and by
/// magnitude within a sign, reversed for the negative.
fn total_order<T: Ord>(left: T, right: T, negative: impl Fn(&T) -> bool) -> Ordering {
    match (negative(&left), negative(&right)) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => left.cmp(&right),
        (true, true) => right.cmp(&left),
    }
}

const fn canonical_f64(bits: u64) -> u64 {
    if f64::from_bits(bits).is_nan() {
        CANONICAL_NAN_F64
    } else {
        bits
    }
}

const fn canonical_f32(bits: u32) -> u32 {
    if f32::from_bits(bits).is_nan() {
        CANONICAL_NAN_F32
    } else {
        bits
    }
}

fn total_order_f64(left: u64, right: u64) -> Ordering {
    total_order(canonical_f64(left), canonical_f64(right), |bits| {
        bits & (1 << 63) != 0
    })
}

fn total_order_f32(left: u32, right: u32) -> Ordering {
    total_order(canonical_f32(left), canonical_f32(right), |bits| {
        bits & (1 << 31) != 0
    })
}

/// The canonical text of a finite value, assembled from its shortest
/// scientific digits: decimal notation with at least one fractional digit
/// when the magnitude is zero or in [1e-4, 1e16), scientific notation with
/// one integral digit and no `+` otherwise.
fn assemble(scientific: &str, negative: bool, zero: bool) -> String {
    let sign = if negative { "-" } else { "" };
    if zero {
        return format!("{sign}0.0");
    }
    let body = scientific.strip_prefix('-').unwrap_or(scientific);
    let (mantissa, exponent) = body
        .split_once('e')
        .map_or((body, 0), |(mantissa, exponent)| {
            (mantissa, exponent.parse::<i32>().unwrap_or(0))
        });
    let digits = mantissa.replace('.', "");
    if !(-4..16).contains(&exponent) {
        let first = digits.get(..1).unwrap_or("");
        let rest = digits.get(1..).unwrap_or("");
        let fraction = if rest.is_empty() {
            String::new()
        } else {
            format!(".{rest}")
        };
        return format!("{sign}{first}{fraction}e{exponent}");
    }
    if exponent >= 0 {
        let whole = usize::try_from(exponent).unwrap_or(0) + 1;
        let padded = format!("{digits:0<whole$}");
        let integral = padded.get(..whole).unwrap_or("");
        let fraction = padded.get(whole..).unwrap_or("");
        let fraction = if fraction.is_empty() { "0" } else { fraction };
        format!("{sign}{integral}.{fraction}")
    } else {
        let zeros = "0".repeat(usize::try_from(-exponent - 1).unwrap_or(0));
        format!("{sign}0.{zeros}{digits}")
    }
}

fn text_f64(value: f64) -> String {
    if value.is_nan() {
        "nan".to_owned()
    } else if value.is_infinite() {
        if value > 0.0 { "inf" } else { "-inf" }.to_owned()
    } else {
        assemble(
            &format!("{value:e}"),
            value.is_sign_negative(),
            value == 0.0,
        )
    }
}

fn text_f32(value: f32) -> String {
    if value.is_nan() {
        "nan".to_owned()
    } else if value.is_infinite() {
        if value > 0.0 { "inf" } else { "-inf" }.to_owned()
    } else {
        assemble(
            &format!("{value:e}"),
            value.is_sign_negative(),
            value == 0.0,
        )
    }
}

/// The `F64` samples: the zeros, the subnormals, the extremes, the
/// switches of the notation, values that need seventeen digits, and NaNs of
/// both signs and several payloads.
fn f64_special() -> Vec<u64> {
    let values: [f64; 34] = [
        0.0,
        -0.0,
        1.0,
        -1.0,
        0.5,
        -2.5,
        0.1,
        0.2,
        0.1 + 0.2,
        1.0 / 3.0,
        2.0,
        3.0,
        10.0,
        100.0,
        123_456.789,
        f64::EPSILON,
        f64::MIN_POSITIVE,
        f64::MAX,
        f64::MIN,
        1e-4,
        1e-5,
        5e-5,
        1.5e-7,
        1e15,
        1e16,
        1e20,
        1e21,
        9_007_199_254_740_992.0,
        9_007_199_254_740_994.0,
        1e100,
        1e-100,
        1e308,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ];
    let mut bits = values
        .iter()
        .map(|value| value.to_bits())
        .collect::<Vec<_>>();
    for value in [1e-4_f64, 1e16, 1.0] {
        bits.push(value.to_bits() - 1);
        bits.push(value.to_bits() + 1);
    }
    bits.extend([
        1,
        2,
        0x0008_0000_0000_0000,
        0x000F_FFFF_FFFF_FFFF,
        0x8000_0000_0000_0001,
        0x800F_FFFF_FFFF_FFFF,
        0x7FF8_0000_0000_0000,
        0xFFF8_0000_0000_0000,
        0x7FF0_0000_0000_0001,
        0xFFF0_0000_0000_0001,
        0x7FF8_0000_0000_0001,
        0x7FFF_FFFF_FFFF_FFFF,
        0xFFF4_0000_DEAD_BEEF,
    ]);
    bits.sort_unstable();
    bits.dedup();
    bits
}

/// The `F32` samples, chosen as for `F64`.
fn f32_special() -> Vec<u32> {
    let values: [f32; 31] = [
        0.0,
        -0.0,
        1.0,
        -1.0,
        0.5,
        -2.5,
        0.1,
        0.2,
        0.1 + 0.2,
        1.0 / 3.0,
        2.0,
        3.0,
        10.0,
        100.0,
        123_456.79,
        f32::EPSILON,
        f32::MIN_POSITIVE,
        f32::MAX,
        f32::MIN,
        1e-4,
        1e-5,
        5e-5,
        1.5e-7,
        1e15,
        1e16,
        1e20,
        1e30,
        16_777_216.0,
        16_777_218.0,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ];
    let mut bits = values
        .iter()
        .map(|value| value.to_bits())
        .collect::<Vec<_>>();
    for value in [1e-4_f32, 1e16, 1.0] {
        bits.push(value.to_bits() - 1);
        bits.push(value.to_bits() + 1);
    }
    bits.extend([
        1,
        2,
        0x0040_0000,
        0x007F_FFFF,
        0x8000_0001,
        0x807F_FFFF,
        0x7FC0_0000,
        0xFFC0_0000,
        0x7F80_0001,
        0xFF80_0001,
        0x7FC0_0001,
        0x7FFF_FFFF,
        0xFFA0_BEEF,
    ]);
    bits.sort_unstable();
    bits.dedup();
    bits
}

/// Every third sample, so a pair table stays a few thousand vectors a
/// width.
fn every_third<T: Copy>(all: &[T]) -> Vec<T> {
    all.iter().copied().step_by(3).collect()
}

/// The rows of one float type over its samples.
macro_rules! float_rows {
    ($name:ident, $t:ty, $float:expr, $special:ident, $make:path, $canonical:ident, $order:ident, $text:ident) => {
        fn $name(out: &mut Vec<Vector>) {
            let row = |op| CompilerIntrinsic::Float($float, op);
            // A result as the observer sees it: a NaN is the canonical one.
            let observed = |value: $t| $make($canonical(value.to_bits()));
            let samples = $special();
            let binary = |out: &mut Vec<Vector>, left, right, ops: &[FloatOp]| {
                let (a, b) = (<$t>::from_bits(left), <$t>::from_bits(right));
                let operands = || vec![$make(left), $make(right)];
                for op in ops {
                    let outcome = match op {
                        FloatOp::Add => Outcome::Value(observed(a + b)),
                        FloatOp::Sub => Outcome::Value(observed(a - b)),
                        FloatOp::Mul => Outcome::Value(observed(a * b)),
                        FloatOp::Div => Outcome::Value(observed(a / b)),
                        FloatOp::Equal => Outcome::Value(Value::bool(a == b)),
                        FloatOp::CompareTotal => {
                            Outcome::Order(order($order(left, right)))
                        }
                        FloatOp::Neg | FloatOp::ToStr | FloatOp::Parse => continue,
                    };
                    push(out, row(*op), operands(), outcome);
                }
            };
            let thin = every_third(&samples);
            for &left in &thin {
                for &right in &thin {
                    binary(
                        out,
                        left,
                        right,
                        &[
                            FloatOp::Add,
                            FloatOp::Sub,
                            FloatOp::Mul,
                            FloatOp::Div,
                            FloatOp::Equal,
                            FloatOp::CompareTotal,
                        ],
                    );
                }
            }
            for &bits in &samples {
                let value = <$t>::from_bits(bits);
                push(
                    out,
                    row(FloatOp::Neg),
                    vec![$make(bits)],
                    Outcome::Value(observed(-value)),
                );
                push(
                    out,
                    row(FloatOp::ToStr),
                    vec![$make(bits)],
                    Outcome::Value(Value::Str($text(value))),
                );
                // Against every sample, so NaNs of every payload meet every
                // value.
                for pivot in [0.0 as $t, 1.0, <$t>::NAN] {
                    let pivot = pivot.to_bits();
                    for (left, right) in [(bits, pivot), (pivot, bits)] {
                        binary(
                            out,
                            left,
                            right,
                            &[
                                FloatOp::Equal,
                                FloatOp::CompareTotal,
                                FloatOp::Add,
                                FloatOp::Div,
                            ],
                        );
                    }
                }
            }
        }
    };
}

float_rows!(
    rows_f64,
    f64,
    FloatType::F64,
    f64_special,
    Value::F64,
    canonical_f64,
    total_order_f64,
    text_f64
);
float_rows!(
    rows_f32,
    f32,
    FloatType::F32,
    f32_special,
    Value::F32,
    canonical_f32,
    total_order_f32,
    text_f32
);

fn floats(out: &mut Vec<Vector>) {
    rows_f64(out);
    rows_f32(out);
    // `toStr` over a fixed set of bit patterns, whatever they are.
    let mut random = Random(0x9E37_79B9_7F4A_7C15);
    for _ in 0..1_000 {
        let bits = random.next();
        push(
            out,
            CompilerIntrinsic::Float(FloatType::F64, FloatOp::ToStr),
            vec![Value::F64(bits)],
            Outcome::Value(Value::Str(text_f64(f64::from_bits(bits)))),
        );
        let narrow = u32::try_from(random.next() >> 32).unwrap_or(0);
        push(
            out,
            CompilerIntrinsic::Float(FloatType::F32, FloatOp::ToStr),
            vec![Value::F32(narrow)],
            Outcome::Value(Value::Str(text_f32(f32::from_bits(narrow)))),
        );
    }
}

// -- float text ---------------------------------------------------------------

/// Whether `text` is the unsuffixed decimal float-literal grammar: an
/// optional `-`, one or more digits, then a fraction (`.` and one or more
/// digits), an exponent (`e` or `E`, an optional sign, one or more digits),
/// or both, but not neither.
fn in_float_grammar(text: &str) -> bool {
    let bytes = text.as_bytes();
    let digit = |at: usize| bytes.get(at).is_some_and(u8::is_ascii_digit);
    let skip_digits = |mut at: usize| {
        while digit(at) {
            at += 1;
        }
        at
    };
    let start = usize::from(bytes.first() == Some(&b'-'));
    let mut at = skip_digits(start);
    if at == start {
        return false;
    }
    let mut fraction = false;
    if bytes.get(at) == Some(&b'.') {
        let first = at + 1;
        at = skip_digits(first);
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
        at = skip_digits(first);
        if at == first {
            return false;
        }
        exponent = true;
    }
    at == bytes.len() && (fraction || exponent)
}

/// Float text in the grammar and out of it.
const FLOAT_TEXTS: [&str; 60] = [
    "0.0",
    "-0.0",
    "0e0",
    "-0e0",
    "1.0",
    "2.5e-3",
    "2.5E-3",
    "1e5",
    "1e+5",
    "1E5",
    "-1.5e-7",
    "007.50",
    "0.0000001",
    "123456789012345678901234567890.5",
    "0.30000000000000004",
    "0.1",
    "1.7976931348623157e308",
    "1.7976931348623159e308",
    "1.797693134862315807e308",
    "3.4028235e38",
    "3.4028236e38",
    "3.40282357e38",
    "-3.4028235e38",
    "5e-324",
    "2.4703282292062327e-324",
    "2.4703282292062328e-324",
    "1e-400",
    "1e-45",
    "7e-46",
    "1.4e-45",
    "1e309",
    "-1e309",
    "1e39",
    "1e999999999999",
    "1e-999999999999",
    "9007199254740993.0",
    "16777217.0",
    "100000000000000000000000.0",
    "0.00001",
    "",
    "-",
    "1",
    "-1",
    ".5",
    "1.",
    "1.e5",
    "e5",
    "1e",
    "1e+",
    "+1.0",
    "1.0f64",
    "1.0f32",
    " 1.0",
    "1.0 ",
    "nan",
    "inf",
    "-inf",
    "1.0.0",
    "1,5",
    "\u{661}.\u{660}",
];

/// The float `parse` vectors of both widths: the fixed list, the text of
/// every special value in scientific and plain notation, and a fixed set of
/// decimal strings.
fn float_text(out: &mut Vec<Vector>) {
    let mut texts = FLOAT_TEXTS
        .iter()
        .map(|text| (*text).to_owned())
        .collect::<Vec<_>>();
    for bits in f64_special() {
        let value = f64::from_bits(bits);
        if value.is_finite() {
            texts.push(format!("{value:e}"));
            texts.push(format!("{value:?}"));
        }
    }
    let mut random = Random(0xD1B5_4A32_D192_ED03);
    for _ in 0..1_000 {
        let digits = (random.next() % 30) + 1;
        let mut text = String::new();
        if random.next().is_multiple_of(4) {
            text.push('-');
        }
        for _ in 0..digits {
            text.push(random.digit());
        }
        match random.next() % 3 {
            0 => {
                text.push('.');
                for _ in 0..=(random.next() % 20) {
                    text.push(random.digit());
                }
            }
            1 => {
                let exponent = i64::try_from(random.next() % 700).unwrap_or(0) - 350;
                text.push_str(&format!("e{exponent}"));
            }
            _ => text.push_str(&format!(".5E-{}", random.next() % 330)),
        }
        texts.push(text);
    }
    for text in texts {
        let grammatical = in_float_grammar(&text);
        let wide = if grammatical {
            text.parse::<f64>()
                .ok()
                .filter(|value| value.is_finite())
                .map_or(Outcome::Err("outOfRange"), |value| {
                    Outcome::Ok(Value::F64(value.to_bits()))
                })
        } else {
            Outcome::Err("invalidFormat")
        };
        let narrow = if grammatical {
            text.parse::<f32>()
                .ok()
                .filter(|value| value.is_finite())
                .map_or(Outcome::Err("outOfRange"), |value| {
                    Outcome::Ok(Value::F32(value.to_bits()))
                })
        } else {
            Outcome::Err("invalidFormat")
        };
        let operand = vec![Value::Str(text)];
        push(
            out,
            CompilerIntrinsic::Float(FloatType::F64, FloatOp::Parse),
            operand.clone(),
            wide,
        );
        push(
            out,
            CompilerIntrinsic::Float(FloatType::F32, FloatOp::Parse),
            operand,
            narrow,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_assembled_text_follows_the_chapter_and_agrees_with_the_debug_format() {
        for (value, want) in [
            (100.0_f64, "100.0"),
            (-0.0, "-0.0"),
            (0.0025, "0.0025"),
            (1e20, "1e20"),
            (1.5e-7, "1.5e-7"),
            (1e16, "1e16"),
            (9_999_999_999_999_998.0, "9999999999999998.0"),
            (1e-4, "0.0001"),
            (9.999e-5, "9.999e-5"),
            (5e-324, "5e-324"),
            (0.1 + 0.2, "0.30000000000000004"),
            (123_456.789, "123456.789"),
        ] {
            assert_eq!(text_f64(value), want);
        }
        // The encoder writes Rust's debug format; it is the same function.
        let mut random = Random(7);
        for _ in 0..5_000 {
            let value = f64::from_bits(random.next());
            if value.is_finite() {
                assert_eq!(text_f64(value), format!("{value:?}"), "{value:e}");
            }
            let narrow =
                f32::from_bits(u32::try_from(random.next() >> 32).unwrap_or(0));
            if narrow.is_finite() {
                assert_eq!(text_f32(narrow), format!("{narrow:?}"), "{narrow:e}");
            }
        }
    }

    #[test]
    fn total_order_by_sign_and_magnitude_is_the_standard_total_order() {
        let mut random = Random(11);
        let mut bits = f64_special();
        bits.extend((0..200).map(|_| random.next()));
        let canonical = |bits: u64| f64::from_bits(canonical_f64(bits));
        for &left in &bits {
            for &right in &bits {
                assert_eq!(
                    total_order_f64(left, right),
                    canonical(left).total_cmp(&canonical(right)),
                    "{left:#x} {right:#x}"
                );
            }
        }
    }
}
