//! Producer-validates tests of the diagnostic document.

use serde_json::{Value, json};
use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode, LineIndex};
use vibra_schema::{DIAGNOSTIC_SCHEMA, DiagnosticDocument, validate_diagnostic};

fn render(diagnostic: &Diagnostic, source: &str) -> Value {
    let document = DiagnosticDocument::render(diagnostic, &LineIndex::new(source))
        .expect("the diagnostic renders");
    serde_json::to_value(document).expect("the document serializes")
}

#[test]
fn a_rendered_diagnostic_validates_and_names_the_exact_atoms() {
    let source = "let x = \u{e9}\nlet y\n";
    let diagnostic = Diagnostic::new(
        DiagnosticCode::SyntaxInvalidForm,
        ByteSpan::new(11, 16),
        "invalid form",
    )
    .with_source_id("src/a.vib")
    .with_related(ByteSpan::new(0, 3), "the opener")
    .with_note("a note");
    let value = render(&diagnostic, source);
    validate_diagnostic(&value).expect("the document validates");
    assert_eq!(value["schemaVersion"], 2);
    assert_eq!(value["code"], "@syntax.invalidForm");
    assert_eq!(value["level"], "@error");
    assert_eq!(value["primarySpan"]["sourceId"], "src/a.vib");
    assert_eq!(value["primarySpan"]["start"], 11);
    assert_eq!(
        value["primarySpan"]["startPosition"],
        json!({"line": 2, "column": 1})
    );
    assert_eq!(value["related"][0]["span"]["sourceId"], "src/a.vib");
    assert_eq!(value["notes"], json!(["a note"]));
    assert_eq!(value["fixes"], json!([]));
}

#[test]
fn a_warning_is_rendered_with_its_registered_level() {
    let diagnostic =
        Diagnostic::new(DiagnosticCode::StyleCasing, ByteSpan::new(0, 1), "casing");
    let value = render(&diagnostic, "x");
    validate_diagnostic(&value).expect("the document validates");
    assert_eq!(value["level"], "@warning");
}

#[test]
fn a_failure_with_no_location_has_a_null_source_and_position_one_one() {
    let diagnostic = Diagnostic::new(
        DiagnosticCode::ToolUnavailable,
        ByteSpan::empty_at(0),
        "unavailable",
    );
    let value = render(&diagnostic, "");
    validate_diagnostic(&value).expect("the document validates");
    assert_eq!(
        value["primarySpan"],
        json!({
            "sourceId": null, "start": 0, "end": 0,
            "startPosition": {"line": 1, "column": 1},
            "endPosition": {"line": 1, "column": 1}
        })
    );
}

#[test]
fn a_related_span_in_another_document_needs_its_index() {
    let diagnostic = Diagnostic::new(
        DiagnosticCode::NameRedeclaration,
        ByteSpan::new(0, 1),
        "redeclared",
    )
    .with_source_id("a.vib")
    .with_related_source("b.vib", ByteSpan::new(0, 1), "first");
    let rendered = DiagnosticDocument::render(&diagnostic, &LineIndex::new("x"));
    assert!(rendered.is_err());
}

#[test]
fn the_schema_is_closed_and_rejects_a_retired_spelling() {
    let schema: Value = serde_json::from_str(DIAGNOSTIC_SCHEMA).expect("valid JSON");
    assert_eq!(schema["$id"], "urn:vibra:schema:v2:diagnostic");
    let good = render(
        &Diagnostic::new(DiagnosticCode::SyntaxInvalidForm, ByteSpan::new(0, 1), "m"),
        "x",
    );
    validate_diagnostic(&good).expect("the document validates");

    let mut extra = good.clone();
    extra["extra"] = json!(1);
    assert!(validate_diagnostic(&extra).is_err());

    let mut kebab = good.clone();
    kebab["code"] = json!("@syntax.invalid-form");
    assert!(validate_diagnostic(&kebab).is_err());

    let mut version = good.clone();
    version["schemaVersion"] = json!(1);
    assert!(validate_diagnostic(&version).is_err());

    let mut level = good.clone();
    level["level"] = json!("@note");
    assert!(validate_diagnostic(&level).is_err());

    let mut missing = good;
    missing.as_object_mut().expect("object").remove("fixes");
    assert!(validate_diagnostic(&missing).is_err());
}

#[test]
fn a_span_that_starts_after_its_end_is_a_violation() {
    let mut value = render(
        &Diagnostic::new(DiagnosticCode::SyntaxInvalidForm, ByteSpan::new(0, 2), "m"),
        "xy",
    );
    value["primarySpan"]["start"] = json!(2);
    value["primarySpan"]["end"] = json!(1);
    let violation = validate_diagnostic(&value).expect_err("start after end");
    assert!(
        violation
            .messages()
            .iter()
            .any(|message| message.contains("after its end")),
        "{violation}"
    );
}
