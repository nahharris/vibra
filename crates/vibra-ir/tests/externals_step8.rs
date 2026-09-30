//! Closed compiler registry contract.

#![allow(clippy::expect_used, clippy::indexing_slicing)]

use vibra_diagnostics::ByteSpan;
use vibra_ir::external::{
    CompilerIntrinsic, REGISTRY_VERSION, RoleTypes, SemanticIdentity,
};
use vibra_ir::{
    CheckedFunction, CheckedProgram, Expr, FunctionSignature, SourceOrigin, Type, Value,
};

#[test]
fn registry_contains_only_the_reviewed_operations() {
    let symbols = CompilerIntrinsic::all()
        .into_iter()
        .map(CompilerIntrinsic::symbol)
        .collect::<Vec<_>>();
    // Twelve methods for each signed and eleven for each unsigned integer
    // type, nine for each float type, and the module and collection rows.
    assert_eq!(symbols.len(), 4 * 12 + 4 * 11 + 2 * 9 + 24);
    let distinct = symbols.iter().collect::<std::collections::BTreeSet<_>>();
    assert_eq!(distinct.len(), symbols.len());
    for symbol in [
        "i8.neg-checked",
        "u64.shift-right",
        "i32.parse",
        "f32.compare-total",
        "f64.to-str",
        "char.from-u32",
        "text.from-utf8",
        "bytes.to-array",
        "array.slice",
    ] {
        assert!(symbols.contains(&symbol), "{symbol} is missing");
    }
    assert!(CompilerIntrinsic::from_symbol("u8.neg-checked").is_none());
    assert!(CompilerIntrinsic::from_symbol("f64.add-checked").is_none());
    assert!(CompilerIntrinsic::from_symbol("integer.add-checked").is_none());
    assert!(CompilerIntrinsic::from_symbol("host.read").is_none());
    for intrinsic in CompilerIntrinsic::all() {
        assert_eq!(
            CompilerIntrinsic::from_symbol(intrinsic.symbol()),
            Some(intrinsic)
        );
    }
}

#[test]
fn registry_signatures_are_exact_and_backend_neutral() {
    let concat = CompilerIntrinsic::TextConcat.signature(&RoleTypes::default());
    assert_eq!(concat.parameters().len(), 2);
    assert_eq!(concat.parameters()[0], vibra_ir::Type::Str);
    assert_eq!(concat.parameters()[1], vibra_ir::Type::Str);
    assert_eq!(concat.result(), vibra_ir::Type::Str);

    let length = CompilerIntrinsic::TextLength.signature(&RoleTypes::default());
    assert_eq!(length.parameters(), &[vibra_ir::Type::Str]);
    assert_eq!(length.result(), vibra_ir::Type::U64);
}

#[test]
fn registry_entries_expose_the_versioned_semantic_identity() {
    assert_eq!(REGISTRY_VERSION, "vibra_v1");
    assert_eq!(
        CompilerIntrinsic::TextConcat.registry_version(),
        REGISTRY_VERSION
    );
    assert_eq!(
        CompilerIntrinsic::TextLength.registry_version(),
        REGISTRY_VERSION
    );
    assert_eq!(
        CompilerIntrinsic::TextConcat.semantic_identity(),
        SemanticIdentity::UnicodeScalarConcatenation
    );
    assert_eq!(
        CompilerIntrinsic::TextLength.semantic_identity(),
        SemanticIdentity::UnicodeScalarLength
    );
}

#[test]
fn external_operands_remain_in_recursive_call_analysis() {
    let origin = SourceOrigin::new("recursive-external.vib", ByteSpan::new(0, 1));
    let recursive = Expr::call(0, Vec::new(), Type::Str, origin.clone());
    let body = Expr::external(
        CompilerIntrinsic::TextConcat,
        vec![
            recursive,
            Expr::literal(Value::Str(String::new()), origin.clone()),
        ],
        origin.clone(),
    );
    let function = CheckedFunction::new(
        "loop",
        FunctionSignature::new(Vec::new(), Type::Str),
        body,
        origin,
    )
    .expect("valid shape");
    let program = CheckedProgram::try_new(vec![function], 0)
        .expect("external operand must remain visible to recursive-group analysis");
    assert_eq!(program.recursive_groups(), [vec![0]]);
}
