//! The `@compiler` registry at major 2, minor 0
//! (`docs/spec/06-runtime.md`, "Compiler intrinsic registry").

use vibra_ir::external::{
    CompilerIntrinsic, IntegerOp, IntegerType, Outcome, REGISTRY_MAJOR, REGISTRY_MINOR,
    is_assertion,
};
use vibra_ir::{Value, encode_value};

use crate::{library, table};

const SNAPSHOT: &str = include_str!("../../registry/compiler-2.0.txt");

#[test]
fn the_snapshot_is_the_table() {
    assert_eq!((REGISTRY_MAJOR, REGISTRY_MINOR), (2, 0));
    let rendered = CompilerIntrinsic::all()
        .into_iter()
        .map(|intrinsic| intrinsic.row().snapshot_line() + "\n")
        .collect::<String>();
    assert_eq!(rendered, SNAPSHOT);
    assert_eq!(SNAPSHOT.lines().count(), 240);
}

#[test]
fn the_rows_are_sorted_by_symbol_bytes_and_each_symbol_resolves() {
    let symbols = CompilerIntrinsic::all()
        .into_iter()
        .map(CompilerIntrinsic::symbol)
        .collect::<Vec<_>>();
    let mut sorted = symbols.clone();
    sorted.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
    sorted.dedup();
    assert_eq!(symbols, sorted, "sorted and unique");
    for symbol in &symbols {
        let intrinsic = CompilerIntrinsic::from_symbol(symbol).unwrap();
        assert_eq!(intrinsic.symbol(), *symbol);
    }
    assert_eq!(CompilerIntrinsic::from_symbol("i32.add-checked"), None);
}

#[test]
fn every_symbol_is_a_lower_case_owner_and_a_camel_case_member() {
    for line in SNAPSHOT.lines() {
        let symbol = line.split('\t').next().unwrap();
        assert!(!symbol.contains('-'), "{symbol}");
        let (owner, member) = symbol.split_once('.').unwrap();
        assert!(
            owner.chars().all(|character| character.is_ascii_lowercase()
                || character.is_ascii_digit()),
            "{symbol}"
        );
        assert!(
            member.starts_with(|character: char| character.is_ascii_lowercase())
                && member
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric()),
            "{symbol}"
        );
    }
}

#[test]
fn the_deleted_rows_are_absent() {
    for line in SNAPSHOT.lines() {
        let symbol = line.split('\t').next().unwrap();
        let (owner, member) = symbol.split_once('.').unwrap();
        assert!(owner != "array" && !member.contains("Array"), "{symbol}");
        assert!(member != "of", "{symbol}");
        let integer = IntegerType::ALL
            .iter()
            .any(|integer| integer.owner() == owner);
        assert!(
            !(integer && (member == "div" || member == "rem")),
            "{symbol}"
        );
    }
    for symbol in [
        "list.next",
        "dict.next",
        "list.fold",
        "bytes.toList",
        "bytes.fromList",
    ] {
        assert!(CompilerIntrinsic::from_symbol(symbol).is_some(), "{symbol}");
    }
}

#[test]
fn only_the_three_assertion_rows_are_impure() {
    let impure = CompilerIntrinsic::all()
        .into_iter()
        .filter(|intrinsic| !intrinsic.row().pure)
        .map(CompilerIntrinsic::symbol)
        .collect::<Vec<_>>();
    assert_eq!(impure, ["assert.equal", "assert.false", "assert.true"]);
    for symbol in &impure {
        assert!(is_assertion(symbol));
    }
    assert!(!is_assertion("i32.add"));
    assert!(!is_assertion("assert.unknown"));
}

/// Each vector's operands have the row's parameter types and its stated
/// result has the row's result type, by the canonical encoding at those
/// types.
#[test]
fn every_vector_fits_its_row() {
    let types = table(Vec::new());
    let library = library();
    let mut vectored = 0;
    for intrinsic in CompilerIntrinsic::all() {
        let row = intrinsic.row();
        let vectors = intrinsic.vectors();
        if vectors.is_empty() {
            continue;
        }
        vectored += 1;
        assert!(row.generics.is_empty(), "{}", row.symbol);
        let parameters = row
            .parameters
            .iter()
            .map(|parameter| parameter.instantiate(&library, &[]).unwrap())
            .collect::<Vec<_>>();
        let result = row.result.instantiate(&library, &[]).unwrap();
        for vector in vectors {
            assert_eq!(vector.operands.len(), parameters.len(), "{}", row.symbol);
            for (operand, parameter) in vector.operands.iter().zip(&parameters) {
                encode_value(operand, parameter, &types)
                    .unwrap_or_else(|error| panic!("{}: {error}", row.symbol));
            }
            encode_value(&vector.outcome.value(), &result, &types)
                .unwrap_or_else(|error| panic!("{} {vector:?}: {error}", row.symbol));
        }
    }
    assert!(vectored > 200, "{vectored} rows have vectors");
}

#[test]
fn the_rows_with_vectors_are_the_number_char_text_and_bytes_rows() {
    for intrinsic in CompilerIntrinsic::all() {
        let expected = !matches!(
            intrinsic,
            CompilerIntrinsic::StrToChars
                | CompilerIntrinsic::StrFromChars
                | CompilerIntrinsic::BytesToList
                | CompilerIntrinsic::BytesFromList
                | CompilerIntrinsic::DictEntries
                | CompilerIntrinsic::DictFromList
                | CompilerIntrinsic::DictNext
                | CompilerIntrinsic::ListLength
                | CompilerIntrinsic::ListAppend
                | CompilerIntrinsic::ListConcat
                | CompilerIntrinsic::ListSlice
                | CompilerIntrinsic::ListFold
                | CompilerIntrinsic::ListNext
                | CompilerIntrinsic::AssertTrue
                | CompilerIntrinsic::AssertFalse
                | CompilerIntrinsic::AssertEqual
        );
        assert_eq!(
            !intrinsic.vectors().is_empty(),
            expected,
            "{}",
            intrinsic.symbol()
        );
    }
}

fn row(integer: IntegerType, op: IntegerOp) -> CompilerIntrinsic {
    CompilerIntrinsic::Integer(integer, op)
}

/// Vectors written by hand from the chapter's prose, to be found in the
/// table.
#[test]
fn the_table_holds_the_examples_the_chapter_gives() {
    use IntegerOp::{
        AddChecked, Compare, DivChecked, MulChecked, NegChecked, RemChecked, ShiftLeft,
        ShiftLeftChecked, ShiftRight, ShiftRightChecked, SubChecked,
    };
    use IntegerType::{I8, I16, I32, I64, U8, U32, U64};
    let examples = vec![
        // The exact result, or `overflow`.
        (
            row(U64, AddChecked),
            vec![Value::U64(u64::MAX), Value::U64(1)],
            Outcome::Err("overflow"),
        ),
        (
            row(U64, MulChecked),
            vec![Value::U64(u64::MAX), Value::U64(1)],
            Outcome::Ok(Value::U64(u64::MAX)),
        ),
        (
            row(U8, SubChecked),
            vec![Value::U8(0), Value::U8(1)],
            Outcome::Err("overflow"),
        ),
        (
            row(I32, AddChecked),
            vec![Value::I32(i32::MAX - 1), Value::I32(1)],
            Outcome::Ok(Value::I32(i32::MAX)),
        ),
        // The quotient truncated toward zero; the remainder with the
        // dividend's sign; the signed minimum and -1.
        (
            row(I8, DivChecked),
            vec![Value::I8(-7), Value::I8(2)],
            Outcome::Ok(Value::I8(-3)),
        ),
        (
            row(I8, RemChecked),
            vec![Value::I8(-7), Value::I8(2)],
            Outcome::Ok(Value::I8(-1)),
        ),
        (
            row(I8, RemChecked),
            vec![Value::I8(7), Value::I8(-2)],
            Outcome::Ok(Value::I8(1)),
        ),
        (
            row(I64, DivChecked),
            vec![Value::I64(i64::MIN), Value::I64(-1)],
            Outcome::Err("overflow"),
        ),
        (
            row(I64, RemChecked),
            vec![Value::I64(i64::MIN), Value::I64(-1)],
            Outcome::Ok(Value::I64(0)),
        ),
        (
            row(U8, DivChecked),
            vec![Value::U8(1), Value::U8(0)],
            Outcome::Err("divisionByZero"),
        ),
        (
            row(I16, NegChecked),
            vec![Value::I16(i16::MIN)],
            Outcome::Err("overflow"),
        ),
        // `invalidShift` at the width, `overflow` when the product leaves
        // the type, and a flooring right shift.
        (
            row(U32, ShiftLeftChecked),
            vec![Value::U32(1), Value::U32(32)],
            Outcome::Err("invalidShift"),
        ),
        (
            row(U8, ShiftLeftChecked),
            vec![Value::U8(1), Value::U32(7)],
            Outcome::Ok(Value::U8(128)),
        ),
        (
            row(U8, ShiftLeftChecked),
            vec![Value::U8(2), Value::U32(7)],
            Outcome::Err("overflow"),
        ),
        (
            row(I8, ShiftLeftChecked),
            vec![Value::I8(-1), Value::U32(7)],
            Outcome::Ok(Value::I8(i8::MIN)),
        ),
        (
            row(I8, ShiftLeftChecked),
            vec![Value::I8(1), Value::U32(7)],
            Outcome::Err("overflow"),
        ),
        (
            row(I64, ShiftRightChecked),
            vec![Value::I64(-7), Value::U32(1)],
            Outcome::Ok(Value::I64(-4)),
        ),
        (
            row(I16, ShiftRightChecked),
            vec![Value::I16(-1), Value::U32(15)],
            Outcome::Ok(Value::I16(-1)),
        ),
        (
            row(U64, ShiftRightChecked),
            vec![Value::U64(1), Value::U32(64)],
            Outcome::Err("invalidShift"),
        ),
        // The plain shifts clamp, and zero stays zero.
        (
            row(U8, ShiftLeft),
            vec![Value::U8(2), Value::U32(7)],
            Outcome::Value(Value::U8(u8::MAX)),
        ),
        (
            row(I8, ShiftLeft),
            vec![Value::I8(-1), Value::U32(u32::MAX)],
            Outcome::Value(Value::I8(i8::MIN)),
        ),
        (
            row(I8, ShiftLeft),
            vec![Value::I8(0), Value::U32(u32::MAX)],
            Outcome::Value(Value::I8(0)),
        ),
        (
            row(I32, ShiftRight),
            vec![Value::I32(-7), Value::U32(32)],
            Outcome::Value(Value::I32(-1)),
        ),
        (
            row(U32, ShiftRight),
            vec![Value::U32(7), Value::U32(32)],
            Outcome::Value(Value::U32(0)),
        ),
        // Order is numeric.
        (
            row(I32, Compare),
            vec![Value::I32(-1), Value::I32(1)],
            Outcome::Order("less"),
        ),
        (
            row(U64, Compare),
            vec![Value::U64(u64::MAX), Value::U64(0)],
            Outcome::Order("greater"),
        ),
        // A conversion keeps the value or reports `outOfRange`.
        (
            CompilerIntrinsic::Convert(I8, U8),
            vec![Value::I8(-1)],
            Outcome::Err("outOfRange"),
        ),
        (
            CompilerIntrinsic::Convert(U8, I8),
            vec![Value::U8(128)],
            Outcome::Err("outOfRange"),
        ),
        (
            CompilerIntrinsic::Convert(U8, I64),
            vec![Value::U8(255)],
            Outcome::Value(Value::I64(255)),
        ),
        (
            CompilerIntrinsic::Convert(U64, I64),
            vec![Value::U64(1 << 63)],
            Outcome::Err("outOfRange"),
        ),
        (
            CompilerIntrinsic::Convert(I64, U64),
            vec![Value::I64(i64::MAX)],
            Outcome::Ok(Value::U64(i64::MAX.unsigned_abs())),
        ),
        // A `Char` is a Unicode scalar, without the surrogates.
        (
            CompilerIntrinsic::CharFromU32,
            vec![Value::U32(0xD800)],
            Outcome::None,
        ),
        (
            CompilerIntrinsic::CharFromU32,
            vec![Value::U32(0x11_0000)],
            Outcome::None,
        ),
        (
            CompilerIntrinsic::CharFromU32,
            vec![Value::U32(0x10_FFFF)],
            Outcome::Some(Value::Char('\u{10FFFF}')),
        ),
        (
            CompilerIntrinsic::CharToU32,
            vec![Value::Char('\u{E000}')],
            Outcome::Value(Value::U32(0xE000)),
        ),
        // `Str.length` counts scalars; `Str.slice` is a half-open range of
        // scalars and none past the end.
        (
            CompilerIntrinsic::StrLength,
            vec![Value::Str("h\u{1D11E}llo".to_owned())],
            Outcome::Value(Value::U64(5)),
        ),
        (
            CompilerIntrinsic::StrSlice,
            vec![Value::Str("ab".to_owned()), Value::U64(0), Value::U64(3)],
            Outcome::None,
        ),
        (
            CompilerIntrinsic::StrFromUtf8,
            vec![Value::Bytes(vec![0xED, 0xA0, 0x80])],
            Outcome::Err("invalidFormat"),
        ),
    ];
    for (intrinsic, operands, outcome) in examples {
        let found = intrinsic
            .vectors()
            .iter()
            .find(|vector| vector.operands == operands)
            .unwrap_or_else(|| {
                panic!("no vector of {} for {operands:?}", intrinsic.symbol())
            });
        assert_eq!(
            found.outcome,
            outcome,
            "{} {operands:?}",
            intrinsic.symbol()
        );
    }
}

#[test]
fn every_integer_type_is_sampled_at_each_of_its_bounds() {
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
    for integer in IntegerType::ALL {
        let (min, max) = integer.range();
        let vectors = row(integer, IntegerOp::AddChecked).vectors();
        for bound in [min, min + 1, 0, 1, max - 1, max] {
            assert!(
                vectors.iter().any(|vector| vector
                    .operands
                    .iter()
                    .all(|value| widened(value) == bound)),
                "{integer:?} is not sampled at {bound}"
            );
        }
    }
}

#[test]
fn the_float_rows_observe_one_nan_and_the_canonical_text() {
    use vibra_ir::external::{FloatOp, FloatType};
    let to_str = CompilerIntrinsic::Float(FloatType::F64, FloatOp::ToStr);
    for (bits, text) in [
        (1e20_f64.to_bits(), "1e20"),
        ((-0.0_f64).to_bits(), "-0.0"),
        (100.0_f64.to_bits(), "100.0"),
        (0xFFF4_0000_DEAD_BEEF, "nan"),
        (f64::NEG_INFINITY.to_bits(), "-inf"),
    ] {
        let vector = to_str
            .vectors()
            .iter()
            .find(|vector| vector.operands == [Value::F64(bits)])
            .unwrap();
        assert_eq!(vector.outcome, Outcome::Value(Value::Str(text.to_owned())));
    }
    let neg = CompilerIntrinsic::Float(FloatType::F32, FloatOp::Neg);
    for vector in neg.vectors() {
        if let Outcome::Value(Value::F32(bits)) = &vector.outcome
            && f32::from_bits(*bits).is_nan()
        {
            assert_eq!(*bits, vibra_ir::CANONICAL_NAN_F32);
        }
    }
}
