//! Validation of a document against its embedded schema.
//!
//! A producer MUST validate every document it emits, "including rules that
//! relate fields" (`docs/spec/05-tooling.md`, "Versioned envelope and
//! results"). The schemas state the relations a schema can state; the one it
//! cannot, a span whose start is not after its end, is checked here.

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
