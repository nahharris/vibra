//! Producer and consumer tests for the Step 10 query schema.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::path::Path;

use jsonschema::Validator;
use serde_json::{Value, json};
use vibra_diagnostics::LineIndex;
use vibra_schema::{
    SCHEMA_VERSION, SOURCE_POSITION_QUERY_SCHEMA, SourcePositionQueryDocument,
};
use vibra_syntax::{FactStatus, parse_source};

fn validator(schema_text: &str) -> Validator {
    let schema: Value =
        serde_json::from_str(schema_text).expect("the schema is valid JSON");
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

#[test]
fn rendered_queries_validate_and_round_trip_deterministically() {
    let source = "; 🌱\n(defn f (value str) str value)";
    let document = parse_source(Path::new("query.vib"), source).expect("source loader");
    let index = LineIndex::new(source);
    let offset = source.find("value").expect("pattern value");
    let query = document.query_position(offset).expect("query result");
    let rendered = SourcePositionQueryDocument::render(&query, &index);

    let first = serde_json::to_string(&rendered).expect("query serializes");
    let second = serde_json::to_string(&rendered).expect("query serializes twice");
    assert_eq!(first, second);
    assert_eq!(rendered.schema_version, SCHEMA_VERSION);
    assert_eq!(rendered.status, "exact");

    let instance = serde_json::to_value(&rendered).expect("query becomes JSON");
    assert_valid(&validator(SOURCE_POSITION_QUERY_SCHEMA), &instance);
    let parsed: SourcePositionQueryDocument =
        serde_json::from_str(&first).expect("query reads back");
    assert_eq!(parsed, rendered);
}

#[test]
fn null_and_empty_continuation_facts_are_distinct_on_the_wire() {
    let source = "; comment\n(defn f (value str) str value)";
    let document = parse_source(Path::new("query.vib"), source).expect("source loader");
    let index = LineIndex::new(source);
    let trivia = document
        .query_position(source.find("comment").expect("comment"))
        .expect("trivia query");
    let trivia_document = SourcePositionQueryDocument::render(&trivia, &index);
    assert_eq!(trivia.status(), FactStatus::Unavailable);
    let trivia_instance = serde_json::to_value(&trivia_document).expect("trivia JSON");
    assert_eq!(trivia_instance["permittedForms"], Value::Null);
    assert_eq!(trivia_instance["permittedLabels"], Value::Null);

    let type_query = document
        .query_position(source.find("str").expect("type"))
        .expect("type query");
    let type_document = SourcePositionQueryDocument::render(&type_query, &index);
    let type_instance = serde_json::to_value(&type_document).expect("type JSON");
    assert_eq!(type_instance["permittedLabels"], json!([]));
    assert!(type_instance["permittedForms"].is_array());
}

#[test]
fn unknown_query_fields_are_rejected_by_schema_and_consumer() {
    let source = "(defn f (value str) str value)";
    let document = parse_source(Path::new("query.vib"), source).expect("source loader");
    let index = LineIndex::new(source);
    let query = document.query_position(1).expect("query result");
    let rendered = SourcePositionQueryDocument::render(&query, &index);
    let mut instance = serde_json::to_value(&rendered).expect("query JSON");
    instance["futureField"] = json!(true);

    assert!(
        validator(SOURCE_POSITION_QUERY_SCHEMA)
            .iter_errors(&instance)
            .next()
            .is_some()
    );
    assert!(serde_json::from_value::<SourcePositionQueryDocument>(instance).is_err());
}

#[test]
fn the_query_schema_publishes_a_stable_identifier() {
    let schema: Value =
        serde_json::from_str(SOURCE_POSITION_QUERY_SCHEMA).expect("schema JSON");
    assert_eq!(
        schema["$id"],
        json!("urn:vibra:schema:v1:source-position-query")
    );
}
