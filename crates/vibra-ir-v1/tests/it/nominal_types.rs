//! Declared-type IR: the canonical encoding of an observed value.

use vibra_ir_v1::{ObservedValue, Value};

#[test]
fn observed_values_render_the_canonical_value_encoding() {
    let anonymous = ObservedValue::Record {
        type_id: None,
        fields: vec![("a".to_owned(), ObservedValue::Primitive(Value::Bool(true)))],
    };
    assert_eq!(
        anonymous.canonical_vibon(),
        "(record kind: @record fields: (record a: true))"
    );
    assert!(ObservedValue::Primitive(Value::I32(2)) == Value::I32(2));
}
