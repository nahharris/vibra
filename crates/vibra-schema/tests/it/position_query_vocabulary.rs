//! The closed position-query vocabulary after the pre-M4 binding revision
//! (`docs/roadmap/pre-m4/01-bindings-return-never.md`, rows E32 and G06).

const SCHEMA: &str = include_str!("../../schemas/v1/workspace-position-query.json");

fn enum_of(name: &str) -> Vec<String> {
    let document: serde_json::Value =
        serde_json::from_str(SCHEMA).expect("schema JSON");
    document["$defs"][name]["enum"]
        .as_array()
        .unwrap_or_else(|| panic!("{name} is an enum"))
        .iter()
        .map(|value| value.as_str().expect("string").to_owned())
        .collect()
}

#[test]
fn the_context_vocabulary_has_the_new_contexts_and_no_let_body() {
    let contexts = enum_of("contextName");
    for expected in ["let-value", "let-else-fallback", "return-operand"] {
        assert!(contexts.iter().any(|name| name == expected), "{expected}");
    }
    assert!(!contexts.iter().any(|name| name == "let-body"));
}

#[test]
fn never_is_a_primitive_type_name() {
    assert!(enum_of("primitiveName").iter().any(|name| name == "never"));
}

/// The schema lists the primitive names on its own, so validating against it
/// stays independent of the table the checker uses; this keeps the two from
/// drifting.
#[test]
fn the_schema_primitive_names_are_the_syntax_table() {
    let table: Vec<String> = vibra_syntax::PRIMITIVE_TYPES
        .iter()
        .map(|(name, _)| (*name).to_owned())
        .collect();
    assert_eq!(enum_of("primitiveName"), table);
}
