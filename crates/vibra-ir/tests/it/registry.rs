//! The `@compiler` registry at major 2, minor 0
//! (`docs/spec/06-runtime.md`, "Compiler intrinsic registry").

use vibra_ir::external::{
    CompilerIntrinsic, IntegerOp, IntegerType, IntrinsicRow, Outcome, REGISTRY_MAJOR,
    REGISTRY_MINOR, spell_generics,
};
use vibra_ir::{Value, encode_value};

use crate::{library, table};

const SNAPSHOT: &str = include_str!("../../registry/compiler-2.0.txt");

/// The line of `row` in `registry/compiler-2.0.txt`: the symbol, the
/// generics, each with its bound as `T: Encodable`, the parameters as an
/// input vector, the result, and the marks (`@total` or nothing), separated
/// by tabs.
fn snapshot_line(row: &IntrinsicRow) -> String {
    let parameters = row
        .parameters
        .iter()
        .map(|parameter| parameter.spelling(row.generics))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "{}\t{}\t{{{}}}\t{}\t{}",
        row.symbol,
        spell_generics(row.generics, row.bounds),
        parameters,
        row.result.spelling(row.generics),
        if row.total { "@total" } else { "" }
    )
}

#[test]
fn the_snapshot_is_the_table() {
    assert_eq!((REGISTRY_MAJOR, REGISTRY_MINOR), (2, 0));
    let rendered = CompilerIntrinsic::all()
        .into_iter()
        .map(|intrinsic| snapshot_line(&intrinsic.row()) + "\n")
        .collect::<String>();
    assert_eq!(rendered, SNAPSHOT);
    assert_eq!(SNAPSHOT.lines().count(), 242);
}

/// The three narrowing members are rows of 2.0, generic over the grant
/// type, with the signatures of the chapter (Q25).
#[test]
fn the_three_grant_rows_have_the_chapter_s_signatures() {
    let expected = [
        (
            CompilerIntrinsic::GrantRestrict,
            "grant.restrict",
            "Fn({G FsPolicy} Result(G ScopeError))",
        ),
        (
            CompilerIntrinsic::GrantSub,
            "grant.sub",
            "Fn({G RelativePath} Result(G ScopeError))",
        ),
        (
            CompilerIntrinsic::GrantLimit,
            "grant.limit",
            "Fn({G GrantLimits} G)",
        ),
    ];
    for (intrinsic, symbol, signature) in expected {
        assert_eq!(CompilerIntrinsic::from_symbol(symbol), Some(intrinsic));
        let row = intrinsic.row();
        assert_eq!(row.generics, ["G"], "{symbol}");
        assert!(row.pure, "{symbol}");
        let parameters = row
            .parameters
            .iter()
            .map(|parameter| parameter.spelling(row.generics))
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(
            format!("Fn({{{parameters}}} {})", row.result.spelling(row.generics)),
            signature
        );
        assert!(
            SNAPSHOT.lines().any(|line| line == snapshot_line(&row)),
            "{symbol}"
        );
    }
    let grant_rows = SNAPSHOT
        .lines()
        .filter(|line| line.starts_with("grant."))
        .count();
    assert_eq!(grant_rows, 3);
}

/// `fs.validNames` is the one row marked total: `Fn({List(Str)} Bool)`,
/// pure, with no generics, written with the mark `@total` in the snapshot
/// (`docs/spec/06-runtime.md`, "Compiler intrinsic registry").
#[test]
fn fs_valid_names_is_the_one_total_row() {
    let total = CompilerIntrinsic::all()
        .into_iter()
        .filter(|intrinsic| intrinsic.is_total())
        .collect::<Vec<_>>();
    assert_eq!(total, [CompilerIntrinsic::FsValidNames]);
    let row = CompilerIntrinsic::FsValidNames.row();
    assert_eq!(row.symbol, "fs.validNames");
    assert!(row.pure && row.total);
    assert!(row.generics.is_empty());
    assert_eq!(
        snapshot_line(&row),
        "fs.validNames\t\t{List(Str)}\tBool\t@total"
    );
    let marked = SNAPSHOT
        .lines()
        .filter(|line| line.ends_with("\t@total"))
        .collect::<Vec<_>>();
    assert_eq!(marked, ["fs.validNames\t\t{List(Str)}\tBool\t@total"]);
    assert!(
        SNAPSHOT
            .lines()
            .all(|line| line.ends_with('\t') || line.ends_with("\t@total")),
        "every row has a marks column"
    );
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
                | CompilerIntrinsic::GrantRestrict
                | CompilerIntrinsic::GrantSub
                | CompilerIntrinsic::GrantLimit
                | CompilerIntrinsic::FsValidNames
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

mod host {
    //! The host registry of major 2 and the test-world registry
    //! (`docs/spec/06-runtime.md`, "Host registry", "Error types", and
    //! "Registry versioning"). Every expected value below is copied by hand
    //! from the chapter's tables.

    use std::collections::BTreeMap;

    use vibra_ir::external::host::{
        Delivery, FS_KIND_CASES, FS_METADATA_FIELDS, HOST_REGISTRY_MAJOR,
        HOST_REGISTRY_MINOR, HOST_ROWS, HostErrorType, HostInterface, HostOp, HostType,
        SHARED_ERROR_CASES, TEST_WORLD_ROWS, TestWorldOp, host_row, test_world_row,
    };

    const SNAPSHOT: &str = include_str!("../../registry/host-2.0.txt");

    /// One row of the chapter's host table: symbol, owner module, owner
    /// interface, member, further parameters, `Ok` type, and error type.
    type Expected = (
        &'static str,
        &'static str,
        &'static str,
        &'static str,
        &'static [(&'static str, &'static str)],
        &'static str,
        &'static str,
    );

    /// The chapter's host table.
    const TABLE: [Expected; 17] = [
        (
            "io.stdinText",
            "@std.io",
            "StdinText",
            "read",
            &[("max", "U64")],
            "Str",
            "IoError",
        ),
        (
            "io.stdinBytes",
            "@std.io",
            "StdinBytes",
            "read",
            &[("max", "U64")],
            "Bytes",
            "IoError",
        ),
        (
            "io.stdoutText",
            "@std.io",
            "StdoutText",
            "write",
            &[("value", "Str")],
            "Void",
            "IoError",
        ),
        (
            "io.stdoutBytes",
            "@std.io",
            "StdoutBytes",
            "write",
            &[("value", "Bytes")],
            "Void",
            "IoError",
        ),
        (
            "io.stderrText",
            "@std.io",
            "StderrText",
            "write",
            &[("value", "Str")],
            "Void",
            "IoError",
        ),
        (
            "io.stderrBytes",
            "@std.io",
            "StderrBytes",
            "write",
            &[("value", "Bytes")],
            "Void",
            "IoError",
        ),
        (
            "fs.readFile",
            "@std.fs",
            "DirReadBytes",
            "read",
            &[("at", "RelativePath")],
            "Bytes",
            "FsError",
        ),
        (
            "fs.readText",
            "@std.fs",
            "DirReadText",
            "read",
            &[("at", "RelativePath")],
            "Str",
            "FsError",
        ),
        (
            "fs.readEntries",
            "@std.fs",
            "DirList",
            "entries",
            &[("at", "RelativePath")],
            "List(Str)",
            "FsError",
        ),
        (
            "fs.writeFile",
            "@std.fs",
            "DirWriteBytes",
            "write",
            &[("at", "RelativePath"), ("contents", "Bytes")],
            "Void",
            "FsError",
        ),
        (
            "fs.writeText",
            "@std.fs",
            "DirWriteText",
            "write",
            &[("at", "RelativePath"), ("contents", "Str")],
            "Void",
            "FsError",
        ),
        (
            "fs.metadataStat",
            "@std.fs",
            "DirStat",
            "stat",
            &[("at", "RelativePath")],
            "FsMetadata",
            "FsError",
        ),
        (
            "env.readVar",
            "@std.env",
            "EnvReadVar",
            "read",
            &[("name", "Str")],
            "Str",
            "EnvError",
        ),
        (
            "env.readArgs",
            "@std.env",
            "EnvReadArgs",
            "read",
            &[],
            "List(Str)",
            "EnvError",
        ),
        (
            "time.nowWall",
            "@std.time",
            "WallClock",
            "read",
            &[],
            "I64",
            "TimeError",
        ),
        (
            "time.nowMonotonic",
            "@std.time",
            "MonotonicClock",
            "read",
            &[],
            "U64",
            "TimeError",
        ),
        (
            "random.generateBytes",
            "@std.random",
            "RandomBytes",
            "generate",
            &[("count", "U64")],
            "Bytes",
            "RandomError",
        ),
    ];

    #[test]
    fn each_host_row_has_the_owner_parameters_and_result_of_the_chapter() {
        assert_eq!((HOST_REGISTRY_MAJOR, HOST_REGISTRY_MINOR), (2, 0));
        assert_eq!(HOST_ROWS.len(), TABLE.len());
        for (row, (symbol, module, owner, member, further, ok, error)) in
            HOST_ROWS.iter().zip(TABLE)
        {
            assert_eq!(row.symbol, symbol);
            let found =
                host_row(symbol).unwrap_or_else(|| panic!("`{symbol}` is a row"));
            assert_eq!(found, row, "{symbol}");
            assert_eq!(row.owner.module(), module, "{symbol}");
            assert_eq!(row.owner.name(), owner, "{symbol}");
            assert_eq!(row.owner.member(), member, "{symbol}");
            assert_eq!(row.owner.owner().path(), format!("{module}.{owner}"));
            let parameters = row
                .parameters
                .iter()
                .map(|parameter| (parameter.name, parameter.ty.spelling()))
                .collect::<Vec<_>>();
            let expected = further
                .iter()
                .map(|(name, ty)| (*name, (*ty).to_owned()))
                .collect::<Vec<_>>();
            assert_eq!(parameters, expected, "{symbol}");
            assert_eq!(row.ok.spelling(), ok, "{symbol}");
            assert_eq!(row.error.name(), error, "{symbol}");
            assert_eq!(row.error.module(), module, "{symbol}");
            assert_eq!(row.op.symbol(), symbol);
            assert_eq!(row.op.row(), *row);
        }
    }

    #[test]
    fn the_stdin_rows_take_a_count() {
        for symbol in ["io.stdinText", "io.stdinBytes"] {
            let row = host_row(symbol).unwrap();
            let parameters = row
                .parameters
                .iter()
                .map(|parameter| (parameter.name, parameter.ty.spelling()))
                .collect::<Vec<_>>();
            assert_eq!(parameters, [("max", "U64".to_owned())], "{symbol}");
        }
    }

    #[test]
    fn one_owner_per_operation_and_one_operation_per_owner() {
        assert_eq!(HostOp::ALL.len(), 17);
        assert_eq!(HostInterface::ALL.len(), 17);
        for (op, interface) in HostOp::ALL.into_iter().zip(HostInterface::ALL) {
            assert_eq!(op.interface(), interface);
            assert_eq!(interface.operation(), op);
        }
        let mut owners = HOST_ROWS.iter().map(|row| row.owner).collect::<Vec<_>>();
        owners.sort();
        owners.dedup();
        assert_eq!(owners.len(), 17);
        let mut ops = HOST_ROWS.iter().map(|row| row.op).collect::<Vec<_>>();
        ops.sort();
        ops.dedup();
        assert_eq!(ops.len(), 17);
    }

    #[test]
    fn every_host_row_is_once_and_since_zero() {
        for row in &HOST_ROWS {
            assert_eq!(row.delivery, Delivery::Once, "{}", row.symbol);
            assert_eq!(row.delivery.atom(), "@once");
            assert_eq!(row.since, 0, "{}", row.symbol);
        }
    }

    #[test]
    fn each_error_type_has_its_own_cases_then_the_four_shared_ones() {
        let shared = ["denied", "limitExceeded", "unsupported", "outcomeUnknown"];
        assert_eq!(SHARED_ERROR_CASES, shared);
        let expected: [(HostErrorType, &[&str]); 5] = [
            (HostErrorType::Io, &["invalidText", "closed", "other"]),
            (
                HostErrorType::Fs,
                &[
                    "notFound",
                    "permissionDenied",
                    "notAFile",
                    "notADirectory",
                    "invalidPath",
                    "invalidText",
                    "other",
                ],
            ),
            (HostErrorType::Env, &["notPresent", "notUnicode"]),
            (HostErrorType::Time, &["unavailable"]),
            (HostErrorType::Random, &["unavailable"]),
        ];
        assert_eq!(HostErrorType::ALL.len(), expected.len());
        for (error, own) in expected {
            let mut cases = own.to_vec();
            cases.extend(shared);
            assert_eq!(error.cases(), cases, "{}", error.name());
            for case in error.cases() {
                assert!(
                    case.starts_with(|c: char| c.is_ascii_lowercase())
                        && case.chars().all(|c| c.is_ascii_alphanumeric()),
                    "`{case}` is camelCase"
                );
            }
        }
    }

    /// The chapter's `TestWorld` table: member, labelled parameters, and
    /// what an invocation returns.
    const TEST_WORLD: [(&str, &str, &str); 6] = [
        (
            "testWorld.directory",
            "files: Dict(RelativePath Str)",
            "{readBytes: DirReadBytes readText: DirReadText list: DirList \
             writeBytes: DirWriteBytes writeText: DirWriteText stat: DirStat}",
        ),
        (
            "testWorld.streams",
            "stdin: Str",
            "{stdinText: StdinText stdinBytes: StdinBytes stdoutText: StdoutText \
             stdoutBytes: StdoutBytes stderrText: StderrText stderrBytes: StderrBytes}",
        ),
        (
            "testWorld.environment",
            "variables: Dict(Str Str) arguments: List(Str)",
            "{variables: EnvReadVar arguments: EnvReadArgs}",
        ),
        (
            "testWorld.clocks",
            "",
            "{wall: WallClock monotonic: MonotonicClock}",
        ),
        ("testWorld.random", "", "RandomBytes"),
        ("testWorld.output", "", "{stdout: Bytes stderr: Bytes}"),
    ];

    #[test]
    fn each_test_world_row_is_owned_by_test_world_in_std_test() {
        assert_eq!(TEST_WORLD_ROWS.len(), TEST_WORLD.len());
        assert_eq!(TestWorldOp::ALL.len(), 6);
        for ((row, op), (symbol, labelled, result)) in
            TEST_WORLD_ROWS.iter().zip(TestWorldOp::ALL).zip(TEST_WORLD)
        {
            assert_eq!(row.symbol, symbol);
            assert_eq!(row.op, op);
            assert_eq!(test_world_row(symbol), Some(row));
            assert_eq!(row.owner().module, "@std.test", "{symbol}");
            assert_eq!(row.owner().name, "TestWorld", "{symbol}");
            assert_eq!(row.since, 0, "{symbol}");
            assert_eq!(row.delivery, Delivery::Once, "{symbol}");
            assert_eq!(format!("testWorld.{}", op.member()), symbol);
            let inputs = if labelled.is_empty() {
                "TestWorld".to_owned()
            } else {
                format!("TestWorld {labelled}")
            };
            assert_eq!(
                row.signature(),
                format!("Proc({{{inputs}}} {result} {{TestWorld}})")
            );
        }
    }

    /// The major-1 spelling of a symbol: each upper-case letter of the
    /// member becomes `-` and its lower case, so `fs.readText` becomes
    /// `fs.read-text`.
    fn major_1(symbol: &str) -> String {
        symbol
            .chars()
            .flat_map(|c| {
                if c.is_ascii_uppercase() {
                    vec!['-', c.to_ascii_lowercase()]
                } else {
                    vec![c]
                }
            })
            .collect()
    }

    #[test]
    fn a_major_1_spelling_is_no_row() {
        assert_eq!(major_1("fs.readText"), "fs.read-text");
        for row in &HOST_ROWS {
            let old = major_1(row.symbol);
            assert_ne!(old, row.symbol);
            assert_eq!(host_row(&old), None, "{old}");
            assert_eq!(test_world_row(&old), None, "{old}");
        }
    }

    #[test]
    fn the_two_tables_are_disjoint() {
        for row in &TEST_WORLD_ROWS {
            assert_eq!(host_row(row.symbol), None, "{}", row.symbol);
        }
        for row in &HOST_ROWS {
            assert_eq!(test_world_row(row.symbol), None, "{}", row.symbol);
        }
        assert_eq!(host_row("testWorld.output"), None);
        assert_eq!(test_world_row("io.stdoutText"), None);
    }

    #[test]
    fn the_snapshot_is_the_table() {
        assert_eq!(registry_snapshot(), SNAPSHOT);
        assert!(snapshot_violations(SNAPSHOT, &registry_snapshot()).is_empty());
        let count = |kind: &str| {
            SNAPSHOT
                .lines()
                .filter(|line| line.split('\t').next() == Some(kind))
                .count()
        };
        assert_eq!((count("row"), count("testWorld")), (17, 6));
        assert_eq!(SNAPSHOT.lines().next(), Some("registry host 2.0"));
    }

    #[test]
    fn a_changed_result_type_is_one_violation_naming_the_symbol() {
        let changed = SNAPSHOT.replace(
            "Proc({DirReadText RelativePath} Result(Str FsError) {DirReadText})",
            "Proc({DirReadText RelativePath} Result(Bytes FsError) {DirReadText})",
        );
        assert_ne!(changed, SNAPSHOT);
        let violations = snapshot_violations(SNAPSHOT, &changed);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(violations[0].contains("fs.readText"), "{violations:?}");
    }

    #[test]
    fn a_removed_or_added_entry_is_a_violation_and_a_next_minor_is_not() {
        let removed = SNAPSHOT
            .lines()
            .filter(|line| !line.contains("\ttestWorld.output\t"))
            .map(|line| format!("{line}\n"))
            .collect::<String>();
        let violations = snapshot_violations(SNAPSHOT, &removed);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(violations[0].contains("testWorld.output"));
        let case = SNAPSHOT.replace("notPresent notUnicode", "notPresent");
        let violations = snapshot_violations(SNAPSHOT, &case);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(violations[0].contains("EnvError"));
        let added = "row\tfs.appendText\t@std.fs.DirAppendText\t\
                     Proc({DirAppendText RelativePath Str} Result(Void FsError) {DirAppendText})";
        let next = format!("{SNAPSHOT}{added}\t1\t@once\n")
            .replace("registry host 2.0", "registry host 2.1");
        assert!(snapshot_violations(SNAPSHOT, &next).is_empty());
        let same_minor = format!("{SNAPSHOT}{added}\t0\t@once\n");
        let violations = snapshot_violations(SNAPSHOT, &same_minor);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(violations[0].contains("fs.appendText"));
    }

    #[test]
    fn an_added_test_world_row_is_a_next_minor_as_an_added_host_row_is() {
        let added = "testWorld\ttestWorld.network\t@std.test.TestWorld\t\
                     Proc({TestWorld} Void {TestWorld})";
        let next = format!("{SNAPSHOT}{added}\t1\t@once\n")
            .replace("registry host 2.0", "registry host 2.1");
        assert!(snapshot_violations(SNAPSHOT, &next).is_empty());
        let same_minor = format!("{SNAPSHOT}{added}\t0\t@once\n");
        let violations = snapshot_violations(SNAPSHOT, &same_minor);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(violations[0].contains("testWorld.network"));
    }

    #[test]
    fn the_derived_minor_of_no_symbol_and_of_all_rows_is_zero() {
        assert_eq!(derived_registry_minor([]), Some(0));
        assert_eq!(
            derived_registry_minor(HOST_ROWS.iter().map(|row| row.symbol)),
            Some(0)
        );
        assert_eq!(
            derived_registry_minor(TEST_WORLD_ROWS.iter().map(|row| row.symbol)),
            Some(0)
        );
        assert_eq!(
            derived_registry_minor(["io.stdinText", "testWorld.output"]),
            Some(0)
        );
        assert_eq!(derived_registry_minor(["testWorld.network"]), None);
        assert_eq!(
            derived_registry_minor([major_1("io.stdinText").as_str()]),
            None
        );
    }

    /// The `since` of the row of either table with the wire symbol `symbol`.
    fn row_since_of(symbol: &str) -> Option<u32> {
        host_row(symbol)
            .map(|row| row.since)
            .or_else(|| test_world_row(symbol).map(|row| row.since))
    }

    /// The derived registry minor of a set of symbols of either table: the
    /// greatest `since` among them, and `0` when the set is empty. A symbol that
    /// is a row of neither table has no minor, so the answer is `None`.
    fn derived_registry_minor<'a>(
        symbols: impl IntoIterator<Item = &'a str>,
    ) -> Option<u32> {
        let mut minor = 0;
        for symbol in symbols {
            minor = minor.max(row_since_of(symbol)?);
        }
        Some(minor)
    }

    /// Both tables and their types as the text of `registry/host-2.0.txt`.
    fn registry_snapshot() -> String {
        let minor = HOST_ROWS
            .iter()
            .map(|row| row.since)
            .chain(TEST_WORLD_ROWS.iter().map(|row| row.since))
            .max()
            .unwrap_or(0);
        let mut lines = vec![format!("registry host {HOST_REGISTRY_MAJOR}.{minor}")];
        for row in &HOST_ROWS {
            lines.push(format!(
                "row\t{}\t{}\t{}\t{}\t{}",
                row.symbol,
                row.owner.owner().path(),
                row.signature(),
                row.since,
                row.delivery.atom()
            ));
        }
        for row in &TEST_WORLD_ROWS {
            lines.push(format!(
                "testWorld\t{}\t{}\t{}\t{}\t{}",
                row.symbol,
                row.owner().path(),
                row.signature(),
                row.since,
                row.delivery.atom()
            ));
        }
        for error in HostErrorType::ALL {
            lines.push(format!(
                "type\t{}\t{}\t{}",
                error.name(),
                error.module(),
                error.cases().join(" ")
            ));
        }
        lines.push(format!(
            "type\tFsKind\t@std.fs\t{}",
            FS_KIND_CASES.join(" ")
        ));
        lines.push(format!(
            "type\tFsMetadata\t@std.fs\t{}",
            HostType::Record(FS_METADATA_FIELDS).spelling()
        ));
        lines.into_iter().map(|line| line + "\n").collect()
    }

    /// The parsed snapshot: its minor, and each entry keyed by its kind and
    /// name, with its whole line.
    fn snapshot_entries(
        text: &str,
    ) -> Result<(u32, BTreeMap<String, String>), Vec<String>> {
        let mut minor = None;
        let mut entries = BTreeMap::new();
        let mut problems = Vec::new();
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            let mut fields = line.split('\t');
            match (fields.next(), fields.next()) {
                (Some(header), None) if header.starts_with("registry ") => match header
                    .strip_prefix("registry host 2.")
                    .and_then(|number| number.parse::<u32>().ok())
                {
                    Some(number) => minor = Some(number),
                    None => {
                        problems.push(format!("unparsable registry line `{header}`"))
                    }
                },
                (Some(kind @ ("row" | "testWorld" | "type")), Some(name)) => {
                    entries.insert(format!("{kind} {name}"), line.to_owned());
                }
                _ => problems.push(format!("unparsable snapshot line `{line}`")),
            }
        }
        if !problems.is_empty() {
            return Err(problems);
        }
        minor.map(|minor| (minor, entries)).ok_or_else(|| {
            vec!["the snapshot has no `registry host 2.N` line".to_owned()]
        })
    }

    /// The `since` field of a `row` or `testWorld` line.
    fn row_since(line: &str) -> Option<u32> {
        line.split('\t').nth(4)?.parse().ok()
    }

    /// Every way `current` breaks the versioning rule against the `recorded`
    /// snapshot. A changed or removed entry (a host row, a test-world row, or a
    /// type) needs a new major. An added row of either table needs a `since`
    /// greater than the recorded minor. An added type never violates the rule by
    /// itself. An empty answer means `current` is `recorded`, or a legitimate
    /// next minor of it.
    fn snapshot_violations(recorded: &str, current: &str) -> Vec<String> {
        let (recorded_minor, recorded) = match snapshot_entries(recorded) {
            Ok(parsed) => parsed,
            Err(problems) => return problems,
        };
        let (_, current) = match snapshot_entries(current) {
            Ok(parsed) => parsed,
            Err(problems) => return problems,
        };
        let mut violations = Vec::new();
        for (key, line) in &recorded {
            match current.get(key) {
                None => violations
                    .push(format!("{key} was removed; this needs a new major")),
                Some(now) if now != line => violations.push(format!(
                    "{key} changed from `{line}` to `{now}`; this needs a new major"
                )),
                Some(_) => {}
            }
        }
        for (key, line) in &current {
            if recorded.contains_key(key) || key.starts_with("type ") {
                continue;
            }
            match row_since(line) {
                Some(since) if since > recorded_minor => {}
                _ => violations.push(format!(
                    "{key} was added with a `since` that is not greater than the recorded \
                     minor {HOST_REGISTRY_MAJOR}.{recorded_minor}"
                )),
            }
        }
        violations
    }
}
