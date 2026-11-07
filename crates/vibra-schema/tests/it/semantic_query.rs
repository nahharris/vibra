//! Producer tests for the semantic workspace-position schema: every rendered
//! document validates against the published JSON Schema, the rules a schema
//! cannot express are asserted directly on the produced value, and the schema
//! rejects a malformed instance. `vibra-schema` renders these documents and
//! never reads them back, so the schema is the consumer contract.

use std::fs;
use std::path::PathBuf;

use jsonschema::Validator;
use serde_json::{Value, json};
use vibra_diagnostics::LineIndex;
use vibra_schema::{
    SpanDocument, WORKSPACE_POSITION_QUERY_SCHEMA, WorkspacePositionQueryDocument,
};
use vibra_workspace::WorkspaceSnapshot;
use vibra_workspace::query::WorkspacePositionQuery;

use crate::support::{self, assert_valid, temporary_directory};

fn validator() -> Validator {
    support::validator(WORKSPACE_POSITION_QUERY_SCHEMA)
}

/// A span's own rules: a forward range with one-based positions, carrying
/// `owner` as its source identity when the span has one to match, and never an
/// empty identity.
fn assert_span(span: &SpanDocument, owner: Option<&str>, what: &str) {
    assert!(span.start <= span.end, "{what}: span start exceeds end");
    for position in [&span.start_position, &span.end_position] {
        assert!(
            position.line >= 1 && position.column >= 1,
            "{what}: span positions are one-based"
        );
    }
    if let Some(owner) = owner {
        assert_eq!(
            span.source_id.as_deref(),
            Some(owner),
            "{what}: the span's sourceId is its owner's"
        );
    }
    assert!(
        span.source_id.as_deref() != Some(""),
        "{what}: sourceId cannot be empty"
    );
}

/// The rules that cross fields of one produced document. The JSON Schema
/// checks each field's own shape; it cannot relate `offset`, `nodeId`,
/// `sourceId`, the structural result, spans, and witnesses to one another, so
/// these are asserted on the value the producer built.
fn assert_cross_field_rules(document: &WorkspacePositionQueryDocument) {
    let source_id = document.source_id.as_str();
    let structural = &document.structural;
    assert_eq!(structural.schema_version, document.schema_version);
    assert_eq!(structural.offset, document.offset, "structural offset");
    assert_eq!(structural.source_id.as_deref(), Some(source_id));
    assert_span(&structural.span, Some(source_id), "structural span");
    assert_eq!(
        document.node_id,
        format!(
            "{source_id}#{}-{}",
            structural.span.start, structural.span.end
        ),
        "nodeId identifies the structural span exactly"
    );

    if let Some(imports) = &document.visible_imports.value {
        for import in imports {
            assert_span(&import.span, Some(&import.source_id), "import span");
        }
    }

    if let Some(effects) = &document.effects.value
        && let Some(witnesses) = &effects.witnesses
    {
        let performed: Vec<&str> = effects
            .enclosing
            .as_ref()
            .map(|enclosing| {
                enclosing
                    .performed
                    .roots
                    .iter()
                    .map(String::as_str)
                    .collect()
            })
            .unwrap_or_default();
        let roots: Vec<&str> = witnesses
            .iter()
            .map(|witness| witness.root.as_str())
            .collect();
        assert_eq!(roots, performed, "one witness per performed root, in order");
        for hop in witnesses.iter().flat_map(|witness| &witness.hops) {
            assert_span(&hop.span, None, "hop span");
            assert!(hop.span.source_id.is_some(), "a hop span names its source");
        }
    }
}

/// Renders `query` and asserts the cross-field rules of what was produced.
fn render(
    query: &WorkspacePositionQuery,
    source: &str,
) -> WorkspacePositionQueryDocument {
    let document = WorkspacePositionQueryDocument::render_with_source(
        query,
        &LineIndex::new(source),
    );
    assert_cross_field_rules(&document);
    document
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

/// The rendered document of the fixture's literal, as JSON.
fn fixture_instance() -> Value {
    let (root, workspace, source) = fixture();
    let query = workspace
        .query_position("src/main.vib", source.find("1i32").expect("literal"))
        .expect("workspace query");
    let instance = serde_json::to_value(render(&query, &source)).expect("query JSON");
    let _ = fs::remove_dir_all(root);
    instance
}

/// Asserts that the schema refuses `instance`, saying `why` when it does not.
fn assert_rejected(validator: &Validator, instance: &Value, why: &str) {
    assert!(validator.iter_errors(instance).next().is_some(), "{why}");
}

#[test]
fn a_rendered_semantic_query_validates() {
    let instance = fixture_instance();
    assert_valid(&validator(), &instance);
    assert_eq!(instance["sourceId"], json!("src/main.vib"));
    assert_eq!(instance["role"]["value"], json!("@literal"));
    assert_eq!(instance["expectedType"]["value"]["name"], json!("i32"));
}

/// The cross-field assertions must fail on a document that breaks a rule, or a
/// passing test proves nothing.
#[test]
fn the_cross_field_rules_reject_an_inconsistent_document() {
    let (root, workspace, source) = fixture();
    let query = workspace
        .query_position("src/main.vib", source.find("1i32").expect("literal"))
        .expect("workspace query");
    let rendered = render(&query, &source);
    let _ = fs::remove_dir_all(root);
    let fails = |document: &WorkspacePositionQueryDocument| {
        std::panic::catch_unwind(|| assert_cross_field_rules(document)).is_err()
    };

    let mut offset = rendered.clone();
    offset.structural.offset = rendered.offset + 1;
    assert!(
        fails(&offset),
        "a structural offset that differs from the envelope"
    );

    let mut node = rendered.clone();
    node.node_id = "src/main.vib#0-1".to_owned();
    assert!(fails(&node), "a nodeId that is not the structural span");

    let mut source_id = rendered.clone();
    source_id.structural.source_id = Some("src/other.vib".to_owned());
    assert!(fails(&source_id), "a structural sourceId that differs");

    let mut backwards = rendered;
    backwards.structural.span.start = backwards.structural.span.end + 1;
    assert!(fails(&backwards), "a span whose start exceeds its end");
}

#[test]
fn semantic_fact_statuses_are_independent_and_closed() {
    let validator = validator();
    let rendered = fixture_instance();

    let mut unknown = rendered.clone();
    unknown["role"]["futureField"] = json!(true);
    assert_rejected(&validator, &unknown, "an unknown field in a fact");

    let mut unavailable_with_value = rendered.clone();
    unavailable_with_value["role"] =
        json!({"status": "unavailable", "value": "@literal"});
    assert_rejected(
        &validator,
        &unavailable_with_value,
        "an unavailable fact with a value",
    );

    let mut exact_without_value = rendered;
    exact_without_value["expectedType"] = json!({"status": "exact", "value": null});
    assert_rejected(
        &validator,
        &exact_without_value,
        "an exact fact without a value",
    );
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
fn the_schema_rejects_missing_values_and_closed_vocabularies() {
    let validator = validator();
    let rendered = fixture_instance();

    let mut missing_value = rendered.clone();
    missing_value["role"]
        .as_object_mut()
        .expect("role object")
        .remove("value");
    assert_rejected(&validator, &missing_value, "a fact without a value field");

    let mut invalid_identity = rendered.clone();
    invalid_identity["identity"] = json!({
        "status": "exact",
        "value": {"kind": "future", "canonical": "hello"}
    });
    assert_rejected(&validator, &invalid_identity, "an unknown identity kind");

    let mut invalid_type = rendered.clone();
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
    assert_rejected(&validator, &invalid_type, "an unknown type kind");

    let mut invalid_application = rendered.clone();
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
    assert_rejected(&validator, &invalid_application, "an unknown application");

    let mut invalid_revision = rendered;
    invalid_revision["workspaceRevision"] = json!("revision");
    assert_rejected(&validator, &invalid_revision, "a malformed revision");
}

#[test]
fn the_schema_rejects_inconsistent_structure_and_invalid_scope_records() {
    let validator = validator();
    let rendered = fixture_instance();

    let mut invalid_structural = rendered.clone();
    invalid_structural["structural"]["schemaVersion"] = json!(99);
    assert_rejected(
        &validator,
        &invalid_structural,
        "a structural result of another major version",
    );

    let mut invalid_local = rendered.clone();
    invalid_local["visibleLocals"] = json!({
        "status": "exact",
        "value": [{"name": "", "identity": "garbage"}]
    });
    assert_rejected(&validator, &invalid_local, "a malformed local binding");

    let mut invalid_import = rendered;
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
    assert_rejected(&validator, &invalid_import, "a malformed import alias");
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
    render(&query, &source)
}

#[test]
fn type_metadata_validates_and_names_what_it_resolved() {
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
        let instance = serde_json::to_value(typed(needle)).expect("query JSON");
        assert_valid(&validator, &instance);
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
        ("(from.convert value)", "static", true),
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
fn the_schema_rejects_malformed_type_metadata() {
    let validator = validator();
    let rejected = |instance: Value, what: &str| {
        assert_rejected(&validator, &instance, what);
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
    render(&query, &source)
}

#[test]
fn the_effects_fact_validates_with_and_without_witnesses() {
    let validator = validator();
    for witnesses in [false, true] {
        let rendered = effects_query(
            "V1-TOOL-workspace-position-effect-witnesses",
            "src/main.vib",
            "(three))",
            witnesses,
        );
        let instance = serde_json::to_value(&rendered).expect("query JSON");
        assert_valid(&validator, &instance);
        let value = &instance["effects"]["value"];
        assert_eq!(value["witnesses"].is_array(), witnesses);
        assert_eq!(
            value["enclosing"]["performed"]["roots"]
                .as_array()
                .map(Vec::len),
            Some(2)
        );
        // The witness rule `render` asserted is not vacuous here: one record
        // for each performed root, in the order of the row.
        if witnesses {
            let effects = rendered.effects.value.expect("effects");
            assert_eq!(effects.witnesses.expect("witnesses").len(), 2);
        }
    }
}

/// Witnesses that omit or reorder a performed root are a cross-field fault the
/// schema cannot see; the producer-side assertion must catch them.
#[test]
fn the_cross_field_rules_reject_witnesses_that_do_not_match_the_performed_row() {
    let rendered = effects_query(
        "V1-TOOL-workspace-position-effect-witnesses",
        "src/main.vib",
        "(three))",
        true,
    );
    let fails = |document: &WorkspacePositionQueryDocument| {
        std::panic::catch_unwind(|| assert_cross_field_rules(document)).is_err()
    };

    let mut omitted = rendered.clone();
    omitted
        .effects
        .value
        .as_mut()
        .and_then(|effects| effects.witnesses.as_mut())
        .expect("witnesses")
        .pop();
    assert!(fails(&omitted), "witnesses that omit a performed root");

    let mut reordered = rendered;
    reordered
        .effects
        .value
        .as_mut()
        .and_then(|effects| effects.witnesses.as_mut())
        .expect("witnesses")
        .reverse();
    assert!(fails(&reordered), "witnesses out of the row's order");
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
    let rejected =
        |instance: Value, why: &str| assert_rejected(&validator, &instance, why);

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

    let mut duplicate = base.clone();
    duplicate["effects"]["value"]["enclosing"]["performed"]["roots"] =
        json!(["app.main.audit", "app.main.audit"]);
    rejected(duplicate, "a row with a duplicate root");

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
