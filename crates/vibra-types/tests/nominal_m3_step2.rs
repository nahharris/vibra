//! M3 Step 2 declared-type checking through the single-source checker.

#![allow(clippy::expect_used, clippy::indexing_slicing)]

use vibra_diagnostics::{ByteSpan, DiagnosticCode};
use vibra_types::check_source;

fn codes(source: &str) -> Vec<(DiagnosticCode, ByteSpan)> {
    check_source("case.vib", source)
        .diagnostics()
        .iter()
        .map(|diagnostic| (diagnostic.code(), diagnostic.primary_span()))
        .collect()
}

#[test]
fn a_type_naming_an_unavailable_type_is_unavailable_at_its_declaration_and_uses() {
    let source = "(deftype number (intrinsic-type @number))\n\
                  (deftype wrapper (record value number))\n\
                  (defn read (value wrapper) i32 0i32)";
    let codes = codes(source);
    // The intrinsic type, then the record that names it, then the parameter
    // naming the record.
    assert_eq!(
        codes,
        vec![
            (DiagnosticCode::ToolUnavailable, ByteSpan::new(0, 41)),
            (DiagnosticCode::ToolUnavailable, ByteSpan::new(42, 81)),
            (DiagnosticCode::ToolUnavailable, ByteSpan::new(94, 107)),
        ]
    );
}

#[test]
fn impl_blocks_target_a_visible_interface() {
    let with_impl = codes(
        "(deftype user (record name str)\n  (impl printable (defn render (value self) str \"u\")))",
    );
    assert!(
        with_impl
            .iter()
            .any(|(code, _)| *code == DiagnosticCode::NameUnknownSymbol)
    );
}

#[test]
fn a_repeated_type_name_is_one_redeclaration() {
    let codes = codes("(deftype point (record x i32))\n(deftype point (record y i32))");
    assert_eq!(
        codes
            .iter()
            .filter(|(code, _)| *code == DiagnosticCode::NameRedeclaration)
            .count(),
        1
    );
}

#[test]
fn a_nested_method_sees_self_and_is_not_a_module_name() {
    let result = check_source(
        "case.vib",
        "(defn main () i32 (counter.read (counter value: 3i32)))\n\
         (deftype counter (record value i32)\n  (defn read (value self) i32 (value @value)))",
    );
    assert!(result.accepted(), "{:?}", result.diagnostics());
    let unknown = check_source(
        "case.vib",
        "(defn main () i32 (read 1i32))\n(deftype c (record v i32) (defn read (value self) i32 0i32))",
    );
    assert!(
        unknown
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code() == DiagnosticCode::NameUnknownSymbol)
    );
}

#[test]
fn an_anonymous_record_matches_regardless_of_written_order() {
    let result = check_source(
        "case.vib",
        "(defn main () (record b i32 a str) (id (recordof a: \"x\" b: 1i32)))\n\
         (defn id (value (record a str b i32)) (record b i32 a str) value)",
    );
    assert!(result.accepted(), "{:?}", result.diagnostics());
}
