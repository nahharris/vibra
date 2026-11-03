//! Helpers shared by the `vibra-schema` integration tests: compiling a checked-in
//! schema, asserting an instance against it, and a temporary workspace root.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use jsonschema::Validator;
use serde_json::Value;

/// A counter, not the clock, makes a name unique within the process.
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

/// The validator for the schema document `schema_text`.
pub(crate) fn validator(schema_text: &str) -> Validator {
    let schema: Value =
        serde_json::from_str(schema_text).expect("the schema is valid JSON");
    jsonschema::validator_for(&schema).expect("the schema is a valid JSON Schema")
}

/// Asserts that `instance` validates, listing every error when it does not.
pub(crate) fn assert_valid(validator: &Validator, instance: &Value) {
    let errors: Vec<String> = validator
        .iter_errors(instance)
        .map(|error| error.to_string())
        .collect();
    assert!(
        errors.is_empty(),
        "instance does not validate:\n{}\n{}",
        serde_json::to_string_pretty(instance).unwrap_or_default(),
        errors.join("\n")
    );
}

/// A fresh directory under the canonical system temporary directory.
pub(crate) fn temporary_directory(label: &str) -> PathBuf {
    let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let parent = fs::canonicalize(std::env::temp_dir()).expect("canonical temp parent");
    let root = parent.join(format!(
        "vibra-schema-{label}-{serial}-{}",
        std::process::id()
    ));
    fs::create_dir_all(&root).expect("temporary directory");
    root
}
