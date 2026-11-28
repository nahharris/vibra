//! Closed compiler registry contract.

use vibra_ir_v1::external::{CompilerIntrinsic, RoleTypes};

#[test]
fn registry_contains_only_the_reviewed_operations() {
    let symbols = CompilerIntrinsic::all()
        .into_iter()
        .map(CompilerIntrinsic::symbol)
        .collect::<Vec<_>>();
    // Twenty methods for each signed and eighteen for each unsigned integer
    // type (each arithmetic row unchecked and checked), a conversion to each
    // of the seven other integer types, nine for each float type, and the
    // module and collection rows.
    assert_eq!(symbols.len(), 4 * 20 + 4 * 18 + 8 * 7 + 2 * 9 + 26);
    let distinct = symbols.iter().collect::<std::collections::BTreeSet<_>>();
    assert_eq!(distinct.len(), symbols.len());
    for symbol in [
        "i8.neg-checked",
        "u64.shift-right-checked",
        "i32.parse",
        "f32.compare-total",
        "f64.to-str",
        "char.from-u32",
        "str.from-utf8",
        "bytes.to-array",
        "array.slice",
    ] {
        assert!(symbols.contains(&symbol), "{symbol} is missing");
    }
}

#[test]
fn registry_signatures_are_exact_and_backend_neutral() {
    let concat = CompilerIntrinsic::StrConcat.signature(&RoleTypes::default());
    assert_eq!(concat.parameters().len(), 2);
    assert_eq!(concat.parameters()[0], vibra_ir_v1::Type::Str);
    assert_eq!(concat.parameters()[1], vibra_ir_v1::Type::Str);
    assert_eq!(concat.result(), vibra_ir_v1::Type::Str);

    let length = CompilerIntrinsic::StrLength.signature(&RoleTypes::default());
    assert_eq!(length.parameters(), &[vibra_ir_v1::Type::Str]);
    assert_eq!(length.result(), vibra_ir_v1::Type::U64);
}
