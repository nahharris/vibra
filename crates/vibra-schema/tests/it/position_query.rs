//! Producer tests for the Step 10 query schema.

use std::path::Path;

use serde_json::{Value, json};
use vibra_diagnostics::LineIndex;
use vibra_schema::{
    SCHEMA_VERSION, SOURCE_POSITION_QUERY_SCHEMA, SourcePositionQueryDocument,
};
use vibra_syntax::{FactStatus, parse_source};

use crate::support::{assert_valid, validator};

#[test]
fn rendered_queries_validate_and_are_deterministic() {
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
fn unknown_query_fields_are_rejected_by_the_schema() {
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
