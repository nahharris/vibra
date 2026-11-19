//! Schema-typed decoding and canonical bytes
//! (`docs/spec/04-programs-and-packages.md`, "Schemas" and "Decoded values and
//! canonical bytes").

use vibra_diagnostics::{ByteSpan, DiagnosticCode};
use vibra_syntax::{
    Decoded, FieldSchema, IntegerSuffix, RecordSchema, Schema, Slot, TypedValue,
    VariantSchema, decode,
};

fn no_spaces(text: &str) -> Option<&'static str> {
    text.contains(' ').then_some("the name has no space")
}

/// A schema with every kind of slot the step uses: a constrained string, two
/// integer widths, a field with a default, two dicts with typed keys, a closed
/// atom set, a variant, and an opaque slot.
fn sample() -> Schema {
    let variant = VariantSchema::new(
        "kind",
        vec![
            (
                "@path",
                RecordSchema::new(vec![
                    FieldSchema::required("kind", Slot::AtomOf(vec!["@path"])),
                    FieldSchema::required("path", Slot::Str(None)),
                ]),
            ),
            (
                "@git",
                RecordSchema::new(vec![
                    FieldSchema::required("kind", Slot::AtomOf(vec!["@git"])),
                    FieldSchema::required("rev", Slot::Str(None)),
                ]),
            ),
        ],
    );
    Schema::new(
        "@sample.v1",
        vec![
            FieldSchema::required("name", Slot::Str(Some(no_spaces))),
            FieldSchema::required("count", Slot::Integer(IntegerSuffix::U32)),
            FieldSchema::with_default(
                "level",
                Slot::Integer(IntegerSuffix::U8),
                || TypedValue::Integer(3, IntegerSuffix::U8),
            ),
            FieldSchema::optional("tags", Slot::dict(Slot::Str(None), Slot::Bool)),
            FieldSchema::optional(
                "ranks",
                Slot::dict(Slot::Integer(IntegerSuffix::U64), Slot::Bool),
            ),
            FieldSchema::optional("mode", Slot::AtomOf(vec!["@fast", "@slow"])),
            FieldSchema::optional(
                "items",
                Slot::non_empty_list(Slot::Variant(variant)),
            ),
            FieldSchema::optional("extra", Slot::Opaque),
        ],
    )
}

fn read(text: &str) -> Decoded {
    decode(&sample(), Some("sample.vibon"), text.as_bytes())
}

/// The codes and spans of a reading, in order.
fn faults(decoded: &Decoded) -> Vec<(DiagnosticCode, ByteSpan)> {
    decoded
        .diagnostics()
        .iter()
        .map(|diagnostic| (diagnostic.code(), diagnostic.primary_span()))
        .collect()
}

/// The span of the `occurrence`-th (from 0) instance of `needle` in `text`.
fn at(text: &str, needle: &str, occurrence: usize) -> ByteSpan {
    let start = text
        .match_indices(needle)
        .nth(occurrence)
        .map(|(start, _)| start)
        .expect("the needle occurs");
    ByteSpan::new(start, start + needle.len())
}

fn canonical(text: &str) -> String {
    let decoded = read(text);
    assert!(decoded.diagnostics().is_empty(), "{:?}", faults(&decoded));
    decoded
        .document()
        .expect("a valid document")
        .canonical_bytes()
}

#[test]
fn canonical_bytes_are_one_line_without_comments_defaults_or_unsuffixed_numbers() {
    let text = "{\n  // a comment\n  format: @sample.v1\n  count: 7\n  name: \"x\"\n  level: 3u8\n}\n";
    let bytes = canonical(text);
    assert_eq!(bytes, "{format: @sample.v1 name: \"x\" count: 7u32}");
    assert!(!bytes.contains('\n') && !bytes.contains('\r'));
}

#[test]
fn canonical_bytes_are_a_function_of_the_decoded_value() {
    let left = canonical(
        "{format: @sample.v1 name: \"a\" count: 1 tags: [\"b\": true \"a\": false]}",
    );
    let right = canonical(
        "{tags: [\"\\u{61}\": false \"b\": true] count: 1u32 name: \"\\u{61}\" format: @sample.v1 level: 3}",
    );
    assert_eq!(left, right);
    assert_eq!(
        left,
        "{format: @sample.v1 name: \"a\" count: 1u32 tags: [\"a\": false \"b\": true]}"
    );
}

#[test]
fn dict_entries_ascend_by_the_key_types_order() {
    let bytes = canonical(
        "{format: @sample.v1 name: \"a\" count: 1 ranks: [10: true 2u64: false] tags: [\"b\": true \"B\": true \"10\": true]}",
    );
    assert_eq!(
        bytes,
        "{format: @sample.v1 name: \"a\" count: 1u32 tags: [\"10\": true \"B\": true \"b\": true] ranks: [2u64: false 10u64: true]}"
    );
}

#[test]
fn an_absent_defaulted_field_holds_its_default() {
    let decoded = read("{format: @sample.v1 name: \"a\" count: 1}");
    let document = decoded.document().expect("valid");
    let level = document.root().field("level").expect("the default is held");
    assert_eq!(
        level.value().value(),
        &TypedValue::Integer(3, IntegerSuffix::U8)
    );
    assert!(level.is_default());
    assert_eq!(level.name_span(), None);
    let written = read("{format: @sample.v1 name: \"a\" count: 1 level: 4}");
    let written = written.document().expect("valid");
    assert!(!written.root().field("level").expect("written").is_default());
    assert_eq!(
        written.canonical_bytes(),
        "{format: @sample.v1 name: \"a\" count: 1u32 level: 4u8}"
    );
}

#[test]
fn a_grammar_fault_ends_the_reading_before_a_repeated_field() {
    let text = "{format: @sample.v1 name: \"a\" name: \"b\" count: 1 extra: [1u8}";
    let decoded = read(text);
    let found = faults(&decoded);
    assert!(!found.is_empty());
    assert!(
        found
            .iter()
            .all(|(code, _)| code.as_atom().starts_with("@syntax.")),
        "{found:?}"
    );
}

#[test]
fn a_repeated_field_ends_the_reading_before_the_format_and_the_shape() {
    let text = "{format: @other.v1 name: \"a\" name: \"b\" count: 1 author: \"x\"}";
    let decoded = read(text);
    assert_eq!(
        faults(&decoded),
        [(DiagnosticCode::DataDuplicateField, at(text, "name", 1))]
    );
    assert!(decoded.document().is_none());
}

#[test]
fn an_unknown_format_ends_the_reading_before_the_shape() {
    let text = "{format: @sample.v2 count: \"no\" author: \"x\"}";
    assert_eq!(
        faults(&read(text)),
        [(DiagnosticCode::DataInvalidValue, at(text, "@sample.v2", 0))]
    );
    let text = "{name: \"a\" count: 1}";
    assert_eq!(
        faults(&read(text)),
        [(
            DiagnosticCode::DataInvalidShape,
            ByteSpan::new(0, text.len())
        )]
    );
    let text = "[1u8]";
    assert_eq!(
        faults(&read(text)),
        [(
            DiagnosticCode::DataInvalidShape,
            ByteSpan::new(0, text.len())
        )]
    );
}

#[test]
fn shape_faults_end_the_reading_before_value_faults() {
    // `mode` is a value fault, `author` and the missing `count` shape faults.
    let text = "{format: @sample.v1 name: \"a b\" author: \"x\" mode: @medium}";
    assert_eq!(
        faults(&read(text)),
        [
            (
                DiagnosticCode::DataInvalidShape,
                ByteSpan::new(0, text.len())
            ),
            (DiagnosticCode::DataInvalidShape, at(text, "author", 0)),
        ]
    );
}

#[test]
fn value_faults_end_the_reading_before_repeated_keys() {
    let text =
        "{format: @sample.v1 name: \"a b\" count: 1 tags: [\"a\": true \"a\": false]}";
    assert_eq!(
        faults(&read(text)),
        [(DiagnosticCode::DataInvalidValue, at(text, "\"a b\"", 0))]
    );
}

#[test]
fn repeated_keys_are_compared_as_typed_values() {
    let text = "{format: @sample.v1 name: \"a\" count: 1 tags: [\"a\": true \"\\u{61}\": false]}";
    assert_eq!(
        faults(&read(text)),
        [(DiagnosticCode::DataDuplicateKey, at(text, "\"\\u{61}\"", 0))]
    );
    // A grammar-only reader keeps `1` and `1u64` apart; the schema types both
    // as `U64`.
    let text = "{format: @sample.v1 name: \"a\" count: 1 ranks: [1: true 1u64: false]}";
    assert_eq!(
        faults(&read(text)),
        [(DiagnosticCode::DataDuplicateKey, at(text, "1u64", 0))]
    );
}

#[test]
fn numbers_are_typed_by_their_slot() {
    // An unsuffixed number in a slot with no number type is a value fault.
    let text = "{format: @sample.v1 name: 1 count: 1}";
    assert_eq!(
        faults(&read(text)),
        [(DiagnosticCode::DataInvalidValue, at(text, "1", 1))]
    );
    // A suffixed one is a value of the wrong form.
    let text = "{format: @sample.v1 name: 1u8 count: 1}";
    assert_eq!(
        faults(&read(text)),
        [(DiagnosticCode::DataInvalidShape, at(text, "1u8", 0))]
    );
    // The wrong width and a value out of range are value faults.
    let text = "{format: @sample.v1 name: \"a\" count: 1u8 level: 300}";
    assert_eq!(
        faults(&read(text)),
        [
            (DiagnosticCode::DataInvalidValue, at(text, "1u8", 0)),
            (DiagnosticCode::DataInvalidValue, at(text, "300", 0)),
        ]
    );
    let text = "{format: @sample.v1 name: \"a\" count: -1}";
    assert_eq!(
        faults(&read(text)),
        [(DiagnosticCode::DataInvalidValue, at(text, "-1", 0))]
    );
}

#[test]
fn a_record_slot_rejects_other_forms_and_a_list_its_minimum() {
    let text = "{format: @sample.v1 name: \"a\" count: 1 items: [{}]}";
    assert_eq!(
        faults(&read(text)),
        [(DiagnosticCode::DataInvalidShape, at(text, "{}", 0))]
    );
    let text = "{format: @sample.v1 name: \"a\" count: 1 items: []}";
    assert_eq!(
        faults(&read(text)),
        [(DiagnosticCode::DataInvalidValue, at(text, "[]", 0))]
    );
    let text = "{format: @sample.v1 name: \"a\" count: 1 tags: []}";
    assert_eq!(
        faults(&read(text)),
        [(DiagnosticCode::DataInvalidShape, at(text, "[]", 0))]
    );
}

#[test]
fn a_variant_is_selected_by_its_tag() {
    let text = "{format: @sample.v1 name: \"a\" count: 1 items: [{kind: @git rev: \"r\"} {path: \"p\" kind: @path}]}";
    assert_eq!(
        canonical(text),
        "{format: @sample.v1 name: \"a\" count: 1u32 items: [{kind: @git rev: \"r\"} {kind: @path path: \"p\"}]}"
    );
    // A field of the other case is unknown.
    let text = "{format: @sample.v1 name: \"a\" count: 1 items: [{kind: @git rev: \"r\" path: \"p\"}]}";
    assert_eq!(
        faults(&read(text)),
        [(DiagnosticCode::DataInvalidShape, at(text, "path", 0))]
    );
    // An unknown tag selects no record, so nothing else is checked.
    let text =
        "{format: @sample.v1 name: \"a\" count: 1 items: [{kind: @svn url: 1u8}]}";
    assert_eq!(
        faults(&read(text)),
        [(DiagnosticCode::DataInvalidValue, at(text, "@svn", 0))]
    );
}

#[test]
fn opaque_data_has_its_own_canonical_rules() {
    let bytes = canonical(
        "{format: @sample.v1 name: \"a\" count: 1 extra: {zeta: 007 alpha: [\"b\": 1.50 \"a\": 2f64] mid: {1 \"\\u{62}\"}}}",
    );
    assert_eq!(
        bytes,
        "{format: @sample.v1 name: \"a\" count: 1u32 extra: {alpha: [\"a\": 2.0f64 \"b\": 1.5] mid: {1 \"b\"} zeta: 7}}"
    );
    let text = "{format: @sample.v1 name: \"a\" count: 1 extra: [1: true 01: false]}";
    assert_eq!(
        faults(&read(text)),
        [(DiagnosticCode::DataDuplicateKey, at(text, "01", 0))]
    );
}

#[test]
fn opaque_dict_keys_nested_in_key_position_are_encoded_without_recursion() {
    const DEPTH: usize = 10_000;
    // `K` starts as `0u8` and becomes `[K: 0u8]` at each level, so every
    // level is a dict in the key position of the next.
    let key = format!("{}0u8{}", "[".repeat(DEPTH), ": 0u8]".repeat(DEPTH));
    let text = format!("{{format: @sample.v1 name: \"a\" count: 1 extra: {key}}}");
    let expected =
        format!("{{format: @sample.v1 name: \"a\" count: 1u32 extra: {key}}}");
    let bytes = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(move || canonical(&text))
        .expect("a thread")
        .join()
        .expect("no stack overflow");
    assert_eq!(bytes, expected);
}

#[test]
fn an_opaque_number_out_of_its_written_types_range_is_a_value_fault() {
    for number in ["1e999f64", "-1e999f64", "1e39f32", "300u8", "-1u8", "128i8"] {
        let text =
            format!("{{format: @sample.v1 name: \"a\" count: 1 extra: {number}}}");
        assert_eq!(
            faults(&read(&text)),
            [(DiagnosticCode::DataInvalidValue, at(&text, number, 0))],
            "{text}"
        );
        // In key position too, and the value fault comes before any repeated
        // key.
        let text = format!(
            "{{format: @sample.v1 name: \"a\" count: 1 extra: [{number}: 0u8 {number}: 1u8]}}"
        );
        assert_eq!(
            faults(&read(&text)),
            [
                (DiagnosticCode::DataInvalidValue, at(&text, number, 0)),
                (DiagnosticCode::DataInvalidValue, at(&text, number, 1)),
            ],
            "{text}"
        );
    }
    // The limits themselves are in range.
    let bytes = canonical(
        "{format: @sample.v1 name: \"a\" count: 1 extra: {255u8 -128i8 1.7976931348623157e308f64}}",
    );
    assert_eq!(
        bytes,
        "{format: @sample.v1 name: \"a\" count: 1u32 extra: {255u8 -128i8 1.7976931348623157e308f64}}"
    );
}

#[test]
fn an_unsuffixed_opaque_float_keeps_its_exact_exponent() {
    // Exponents beyond 64 bits are normalized exactly: two different keys
    // stay two keys, and no exponent becomes zero.
    let bytes = canonical(
        "{format: @sample.v1 name: \"a\" count: 1 extra: [1e9223372036854775809: 1u8 1e9223372036854775808: 0u8]}",
    );
    assert_eq!(
        bytes,
        "{format: @sample.v1 name: \"a\" count: 1u32 extra: [1e9223372036854775808: 0u8 1e9223372036854775809: 1u8]}"
    );
    let huge = "9".repeat(60);
    let bytes = canonical(&format!(
        "{{format: @sample.v1 name: \"a\" count: 1 extra: {{1.50e{huge} 120e-{huge} 0.0e{huge}}}}}"
    ));
    let mut plus_one = "1".to_owned();
    plus_one.push_str(&"0".repeat(60));
    let minus_two = format!("{}7", "9".repeat(59));
    assert_eq!(
        bytes,
        format!(
            "{{format: @sample.v1 name: \"a\" count: 1u32 extra: {{1.5e{huge} 1.2e-{minus_two} 0.0}}}}"
        )
    );
    // Equal exact values are one key.
    let text = format!(
        "{{format: @sample.v1 name: \"a\" count: 1 extra: [1e{plus_one}: 0u8 10e{huge}: 1u8]}}"
    );
    assert_eq!(
        faults(&read(&text)),
        [(
            DiagnosticCode::DataDuplicateKey,
            at(&text, &format!("10e{huge}"), 0)
        )]
    );
}

#[test]
fn a_format_of_the_wrong_form_is_a_shape_fault() {
    for value in ["[]", "\"@sample.v1\"", "{}", "true", "1u8"] {
        let text = format!("{{format: {value} name: \"a\" count: 1}}");
        assert_eq!(
            faults(&read(&text)),
            [(DiagnosticCode::DataInvalidShape, at(&text, value, 0))],
            "{text}"
        );
    }
    // A well-formed atom of another version, and an unsuffixed number in the
    // atom slot, are value faults.
    for value in ["@other", "@sample.v2", "1"] {
        let text = format!("{{format: {value} name: \"a\" count: 1}}");
        assert_eq!(
            faults(&read(&text)),
            [(DiagnosticCode::DataInvalidValue, at(&text, value, 0))],
            "{text}"
        );
    }
}

#[test]
fn diagnostics_carry_the_source_identity() {
    let decoded = read("{format: @sample.v1 name: \"a\"}");
    assert_eq!(
        decoded
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.source_id())
            .collect::<Vec<_>>(),
        [Some("sample.vibon")]
    );
}
