//! The `fmt` and `check` payloads and their relations to the result.

use serde_json::{Value, json};
use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode, LineIndex};
use vibra_schema::{
    CheckPayload, CommandOutcome, DiagnosticDocument, Envelope, FmtPayload, Limits,
    ObligationKind, ObligationRecord, Payload, SpanDocument, Status,
    validate_command_result,
};

const REVISION: &str =
    "sha256:0000000000000000000000000000000000000000000000000000000000000000";

fn envelope(command: &str, status: Status, result: CommandOutcome) -> Envelope {
    let mut envelope = Envelope::new(
        "0.0.1",
        command,
        status,
        result,
        Limits {
            profile: "m1Default".to_owned(),
            reached: Vec::new(),
            complete: true,
        },
    );
    envelope.workspace_revision = Some(REVISION.to_owned());
    envelope
}

fn value(envelope: &Envelope) -> Value {
    serde_json::to_value(envelope).expect("serializes")
}

fn diagnostic(code: DiagnosticCode) -> DiagnosticDocument {
    let diagnostic = Diagnostic::new(code, ByteSpan::new(0, 1), "a fault")
        .with_source_id("src/app/main.vib");
    DiagnosticDocument::render(&diagnostic, &LineIndex::new("x")).expect("renders")
}

fn fmt(changed: bool, written: bool, text: Option<&str>) -> Payload {
    Payload::Fmt(FmtPayload {
        path: "src/app/main.vib".to_owned(),
        changed,
        written,
        text: text.map(str::to_owned),
    })
}

fn obligation(status: Status) -> ObligationRecord {
    ObligationRecord {
        kind: ObligationKind::Requires,
        declaration: REVISION.to_owned(),
        predicate_identity: REVISION.to_owned(),
        source: SpanDocument::render(
            ByteSpan::new(0, 1),
            &LineIndex::new("x"),
            Some("src/app/main.vib"),
        ),
        status,
        rule_ids: vec!["GUARD-1".to_owned()],
        dependencies: Vec::new(),
        counterexample: None,
    }
}

#[test]
fn the_three_rows_of_the_fmt_outcome_table_validate() {
    let mut preview = envelope("fmt", Status::Passed, CommandOutcome::Ok);
    preview.payload = fmt(true, false, Some("fn f endfn\n"));
    validate_command_result(&value(&preview)).expect("a preview validates");

    let mut written = envelope("fmt", Status::Passed, CommandOutcome::Ok);
    written.payload = fmt(true, true, None);
    validate_command_result(&value(&written)).expect("a write validates");

    let mut blocked = envelope("fmt", Status::Refuted, CommandOutcome::Diagnostics);
    blocked
        .diagnostics
        .push(diagnostic(DiagnosticCode::TypeMismatch));
    blocked.payload = fmt(true, false, Some("planned\n"));
    validate_command_result(&value(&blocked)).expect("a blocked write validates");
}

#[test]
fn the_fmt_payload_relates_its_fields() {
    let mut envelope = envelope("fmt", Status::Passed, CommandOutcome::Ok);
    for (changed, written, text) in [
        (true, true, Some("text")),
        (false, true, None),
        (true, false, None),
    ] {
        envelope.payload = fmt(changed, written, text);
        assert!(
            validate_command_result(&value(&envelope)).is_err(),
            "{changed} {written} {text:?}"
        );
    }
    let mut blocked = envelope.clone();
    blocked.result = CommandOutcome::Diagnostics;
    blocked.status = Status::Refuted;
    blocked.payload = fmt(true, true, None);
    assert!(
        validate_command_result(&value(&blocked)).is_err(),
        "diagnostics prevent writing"
    );
    let mut empty = envelope;
    empty.payload = Payload::Empty {};
    assert!(validate_command_result(&value(&empty)).is_err());
    let mut extra = value(&empty);
    extra["payload"] = json!({"path": "a.vib", "changed": false, "written": false,
        "text": "", "extra": 1});
    assert!(validate_command_result(&extra).is_err());
}

#[test]
fn the_check_payload_is_accepted_exactly_when_the_result_is_ok() {
    let mut passed = envelope("check", Status::Passed, CommandOutcome::Ok);
    passed.payload = Payload::Check(CheckPayload {
        accepted: true,
        obligations: vec![obligation(Status::Passed)],
    });
    validate_command_result(&value(&passed)).expect("validates");

    let mut not_accepted = passed.clone();
    not_accepted.payload = Payload::Check(CheckPayload {
        accepted: false,
        obligations: Vec::new(),
    });
    assert!(validate_command_result(&value(&not_accepted)).is_err());

    let mut refuted = envelope("check", Status::Refuted, CommandOutcome::Diagnostics);
    refuted
        .diagnostics
        .push(diagnostic(DiagnosticCode::TypeMismatch));
    refuted.payload = Payload::Check(CheckPayload {
        accepted: false,
        obligations: vec![obligation(Status::Refuted)],
    });
    validate_command_result(&value(&refuted)).expect("validates");

    let mut unsupported =
        envelope("check", Status::Unsupported, CommandOutcome::Unavailable);
    unsupported
        .diagnostics
        .push(diagnostic(DiagnosticCode::ToolUnavailable));
    unsupported.payload = Payload::Check(CheckPayload {
        accepted: false,
        obligations: Vec::new(),
    });
    validate_command_result(&value(&unsupported)).expect("validates");
}

#[test]
fn an_obligation_record_is_closed() {
    let mut passed = envelope("check", Status::Passed, CommandOutcome::Ok);
    passed.payload = Payload::Check(CheckPayload {
        accepted: true,
        obligations: vec![obligation(Status::Unknown)],
    });
    let good = value(&passed);
    validate_command_result(&good).expect("validates");
    assert_eq!(good["payload"]["obligations"][0]["kind"], "requires");
    let mut kind = good.clone();
    kind["payload"]["obligations"][0]["kind"] = json!("@requires");
    assert!(validate_command_result(&kind).is_err());
    let mut missing = good.clone();
    missing["payload"]["obligations"][0]
        .as_object_mut()
        .expect("object")
        .remove("ruleIds");
    assert!(validate_command_result(&missing).is_err());
    let mut span = good;
    span["payload"]["obligations"][0]["source"]["start"] = json!(9);
    assert!(validate_command_result(&span).is_err(), "start after end");
}

#[test]
fn a_refused_fmt_or_check_has_the_empty_payload() {
    for command in ["fmt", "check"] {
        let mut invalid =
            envelope(command, Status::Unknown, CommandOutcome::InvalidInput);
        invalid.workspace_revision = None;
        validate_command_result(&value(&invalid)).expect("validates");
        let mut trap = envelope(command, Status::Unknown, CommandOutcome::Trap);
        trap.workspace_revision = None;
        validate_command_result(&value(&trap)).expect("validates");
    }
}
