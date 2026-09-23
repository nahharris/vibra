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
fn test_result_schema_closes_items_and_rejects_m2_trace_events() {
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
                    "assertion": "@std.assert.equal-i32",
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
