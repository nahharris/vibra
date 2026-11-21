//! Validation of a document against its embedded schema.
//!
//! A producer MUST validate every document it emits, "including rules that
//! relate fields" (`docs/spec/05-tooling.md`, "Versioned envelope and
//! results"). The schemas state the relations a schema can state; the ones
//! it cannot are checked here:
//!
//! - a span's start is not after its end;
//! - a `test` payload's `selected` is its number of items, `passed` the
//!   number of `@test.passed` items, and `failed` their difference
//!   ("Tests and selection");
//! - every code attributed to a test item is the code of a diagnostic of the
//!   envelope, where it appears once.

use std::fmt;
use std::sync::OnceLock;

use jsonschema::{Resource, Validator};
use serde_json::Value;

use crate::{COMMAND_RESULT_SCHEMA, DIAGNOSTIC_SCHEMA, DIAGNOSTIC_SCHEMA_ID};

/// A document that does not validate against its schema.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaViolation {
    document: &'static str,
    messages: Vec<String>,
}

impl SchemaViolation {
    /// A violation of the schema of `document`, with its reasons.
    #[must_use]
    pub fn new(document: &'static str, messages: Vec<String>) -> Self {
        Self { document, messages }
    }

    /// The kind of document that failed.
    #[must_use]
    pub const fn document(&self) -> &'static str {
        self.document
    }

    /// Every reason the document failed.
    #[must_use]
    pub fn messages(&self) -> &[String] {
        &self.messages
    }
}

impl fmt::Display for SchemaViolation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "the {} document violates its schema: {}",
            self.document,
            self.messages.join("; ")
        )
    }
}

impl std::error::Error for SchemaViolation {}

fn schema_value(text: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|error| error.to_string())
}

fn diagnostic_validator() -> Result<Validator, String> {
    let schema = schema_value(DIAGNOSTIC_SCHEMA)?;
    jsonschema::validator_for(&schema).map_err(|error| error.to_string())
}

fn command_result_validator() -> Result<Validator, String> {
    let schema = schema_value(COMMAND_RESULT_SCHEMA)?;
    let diagnostic = schema_value(DIAGNOSTIC_SCHEMA)?;
    jsonschema::options()
        .with_resource(DIAGNOSTIC_SCHEMA_ID, Resource::from_contents(diagnostic))
        .build(&schema)
        .map_err(|error| error.to_string())
}

/// Every span in `value` whose start is after its end.
fn misordered_spans(value: &Value, path: &str, found: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            if let (Some(start), Some(end)) = (
                map.get("start").and_then(Value::as_u64),
                map.get("end").and_then(Value::as_u64),
            ) && map.contains_key("startPosition")
                && start > end
            {
                found.push(format!(
                    "{path}: the span starts at {start}, after its end {end}"
                ));
            }
            for (key, child) in map {
                misordered_spans(child, &format!("{path}/{key}"), found);
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                misordered_spans(child, &format!("{path}/{index}"), found);
            }
        }
        _ => {}
    }
}

/// The count and attribution relations of a `test` payload.
fn test_relations(envelope: &Value, found: &mut Vec<String>) {
    if envelope.get("command").and_then(Value::as_str) != Some("test") {
        return;
    }
    let payload = envelope.get("payload");
    let Some(tests) = payload
        .and_then(|payload| payload.get("tests"))
        .and_then(Value::as_array)
    else {
        return;
    };
    let count = |field: &str| {
        payload
            .and_then(|payload| payload.get(field))
            .and_then(Value::as_u64)
    };
    let selected = u64::try_from(tests.len()).ok();
    let passed = u64::try_from(
        tests
            .iter()
            .filter(|item| {
                item.get("result").and_then(Value::as_str) == Some("@test.passed")
            })
            .count(),
    )
    .ok();
    let failed = selected.zip(passed).map(|(all, good)| all - good);
    for (field, expected) in [
        ("selected", selected),
        ("passed", passed),
        ("failed", failed),
    ] {
        if count(field) != expected {
            found.push(format!(
                "/payload/{field}: {:?} is not the count of the items, {expected:?}",
                count(field)
            ));
        }
    }
    let codes = envelope
        .get("diagnostics")
        .and_then(Value::as_array)
        .map(|diagnostics| {
            diagnostics
                .iter()
                .filter_map(|diagnostic| diagnostic.get("code").and_then(Value::as_str))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for (index, item) in tests.iter().enumerate() {
        let attributed = item
            .get("diagnostics")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        for code in attributed.iter().filter_map(Value::as_str) {
            if !codes.contains(&code) {
                found.push(format!(
                    "/payload/tests/{index}/diagnostics: `{code}` is not a diagnostic of the envelope"
                ));
            }
        }
    }
}

fn check(
    document: &'static str,
    validator: &Result<Validator, String>,
    value: &Value,
) -> Result<(), SchemaViolation> {
    let validator = validator
        .as_ref()
        .map_err(|message| SchemaViolation::new(document, vec![message.clone()]))?;
    let mut messages = validator
        .iter_errors(value)
        .map(|error| format!("{}: {error}", error.instance_path))
        .collect::<Vec<_>>();
    misordered_spans(value, "", &mut messages);
    test_relations(value, &mut messages);
    if messages.is_empty() {
        Ok(())
    } else {
        Err(SchemaViolation::new(document, messages))
    }
}

/// Validates `value` against `urn:vibra:schema:v2:command-result`, and the
/// diagnostics it carries against `urn:vibra:schema:v2:diagnostic`.
///
/// # Errors
///
/// Fails with every reason the document does not validate.
pub fn validate_command_result(value: &Value) -> Result<(), SchemaViolation> {
    static VALIDATOR: OnceLock<Result<Validator, String>> = OnceLock::new();
    check(
        "command-result",
        VALIDATOR.get_or_init(command_result_validator),
        value,
    )
}

/// Validates `value` against `urn:vibra:schema:v2:diagnostic`.
///
/// # Errors
///
/// Fails with every reason the document does not validate.
pub fn validate_diagnostic(value: &Value) -> Result<(), SchemaViolation> {
    static VALIDATOR: OnceLock<Result<Validator, String>> = OnceLock::new();
    check(
        "diagnostic",
        VALIDATOR.get_or_init(diagnostic_validator),
        value,
    )
}
