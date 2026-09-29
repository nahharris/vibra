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

#[test]
fn a_let_bound_generic_lambda_is_instantiated_at_each_call() {
    assert_eq!(
        codes(
            "(defn main () str\n  (let pick (lambda (a t b t) t\n    where: (t any)\n    a)\n    \
             (do (pick 1i32 2i32) (pick \"x\" \"y\"))))"
        ),
        Vec::new()
    );
    assert_eq!(
        codes(
            "(defn main () i32\n  (let keep (lambda (a t) t\n    where: (t any)\n    a)\n    \
             (keep types: (i32) \"x\")))"
        ),
        vec![DiagnosticCode::TypeTypeArgumentMismatch]
    );
}

#[test]
fn a_generic_lambda_value_needs_an_expected_type() {
    assert_eq!(
        codes("(defn main () i32 ((lambda (a t) t\n  where: (t any)\n  a) 3i32))"),
        Vec::new()
    );
    assert_eq!(
        codes("(defn main () void (do (lambda (a t) t\n  where: (t any)\n  a) void))"),
        vec![DiagnosticCode::TypeAmbiguousInference]
    );
}

#[test]
fn a_lambda_sees_and_must_not_redeclare_enclosing_generic_names() {
    assert_eq!(
        codes(
            "(defn outer (v t) t\n  where: (t any)\n  \
             (let f (lambda (a u) t\n    where: (u any)\n    v)\n    (f 1i32)))"
        ),
        Vec::new()
    );
    assert_eq!(
        codes(
            "(defn outer (v t) t\n  where: (t any)\n  \
             (let f (lambda (a t) t\n    where: (t any)\n    a)\n    (f v)))"
        ),
        vec![DiagnosticCode::NameGenericRedeclaration]
    );
}

fn messages(body: &str) -> Vec<(DiagnosticCode, String)> {
    let source = format!("{PRELUDE}{body}");
    check_source("case.vib", &source)
        .diagnostics()
        .iter()
        .map(|diagnostic| (diagnostic.code(), diagnostic.message().to_owned()))
        .collect()
}

#[test]
fn a_parameter_no_operand_mentions_is_ambiguous_unless_supplied() {
    let unused = "(defn second (x b) b\n  where: (a any b any)\n  x)\n";
    assert_eq!(
        codes(&format!("{unused}(defn main () i32 (second 1i32))")),
        vec![DiagnosticCode::TypeAmbiguousInference]
    );
    assert_eq!(
        codes(&format!(
            "{unused}(defn main () i32 (second types: (str i32) 1i32))"
        )),
        Vec::new()
    );
    assert_eq!(
        codes(
            "(deftype cell (record v t)\n  where: (t any)\n  (defn make (x i32) i32 x))\n\
             (defn main () i32 (cell.make 1i32))"
        ),
        vec![DiagnosticCode::TypeAmbiguousInference]
    );
}

#[test]
fn a_generic_lambda_takes_its_complete_where_list() {
    let body = |call: &str| {
        format!(
            "(defn main () i32\n  (let g (lambda (x b) b\n    where: (a any b any)\n    x)\n    {call}))"
        )
    };
    assert_eq!(codes(&body("(g types: (str i32) 1i32)")), Vec::new());
    assert_eq!(
        codes(&body("(g types: (i32) 1i32)")),
        vec![DiagnosticCode::TypeTypeArgumentMismatch]
    );
    assert_eq!(
        codes(&body("(g 1i32)")),
        vec![DiagnosticCode::TypeAmbiguousInference]
    );
}

#[test]
fn a_generic_value_contradicting_its_expected_type_is_a_mismatch() {
    assert_eq!(
        codes("(defn take (x i32) i32 x)\n(defn main () i32 (take identity))"),
        vec![DiagnosticCode::TypeArgumentMismatch]
    );
}

#[test]
fn diagnostics_spell_generic_parameters_as_written() {
    for (_, message) in messages(
        "(deftype boxed t\n  where: (t any))\n\
         (defn open (value (boxed t)) i32\n  where: (t any)\n  0i32)\n\
         (defn main () i32 (open 1i32))",
    ) {
        assert!(
            !message.contains('?') && !message.contains('#'),
            "{message}"
        );
    }
}

#[test]
fn only_a_types_fixed_operand_contradicts_types() {
    assert_eq!(
        codes(
            "(defn run (f (fn (t) i32) v t) i32\n  where: (t any)\n  0i32)\n\
             (defn main () i32 (run types: (i32) (lambda (v i32) str \"s\") 1i32))"
        ),
        vec![DiagnosticCode::TypeArgumentMismatch]
    );
    assert_eq!(
        codes(
            "(deftype boxed t\n  where: (t any))\n\
             (defn main () (boxed i32) (identity types: ((boxed i32)) (boxed \"x\")))"
        ),
        vec![DiagnosticCode::TypeTypeArgumentMismatch]
    );
}

#[test]
fn constructors_warn_about_a_late_types_group_only_when_they_check() {
    let cell = "(deftype cell (record value t)\n  where: (t any))\n";
    assert_eq!(
        codes(&format!(
            "{cell}(defn main () (cell i32) (cell value: 1i32 types: (i32)))"
        )),
        vec![DiagnosticCode::StyleArgumentOrder]
    );
    assert_eq!(
        codes(&format!(
            "{cell}(defn main () (cell i32) (cell value: \"x\" types: (i32)))"
        )),
        vec![DiagnosticCode::TypeTypeArgumentMismatch]
    );
    assert_eq!(
        codes(
            "(deftype boxed t\n  where: (t any))\n\
             (defn main () (boxed i32) (boxed 1i32 types: (i32)))"
        ),
        vec![DiagnosticCode::StyleArgumentOrder]
    );
}

#[test]
fn any_self_and_a_types_slot_are_reserved() {
    assert_eq!(
        codes("(defn f (x i32) i32\n  where: (any any)\n  x)"),
        vec![DiagnosticCode::NameReservedDeclaration]
    );
    assert_eq!(
        codes("(defn f (x i32) i32\n  where: (self any)\n  x)"),
        vec![DiagnosticCode::NameReservedDeclaration]
    );
    assert_eq!(
        codes("(defn f (g (fn (i32) i32 labelled: (types i32))) i32 0i32)"),
        vec![DiagnosticCode::NameReservedLabel]
    );
}

#[test]
fn an_instantiated_void_payload_is_nullary() {
    let maybe = "(deftype maybe (enum some t none void)\n  where: (t any))\n\
                 (defn count (value t) i32\n  where: (t any)\n  0i32)\n";
    assert_eq!(
        codes(&format!("{maybe}(defn main () (maybe void) (maybe.some))")),
        Vec::new()
    );
    // Zero operands fix the payload's argument to `void`.
    assert_eq!(
        codes(&format!("{maybe}(defn main () i32 (count (maybe.some)))")),
        Vec::new()
    );
    assert_eq!(
        codes(&format!(
            "{maybe}(defn main () (maybe void) (maybe.some void))"
        )),
        vec![DiagnosticCode::TypeArgumentMismatch]
    );
    assert_eq!(
        codes(&format!("{maybe}(defn main () (maybe i32) (maybe.some))")),
        vec![DiagnosticCode::TypeArgumentMismatch]
    );
}

#[test]
fn a_function_type_mismatch_spells_both_signatures() {
    let found = messages(
        "(defn take (f (fn (i32) i32)) i32 0i32)\n\
         (defn other (value str) i32 0i32)\n\
         (defn main () i32 (take other))",
    );
    assert_eq!(found.len(), 1);
    assert!(found[0].1.contains("(fn (i32) i32)"), "{}", found[0].1);
    assert!(found[0].1.contains("(fn (str) i32)"), "{}", found[0].1);
}

#[test]
fn ambiguity_notes_each_unfixed_parameter() {
    let source = format!(
        "{PRELUDE}(defn pair-of () (pair a b)\n  where: (a any b any)\n  (pair-of))\n\
         (defn main () i32 (let p (pair-of) 0i32))"
    );
    let checked = check_source("case.vib", &source);
    let ambiguous = checked
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.code() == DiagnosticCode::TypeAmbiguousInference)
        .expect("ambiguity");
    assert_eq!(ambiguous.notes().len(), 2);
}
