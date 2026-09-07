//! Producer and consumer tests for the M2 semantic workspace-position schema.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use jsonschema::Validator;
use serde_json::{Value, json};
use vibra_diagnostics::LineIndex;
use vibra_schema::{WORKSPACE_POSITION_QUERY_SCHEMA, WorkspacePositionQueryDocument};
use vibra_workspace::WorkspaceSnapshot;

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

fn validator() -> Validator {
    let schema: Value =
        serde_json::from_str(WORKSPACE_POSITION_QUERY_SCHEMA).expect("schema JSON");
    jsonschema::validator_for(&schema).expect("schema is valid JSON Schema")
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

fn fixture() -> (PathBuf, WorkspaceSnapshot, String) {
    let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let parent = fs::canonicalize(std::env::temp_dir()).expect("canonical temp parent");
    let root = parent.join(format!(
        "vibra-schema-query-{serial}-{}",
        std::process::id()
    ));
    fs::create_dir_all(root.join("src")).expect("source directory");
    fs::write(
        root.join("project.vibon"),
        "(record format: @project.v1 package: (record name: \"hello\" version: \"0.1.0\") targets: (array (record name: @hello kind: @lib root: \"src\")) dependencies: (map))",
    )
    .expect("project marker");
    let source =
        "(defn choose (value i32) i32 value)\n(defn answer () i32 (choose 1i32))";
    fs::write(root.join("src/main.vib"), source).expect("source module");
    let workspace = WorkspaceSnapshot::load(&root).expect("workspace snapshot");
    (root, workspace, source.to_owned())
}

#[test]
fn a_rendered_semantic_query_validates_and_round_trips() {
    let (root, workspace, source) = fixture();
    let offset = source.find("1i32").expect("literal");
    let query = workspace
        .query_position("src/main.vib", offset)
        .expect("workspace query");
    let index = LineIndex::new(&source);
    let rendered = WorkspacePositionQueryDocument::render_with_source(&query, &index);
    let text = serde_json::to_string(&rendered).expect("query serializes");
    let instance: Value = serde_json::from_str(&text).expect("query JSON");
    assert_valid(&validator(), &instance);
    assert_eq!(instance["sourceId"], json!("src/main.vib"));
    assert_eq!(instance["role"]["value"], json!("@literal"));
    assert_eq!(instance["expectedType"]["value"]["name"], json!("i32"));
    let parsed: WorkspacePositionQueryDocument =
        serde_json::from_str(&text).expect("query reads back");
    assert_eq!(parsed, rendered);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn semantic_fact_statuses_are_independent_and_closed() {
    let (root, workspace, source) = fixture();
    let query = workspace
        .query_position("src/main.vib", source.find("1i32").expect("literal"))
        .expect("workspace query");
    let rendered = WorkspacePositionQueryDocument::render_with_source(
        &query,
        &LineIndex::new(&source),
    );
    let mut unknown: Value = serde_json::to_value(&rendered).expect("query JSON");
    unknown["role"]["futureField"] = json!(true);
    assert!(validator().iter_errors(&unknown).next().is_some());
    assert!(serde_json::from_value::<WorkspacePositionQueryDocument>(unknown).is_err());

    let mut unavailable_with_value: Value =
        serde_json::to_value(&rendered).expect("query JSON");
    unavailable_with_value["role"] =
        json!({"status": "unavailable", "value": "@literal"});
    assert!(
        validator()
            .iter_errors(&unavailable_with_value)
            .next()
            .is_some()
    );
    assert!(
        serde_json::from_value::<WorkspacePositionQueryDocument>(
            unavailable_with_value
        )
        .is_err()
    );

    let mut exact_without_value: Value =
        serde_json::to_value(&rendered).expect("query JSON");
    exact_without_value["expectedType"] = json!({"status": "exact", "value": null});
    assert!(
        validator()
            .iter_errors(&exact_without_value)
            .next()
            .is_some()
    );
    assert!(
        serde_json::from_value::<WorkspacePositionQueryDocument>(exact_without_value)
            .is_err()
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn schema_identifier_is_stable() {
    let schema: Value =
        serde_json::from_str(WORKSPACE_POSITION_QUERY_SCHEMA).expect("schema JSON");
    assert_eq!(
        schema["$id"],
        json!("urn:vibra:schema:v1:workspace-position-query")
    );
}
