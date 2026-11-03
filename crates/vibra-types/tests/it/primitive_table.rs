//! The primitive type table: the spellings below the IR and the IR's own
//! enumeration of primitive types list the same types in the same order.

use vibra_ir::Type;
use vibra_syntax::PRIMITIVE_TYPES;

#[test]
fn the_syntax_table_and_the_ir_enumeration_agree() {
    let spellings: Vec<&str> = PRIMITIVE_TYPES.iter().map(|(name, _)| *name).collect();
    let types: Vec<&str> = Type::PRIMITIVES.iter().map(Type::as_str).collect();
    assert_eq!(spellings, types);
}

#[test]
fn every_primitive_spelling_names_its_type() {
    for (name, _) in PRIMITIVE_TYPES {
        let ty = Type::primitive(name).expect("a primitive spelling names a type");
        assert_eq!(ty.as_str(), *name);
    }
    assert_eq!(Type::primitive("array"), None);
}
