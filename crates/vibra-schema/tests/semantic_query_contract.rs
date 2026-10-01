//! Producer and consumer tests for the semantic workspace-position schema.

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
        "(record format: @project.v1 package: (record name: \"hello\" version: \"0.1.0\") targets: (array (record name: @hello kind: @lib root: \"src\")) dependencies: (dict))",
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

#[test]
fn consumer_rejects_missing_values_and_closed_vocabularies() {
    let (root, workspace, source) = fixture();
    let query = workspace
        .query_position("src/main.vib", source.find("1i32").expect("literal"))
        .expect("workspace query");
    let rendered = WorkspacePositionQueryDocument::render_with_source(
        &query,
        &LineIndex::new(&source),
    );

    let mut missing_value: Value = serde_json::to_value(&rendered).expect("query JSON");
    missing_value["role"]
        .as_object_mut()
        .expect("role object")
        .remove("value");
    assert!(validator().iter_errors(&missing_value).next().is_some());
    assert!(
        serde_json::from_value::<WorkspacePositionQueryDocument>(missing_value)
            .is_err()
    );

    let mut invalid_identity: Value =
        serde_json::to_value(&rendered).expect("query JSON");
    invalid_identity["identity"] = json!({
        "status": "exact",
        "value": {"kind": "future", "canonical": "hello"}
    });
    assert!(validator().iter_errors(&invalid_identity).next().is_some());
    assert!(
        serde_json::from_value::<WorkspacePositionQueryDocument>(invalid_identity)
            .is_err()
    );

    let mut invalid_type: Value = serde_json::to_value(&rendered).expect("query JSON");
    invalid_type["expectedType"] = json!({
        "status": "exact",
        "value": {
            "kind": "future",
            "name": "anything",
            "parameters": [],
            "result": null,
            "labelled": []
        }
    });
    assert!(validator().iter_errors(&invalid_type).next().is_some());
    assert!(
        serde_json::from_value::<WorkspacePositionQueryDocument>(invalid_type).is_err()
    );

    let mut invalid_application: Value =
        serde_json::to_value(&rendered).expect("query JSON");
    invalid_application["application"] = json!({
        "status": "exact",
        "value": {
            "kind": "future",
            "callee": null,
            "calleeType": null,
            "positional": [],
            "labelled": [],
            "resultType": {
                "kind": "primitive",
                "name": "i32",
                "parameters": [],
                "result": null,
                "labelled": []
            }
        }
    });
    assert!(
        validator()
            .iter_errors(&invalid_application)
            .next()
            .is_some()
    );
    assert!(
        serde_json::from_value::<WorkspacePositionQueryDocument>(invalid_application)
            .is_err()
    );

    let mut invalid_revision: Value =
        serde_json::to_value(&rendered).expect("query JSON");
    invalid_revision["workspaceRevision"] = json!("revision");
    assert!(validator().iter_errors(&invalid_revision).next().is_some());
    assert!(
        serde_json::from_value::<WorkspacePositionQueryDocument>(invalid_revision)
            .is_err()
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn consumer_rejects_inconsistent_structure_and_invalid_scope_records() {
    let (root, workspace, source) = fixture();
    let query = workspace
        .query_position("src/main.vib", source.find("1i32").expect("literal"))
        .expect("workspace query");
    let rendered = WorkspacePositionQueryDocument::render_with_source(
        &query,
        &LineIndex::new(&source),
    );

    let mut invalid_structural: Value =
        serde_json::to_value(&rendered).expect("query JSON");
    invalid_structural["structural"]["schemaVersion"] = json!(99);
    assert!(
        validator()
            .iter_errors(&invalid_structural)
            .next()
            .is_some()
    );
    assert!(
        serde_json::from_value::<WorkspacePositionQueryDocument>(invalid_structural)
            .is_err()
    );

    let mut mismatched_offset: Value =
        serde_json::to_value(&rendered).expect("query JSON");
    mismatched_offset["structural"]["offset"] = json!(0);
    assert!(validator().iter_errors(&mismatched_offset).next().is_none());
    assert!(
        serde_json::from_value::<WorkspacePositionQueryDocument>(mismatched_offset)
            .is_err()
    );

    let mut mismatched_node: Value =
        serde_json::to_value(&rendered).expect("query JSON");
    mismatched_node["nodeId"] = json!("src/main.vib#0-1");
    assert!(validator().iter_errors(&mismatched_node).next().is_none());
    assert!(
        serde_json::from_value::<WorkspacePositionQueryDocument>(mismatched_node)
            .is_err()
    );

    let mut invalid_local: Value = serde_json::to_value(&rendered).expect("query JSON");
    invalid_local["visibleLocals"] = json!({
        "status": "exact",
        "value": [{"name": "", "identity": "garbage"}]
    });
    assert!(validator().iter_errors(&invalid_local).next().is_some());
    assert!(
        serde_json::from_value::<WorkspacePositionQueryDocument>(invalid_local)
            .is_err()
    );

    let mut invalid_import: Value =
        serde_json::to_value(&rendered).expect("query JSON");
    invalid_import["visibleImports"] = json!({
        "status": "exact",
        "value": [{
            "alias": "",
            "module": "",
            "sourceId": "",
            "span": {
                "sourceId": "",
                "start": 8,
                "end": 2,
                "startPosition": {"line": 0, "column": 0},
                "endPosition": {"line": 0, "column": 0}
            }
        }]
    });
    assert!(validator().iter_errors(&invalid_import).next().is_some());
    assert!(
        serde_json::from_value::<WorkspacePositionQueryDocument>(invalid_import)
            .is_err()
    );
    let _ = fs::remove_dir_all(root);
}

/// One query of the type-metadata corpus project, at the first `needle`.
fn typed(needle: &str) -> WorkspacePositionQueryDocument {
    let tree = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/cases/V1-TOOL-workspace-position-types/tree");
    let source = fs::read_to_string(tree.join("src/main.vib")).expect("source");
    let workspace = WorkspaceSnapshot::load_confined(tree).expect("workspace");
    let query = workspace
        .query_position_with_embedded_stdlib(
            "src/main.vib",
            source.find(needle).expect("needle"),
        )
        .expect("workspace query");
    WorkspacePositionQueryDocument::render_with_source(&query, &LineIndex::new(&source))
}

#[test]
fn type_metadata_validates_round_trips_and_reads_back() {
    let validator = validator();
    for needle in [
        "(point x: 0i32",
        "(meters (as",
        "(as i32 -)",
        "(option.some (point",
        "(point x: x y: y)",
        "(shape.name value))\n\n(defn labelled",
        "(shape.label value)",
        "(shape.name value))\n\n(defn widened",
        "(from.convert value))\n\n(defn main",
        "(from.convert value)",
    ] {
        let rendered = typed(needle);
        let text = serde_json::to_string(&rendered).expect("query serializes");
        let instance: Value = serde_json::from_str(&text).expect("query JSON");
        assert_valid(&validator, &instance);
        let parsed: WorkspacePositionQueryDocument =
            serde_json::from_str(&text).expect("query reads back");
        assert_eq!(parsed, rendered, "{needle}");
    }

    // A constructor's contract is the declared type's fields.
    let constructor = serde_json::to_value(typed("(point x: 0i32")).expect("JSON");
    let application = &constructor["application"]["value"];
    assert_eq!(application["kind"], json!("@constructor"));
    assert_eq!(application["dispatch"], Value::Null);
    assert_eq!(application["resultType"]["kind"], json!("declared"));
    assert_eq!(application["labelled"][0]["name"], json!("x"));

    // An `as` arm names the member it narrows to and the union's members.
    let narrowing = serde_json::to_value(typed("(as i32 -)")).expect("JSON");
    let pattern = &narrowing["pattern"]["value"];
    assert_eq!(pattern["kind"], json!("@as"));
    assert_eq!(pattern["narrowed"]["name"], json!("i32"));
    assert_eq!(
        pattern["unionMembers"].as_array().expect("members").len(),
        2
    );

    // A contract call names what it resolved to.
    let selections = [
        ("(shape.name value))\n\n(defn labelled", "static", false),
        ("(shape.label value)", "default", false),
        ("(shape.name value))\n\n(defn widened", "dynamic", false),
        ("(from.convert value))\n\n(defn main", "static", true),
        ("(from.convert value)", "closed", true),
    ];
    for (needle, selection, destination) in selections {
        let call = serde_json::to_value(typed(needle)).expect("JSON");
        let dispatch = &call["application"]["value"]["dispatch"];
        assert_eq!(call["application"]["value"]["kind"], json!("@contract"));
        assert_eq!(dispatch["selection"], json!(selection), "{needle}");
        assert_eq!(dispatch["destination"], json!(destination), "{needle}");
    }
}

#[test]
fn consumer_rejects_malformed_type_metadata() {
    let validator = validator();
    let rejected = |instance: Value, what: &str| {
        assert!(validator.iter_errors(&instance).next().is_some(), "{what}");
        assert!(
            serde_json::from_value::<WorkspacePositionQueryDocument>(instance).is_err(),
            "{what}"
        );
    };
    let contract = serde_json::to_value(typed("(shape.label value)")).expect("JSON");

    let mut selection = contract.clone();
    selection["application"]["value"]["dispatch"]["selection"] = json!("virtual");
    rejected(selection, "an unknown selection");

    let mut missing = contract.clone();
    missing["application"]["value"]["dispatch"] = Value::Null;
    rejected(missing, "a contract call without a dispatch");

    let mut function = contract.clone();
    function["application"]["value"]["kind"] = json!("@function");
    rejected(function, "a function call with a dispatch");

    let mut kind = contract.clone();
    kind["application"]["value"]["kind"] = json!("@macro");
    rejected(kind, "an unknown application kind");

    let mut shape = contract.clone();
    shape["observedType"]["value"] = json!({
        "kind": "tuple", "name": "pair", "parameters": [], "result": null,
        "labelled": []
    });
    rejected(shape, "a structural type named by something else");

    let mut result = contract;
    result["observedType"]["value"] = json!({
        "kind": "declared", "name": "app.main.point", "parameters": [],
        "result": {"kind": "primitive", "name": "i32", "parameters": [],
            "result": null, "labelled": []},
        "labelled": []
    });
    rejected(result, "a declared type with a result");

    let narrowing = serde_json::to_value(typed("(as i32 -)")).expect("JSON");

    let mut unnarrowed = narrowing.clone();
    unnarrowed["pattern"]["value"]["narrowed"] = Value::Null;
    rejected(unnarrowed, "an `as` pattern that narrows to nothing");

    let mut variant = narrowing.clone();
    variant["pattern"]["value"]["kind"] = json!("@variant");
    rejected(variant, "a variant pattern that narrows");

    let mut unknown = narrowing.clone();
    unknown["pattern"]["value"]["kind"] = json!("@guard");
    rejected(unknown, "an unknown pattern kind");

    let mut absent = narrowing;
    absent.as_object_mut().expect("object").remove("pattern");
    rejected(absent, "a query without a pattern fact");
}
