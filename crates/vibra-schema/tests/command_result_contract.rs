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
