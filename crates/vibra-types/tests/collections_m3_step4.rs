//! M3 Step 4 collections through the single-source checker: tuples, lookups,
//! map keys, variadic tails, and the builtin static methods.

#![allow(clippy::expect_used)]

use vibra_diagnostics::DiagnosticCode;
use vibra_types::check_source;

fn codes(source: &str) -> Vec<DiagnosticCode> {
    check_source("case.vib", source)
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code())
        .collect()
}

#[test]
fn a_tuple_index_must_be_a_canonical_literal_within_the_arity() {
    assert_eq!(
        codes("(defn main () str ((tupleof 1i32 \"x\") 1))"),
        Vec::new()
    );
    for index in ["2", "01", "1u64"] {
        assert_eq!(
            codes(&format!(
                "(defn main () str ((tupleof 1i32 \"x\") {index}))"
            )),
            if index == "1u64" {
                vec![DiagnosticCode::TypeArgumentMismatch]
            } else {
                vec![DiagnosticCode::TypeInvalidTupleIndex]
            },
            "{index}"
        );
    }
}

#[test]
fn lookups_answer_with_the_standard_option() {
    let source = "(import option @std.option)\n\
                  (defn items (a (array i32)) (option.option i32) (a 0u64))\n\
                  (defn table (m (map str i32)) (option.option i32) (m \"k\"))\n\
                  (defn scalar (s str) (option.option char) (s 0u64))\n\
                  (defn byte (b bytes) (option.option u8) (b 0u64))";
    assert_eq!(codes(source), Vec::new());
    assert_eq!(
        codes(
            "(import option @std.option)\n(defn f (m (map str i32)) (option.option i32) (m 1i32))"
        ),
        vec![DiagnosticCode::TypeArgumentMismatch]
    );
}

#[test]
fn map_keys_follow_the_closed_conformance() {
    assert_eq!(
        codes("(defn f (m (map (tuple str (enum on void off void)) i32)) i32 0i32)"),
        Vec::new()
    );
    assert_eq!(
        codes("(defn f (m (map f32 i32)) i32 0i32)"),
        vec![DiagnosticCode::TypeInvalidMapKey]
    );
    assert_eq!(
        codes("(defn f (m (map (tuple str (fn () i32)) i32)) i32 0i32)"),
        vec![DiagnosticCode::TypeFunctionNotEquatable]
    );
    assert_eq!(
        codes("(defn f (m (map t i32)) i32\n  where: (t any)\n  0i32)"),
        vec![DiagnosticCode::ToolUnavailable]
    );
}

#[test]
fn builtin_methods_infer_from_their_tails_and_need_an_expected_empty_type() {
    assert_eq!(
        codes("(defn main () (array i32) (array.of 1i32 2i32))"),
        Vec::new()
    );
    assert_eq!(codes("(defn main () (array i32) (array.of))"), Vec::new());
    assert_eq!(
        codes("(defn main () i32 (let items (array.of) 0i32))"),
        vec![DiagnosticCode::TypeAmbiguousInference]
    );
    assert_eq!(
        codes("(defn main () (map str i32) (map.of \"a\"))"),
        vec![DiagnosticCode::TypeArgumentMismatch]
    );
    assert_eq!(
        codes("(defn main () (array i32) (array.reverse (array.of 1i32)))"),
        vec![DiagnosticCode::NameUnknownSymbol]
    );
}

#[test]
fn a_variadic_function_value_keeps_its_tail_in_its_type() {
    assert_eq!(
        codes(
            "(defn main () (array i32) (apply array.of))\n\
             (defn apply (f (fn () (array i32) variadic: (array i32))) (array i32) (f 1i32 2i32))"
        ),
        Vec::new()
    );
    assert_eq!(
        codes(
            "(defn main () (array i32) (apply array.of))\n\
             (defn apply (f (fn ((array i32)) (array i32))) (array i32) (f (array.of 1i32)))"
        ),
        vec![DiagnosticCode::TypeArgumentMismatch]
    );
}
