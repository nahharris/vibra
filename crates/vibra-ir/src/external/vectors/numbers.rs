//! The sample vectors of the number-text rows and the float rows.
//!
//! Every outcome here comes from a source that is neither the reference
//! interpreter nor the WebAssembly lowering nor the toolchain's native code:
//!
//! - the float arithmetic rows are Rust's own `f32` and `f64` operators, whose
//!   results are IEEE 754 round-to-nearest, ties-to-even, and a NaN result is the
//!   canonical quiet NaN because no operation observes any other;
//! - `equal` is Rust's `==`, and `compare-total` is IEEE 754 `totalOrder`, derived
//!   here from the sign and the unsigned magnitude of the bits and not from
//!   `total_cmp`, after a NaN is replaced by the quiet NaN of its width;
//! - integer `to-str` is the standard library's `to_string`, and integer `parse`
//!   is the standard library's integer parser behind a check of the
//!   specification's grammar, which admits an optional `-` (signed types only), then
//!   ASCII digits only;
//! - float `to-str` assembles the canonical float serialization of the runtime
//!   chapter by hand from the digits of Rust's shortest scientific formatting
//!   (`{:e}`), and float `parse` is the standard library's correctly rounded parser
//!   behind a hand-written recognizer of the unsuffixed float literal grammar.
//!
//! Both widths are sampled at zero and negative zero, the subnormals, the largest
//! and smallest finite values, the infinities, NaNs of every sign and payload, the
//! decimal and scientific switch at `1e-4` and `1e16`, and a deterministic
//! pseudo-random set of bit patterns and decimal strings.

use std::cmp::Ordering;

use super::{Outcome, Vector, order, push};
use crate::external::{CompilerIntrinsic, FloatOp, IntegerOp, NumericType};
use crate::{CANONICAL_NAN_F32, CANONICAL_NAN_F64, Value};

// -- the generator --------------------------------------------------------------

/// A xorshift generator with a fixed seed, so the set never changes.
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

// -- floats ---------------------------------------------------------------------

/// The IEEE `totalOrder` of two binary64 bit patterns, after a NaN is replaced by
/// the canonical quiet NaN: negative before positive, and by magnitude within a
/// sign, reversed for the negative.
fn total_order_f64(left: u64, right: u64) -> Ordering {
    let canonical = |bits: u64| {
        if f64::from_bits(bits).is_nan() {
            CANONICAL_NAN_F64
        } else {
            bits
        }
    };
    let (left, right) = (canonical(left), canonical(right));
    let sign = 1_u64 << 63;
    match (left & sign != 0, right & sign != 0) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => left.cmp(&right),
        (true, true) => right.cmp(&left),
    }
}

fn total_order_f32(left: u32, right: u32) -> Ordering {
    let canonical = |bits: u32| {
        if f32::from_bits(bits).is_nan() {
            CANONICAL_NAN_F32
        } else {
            bits
        }
    };
    let (left, right) = (canonical(left), canonical(right));
    let sign = 1_u32 << 31;
    match (left & sign != 0, right & sign != 0) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => left.cmp(&right),
        (true, true) => right.cmp(&left),
    }
}

/// The canonical float serialization of a finite value, assembled from its
/// shortest scientific digits: decimal notation with at least one fractional
/// digit when the magnitude is zero or in `[1e-4, 1e16)`, scientific notation with
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
        let rest = digits.get(1..).unwrap_or("");
        let first = digits.get(..1).unwrap_or("");
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

/// The binary64 samples every operation is tried at: the zeros, the subnormals,
/// the extremes, the switches of the notation, and values that need seventeen
/// digits.
fn f64_special() -> Vec<u64> {
    let mut values: Vec<f64> = vec![
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
    let mut bits = values.drain(..).map(f64::to_bits).collect::<Vec<_>>();
    let around = |bits: &mut Vec<u64>, value: f64| {
        bits.push(value.to_bits() - 1);
        bits.push(value.to_bits() + 1);
    };
    for value in [1e-4, 1e16, 1.0] {
        around(&mut bits, value);
    }
    // The subnormals: the smallest, one more, the largest, and one in between.
    bits.extend([
        1,
        2,
        0x0008_0000_0000_0000,
        0x000F_FFFF_FFFF_FFFF,
        0x8000_0000_0000_0001,
        0x800F_FFFF_FFFF_FFFF,
    ]);
    // NaNs of both signs and several payloads, quiet and signaling.
    bits.extend([
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

fn f32_special() -> Vec<u32> {
    let mut values: Vec<f32> = vec![
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
    let mut bits = values.drain(..).map(f32::to_bits).collect::<Vec<_>>();
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

/// A binary64 result as the observer sees it: a NaN is the canonical quiet NaN.
fn observed_f64(value: f64) -> Value {
    Value::F64(if value.is_nan() {
        CANONICAL_NAN_F64
    } else {
        value.to_bits()
    })
}

fn observed_f32(value: f32) -> Value {
    Value::F32(if value.is_nan() {
        CANONICAL_NAN_F32
    } else {
        value.to_bits()
    })
}

/// The float arithmetic, equality, and order rows, and `to-str`, for both widths.
pub(super) fn floats(out: &mut Vec<Vector>) {
    let row = |numeric, op| CompilerIntrinsic::Float(numeric, op);
    let wide = f64_special();
    // Pairs are sampled over a thinner list so the table stays a few thousand
    // vectors a width; the full list is tried against `0.0`, `1.0`, and a NaN.
    for &left in &every_third(&wide) {
        for &right in &every_third(&wide) {
            let (a, b) = (f64::from_bits(left), f64::from_bits(right));
            let operands = || vec![Value::F64(left), Value::F64(right)];
            push(
                out,
                row(NumericType::F64, FloatOp::Add),
                operands(),
                Outcome::Value(observed_f64(a + b)),
            );
            push(
                out,
                row(NumericType::F64, FloatOp::Sub),
                operands(),
                Outcome::Value(observed_f64(a - b)),
            );
            push(
                out,
                row(NumericType::F64, FloatOp::Mul),
                operands(),
                Outcome::Value(observed_f64(a * b)),
            );
            push(
                out,
                row(NumericType::F64, FloatOp::Div),
                operands(),
                Outcome::Value(observed_f64(a / b)),
            );
            push(
                out,
                row(NumericType::F64, FloatOp::Equal),
                operands(),
                Outcome::Value(Value::Bool(a == b)),
            );
            push(
                out,
                row(NumericType::F64, FloatOp::CompareTotal),
                operands(),
                Outcome::Order(order(total_order_f64(left, right))),
            );
        }
    }
    for &bits in &wide {
        let value = f64::from_bits(bits);
        push(
            out,
            row(NumericType::F64, FloatOp::Neg),
            vec![Value::F64(bits)],
            Outcome::Value(observed_f64(-value)),
        );
        push(
            out,
            row(NumericType::F64, FloatOp::ToStr),
            vec![Value::F64(bits)],
            Outcome::Value(Value::Str(text_f64(value))),
        );
        // Against every sample, so NaNs of every payload meet every value.
        for pivot in [0.0_f64, 1.0, f64::NAN] {
            let pivot_bits = pivot.to_bits();
            for (left, right) in [(bits, pivot_bits), (pivot_bits, bits)] {
                let (a, b) = (f64::from_bits(left), f64::from_bits(right));
                let operands = || vec![Value::F64(left), Value::F64(right)];
                push(
                    out,
                    row(NumericType::F64, FloatOp::Equal),
                    operands(),
                    Outcome::Value(Value::Bool(a == b)),
                );
                push(
                    out,
                    row(NumericType::F64, FloatOp::CompareTotal),
                    operands(),
                    Outcome::Order(order(total_order_f64(left, right))),
                );
                push(
                    out,
                    row(NumericType::F64, FloatOp::Add),
                    operands(),
                    Outcome::Value(observed_f64(a + b)),
                );
                push(
                    out,
                    row(NumericType::F64, FloatOp::Div),
                    operands(),
                    Outcome::Value(observed_f64(a / b)),
                );
            }
        }
    }

    let narrow = f32_special();
    for &left in &every_third(&narrow) {
        for &right in &every_third(&narrow) {
            let (a, b) = (f32::from_bits(left), f32::from_bits(right));
            let operands = || vec![Value::F32(left), Value::F32(right)];
            push(
                out,
                row(NumericType::F32, FloatOp::Add),
                operands(),
                Outcome::Value(observed_f32(a + b)),
            );
            push(
                out,
                row(NumericType::F32, FloatOp::Sub),
                operands(),
                Outcome::Value(observed_f32(a - b)),
            );
            push(
                out,
                row(NumericType::F32, FloatOp::Mul),
                operands(),
                Outcome::Value(observed_f32(a * b)),
            );
            push(
                out,
                row(NumericType::F32, FloatOp::Div),
                operands(),
                Outcome::Value(observed_f32(a / b)),
            );
            push(
                out,
                row(NumericType::F32, FloatOp::Equal),
                operands(),
                Outcome::Value(Value::Bool(a == b)),
            );
            push(
                out,
                row(NumericType::F32, FloatOp::CompareTotal),
                operands(),
                Outcome::Order(order(total_order_f32(left, right))),
            );
        }
    }
    for &bits in &narrow {
        let value = f32::from_bits(bits);
        push(
            out,
            row(NumericType::F32, FloatOp::Neg),
            vec![Value::F32(bits)],
            Outcome::Value(observed_f32(-value)),
        );
        push(
            out,
            row(NumericType::F32, FloatOp::ToStr),
            vec![Value::F32(bits)],
            Outcome::Value(Value::Str(text_f32(value))),
        );
        for pivot in [0.0_f32, 1.0, f32::NAN] {
            let pivot_bits = pivot.to_bits();
            for (left, right) in [(bits, pivot_bits), (pivot_bits, bits)] {
                let (a, b) = (f32::from_bits(left), f32::from_bits(right));
                let operands = || vec![Value::F32(left), Value::F32(right)];
                push(
                    out,
                    row(NumericType::F32, FloatOp::Equal),
                    operands(),
                    Outcome::Value(Value::Bool(a == b)),
                );
                push(
                    out,
                    row(NumericType::F32, FloatOp::CompareTotal),
                    operands(),
                    Outcome::Order(order(total_order_f32(left, right))),
                );
                push(
                    out,
                    row(NumericType::F32, FloatOp::Add),
                    operands(),
                    Outcome::Value(observed_f32(a + b)),
                );
                push(
                    out,
                    row(NumericType::F32, FloatOp::Div),
                    operands(),
                    Outcome::Value(observed_f32(a / b)),
                );
            }
        }
    }

    // `to-str` over a deterministic set of bit patterns, whatever they are.
    let mut random = Random(0x9E37_79B9_7F4A_7C15);
    for _ in 0..1_000 {
        let bits = random.next();
        push(
            out,
            row(NumericType::F64, FloatOp::ToStr),
            vec![Value::F64(bits)],
            Outcome::Value(Value::Str(text_f64(f64::from_bits(bits)))),
        );
        let narrow = u32::try_from(random.next() >> 32).unwrap_or(0);
        push(
            out,
            row(NumericType::F32, FloatOp::ToStr),
            vec![Value::F32(narrow)],
            Outcome::Value(Value::Str(text_f32(f32::from_bits(narrow)))),
        );
    }
}

/// Every third sample, so a pair table stays a couple of thousand vectors a width.
fn every_third<T: Copy>(all: &[T]) -> Vec<T> {
    all.iter().copied().step_by(3).collect()
}

// -- float text -----------------------------------------------------------------

/// Whether `text` is the unsuffixed decimal float literal grammar: an optional
/// `-`, one or more digits, then a fraction (`.` and one or more digits), an
/// exponent (`e` or `E`, an optional sign, one or more digits), or both, but
/// not neither.
fn in_float_grammar(text: &str) -> bool {
    let bytes = text.as_bytes();
    let digit = |at: usize| bytes.get(at).is_some_and(u8::is_ascii_digit);
    let mut at = usize::from(bytes.first() == Some(&b'-'));
    let start = at;
    while digit(at) {
        at += 1;
    }
    if at == start {
        return false;
    }
    let mut fraction = false;
    if bytes.get(at) == Some(&b'.') {
        at += 1;
        let first = at;
        while digit(at) {
            at += 1;
        }
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
        while digit(at) {
            at += 1;
        }
        if at == first {
            return false;
        }
        exponent = true;
    }
    at == bytes.len() && (fraction || exponent)
}

const FLOAT_TEXTS: &[&str] = &[
    // In the grammar.
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
    // Not in the grammar.
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

/// The float `parse` vectors of both widths: the fixed list, the decimal text of
/// every special value in the scientific and the plain notation, and a
/// deterministic set of decimal strings.
pub(super) fn float_text(out: &mut Vec<Vector>) {
    let mut texts: Vec<String> =
        FLOAT_TEXTS.iter().map(|text| (*text).to_owned()).collect();
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
            text.push(char::from(
                b'0' + u8::try_from(random.next() % 10).unwrap_or(0),
            ));
        }
        match random.next() % 3 {
            0 => {
                text.push('.');
                for _ in 0..=(random.next() % 20) {
                    text.push(char::from(
                        b'0' + u8::try_from(random.next() % 10).unwrap_or(0),
                    ));
                }
            }
            1 => text.push_str(&format!(
                "e{}",
                i64::try_from(random.next() % 700).unwrap_or(0) - 350
            )),
            _ => {
                text.push_str(".5");
                text.push_str(&format!("E-{}", random.next() % 330));
            }
        }
        texts.push(text);
    }
    for text in texts {
        let grammatical = in_float_grammar(&text);
        let operand = || vec![Value::Str(text.clone())];
        let wide = if grammatical {
            text.parse::<f64>()
                .ok()
                .filter(|value| value.is_finite())
                .map_or(Outcome::Err("out-of-range"), |value| {
                    Outcome::Ok(Value::F64(value.to_bits()))
                })
        } else {
            Outcome::Err("invalid-format")
        };
        let narrow = if grammatical {
            text.parse::<f32>()
                .ok()
                .filter(|value| value.is_finite())
                .map_or(Outcome::Err("out-of-range"), |value| {
                    Outcome::Ok(Value::F32(value.to_bits()))
                })
        } else {
            Outcome::Err("invalid-format")
        };
        push(
            out,
            CompilerIntrinsic::Float(NumericType::F64, FloatOp::Parse),
            operand(),
            wide,
        );
        push(
            out,
            CompilerIntrinsic::Float(NumericType::F32, FloatOp::Parse),
            operand(),
            narrow,
        );
    }
}

// -- integer text ---------------------------------------------------------------

/// The integer text vectors of one type: `to-str` is `to_string`, and `parse` is
/// the standard library's parser behind the specification's grammar.
macro_rules! integer_text {
    ($name:ident, $t:ty, $numeric:expr, $make:path) => {
        fn $name(out: &mut Vec<Vector>) {
            let numeric: NumericType = $numeric;
            let (min, max) = numeric.range();
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
            let mut random = Random(0xA076_1D64_78BD_642F ^ u64::from(numeric.bits()));
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
                    CompilerIntrinsic::Integer(numeric, IntegerOp::ToStr),
                    vec![$make(value)],
                    Outcome::Value(Value::Str(value.to_string())),
                );
            }
            // The text of `parse`: the spelling of every boundary and the
            // values one past it, and malformed text of every kind.
            let mut texts: Vec<String> = Vec::new();
            for value in [min - 1, min, min + 1, -1, 0, 1, max - 1, max, max + 1] {
                texts.push(value.to_string());
            }
            for value in [min, max] {
                texts.push(format!("{value}0"));
                texts.push(format!("0{value}"));
            }
            for text in [
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
                "١٢٣",
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
            ] {
                texts.push(text.to_owned());
            }
            for text in texts {
                let outcome = parse_integer_expected::<$t>(numeric, &text, $make);
                push(
                    out,
                    CompilerIntrinsic::Integer(numeric, IntegerOp::Parse),
                    vec![Value::Str(text)],
                    outcome,
                );
            }
        }
    };
}

/// What the specification says `parse` answers for `text`: the grammar first
/// (an optional `-` for a signed type, then one or more ASCII digits, and nothing
/// else), then the standard library's parser, whose only failure after the
/// grammar holds is a value outside the type.
fn parse_integer_expected<T: std::str::FromStr>(
    numeric: NumericType,
    text: &str,
    make: fn(T) -> Value,
) -> Outcome {
    let digits = match text.strip_prefix('-') {
        Some(digits) if numeric.is_signed() => digits,
        Some(_) => return Outcome::Err("invalid-format"),
        None => text,
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Outcome::Err("invalid-format");
    }
    text.parse::<T>()
        .map_or(Outcome::Err("out-of-range"), |value| {
            Outcome::Ok(make(value))
        })
}

integer_text!(text_i8, i8, NumericType::I8, Value::I8);
integer_text!(text_i16, i16, NumericType::I16, Value::I16);
integer_text!(text_i32, i32, NumericType::I32, Value::I32);
integer_text!(text_i64, i64, NumericType::I64, Value::I64);
integer_text!(text_u8, u8, NumericType::U8, Value::U8);
integer_text!(text_u16, u16, NumericType::U16, Value::U16);
integer_text!(text_u32, u32, NumericType::U32, Value::U32);
integer_text!(text_u64, u64, NumericType::U64, Value::U64);

pub(super) fn integer_texts(out: &mut Vec<Vector>) {
    text_i8(out);
    text_i16(out);
    text_i32(out);
    text_i64(out);
    text_u8(out);
    text_u16(out);
    text_u32(out);
    text_u64(out);
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn the_hand_written_text_agrees_with_the_standard_debug_format_and_the_chapter() {
        // The rule of the chapter, spot checked by hand.
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
        // Rust's own general debug format is the same function of the value.
        let mut random = Random(7);
        for _ in 0..5_000 {
            let value = f64::from_bits(random.next());
            if value.is_finite() {
                assert_eq!(text_f64(value), format!("{value:?}"), "{value:e}");
            }
            let value = f32::from_bits(u32::try_from(random.next() >> 32).unwrap());
            if value.is_finite() {
                assert_eq!(text_f32(value), format!("{value:?}"), "{value:e}");
            }
        }
    }

    #[test]
    fn total_order_by_sign_and_magnitude_is_the_standard_total_order() {
        let mut random = Random(11);
        let mut bits = f64_special();
        bits.extend((0..200).map(|_| random.next()));
        for &left in &bits {
            for &right in &bits {
                let canonical = |bits: u64| {
                    let value = f64::from_bits(bits);
                    if value.is_nan() {
                        f64::from_bits(CANONICAL_NAN_F64)
                    } else {
                        value
                    }
                };
                assert_eq!(
                    total_order_f64(left, right),
                    canonical(left).total_cmp(&canonical(right)),
                    "{left:#x} {right:#x}"
                );
            }
        }
    }
}
