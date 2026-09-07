//! Closed M2 compiler registry contract.

#![allow(clippy::expect_used, clippy::indexing_slicing)]

use vibra_diagnostics::ByteSpan;
use vibra_ir::external::CompilerIntrinsic;
use vibra_ir::{
    CheckedFunction, CheckedProgram, Expr, FunctionSignature, PrimitiveType,
    SourceOrigin, Value,
};

#[test]
fn registry_contains_only_the_reviewed_text_operations() {
    assert_eq!(
        CompilerIntrinsic::ALL
            .iter()
            .map(|intrinsic| intrinsic.symbol())
            .collect::<Vec<_>>(),
        ["text.concat", "text.length"]
    );
    assert!(CompilerIntrinsic::from_symbol("integer.add-checked").is_none());
    assert!(CompilerIntrinsic::from_symbol("host.read").is_none());
}

#[test]
fn registry_signatures_are_exact_and_backend_neutral() {
    let concat = CompilerIntrinsic::TextConcat.signature();
    assert_eq!(concat.parameters().len(), 2);
    assert_eq!(concat.parameters()[0], vibra_ir::PrimitiveType::Str);
    assert_eq!(concat.parameters()[1], vibra_ir::PrimitiveType::Str);
    assert_eq!(concat.result(), vibra_ir::PrimitiveType::Str);

    let length = CompilerIntrinsic::TextLength.signature();
    assert_eq!(length.parameters(), &[vibra_ir::PrimitiveType::Str]);
    assert_eq!(length.result(), vibra_ir::PrimitiveType::U64);
}

#[test]
fn external_operands_remain_in_recursive_call_analysis() {
    let origin = SourceOrigin::new("recursive-external.vib", ByteSpan::new(0, 1));
    let recursive = Expr::call(0, Vec::new(), PrimitiveType::Str, origin.clone());
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
        FunctionSignature::new(Vec::new(), PrimitiveType::Str),
        body,
        origin,
    )
    .expect("valid shape");
    let error = CheckedProgram::try_new(vec![function], 0)
        .expect_err("external operand must not hide recursion");
    assert!(error.to_string().contains("recursive"));
}
