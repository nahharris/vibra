//! Producer and consumer tests for the semantic workspace-position schema.

use std::fs;
use std::path::PathBuf;

use jsonschema::Validator;
use serde_json::{Value, json};
use vibra_diagnostics::LineIndex;
use vibra_schema::{WORKSPACE_POSITION_QUERY_SCHEMA, WorkspacePositionQueryDocument};
use vibra_workspace::WorkspaceSnapshot;

use crate::support::{self, assert_valid, temporary_directory};

fn validator() -> Validator {
    support::validator(WORKSPACE_POSITION_QUERY_SCHEMA)
}

fn fixture() -> (PathBuf, WorkspaceSnapshot, String) {
    let root = temporary_directory("query");
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

/// One query of the effects corpus project, at the first `needle`.
fn effects_query(
    case: &str,
    source_id: &str,
    needle: &str,
    witnesses: bool,
) -> WorkspacePositionQueryDocument {
    let tree = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/cases")
        .join(case)
        .join("tree");
    let source = fs::read_to_string(tree.join(source_id)).expect("source");
    let workspace = WorkspaceSnapshot::load_confined(tree).expect("workspace");
    let query = vibra_workspace::query::query_position_expanded(
        &workspace,
        source_id,
        source.find(needle).expect("needle"),
        None,
        vibra_workspace::query::PositionExpansion {
            effect_witnesses: witnesses,
        },
    )
    .expect("workspace query");
    WorkspacePositionQueryDocument::render_with_source(&query, &LineIndex::new(&source))
}

#[test]
fn the_effects_fact_validates_round_trips_and_reads_back() {
    let validator = validator();
    for witnesses in [false, true] {
        let rendered = effects_query(
            "V1-TOOL-workspace-position-effect-witnesses",
            "src/main.vib",
            "(three))",
            witnesses,
        );
        let text = serde_json::to_string(&rendered).expect("query serializes");
        let instance: Value = serde_json::from_str(&text).expect("query JSON");
        assert_valid(&validator, &instance);
        let parsed: WorkspacePositionQueryDocument =
            serde_json::from_str(&text).expect("query reads back");
        assert_eq!(parsed, rendered);
        let value = &instance["effects"]["value"];
        assert_eq!(value["witnesses"].is_array(), witnesses);
        assert_eq!(
            value["enclosing"]["performed"]["roots"]
                .as_array()
                .map(Vec::len),
            Some(2)
        );
    }
}

#[test]
fn the_schema_rejects_a_malformed_effects_fact_or_function_row() {
    let validator = validator();
    let base = serde_json::to_value(effects_query(
        "V1-TOOL-workspace-position-effect-witnesses",
        "src/main.vib",
        "(three))",
        true,
    ))
    .expect("JSON");
    let rejected = |instance: Value, why: &str| {
        assert!(
            validator.iter_errors(&instance).next().is_some(),
            "schema: {why}"
        );
        assert!(
            serde_json::from_value::<WorkspacePositionQueryDocument>(instance).is_err(),
            "reader: {why}"
        );
    };

    let mut unknown_row = base.clone();
    unknown_row["effects"]["value"]["enclosing"]["ceiling"]["tail"] = json!(null);
    rejected(unknown_row, "an unknown field in a row");

    let mut unknown_member = base.clone();
    unknown_member["effects"]["value"]["extra"] = json!(1);
    rejected(unknown_member, "an unknown member of the fact");

    let mut absent = base.clone();
    absent.as_object_mut().expect("object").remove("effects");
    rejected(absent, "an envelope without the effects fact");

    let mut kind = base.clone();
    kind["effects"]["value"]["enclosing"]["kind"] = json!("method");
    rejected(kind, "an unknown enclosing kind");

    let mut lambda = base.clone();
    lambda["effects"]["value"]["enclosing"]["kind"] = json!("lambda");
    rejected(lambda, "a lambda with an identity");

    let mut hops = base.clone();
    hops["effects"]["value"]["witnesses"] = json!([]);
    // The count is a cross-field rule only the consumer checks.
    assert!(
        serde_json::from_value::<WorkspacePositionQueryDocument>(hops).is_err(),
        "reader: witnesses that omit a performed root"
    );

    let mut callee = base.clone();
    callee["effects"]["value"]["witnesses"][0]["hops"][0]["callee"]["kind"] =
        json!("function-value");
    rejected(callee, "a function value with an identity");

    // The application's callee type carries its row, and only a function does.
    let call = serde_json::to_value(effects_query(
        "V1-TOOL-workspace-position-effects",
        "src/app/main.vib",
        "(log.audit.record target)",
        false,
    ))
    .expect("JSON");
    assert_valid(&validator, &call);
    let mut no_row = call.clone();
    no_row["application"]["value"]["calleeType"]
        .as_object_mut()
        .expect("type")
        .remove("effects");
    rejected(no_row, "a function type without its row");
    let mut primitive = call;
    primitive["application"]["value"]["resultType"]["effects"] = json!({"roots": []});
    rejected(primitive, "a primitive type with a row");
}
