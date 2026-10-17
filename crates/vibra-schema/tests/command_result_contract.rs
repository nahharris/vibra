//! Consumer tests for versioned CLI command-result envelopes.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use jsonschema::Validator;
use serde_json::{Value, json};
use vibra_schema::COMMAND_RESULT_SCHEMA;

fn validator() -> Validator {
    let schema: Value =
        serde_json::from_str(COMMAND_RESULT_SCHEMA).expect("schema JSON");
    jsonschema::validator_for(&schema).expect("schema is valid JSON Schema")
}

#[test]
fn command_result_schema_closes_init_and_fmt_payloads() {
    let validator = validator();
    let init = json!({
        "schemaVersion": 1,
        "command": "init",
        "result": "@command.ok",
        "diagnostics": [],
        "payload": { "workspace": "/work/hello", "created": ["project.vibon", "src"] }
    });
    let fmt = json!({
        "schemaVersion": 1,
        "command": "fmt",
        "result": "@command.ok",
        "diagnostics": [],
        "payload": { "path": "src/main.vib", "changed": true, "written": false, "text": "(defn main () void (do))\n" }
    });
    assert!(validator.is_valid(&init));
    assert!(validator.is_valid(&fmt));

    let mut invalid = fmt;
    invalid["payload"]
        .as_object_mut()
        .expect("payload object")
        .remove("text");
    assert!(!validator.is_valid(&invalid));
}

#[test]
fn test_result_schema_closes_items_and_rejects_event_strings() {
    let validator = validator();
    let envelope = json!({
        "schemaVersion": 1,
        "command": "test",
        "result": "@command.test-failed",
        "diagnostics": [],
        "payload": {
            "selected": 1,
            "passed": 0,
            "failed": 1,
            "tests": [{
                "name": "@tests.math::\"bad\"",
                "result": "@test.assertion-failed",
                "failure": {
                    "assertion": "@std.assert.equal",
                    "expected": "4i32",
                    "actual": "5i32",
                    "primarySpan": {
                        "sourceId": "tests/math.vib",
                        "start": 42,
                        "end": 70,
                        "startPosition": { "line": 2, "column": 20 },
                        "endPosition": { "line": 2, "column": 48 }
                    }
                },
                "trap": null,
                "auditTrace": [],
                "diagnostics": []
            }]
        }
    });
    assert!(validator.is_valid(&envelope));

    let mut traced = envelope.clone();
    traced["payload"]["tests"][0]["auditTrace"] = json!(["ambient.clock"]);
    assert!(!validator.is_valid(&traced));

    let mut wrong_outcome = envelope;
    wrong_outcome["payload"]["tests"][0]["result"] = json!("@test.passed");
    assert!(!validator.is_valid(&wrong_outcome));
}

#[test]
fn help_payload_carries_only_the_usage_text() {
    let validator = validator();
    let help = json!({
        "schemaVersion": 1,
        "command": "help",
        "result": "@command.ok",
        "diagnostics": [],
        "payload": { "usage": "vibra [--format human|json] ..." }
    });
    assert!(validator.is_valid(&help));
    let mut extra = help.clone();
    extra["payload"]["commands"] = json!([]);
    assert!(!validator.is_valid(&extra));
    let mut empty = help;
    empty["payload"] = json!({});
    assert!(!validator.is_valid(&empty));
}

fn run_envelope(
    result: &str,
    program_result: Value,
    trap: Value,
    trace: Value,
) -> Value {
    json!({
        "schemaVersion": 1,
        "command": "run",
        "result": result,
        "diagnostics": [],
        "payload": {
            "target": "src/app",
            "programResult": program_result,
            "stdout": "",
            "stderr": "",
            "auditTrace": trace,
            "trap": trap
        }
    })
}

#[test]
fn an_entry_that_returned_err_is_a_program_failure_with_its_result_and_no_trap() {
    let validator = validator();
    let encoded = "(record type: (record type: @std.result.result arguments: (array @void @app.main.failure)) value: (record kind: @enum type: @std.result.result variant: @err payload: (record kind: @enum type: @app.main.failure variant: @missing)))";
    let failed = run_envelope(
        "@command.program-failed",
        json!(encoded),
        Value::Null,
        json!([]),
    );
    assert!(validator.is_valid(&failed));

    // The result is the program's own value: it needs the encoding, not a
    // null, and it is neither a trap nor a diagnostic.
    let mut without_result = failed.clone();
    without_result["payload"]["programResult"] = Value::Null;
    assert!(!validator.is_valid(&without_result));
    let mut with_trap = failed.clone();
    with_trap["payload"]["trap"] =
        json!({ "trapCode": "@runtime.invalid-checked-program", "origin": null });
    assert!(!validator.is_valid(&with_trap));
    let mut with_diagnostic = failed.clone();
    with_diagnostic["diagnostics"] = json!([{
        "schemaVersion": 1,
        "code": "@runtime.invalid-checked-program",
        "level": "@error",
        "message": "m",
        "primarySpan": {
            "sourceId": null, "start": 0, "end": 0,
            "startPosition": { "line": 1, "column": 1 },
            "endPosition": { "line": 1, "column": 1 }
        },
        "related": [], "notes": [], "fixes": []
    }]);
    assert!(!validator.is_valid(&with_diagnostic));

    // Only `run` ends in a program failure.
    let mut wrong_command = failed;
    wrong_command["command"] = json!("check");
    wrong_command["payload"] = json!({ "accepted": true });
    assert!(!validator.is_valid(&wrong_command));
}

#[test]
fn the_audit_trace_is_an_array_of_event_records_and_keeps_an_unknown_kind() {
    let validator = validator();
    let host = json!({
        "kind": "@host",
        "op": "fs.read-file",
        "args": ["(record kind: @wrapper type: @std.fs.path value: \"/in.txt\")"],
        "result": "(record kind: @enum type: @std.result.result variant: @ok payload: (record kind: @bytes values: (array 104u8 105u8)))"
    });
    let ok = run_envelope(
        "@command.ok",
        json!("x"),
        Value::Null,
        json!([host.clone()]),
    );
    assert!(validator.is_valid(&ok));

    // The strings of the earlier format are gone.
    let legacy = run_envelope(
        "@command.ok",
        json!("x"),
        Value::Null,
        json!(["fs.read-file"]),
    );
    assert!(!validator.is_valid(&legacy));

    // A host event is closed and complete.
    let mut missing = host.clone();
    missing.as_object_mut().expect("object").remove("result");
    let missing =
        run_envelope("@command.ok", json!("x"), Value::Null, json!([missing]));
    assert!(!validator.is_valid(&missing));
    let mut extra = host;
    extra["note"] = json!("x");
    let extra = run_envelope("@command.ok", json!("x"), Value::Null, json!([extra]));
    assert!(!validator.is_valid(&extra));

    // An event of a kind the schema does not know is preserved, its fields as
    // text; a minor change adds a kind and no consumer fails on it.
    let future = json!({ "kind": "@deliver", "channel": "(record id: 1u64)" });
    let future = run_envelope("@command.ok", json!("x"), Value::Null, json!([future]));
    assert!(validator.is_valid(&future));
    let untyped = json!({ "kind": "@deliver", "channel": 1 });
    let untyped =
        run_envelope("@command.ok", json!("x"), Value::Null, json!([untyped]));
    assert!(!validator.is_valid(&untyped));

    // A test item carries its events too.
    let test = json!({
        "schemaVersion": 1,
        "command": "test",
        "result": "@command.ok",
        "diagnostics": [],
        "payload": {
            "selected": 1, "passed": 1, "failed": 0,
            "tests": [{
                "name": "@tests.io::\"writes\"",
                "result": "@test.passed",
                "failure": null,
                "trap": null,
                "auditTrace": [{
                    "kind": "@host", "op": "io.stdout-text",
                    "args": ["\"hi\\n\""],
                    "result": "(record kind: @enum type: @std.result.result variant: @ok)"
                }],
                "diagnostics": []
            }]
        }
    });
    assert!(validator.is_valid(&test));
}

#[test]
fn an_invalid_host_value_is_a_trap_code_of_run_and_test_with_no_origin() {
    let validator = validator();
    let trap = json!({ "trapCode": "@runtime.invalid-host-value", "origin": null });
    let run = run_envelope("@command.trap", Value::Null, trap.clone(), json!([]));
    assert!(validator.is_valid(&run));
    let unknown = run_envelope(
        "@command.trap",
        Value::Null,
        json!({ "trapCode": "@runtime.invalid-host", "origin": null }),
        json!([]),
    );
    assert!(!validator.is_valid(&unknown));

    let test = json!({
        "schemaVersion": 1,
        "command": "test",
        "result": "@command.trap",
        "diagnostics": [],
        "payload": {
            "selected": 1, "passed": 0, "failed": 1,
            "tests": [{
                "name": "@tests.io::\"bad\"",
                "result": "@test.trap",
                "failure": null,
                "trap": trap,
                "auditTrace": [],
                "diagnostics": []
            }]
        }
    });
    assert!(validator.is_valid(&test));
}
