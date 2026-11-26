//! The `test` payload: `TestItem`, its failure, trap, and observation error,
//! the counts, and the relation of the items to the suite result.

use serde_json::{Value, json};
use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode, LineIndex};
use vibra_schema::{
    CommandOutcome, DiagnosticDocument, Envelope, Limits, ObservationError,
    ObservationErrorCode, Payload, SpanDocument, Status, TestFailure, TestFailureKind,
    TestItem, TestPayload, TestResult, TrapRecord, validate_command_result,
};

const REVISION: &str =
    "sha256:0000000000000000000000000000000000000000000000000000000000000000";

fn diagnostic(code: DiagnosticCode) -> DiagnosticDocument {
    let diagnostic = Diagnostic::new(code, ByteSpan::new(0, 1), "a fault")
        .with_source_id("tests/math.vib");
    DiagnosticDocument::render(&diagnostic, &LineIndex::new("x")).expect("renders")
}

fn item(result: TestResult, status: Status) -> TestItem {
    TestItem {
        name: "@tests.math::\"adds\"".to_owned(),
        result,
        failure: None,
        trap: None,
        audit_trace: Vec::new(),
        diagnostics: Vec::new(),
        status,
    }
}

fn passed() -> TestItem {
    item(TestResult::Passed, Status::Passed)
}

fn failure(kind: TestFailureKind) -> TestFailure {
    TestFailure {
        kind,
        assertion: Some("assert.equal".to_owned()),
        expected: Some("4i32".to_owned()),
        actual: Some("3i32".to_owned()),
        primary_span: SpanDocument::render(
            ByteSpan::new(0, 1),
            &LineIndex::new("x"),
            Some("tests/math.vib"),
        ),
        observation_error: (kind == TestFailureKind::Observation).then_some(
            ObservationError {
                code: ObservationErrorCode::UnsupportedValue,
                value_type: None,
                origin: None,
            },
        ),
    }
}

fn failed() -> TestItem {
    let mut failed = item(TestResult::AssertionFailed, Status::Refuted);
    failed.failure = Some(failure(TestFailureKind::Assertion));
    failed
}

fn trapped() -> TestItem {
    let mut trapped = item(TestResult::Trap, Status::Unknown);
    trapped.trap = Some(TrapRecord {
        trap_code: "@runtime.invalidCheckedProgram".to_owned(),
        origin: None,
    });
    trapped.diagnostics = vec!["@runtime.invalidCheckedProgram".to_owned()];
    trapped
}

fn unavailable() -> TestItem {
    let mut unavailable = item(TestResult::Unavailable, Status::Unsupported);
    unavailable.diagnostics = vec!["@tool.unavailable".to_owned()];
    unavailable
}

fn exhausted() -> TestItem {
    let mut exhausted = item(TestResult::Exhausted, Status::Unknown);
    exhausted.diagnostics = vec!["@runtime.memoryExhausted".to_owned()];
    exhausted
}

fn invalid() -> TestItem {
    let mut invalid = item(TestResult::Invalid, Status::Refuted);
    invalid.diagnostics = vec!["@type.mismatch".to_owned()];
    invalid
}

/// The envelope of a `test` run over `items`, with the counts they give and
/// one diagnostic per distinct attributed code.
fn run(result: CommandOutcome, status: Status, items: Vec<TestItem>) -> Value {
    let mut envelope = Envelope::new(
        "0.0.1",
        "test",
        status,
        result,
        Limits {
            profile: "m1Default".to_owned(),
            reached: Vec::new(),
            complete: true,
        },
    );
    envelope.workspace_revision = Some(REVISION.to_owned());
    let mut codes = items
        .iter()
        .flat_map(|item| item.diagnostics.clone())
        .collect::<Vec<_>>();
    codes.sort();
    codes.dedup();
    for code in codes {
        let code = code.parse::<DiagnosticCode>().expect("a registered code");
        envelope.diagnostics.push(diagnostic(code));
    }
    let selected = items.len() as u64;
    let good = items
        .iter()
        .filter(|item| item.result == TestResult::Passed)
        .count() as u64;
    envelope.payload = Payload::Test(TestPayload {
        selected,
        passed: good,
        failed: selected - good,
        tests: items,
    });
    serde_json::to_value(&envelope).expect("serializes")
}

#[test]
fn every_suite_result_validates_with_the_items_that_give_it() {
    for (result, status, items) in [
        (CommandOutcome::Ok, Status::Passed, vec![passed(), passed()]),
        (CommandOutcome::Ok, Status::Passed, vec![]),
        (
            CommandOutcome::TestFailed,
            Status::Refuted,
            vec![failed(), passed()],
        ),
        (
            CommandOutcome::Unavailable,
            Status::Unsupported,
            vec![unavailable(), failed(), passed()],
        ),
        (
            CommandOutcome::OperationalFailure,
            Status::Unknown,
            vec![exhausted(), unavailable(), failed()],
        ),
        (
            CommandOutcome::Trap,
            Status::Unknown,
            vec![trapped(), exhausted(), unavailable(), failed()],
        ),
        (
            CommandOutcome::Diagnostics,
            Status::Refuted,
            vec![invalid(), invalid()],
        ),
    ] {
        let document = run(result, status, items);
        validate_command_result(&document)
            .unwrap_or_else(|violation| panic!("{result}: {violation}"));
    }
}

#[test]
fn a_suite_result_that_the_items_do_not_give_is_rejected() {
    for (result, status, items) in [
        (CommandOutcome::Ok, Status::Passed, vec![failed()]),
        (CommandOutcome::TestFailed, Status::Refuted, vec![passed()]),
        (
            CommandOutcome::TestFailed,
            Status::Refuted,
            vec![failed(), unavailable()],
        ),
        (
            CommandOutcome::Unavailable,
            Status::Unsupported,
            vec![unavailable(), exhausted()],
        ),
        (
            CommandOutcome::OperationalFailure,
            Status::Unknown,
            vec![exhausted(), trapped()],
        ),
        (CommandOutcome::Trap, Status::Unknown, vec![failed()]),
        (CommandOutcome::Diagnostics, Status::Refuted, vec![passed()]),
    ] {
        let document = run(result, status, items);
        assert!(validate_command_result(&document).is_err(), "{result}");
    }
}

#[test]
fn a_test_item_projects_the_test_record() {
    let document = run(CommandOutcome::TestFailed, Status::Refuted, vec![failed()]);
    let item = &document["payload"]["tests"][0];
    assert_eq!(item["name"], "@tests.math::\"adds\"");
    assert_eq!(item["result"], "@test.assertionFailed");
    assert_eq!(item["trap"], Value::Null);
    assert_eq!(item["auditTrace"], json!([]));
    assert_eq!(item["status"], "refuted");
    assert_eq!(item["failure"]["kind"], "assertion");
    assert_eq!(item["failure"]["expected"], "4i32");
    assert_eq!(item["failure"]["observationError"], Value::Null);
    let trap = run(CommandOutcome::Trap, Status::Unknown, vec![trapped()]);
    assert_eq!(
        trap["payload"]["tests"][0]["trap"],
        json!({"trapCode": "@runtime.invalidCheckedProgram", "origin": null})
    );
    let mut observed = failed();
    observed.failure = Some(failure(TestFailureKind::Observation));
    observed.diagnostics = vec!["@tool.unsupportedValue".to_owned()];
    let observed = run(CommandOutcome::TestFailed, Status::Refuted, vec![observed]);
    validate_command_result(&observed).expect("an observation failure validates");
    assert_eq!(
        observed["payload"]["tests"][0]["failure"]["observationError"],
        json!({"code": "@tool.unsupportedValue", "type": null, "origin": null})
    );
}

/// 05 `TrapRecord` and 06 Origins: an origin is the `SpanDocument` of the
/// trapping operation, or `null`; an ordinal is not an origin.
#[test]
fn a_trap_origin_is_a_span_document_or_null() {
    let mut known = trapped();
    if let Some(trap) = &mut known.trap {
        trap.origin = Some(SpanDocument::render(
            ByteSpan::new(2, 5),
            &LineIndex::new("a\nbcdef"),
            Some("tests/math.vib"),
        ));
    }
    let document = run(CommandOutcome::Trap, Status::Unknown, vec![known]);
    validate_command_result(&document).expect("a known origin validates");
    assert_eq!(
        document["payload"]["tests"][0]["trap"]["origin"],
        json!({
            "sourceId": "tests/math.vib",
            "start": 2,
            "end": 5,
            "startPosition": {"line": 2, "column": 1},
            "endPosition": {"line": 2, "column": 4}
        })
    );
    let absent = run(CommandOutcome::Trap, Status::Unknown, vec![trapped()]);
    validate_command_result(&absent).expect("an absent origin validates");
    let mut ordinal = absent;
    ordinal["payload"]["tests"][0]["trap"]["origin"] = json!(1);
    assert!(validate_command_result(&ordinal).is_err());
}

#[test]
fn a_test_item_relates_its_result_to_its_fields() {
    let good = run(CommandOutcome::TestFailed, Status::Refuted, vec![failed()]);
    validate_command_result(&good).expect("validates");
    let edits: [(&str, Value); 9] = [
        ("/payload/tests/0/failure", Value::Null),
        ("/payload/tests/0/status", json!("passed")),
        (
            "/payload/tests/0/failure/observationError",
            json!({"code": "@tool.unsupportedValue", "type": null, "origin": null}),
        ),
        ("/payload/tests/0/failure/kind", json!("@assertion")),
        ("/payload/tests/0/auditTrace", json!([{"kind": "@host"}])),
        ("/payload/tests/0/name", json!("@app.math::\"adds\"")),
        ("/payload/tests/0/result", json!("@test.skipped")),
        ("/payload/tests/0/result", Value::Null),
        (
            "/payload/tests/0/trap",
            json!({"trapCode": "@runtime.divisionByZero", "origin": null}),
        ),
    ];
    for (pointer, replacement) in edits {
        let mut bad = good.clone();
        *bad.pointer_mut(pointer).expect("the field") = replacement;
        assert!(validate_command_result(&bad).is_err(), "{pointer}");
    }
}

#[test]
fn the_counts_are_the_counts_of_the_items() {
    let good = run(
        CommandOutcome::TestFailed,
        Status::Refuted,
        vec![failed(), passed()],
    );
    validate_command_result(&good).expect("validates");
    for (field, wrong) in [("selected", 3), ("passed", 2), ("failed", 0)] {
        let mut bad = good.clone();
        bad["payload"][field] = json!(wrong);
        assert!(validate_command_result(&bad).is_err(), "{field}");
    }
}

#[test]
fn an_attributed_code_is_a_diagnostic_of_the_envelope() {
    let mut document = run(
        CommandOutcome::Diagnostics,
        Status::Refuted,
        vec![invalid()],
    );
    validate_command_result(&document).expect("validates");
    document["payload"]["tests"][0]["diagnostics"] = json!(["@name.unresolved"]);
    assert!(validate_command_result(&document).is_err());
}

#[test]
fn a_refused_test_and_the_tool_trap_have_the_empty_payload() {
    let mut refused = run(CommandOutcome::InvalidInput, Status::Unknown, vec![]);
    refused["payload"] = json!({});
    refused["workspaceRevision"] = Value::Null;
    validate_command_result(&refused).expect("invalid input has no payload");
    let mut trap = run(CommandOutcome::Trap, Status::Unknown, vec![]);
    trap["payload"] = json!({});
    validate_command_result(&trap).expect("the tool's own trap has no payload");
    let mut empty = run(CommandOutcome::Ok, Status::Passed, vec![]);
    empty["payload"] = json!({});
    assert!(
        validate_command_result(&empty).is_err(),
        "a run carries its payload"
    );
}

#[test]
fn a_test_item_carries_its_host_events() {
    let mut item = passed();
    item.audit_trace = vec![vibra_schema::AuditEvent(json!({
        "kind": "@host",
        "invocation": 2,
        "parent": 1,
        "phase": "@entry",
        "op": "fs.readText",
        "driver": {"provider": "@hostWorld.v2", "version": "0.0.2"},
        "origin": null,
        "grant": {
            "issuedBy": {"kind": "@testWorld"},
            "policy": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "rights": [{"path": "@std.fs.DirReadText", "seed": "sha256:00"}],
            "limits": null,
            "root": 3
        },
        "args": ["[\"note.txt\"]"],
        "result": "{kind: @enum}"
    }))];
    let good = run(CommandOutcome::Ok, Status::Passed, vec![item]);
    validate_command_result(&good).expect("a host event validates");
    let edits: [(&str, Value); 4] = [
        ("/payload/tests/0/auditTrace/0/kind", json!("@startup")),
        ("/payload/tests/0/auditTrace/0/args", json!([["note.txt"]])),
        ("/payload/tests/0/auditTrace/0/grant/rights", json!([])),
        ("/payload/tests/0/auditTrace/0/phase", json!("@startup")),
    ];
    for (pointer, replacement) in edits {
        let mut bad = good.clone();
        *bad.pointer_mut(pointer).expect("the field") = replacement;
        assert!(validate_command_result(&bad).is_err(), "{pointer}");
    }
}
