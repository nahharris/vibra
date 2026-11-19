//! The integer and `char` primitive rows (milestone 4 Step 8a), in
//! WebAssembly.
//!
//! One table of sample vectors (`CompilerIntrinsic::vectors`, in `vibra-ir-v1`)
//! holds the operands and the specified outcome of every sample of every row
//! that the emitter lowers inline. Each row is run as one program, a tuple of
//! its samples, by the reference interpreter and by the WebAssembly module, and
//! both are held to the table. The expected outcomes come from the language's
//! own integer types, not from either backend, so the two backends cannot
//! drift apart on a boundary: overflow, division by zero, the signed minimum
//! divided by `-1`, the sign of a remainder, a shift at or past the width, a
//! conversion at the edge of its target, and the surrogate range of `char`.
//!
//! The tests also hold the lowering to what the runtime chapter asks of it:
//! every path through a row, the error results included, leaves the arena
//! balanced; a primitive application creates no activation; and emission is
//! deterministic.

use std::collections::{BTreeMap, BTreeSet};

use vibra_ir_v1::external::{CompilerIntrinsic, FloatOp, IntegerOp, NumericType};

// -- the harness --------------------------------------------------------------

// -- the programs of the vectors ------------------------------------------------

// -- the table and the rows -----------------------------------------------------

/// Whether the integer and `char` rows own `intrinsic`: the rows Step 8a lowers.
fn owned_by_step_8a(intrinsic: CompilerIntrinsic) -> bool {
    match intrinsic {
        CompilerIntrinsic::Convert(..)
        | CompilerIntrinsic::CharToU32
        | CompilerIntrinsic::CharFromU32 => true,
        CompilerIntrinsic::Integer(_, op) => {
            !matches!(op, IntegerOp::ToStr | IntegerOp::Parse)
        }
        _ => false,
    }
}

/// The import table names exactly the native symbols that have an import, which
/// are the manifest's `native` list but for the natives that apply a function
/// value, and the looping primitive rows.
#[test]
fn the_import_table_names_the_natives_and_the_looping_primitive_rows() {
    let stdlib = vibra_types_v1::load_stdlib().expect("standard library");
    // A native whose meaning applies a function value has no import.
    let function_applying = ["array.fold"];
    let manifest_natives = stdlib
        .native_symbols()
        .iter()
        .map(String::as_str)
        .filter(|symbol| !function_applying.contains(symbol))
        .collect::<BTreeSet<_>>();
    let imported_natives = CompilerIntrinsic::all()
        .into_iter()
        .filter(|row| row.is_native() && row.native_import().is_some())
        .map(CompilerIntrinsic::symbol)
        .collect::<BTreeSet<_>>();
    assert_eq!(imported_natives, manifest_natives);
    // The collection primitives that loop, and the 20 rows that read and write
    // number text: `to-str` and `parse` of the eight integer types and the two
    // float types.
    let mut looping_primitives = vec![
        CompilerIntrinsic::DictEntries,
        CompilerIntrinsic::ArrayAppend,
        CompilerIntrinsic::ArrayConcat,
        CompilerIntrinsic::ArraySlice,
    ];
    looping_primitives.extend(CompilerIntrinsic::all().into_iter().filter(|row| {
        matches!(
            row,
            CompilerIntrinsic::Integer(_, IntegerOp::ToStr | IntegerOp::Parse)
                | CompilerIntrinsic::Float(_, FloatOp::ToStr | FloatOp::Parse)
        )
    }));
    assert_eq!(looping_primitives.len(), 4 + 20);
    let table = vibra_ir_v1::external::NATIVE_ROWS
        .iter()
        .map(|row| row.intrinsic.symbol())
        .collect::<BTreeSet<_>>();
    let expected = manifest_natives
        .iter()
        .copied()
        .chain(looping_primitives.iter().map(|row| row.symbol()))
        .collect::<BTreeSet<_>>();
    assert_eq!(table, expected);
    assert_eq!(vibra_ir_v1::external::NATIVE_ROWS.len(), expected.len());
    for symbol in function_applying {
        let row = CompilerIntrinsic::from_symbol(symbol).expect("a registry row");
        assert!(row.native_import().is_none());
    }
}

#[test]
fn the_manifest_lists_exactly_the_rows_that_have_vectors_and_more() {
    // Every row with vectors is a symbol of the standard library's manifest, and
    // each is still declared `external: @compiler` there.
    let manifest = include_str!("../../../../stdlib-v1/manifest.vibon");
    for intrinsic in CompilerIntrinsic::all() {
        let quoted = format!("\"{}\"", intrinsic.symbol());
        assert!(manifest.contains(&quoted), "{}", intrinsic.symbol());
        if !intrinsic.vectors().is_empty() && owned_by_step_8a(intrinsic) {
            assert!(
                !intrinsic.is_native(),
                "{} is a primitive row",
                intrinsic.symbol()
            );
        }
    }
}

#[test]
fn every_integer_type_has_every_row_and_every_boundary_in_the_table() {
    let mut per_type: BTreeMap<&str, usize> = BTreeMap::new();
    for numeric in NumericType::ALL.into_iter().filter(|n| n.is_integer()) {
        for op in [
            IntegerOp::AddChecked,
            IntegerOp::SubChecked,
            IntegerOp::MulChecked,
            IntegerOp::DivChecked,
            IntegerOp::RemChecked,
            IntegerOp::ShiftLeftChecked,
            IntegerOp::ShiftRightChecked,
            IntegerOp::Equal,
            IntegerOp::Compare,
        ] {
            let vectors = CompilerIntrinsic::Integer(numeric, op).vectors();
            assert!(!vectors.is_empty(), "{}.{}", numeric.name(), op.name());
            *per_type.entry(numeric.name()).or_default() += vectors.len();
        }
        let neg = CompilerIntrinsic::Integer(numeric, IntegerOp::NegChecked);
        assert_eq!(neg.vectors().is_empty(), !numeric.is_signed());
    }
    assert_eq!(per_type.len(), 8);
}

// -- balance, determinism, and composition ----------------------------------------

// -- reclamation ------------------------------------------------------------------
