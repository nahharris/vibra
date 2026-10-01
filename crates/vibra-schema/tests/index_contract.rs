//! Producer and consumer tests for the `@index.v1` JSON contract.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::path::Path;

use jsonschema::Validator;
use serde_json::{Value, json};
use vibra_schema::{INDEX_SCHEMA, IndexDocumentJson, SCHEMA_VERSION};
use vibra_workspace::WorkspaceSnapshot;

fn validator() -> Validator {
    let schema: Value =
        serde_json::from_str(INDEX_SCHEMA).expect("the schema is valid JSON");
    jsonschema::validator_for(&schema).expect("the schema is valid JSON Schema")
}

fn assert_valid(validator: &Validator, instance: &Value) {
    let errors: Vec<String> = validator
        .iter_errors(instance)
        .map(|error| error.to_string())
        .collect();
    assert!(
        errors.is_empty(),
        "instance does not validate:\n{}\n{}",
        serde_json::to_string_pretty(instance).unwrap_or_default(),
        errors.join("\n")
    );
}

/// The index of one corpus project, rendered as JSON.
fn rendered(case: &str) -> IndexDocumentJson {
    let tree = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/cases")
        .join(case)
        .join("tree");
    let workspace = WorkspaceSnapshot::load_confined(tree).expect("workspace");
    let document =
        vibra_workspace::index::index_with_embedded_stdlib(&workspace).expect("index");
    IndexDocumentJson::render(&document)
}

#[test]
fn a_rendered_index_validates_and_round_trips_deterministically() {
    let document = rendered("V1-TOOL-index-records");
    assert_eq!(document.schema_version, SCHEMA_VERSION);

    let first = serde_json::to_string(&document).expect("index serializes");
    let second = serde_json::to_string(&rendered("V1-TOOL-index-records"))
        .expect("index serializes twice");
    assert_eq!(first, second);

    let instance = serde_json::to_value(&document).expect("index becomes JSON");
    assert_valid(&validator(), &instance);
    let parsed: IndexDocumentJson =
        serde_json::from_str(&first).expect("index reads back");
    assert_eq!(parsed, document);
}

#[test]
fn a_consumer_reads_identities_relations_and_two_target_receivers() {
    let document = rendered("V1-TOOL-index-records");

    // Declarations are sorted by identity and carry their checked facts.
    let ids = document
        .declarations
        .iter()
        .map(|declaration| declaration.id.as_str())
        .collect::<Vec<_>>();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(ids, sorted);
    let describe = document
        .declarations
        .iter()
        .find(|declaration| declaration.id == "app.main.describe")
        .expect("describe");
    assert_eq!(describe.kind, "function");
    assert_eq!(
        describe.facts.as_ref().expect("facts").applications,
        ["app.main.celsius.warmer", "app.main.shape.label"]
    );

    // One receiver implementing a generic interface at two targets is two
    // blocks, told apart by the applied interface.
    let conversions = document
        .implementations
        .iter()
        .filter(|implementation| implementation.interface.contains("std.core.from"))
        .collect::<Vec<_>>();
    assert_eq!(conversions.len(), 2);
    assert_eq!(conversions[0].receiver, conversions[1].receiver);
    assert_ne!(conversions[0].interface, conversions[1].interface);
    assert_eq!(
        conversions[0].members[0].contract,
        conversions[1].members[0].contract
    );

    // A name the resolver leaves to the checker has no target.
    assert!(
        document
            .references
            .iter()
            .any(|reference| reference.written == "option.some"
                && reference.to.is_none())
    );
}

#[test]
fn a_declaration_that_does_not_check_has_null_facts() {
    let document = rendered("V1-TOOL-index-unavailable");
    let instance = serde_json::to_value(&document).expect("index becomes JSON");
    assert_valid(&validator(), &instance);
    for id in [
        "app.main.audit",
        "app.main.audit.record",
        "app.main.unknown",
    ] {
        let declaration = document
            .declarations
            .iter()
            .find(|declaration| declaration.id == id)
            .unwrap_or_else(|| panic!("{id}"));
        assert!(declaration.facts.is_none(), "{id}");
    }
}

#[test]
fn the_schema_rejects_a_malformed_index() {
    let validator = validator();
    let mut instance =
        serde_json::to_value(rendered("V1-TOOL-index-records")).expect("JSON");
    assert_valid(&validator, &instance);

    // An unknown declaration kind.
    let mut unknown_kind = instance.clone();
    unknown_kind["declarations"][0]["kind"] = json!("macro");
    assert!(!validator.is_valid(&unknown_kind));

    // An identity that keeps its `@`.
    let mut marked = instance.clone();
    marked["declarations"][0]["id"] = json!("@app.main");
    assert!(!validator.is_valid(&marked));

    // A record with an unknown field.
    instance["references"][0]["line"] = json!(1);
    assert!(!validator.is_valid(&instance));
}
