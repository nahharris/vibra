//! Producer-validates tests of the command result envelope.

use serde_json::{Value, json};
use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode, LineIndex};
use vibra_schema::{
    CommandCapability, CommandOutcome, DiagnosticDocument, Envelope, HelpPayload,
    Limits, Payload, Status, validate_command_result,
};

fn limits() -> Limits {
    Limits {
        profile: "m1Default".to_owned(),
        reached: Vec::new(),
        complete: true,
    }
}

fn envelope(command: &str, status: Status, result: CommandOutcome) -> Envelope {
    Envelope::new("0.0.1", command, status, result, limits())
}

fn unavailable_diagnostic() -> DiagnosticDocument {
    let diagnostic = Diagnostic::new(
        DiagnosticCode::ToolUnavailable,
        ByteSpan::empty_at(0),
        "unavailable",
    )
    .with_note("the command `check` is not available");
    DiagnosticDocument::render(&diagnostic, &LineIndex::new("")).expect("renders")
}

fn help() -> Envelope {
    let mut help = envelope("help", Status::Passed, CommandOutcome::Ok);
    help.payload = Payload::Help(HelpPayload {
        usage: "usage: vibra".to_owned(),
        capabilities: vec![CommandCapability {
            command: "help".to_owned(),
            operations: vec!["help".to_owned()],
            profiles: vec!["toolingV2".to_owned()],
            supported: true,
            schema_ids: vec!["urn:vibra:schema:v2:command-result".to_owned()],
        }],
    });
    help
}

fn value(envelope: &Envelope) -> Value {
    serde_json::to_value(envelope).expect("serializes")
}

#[test]
fn the_envelope_has_exactly_the_nine_fields_in_order_and_validates() {
    let json = help().to_validated_json().expect("validates");
    let keys = [
        "schemaVersion",
        "toolVersion",
        "command",
        "workspaceRevision",
        "status",
        "result",
        "diagnostics",
        "limits",
        "payload",
    ];
    let mut at = 0;
    for key in keys {
        let found = json[at..].find(&format!("\"{key}\":")).expect(key);
        at += found;
    }
    let parsed: Value = serde_json::from_str(&json).expect("one JSON value");
    assert_eq!(parsed.as_object().expect("object").len(), 9);
    assert_eq!(parsed["schemaVersion"], 2);
    assert_eq!(parsed["workspaceRevision"], Value::Null);
    assert_eq!(
        parsed["limits"],
        json!({"profile": "m1Default", "reached": [], "complete": true})
    );
}

#[test]
fn every_result_atom_is_accepted_and_has_the_specified_spelling() {
    let atoms = [
        "@command.ok",
        "@command.diagnostics",
        "@command.testFailed",
        "@command.invalidInput",
        "@command.operationalFailure",
        "@command.unavailable",
        "@command.trap",
        "@command.programFailed",
    ];
    for (outcome, atom) in CommandOutcome::ALL.into_iter().zip(atoms) {
        assert_eq!(outcome.as_atom(), atom);
        assert_eq!(
            serde_json::to_value(outcome).expect("serializes"),
            json!(atom)
        );
    }
}

#[test]
fn help_validates_and_its_payload_is_closed() {
    validate_command_result(&value(&help())).expect("help validates");

    let mut extra = value(&help());
    extra["payload"]["commands"] = json!([]);
    assert!(validate_command_result(&extra).is_err());

    let mut empty = value(&help());
    empty["payload"] = json!({});
    assert!(
        validate_command_result(&empty).is_err(),
        "help carries its payload"
    );

    let mut capability = value(&help());
    capability["payload"]["capabilities"][0]["extra"] = json!(1);
    assert!(validate_command_result(&capability).is_err());

    let mut revision = value(&help());
    revision["workspaceRevision"] = json!(format!("sha256:{}", "0".repeat(64)));
    assert!(
        validate_command_result(&revision).is_err(),
        "help takes no snapshot"
    );

    let mut refuted = value(&help());
    refuted["status"] = json!("refuted");
    assert!(validate_command_result(&refuted).is_err());
}

#[test]
fn the_empty_payload_is_for_every_response_but_a_successful_help() {
    let mut check = envelope("check", Status::Unsupported, CommandOutcome::Unavailable);
    check.diagnostics.push(unavailable_diagnostic());
    validate_command_result(&value(&check)).expect("unavailable validates");

    let mut payload = value(&check);
    payload["payload"] = json!({"accepted": true});
    assert!(validate_command_result(&payload).is_err());

    let invalid = envelope("help", Status::Unknown, CommandOutcome::InvalidInput);
    validate_command_result(&value(&invalid)).expect("a refused help has no payload");
}

#[test]
fn an_unavailable_result_is_unsupported_and_carries_its_diagnostic() {
    let mut check = envelope("check", Status::Unsupported, CommandOutcome::Unavailable);
    check.diagnostics.push(unavailable_diagnostic());
    validate_command_result(&value(&check)).expect("validates");

    let mut wrong_status = value(&check);
    wrong_status["status"] = json!("passed");
    assert!(validate_command_result(&wrong_status).is_err());

    let mut none = check.clone();
    none.diagnostics.clear();
    assert!(validate_command_result(&value(&none)).is_err());

    let mut wrong_code = value(&check);
    wrong_code["diagnostics"][0]["code"] = json!("@tool.stalePlan");
    assert!(validate_command_result(&wrong_code).is_err());
}

#[test]
fn invalid_input_has_no_snapshot_and_no_diagnostic() {
    let invalid = envelope("fmt", Status::Unknown, CommandOutcome::InvalidInput);
    validate_command_result(&value(&invalid)).expect("validates");

    let mut with_diagnostic = invalid.clone();
    with_diagnostic.diagnostics.push(unavailable_diagnostic());
    assert!(validate_command_result(&value(&with_diagnostic)).is_err());

    let mut with_revision = value(&invalid);
    with_revision["workspaceRevision"] = json!(format!("sha256:{}", "a".repeat(64)));
    assert!(validate_command_result(&with_revision).is_err());
}

#[test]
fn the_envelope_is_closed_and_rejects_unknown_fields_values_and_versions() {
    let good = value(&help());
    validate_command_result(&good).expect("validates");

    let mut extra = good.clone();
    extra["extra"] = json!(1);
    assert!(validate_command_result(&extra).is_err());

    let mut version = good.clone();
    version["schemaVersion"] = json!(1);
    assert!(validate_command_result(&version).is_err());

    let mut kebab = good.clone();
    kebab["result"] = json!("@command.invalid-input");
    assert!(validate_command_result(&kebab).is_err());

    let mut command = good.clone();
    command["command"] = json!("init");
    assert!(
        validate_command_result(&command).is_err(),
        "no alias is a command"
    );

    let mut status = good.clone();
    status["status"] = json!("ok");
    assert!(validate_command_result(&status).is_err());

    let mut revision = good.clone();
    revision["workspaceRevision"] = json!("sha256:abc");
    assert!(validate_command_result(&revision).is_err());

    let mut limits = good.clone();
    limits["limits"]["extra"] = json!(1);
    assert!(validate_command_result(&limits).is_err());

    let mut missing = good;
    missing.as_object_mut().expect("object").remove("limits");
    assert!(validate_command_result(&missing).is_err());
}

#[test]
fn a_diagnostic_inside_the_envelope_is_validated_against_the_diagnostic_schema() {
    let mut check = envelope("check", Status::Unsupported, CommandOutcome::Unavailable);
    check.diagnostics.push(unavailable_diagnostic());
    let mut bad = value(&check);
    bad["diagnostics"][0]["code"] = json!("@tool.unavailable-x");
    assert!(validate_command_result(&bad).is_err());
    let mut span = value(&check);
    span["diagnostics"][0]["primarySpan"]["start"] = json!(5);
    assert!(validate_command_result(&span).is_err(), "start after end");
}

#[test]
fn the_validated_value_is_an_error_not_a_panic_for_an_invalid_envelope() {
    let mut invalid = envelope("check", Status::Passed, CommandOutcome::Unavailable);
    invalid.diagnostics.clear();
    let violation = invalid
        .to_validated_json()
        .expect_err("violates the schema");
    assert_eq!(violation.document(), "command-result");
    assert!(!violation.messages().is_empty());
}
