//! M3 Step 3 `any`-bounded generics through the single-source checker:
//! call-site inference, `types:` agreement, ambiguity, and the style warning.

#![allow(clippy::expect_used, clippy::indexing_slicing)]

use vibra_diagnostics::DiagnosticCode;
use vibra_types::check_source;

const PRELUDE: &str = "(defn identity (value t) t\n  where: (t any)\n  value)\n\
                       (deftype pair (record left a right b)\n  where: (a any b any)\n  \
                       (defn with-right (value self right c) (pair a c)\n    where: (c any)\n    \
                       (pair left: (value @left) right: right)))\n";

fn codes(body: &str) -> Vec<DiagnosticCode> {
    let source = format!("{PRELUDE}{body}");
    check_source("case.vib", &source)
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code())
        .collect()
}

#[test]
fn a_generic_call_infers_from_operands_and_from_the_written_result() {
    assert_eq!(codes("(defn main () i32 (identity 1i32))"), Vec::new());
    assert_eq!(
        codes("(defn main () (pair i32 str) (pair left: 1i32 right: \"x\"))"),
        Vec::new()
    );
}

#[test]
fn types_supplies_the_complete_list_in_where_order() {
    assert_eq!(
        codes("(defn main () str (identity types: (str) \"x\"))"),
        Vec::new()
    );
    // A method's list is its owner's parameters followed by its own.
    assert_eq!(
        codes(
            "(defn main () (pair i32 bool) \
             (pair.with-right types: (i32 str bool) (pair left: 1i32 right: \"x\") true))"
        ),
        Vec::new()
    );
    assert_eq!(
        codes(
            "(defn main () (pair i32 bool) \
             (pair.with-right types: (i32 bool) (pair left: 1i32 right: \"x\") true))"
        ),
        vec![DiagnosticCode::TypeTypeArgumentMismatch]
    );
}

#[test]
fn a_contradicting_type_argument_is_a_type_argument_mismatch() {
    assert_eq!(
        codes("(defn main () i32 (identity types: (i32) \"x\"))"),
        vec![DiagnosticCode::TypeTypeArgumentMismatch]
    );
    assert_eq!(
        codes("(defn main () str (identity types: (i32) 1i32))"),
        vec![DiagnosticCode::TypeTypeArgumentMismatch]
    );
}

#[test]
fn types_on_a_non_generic_callee_is_a_type_argument_mismatch() {
    assert_eq!(
        codes(
            "(defn plain (value i32) i32 value)\n(defn main () i32 (plain types: (i32) 1i32))"
        ),
        vec![DiagnosticCode::TypeTypeArgumentMismatch]
    );
}

#[test]
fn an_unfixed_generic_argument_is_ambiguous() {
    assert_eq!(
        codes(
            "(defn count (value t) i32\n  where: (t any)\n  0i32)\n\
             (defn main () i32 (count (pair.with-right (pair left: 1i32 right: 2i32) \
             (identity (identity 1i32)))))"
        ),
        Vec::new()
    );
    assert_eq!(
        codes(
            "(deftype maybe (enum some t none void)\n  where: (t any))\n\
             (defn count (value t) i32\n  where: (t any)\n  0i32)\n\
             (defn main () i32 (count (maybe.none)))"
        ),
        vec![DiagnosticCode::TypeAmbiguousInference]
    );
}

#[test]
fn a_generic_function_value_is_instantiated_from_its_expected_type() {
    assert_eq!(
        codes(
            "(defn apply (f (fn (i32) i32) value i32) i32 (f value))\n\
             (defn main () i32 (apply identity 1i32))"
        ),
        Vec::new()
    );
    assert_eq!(
        codes("(defn main () i32 (let f identity (f 1i32)))"),
        vec![DiagnosticCode::TypeAmbiguousInference]
    );
}

#[test]
fn a_late_types_group_warns_only_after_a_complete_binding() {
    assert_eq!(
        codes("(defn main () str (identity \"x\" types: (str)))"),
        vec![DiagnosticCode::StyleArgumentOrder]
    );
    assert_eq!(
        codes("(defn main () i32 (identity \"x\" types: (i32)))"),
        vec![DiagnosticCode::TypeTypeArgumentMismatch]
    );
}

#[test]
fn generic_arguments_are_invariant() {
    assert_eq!(
        codes(
            "(defn same (left (pair i32 t) right (pair i32 t)) i32\n  where: (t any)\n  0i32)\n\
             (defn main () i32 (same (pair left: 1i32 right: 1i32) (pair left: 1i32 right: 1i64)))"
        ),
        vec![DiagnosticCode::TypeArgumentMismatch]
    );
}

#[test]
fn a_generic_body_treats_its_parameters_as_rigid() {
    assert_eq!(
        codes("(defn widen (value t) i32\n  where: (t any)\n  value)"),
        vec![DiagnosticCode::TypeMismatch]
    );
}
