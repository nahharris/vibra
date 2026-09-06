//! Producer and consumer tests for the published diagnostic contracts.
//!
//! `docs/spec/07-diagnostics-and-conformance.md` makes "JSON producer/consumer
//! tests validate every published interchange schema" a release-gate
//! requirement. Producer tests assert that what the implementation emits
//! validates against the published schema; consumer tests assert that what the
//! schema admits is what the implementation reads back, and that what it
//! forbids is rejected.

// A test asserts by panicking.
#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use jsonschema::Validator;
use serde_json::{Value, json};
use vibra_diagnostics::{
    ByteSpan, Diagnostic, DiagnosticCode, DocumentRevision, Fix, LineIndex,
};
use vibra_schema::{
    DIAGNOSTIC_SCHEMA, DiagnosticDocument, REGISTRY_ENTRY_SCHEMA,
    RegistryEntryDocument, SCHEMA_VERSION,
};

fn validator(schema_text: &str) -> Validator {
    let schema: Value =
        serde_json::from_str(schema_text).expect("the schema is valid JSON");
    jsonschema::validator_for(&schema).expect("the schema is a valid JSON Schema")
}

fn diagnostic_validator() -> Validator {
    validator(DIAGNOSTIC_SCHEMA)
}

fn registry_entry_validator() -> Validator {
    validator(REGISTRY_ENTRY_SCHEMA)
}

fn assert_valid(validator: &Validator, instance: &Value) {
    let errors: Vec<String> = validator
        .iter_errors(instance)
        .map(|e| e.to_string())
        .collect();
    assert!(
        errors.is_empty(),
        "instance does not validate:\n{}\n{}",
        serde_json::to_string_pretty(instance).unwrap_or_default(),
        errors.join("\n")
    );
}

/// A diagnostic exercising every optional part of the contract.
fn rich_diagnostic() -> (String, Diagnostic) {
    // Unicode before the span, so the derived columns differ from the bytes.
    let source = "(defn 🌱 →)\n  (b\n";
    let diagnostic = Diagnostic::new(
        DiagnosticCode::SyntaxUnmatchedDelimiter,
        ByteSpan::new(16, 18),
        "this list is never closed",
    )
    .with_related(ByteSpan::new(0, 1), "the enclosing form starts here")
    .with_note("a list must be closed before the end of the document")
    .with_fix(Fix::safe(
        "insert the closing delimiter",
        DocumentRevision::new("sha256:0f1e"),
    ));

    (source.to_owned(), diagnostic)
}

#[test]
fn a_rendered_diagnostic_validates_against_the_published_schema() {
    let (source, diagnostic) = rich_diagnostic();
    let index = LineIndex::new(&source);
    let document = DiagnosticDocument::render(&diagnostic, &index);

    let instance = serde_json::to_value(&document).expect("the document serializes");
    assert_valid(&diagnostic_validator(), &instance);
}

#[test]
fn a_minimal_diagnostic_validates_against_the_published_schema() {
    // Empty related, notes, and fixes are the common shape and must not be
    // omitted: the schema requires all three keys.
    let index = LineIndex::new("");
    let document = DiagnosticDocument::render(
        &Diagnostic::new(
            DiagnosticCode::SyntaxRetiredForm,
            ByteSpan::empty_at(0),
            "`while` was retired",
        ),
        &index,
    );

    let instance = serde_json::to_value(&document).expect("the document serializes");
    assert_valid(&diagnostic_validator(), &instance);

    assert_eq!(instance["related"], json!([]));
    assert_eq!(instance["notes"], json!([]));
    assert_eq!(instance["fixes"], json!([]));
}

#[test]
fn every_registered_code_renders_a_valid_registry_entry() {
    // The producer side of the whole registry, not just a sample.
    let validator = registry_entry_validator();
    for code in DiagnosticCode::ALL {
        let entry = RegistryEntryDocument::render(*code);
        let instance = serde_json::to_value(&entry).expect("the entry serializes");
        assert_valid(&validator, &instance);
    }
}

#[test]
fn atoms_are_serialized_with_their_exact_spelling() {
    let entry = RegistryEntryDocument::render(DiagnosticCode::TypeArgumentMismatch);
    let instance = serde_json::to_value(&entry).expect("the entry serializes");

    assert_eq!(instance["code"], json!("@type.argument-mismatch"));
    assert_eq!(instance["level"], json!("@error"));
    assert_eq!(instance["domain"], json!("type"));
    assert_eq!(instance["fixCapability"], json!("@none"));
}

#[test]
fn a_span_carries_both_bytes_and_derived_positions() {
    // "🌱" is four bytes and one column, so a consumer cannot compute one
    // representation from the other without the document. Both are sent.
    let source = "🌱ab";
    let index = LineIndex::new(source);
    let document = DiagnosticDocument::render(
        &Diagnostic::new(
            DiagnosticCode::NameUnknownSymbol,
            ByteSpan::new(4, 6),
            "unknown symbol",
        ),
        &index,
    );

    assert_eq!(document.primary_span.start, 4);
    assert_eq!(document.primary_span.end, 6);
    assert_eq!(document.primary_span.start_position.line, 1);
    assert_eq!(document.primary_span.start_position.column, 2);
    assert_eq!(document.primary_span.end_position.column, 4);
}

#[test]
fn a_rendered_diagnostic_round_trips_through_json() {
    let (source, diagnostic) = rich_diagnostic();
    let index = LineIndex::new(&source);
    let document = DiagnosticDocument::render(&diagnostic, &index);

    let text = serde_json::to_string(&document).expect("the document serializes");
    let parsed: DiagnosticDocument =
        serde_json::from_str(&text).expect("it reads back");
    assert_eq!(parsed, document);
}

#[test]
fn a_registry_entry_round_trips_through_json() {
    for code in DiagnosticCode::ALL {
        let entry = RegistryEntryDocument::render(*code);
        let text = serde_json::to_string(&entry).expect("the entry serializes");
        let parsed: RegistryEntryDocument =
            serde_json::from_str(&text).expect("it reads back");
        assert_eq!(parsed, entry);
    }
}

#[test]
fn an_unknown_field_is_rejected_when_reading() {
    // The tooling chapter permits ignoring an unknown field only where an
    // output schema explicitly allows forward extension. Neither of these
    // does, so both the schema and the reader must refuse it.
    let (source, diagnostic) = rich_diagnostic();
    let index = LineIndex::new(&source);
    let mut instance =
        serde_json::to_value(DiagnosticDocument::render(&diagnostic, &index))
            .expect("the document serializes");
    instance["severity"] = json!("fatal");

    assert!(
        diagnostic_validator()
            .iter_errors(&instance)
            .next()
            .is_some(),
        "the schema must reject an unknown field"
    );
    assert!(
        serde_json::from_value::<DiagnosticDocument>(instance).is_err(),
        "the reader must reject an unknown field"
    );
}

#[test]
fn a_missing_required_field_is_rejected_when_reading() {
    let (source, diagnostic) = rich_diagnostic();
    let index = LineIndex::new(&source);
    let mut instance =
        serde_json::to_value(DiagnosticDocument::render(&diagnostic, &index))
            .expect("the document serializes");
    instance
        .as_object_mut()
        .expect("an object")
        .remove("notes")
        .expect("notes was present");

    assert!(
        diagnostic_validator()
            .iter_errors(&instance)
            .next()
            .is_some(),
        "the schema must require notes even when empty"
    );
    assert!(serde_json::from_value::<DiagnosticDocument>(instance).is_err());
}

#[test]
fn an_unsupported_schema_version_is_rejected_by_the_schema() {
    // A reader must reject a newer major version rather than guessing.
    let (source, diagnostic) = rich_diagnostic();
    let index = LineIndex::new(&source);
    let mut instance =
        serde_json::to_value(DiagnosticDocument::render(&diagnostic, &index))
            .expect("the document serializes");
    instance["schemaVersion"] = json!(SCHEMA_VERSION + 1);

    assert!(
        diagnostic_validator()
            .iter_errors(&instance)
            .next()
            .is_some(),
        "the schema pins its own major version"
    );
}

#[test]
fn an_unregistered_level_is_rejected_by_the_schema() {
    let (source, diagnostic) = rich_diagnostic();
    let index = LineIndex::new(&source);
    let mut instance =
        serde_json::to_value(DiagnosticDocument::render(&diagnostic, &index))
            .expect("the document serializes");
    instance["level"] = json!("@info");

    assert!(
        diagnostic_validator()
            .iter_errors(&instance)
            .next()
            .is_some(),
        "the level vocabulary is closed at @error and @warning"
    );
}

#[test]
fn a_malformed_code_spelling_is_rejected_by_the_schema() {
    let validator = registry_entry_validator();
    for spelling in [
        "type.argument-mismatch",  // missing the leading @
        "@Type.argument-mismatch", // not lowercase
        "@type",                   // no thing component
        "@type.argument.mismatch", // three components
        "@type.",                  // empty thing
        "@type.argument_mismatch", // underscore is not kebab-case
    ] {
        let mut instance = serde_json::to_value(RegistryEntryDocument::render(
            DiagnosticCode::TypeNotApplicable,
        ))
        .expect("the entry serializes");
        instance["code"] = json!(spelling);

        assert!(
            validator.iter_errors(&instance).next().is_some(),
            "{spelling} must not validate as a diagnostic code"
        );
    }
}

#[test]
fn both_published_schemas_declare_a_stable_identifier() {
    for (text, expected) in [
        (DIAGNOSTIC_SCHEMA, "urn:vibra:schema:v1:diagnostic"),
        (
            REGISTRY_ENTRY_SCHEMA,
            "urn:vibra:schema:v1:diagnostic-registry-entry",
        ),
    ] {
        let schema: Value =
            serde_json::from_str(text).expect("the schema is valid JSON");
        assert_eq!(
            schema["$id"],
            json!(expected),
            "schema identifiers are contracts and must not drift"
        );
    }
}
