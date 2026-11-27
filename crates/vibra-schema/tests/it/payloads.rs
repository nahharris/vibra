//! The `fmt`, `check`, and `run` payloads and their relations to the result.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode, LineIndex};
use vibra_schema::{
    AuditEvent, BindingRecordDocument, BindingStatus, CheckPayload, CommandOutcome,
    DiagnosticDocument, DriverRecord, Envelope, FmtPayload, Limits, ObligationKind,
    ObligationRecord, Payload, RunPayload, SpanDocument, StartupFailureDocument,
    StartupRecordDocument, StartupStatus, Status, TrapRecord, validate_command_result,
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

    let mut unread = envelope("fmt", Status::Refuted, CommandOutcome::Diagnostics);
    unread
        .diagnostics
        .push(diagnostic(DiagnosticCode::ModuleIoError));
    unread.payload = fmt(false, false, None);
    validate_command_result(&value(&unread)).expect("a file never read has no text");
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

/// A hash that is a key of the `policies` of [`run_payload`].
const POLICY: &str =
    "sha256:1111111111111111111111111111111111111111111111111111111111111111";

fn driver() -> DriverRecord {
    DriverRecord {
        provider: "@host.io.stdout".to_owned(),
        version: "0.0.2".to_owned(),
    }
}

fn projection(policy: &str) -> Value {
    json!({
        "issuedBy": {"kind": "@binding", "leaf": "@out"},
        "policy": policy,
        "rights": [{"path": "@std.io.StdoutText", "seed": REVISION}],
        "limits": null,
        "root": 1
    })
}

fn host_event(phase: &str, parent: Value, policy: &str) -> AuditEvent {
    AuditEvent(json!({
        "kind": "@host", "invocation": 2, "parent": parent, "phase": phase,
        "op": "io.stdoutText",
        "driver": {"provider": "@host.io.stdout", "version": "0.0.2"},
        "origin": null, "grant": projection(policy),
        "args": ["\"hello\\n\""], "result": "{kind: @enum}"
    }))
}

fn startup_event() -> AuditEvent {
    AuditEvent(json!({
        "kind": "@startup", "binding": "@out", "provider": "@host.io.stdout",
        "status": "@acquired", "grant": projection(""), "failure": null
    }))
}

/// The payload of the hello run: one acquired and released leaf, one write.
fn run_payload() -> RunPayload {
    RunPayload {
        target: "@hello".to_owned(),
        program_result: Some("ExitCode(0u8)".to_owned()),
        application_code: Some(0),
        stdout: "hello\n".to_owned(),
        stderr: String::new(),
        audit_trace: vec![host_event("@entry", json!(1), "")],
        drivers: vec![driver()],
        policies: BTreeMap::from([(
            POLICY.to_owned(),
            "{format: @scopePolicy.v1}".to_owned(),
        )]),
        startup: StartupRecordDocument {
            status: StartupStatus::Acquired,
            bindings: vec![BindingRecordDocument {
                name: "@out".to_owned(),
                recipe_identity: REVISION.to_owned(),
                provider_identity: driver(),
                status: BindingStatus::Released,
                grant_projection: Some(projection("")),
            }],
            audit_trace: vec![startup_event()],
            failure: None,
        },
        trap: None,
    }
}

fn run(result: CommandOutcome, payload: RunPayload) -> Value {
    let status = match result {
        CommandOutcome::Ok => Status::Passed,
        CommandOutcome::Diagnostics => Status::Refuted,
        CommandOutcome::Unavailable => Status::Unsupported,
        _ => Status::Unknown,
    };
    let mut envelope = envelope("run", status, result);
    if matches!(
        result,
        CommandOutcome::Diagnostics | CommandOutcome::Unavailable
    ) {
        let code = if result == CommandOutcome::Unavailable {
            DiagnosticCode::ToolUnavailable
        } else {
            DiagnosticCode::TypeMismatch
        };
        envelope.diagnostics.push(diagnostic(code));
    }
    envelope.payload = Payload::Run(Box::new(payload));
    value(&envelope)
}

#[test]
fn the_run_payload_of_each_result_validates() {
    validate_command_result(&run(CommandOutcome::Ok, run_payload())).expect("ok");

    let mut failed = run_payload();
    failed.application_code = Some(3);
    failed.program_result = Some("ExitCode(3u8)".to_owned());
    validate_command_result(&run(CommandOutcome::ProgramFailed, failed))
        .expect("a non-zero code");

    let not_started = StartupRecordDocument {
        status: StartupStatus::NotStarted,
        bindings: Vec::new(),
        audit_trace: Vec::new(),
        failure: None,
    };
    let mut refuted = run_payload();
    refuted.application_code = None;
    refuted.program_result = None;
    refuted.audit_trace = Vec::new();
    refuted.drivers = Vec::new();
    refuted.policies = BTreeMap::new();
    refuted.stdout = String::new();
    refuted.startup = not_started.clone();
    validate_command_result(&run(CommandOutcome::Diagnostics, refuted.clone()))
        .expect("static diagnostics");
    validate_command_result(&run(CommandOutcome::Unavailable, refuted))
        .expect("no driver at admission");

    let mut refused = run_payload();
    refused.application_code = None;
    refused.program_result = None;
    refused.audit_trace = Vec::new();
    refused.startup.status = StartupStatus::Failed;
    refused.startup.bindings.push(BindingRecordDocument {
        name: "@notes".to_owned(),
        recipe_identity: REVISION.to_owned(),
        provider_identity: driver(),
        status: BindingStatus::Refused,
        grant_projection: None,
    });
    refused.startup.audit_trace.push(AuditEvent(json!({
        "kind": "@startup", "binding": "@notes", "provider": "@host.fs.directory",
        "status": "@refused", "grant": null,
        "failure": {"code": "@grant.requiredUnavailable", "message": "absent"}
    })));
    refused.startup.failure = Some(StartupFailureDocument {
        binding: "@notes".to_owned(),
        code: "@grant.requiredUnavailable".to_owned(),
        origin: SpanDocument::render(
            ByteSpan::new(0, 1),
            &LineIndex::new("x"),
            Some("project.vibon"),
        ),
    });
    validate_command_result(&run(CommandOutcome::OperationalFailure, refused))
        .expect("a refused required leaf");

    let mut trapped = run_payload();
    trapped.application_code = None;
    trapped.program_result = None;
    trapped.trap = Some(TrapRecord {
        trap_code: "@runtime.invalidHostValue".to_owned(),
        origin: None,
    });
    validate_command_result(&run(CommandOutcome::Trap, trapped)).expect("a trap");
}

#[test]
fn the_run_result_relates_to_the_payload() {
    let mut nonzero = run_payload();
    nonzero.application_code = Some(3);
    assert!(
        validate_command_result(&run(CommandOutcome::Ok, nonzero)).is_err(),
        "ok is the code 0"
    );
    assert!(
        validate_command_result(&run(CommandOutcome::ProgramFailed, run_payload()))
            .is_err(),
        "programFailed is a non-zero code"
    );
    assert!(
        validate_command_result(&run(CommandOutcome::Trap, run_payload())).is_err(),
        "a trap has its record"
    );
    assert!(
        validate_command_result(&run(CommandOutcome::Diagnostics, run_payload()))
            .is_err(),
        "static diagnostics start nothing"
    );
    let mut unknown_code = run_payload();
    unknown_code.program_result = None;
    assert!(
        validate_command_result(&run(CommandOutcome::Ok, unknown_code)).is_err(),
        "a code and a program result are known together"
    );
    let mut wide = value(&{
        let mut envelope = envelope("run", Status::Passed, CommandOutcome::Ok);
        envelope.payload = Payload::Run(Box::new(run_payload()));
        envelope
    });
    wide["payload"]["applicationCode"] = json!(256);
    assert!(
        validate_command_result(&wide).is_err(),
        "a code is 0 to 255"
    );
    let mut empty = envelope("run", Status::Passed, CommandOutcome::Ok);
    empty.payload = Payload::Empty {};
    assert!(
        validate_command_result(&value(&empty)).is_err(),
        "a run that selected its target carries its payload"
    );
}

#[test]
fn the_events_of_a_run_are_split_by_phase() {
    let mut startup_in_entry = run_payload();
    startup_in_entry
        .audit_trace
        .push(host_event("@startup", Value::Null, ""));
    assert!(
        validate_command_result(&run(CommandOutcome::Ok, startup_in_entry)).is_err()
    );
    let mut startup_event_in_entry = run_payload();
    startup_event_in_entry.audit_trace.push(startup_event());
    assert!(
        validate_command_result(&run(CommandOutcome::Ok, startup_event_in_entry))
            .is_err()
    );
    let mut entry_in_startup = run_payload();
    entry_in_startup
        .startup
        .audit_trace
        .push(host_event("@entry", json!(1), ""));
    assert!(
        validate_command_result(&run(CommandOutcome::Ok, entry_in_startup)).is_err()
    );
    let mut read = run_payload();
    read.startup
        .audit_trace
        .push(host_event("@startup", Value::Null, ""));
    validate_command_result(&run(CommandOutcome::Ok, read)).expect("a startup read");
}

#[test]
fn every_policy_of_a_projection_is_a_key_of_policies() {
    let mut known = run_payload();
    known.audit_trace = vec![host_event("@entry", json!(1), POLICY)];
    validate_command_result(&run(CommandOutcome::Ok, known)).expect("a known policy");
    let other =
        "sha256:2222222222222222222222222222222222222222222222222222222222222222";
    let mut event = run_payload();
    event.audit_trace = vec![host_event("@entry", json!(1), other)];
    let mut binding = run_payload();
    binding.startup.bindings[0].grant_projection = Some(projection(other));
    let mut started = run_payload();
    started.startup.audit_trace = vec![AuditEvent(json!({
        "kind": "@startup", "binding": "@out", "provider": "@host.io.stdout",
        "status": "@acquired", "grant": projection(other), "failure": null
    }))];
    for payload in [event, binding, started] {
        let violation = validate_command_result(&run(CommandOutcome::Ok, payload))
            .expect_err("an unknown policy");
        assert!(
            violation
                .messages()
                .iter()
                .any(|message| message.contains("is not a key of /payload/policies")),
            "{violation}"
        );
    }
}

#[test]
fn a_released_binding_has_its_projection_and_a_refused_one_none() {
    let mut missing = run_payload();
    missing.startup.bindings[0].grant_projection = None;
    assert!(validate_command_result(&run(CommandOutcome::Ok, missing)).is_err());
    let mut acquired = value(&{
        let mut envelope = envelope("run", Status::Passed, CommandOutcome::Ok);
        envelope.payload = Payload::Run(Box::new(run_payload()));
        envelope
    });
    acquired["payload"]["startup"]["bindings"][0]["status"] = json!("acquired");
    assert!(
        validate_command_result(&acquired).is_err(),
        "every acquired binding of a finished command is released"
    );
}

#[test]
fn a_refused_or_trapped_run_may_have_the_empty_payload() {
    let mut invalid = envelope("run", Status::Unknown, CommandOutcome::InvalidInput);
    invalid.workspace_revision = None;
    validate_command_result(&value(&invalid)).expect("invalid input");
    let mut trap = envelope("run", Status::Unknown, CommandOutcome::Trap);
    trap.workspace_revision = None;
    validate_command_result(&value(&trap)).expect("the tool's own trap");
    let mut diagnostics = envelope("run", Status::Refuted, CommandOutcome::Diagnostics);
    diagnostics
        .diagnostics
        .push(diagnostic(DiagnosticCode::ProjectNotFound));
    validate_command_result(&value(&diagnostics)).expect("a project not captured");
}
