//! Step 7 generic VIBON data tests.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::path::Path;

use vibra_diagnostics::DiagnosticCode;
use vibra_syntax::{AtomRole, DataValue, TypedDataSchema, parse_data};

#[test]
fn valid_literals_and_containers_decode_as_one_data_value() {
    let document = parse_data(
        Path::new("value.vibon"),
        "(record name: \"hello\" values: (array @a 1) empty: (map))",
    )
    .expect("data loader");

    assert!(document.accepted(), "{:?}", document.diagnostics());
    let Some(root) = document.data() else {
        panic!("decoded data root")
    };
    assert!(matches!(root.value(), DataValue::Record(fields) if fields.len() == 3));
}

#[test]
fn trailing_trivia_does_not_invalidate_a_data_root() {
    let document =
        parse_data(Path::new("value.vibon"), "(array @value)\n").expect("data loader");
    assert!(document.accepted(), "{:?}", document.diagnostics());
    assert!(document.data().is_some());
}

#[test]
fn maps_reject_duplicate_keys_and_decode_in_source_order() {
    let document =
        parse_data(Path::new("value.vibon"), "(map @z 2 @a 1)").expect("data loader");
    assert!(document.accepted(), "{:?}", document.diagnostics());
    assert!(
        matches!(document.data().map(|node| node.value()), Some(DataValue::Map(pairs)) if pairs.len() == 2)
    );

    let duplicate =
        parse_data(Path::new("value.vibon"), "(map @a 1 @a 2)").expect("data loader");
    assert!(!duplicate.accepted());
    assert_eq!(
        duplicate.diagnostics()[0].code(),
        DiagnosticCode::DataDuplicateKey
    );
}

#[test]
fn deeply_nested_data_decodes_without_using_the_host_stack() {
    let depth = 1_024;
    let source = format!("{}@value{}", "(array ".repeat(depth), ")".repeat(depth));
    let document = parse_data(Path::new("deep.vibon"), &source).expect("data loader");
    assert!(document.accepted(), "{:?}", document.diagnostics());
    assert!(document.data().is_some());
}

#[test]
fn closed_data_grammar_rejects_symbols_applications_and_odd_shapes() {
    for source in ["symbol", "(call @a)", "(map @a)", "(record name:)"] {
        let document =
            parse_data(Path::new("value.vibon"), source).expect("data loader");
        assert!(!document.accepted(), "accepted {source:?}");
        assert!(document.diagnostics().iter().any(|diagnostic| matches!(
            diagnostic.code(),
            DiagnosticCode::DataInvalidShape | DiagnosticCode::DataInvalidValue
        )));
    }
}

#[test]
fn typed_schema_declares_order_and_atom_roles_without_resolution() {
    let mut schema = TypedDataSchema::new(["name", "target"]);
    schema.set_atom_role("target", AtomRole::Reference);
    assert_eq!(schema.field_order(), ["name", "target"]);
    assert_eq!(schema.atom_role("target"), Some(AtomRole::Reference));
    assert_eq!(schema.atom_role("name"), None);
}
