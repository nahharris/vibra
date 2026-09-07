//! Closed M2 compiler registry contract.

#![allow(clippy::expect_used)]

use vibra_ir::external::CompilerIntrinsic;

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
