//! Focused host and conformance checks for M2 Step 7 function values.

#![allow(clippy::expect_used, clippy::indexing_slicing, missing_docs)]

use vibra_diagnostics::DiagnosticCode;
use vibra_types::check_source;

#[test]
fn probe_named_function_value_and_labelled_call() {
    let source = r#"
(defn answer () i32
  (let f choose
    (f 3i32 preferred: 11i32)))
(defn choose (fallback i32) i32
  labelled: (preferred i32 7i32)
  preferred)
"#;
    let checked = check_source("functions.vib", source);
    assert!(checked.accepted(), "{:?}", checked.diagnostics());
    assert_eq!(checked.application_bindings().len(), 1);
    let program = checked.program().expect("program");
    let result = vibra_interp::run(program).expect("execution");
    assert_eq!(result.value(), &vibra_ir::Value::I32(11));
}

#[test]
fn nested_callee_expression_is_checked_and_evaluated_once() {
    let source = r#"
(defn answer () i32
  ((let f choose f) 3i32))
(defn choose (value i32) i32 value)
"#;
    let checked = check_source("nested-callee.vib", source);
    assert!(checked.accepted(), "{:?}", checked.diagnostics());
    let result =
        vibra_interp::run(checked.program().expect("program")).expect("execution");
    assert_eq!(result.value(), &vibra_ir::Value::I32(3));
}

#[test]
fn written_function_types_can_store_and_call_a_module_function_value() {
    let source = r#"
(def selected (fn (i32) i32 labelled: (required i32)) choose)
(defn answer () i32
  (selected 3i32 required: 8i32))
(defn choose (value i32) i32
  labelled: (required i32 7i32)
  required)
"#;
    let checked = check_source("written-function-type.vib", source);
    assert!(checked.accepted(), "{:?}", checked.diagnostics());
    let result =
        vibra_interp::run(checked.program().expect("program")).expect("execution");
    assert_eq!(result.value(), &vibra_ir::Value::I32(8));
}

#[test]
fn probe_default_and_declaration_order_for_labelled_operands() {
    let source = r#"
(defn answer () i32
  (choose 3i32 second: 11i32 first: 9i32))
(defn choose (fallback i32) i32
  labelled: (first i32 7i32 second i32 8i32)
  first)
"#;
    let checked = check_source("labelled.vib", source);
    assert!(checked.accepted(), "{:?}", checked.diagnostics());
    assert!(
        checked.diagnostics().iter().any(|diagnostic| {
            diagnostic.code() == DiagnosticCode::StyleArgumentOrder
        })
    );
    let binding = checked
        .application_bindings()
        .first()
        .expect("application binding");
    assert_eq!(binding.facts().positional_count(), 1);
    assert_eq!(binding.facts().labelled(), ["first", "second"]);
    let result =
        vibra_interp::run(checked.program().expect("program")).expect("execution");
    assert_eq!(result.value(), &vibra_ir::Value::I32(9));
}

#[test]
fn probe_omitted_label_uses_typed_default_literal() {
    let source = r#"
(defn answer () i32 (choose 3i32))
(defn choose (fallback i32) i32
  labelled: (preferred i32 7i32)
  preferred)
"#;
    let checked = check_source("default.vib", source);
    assert!(checked.accepted(), "{:?}", checked.diagnostics());
    let result =
        vibra_interp::run(checked.program().expect("program")).expect("execution");
    assert_eq!(result.value(), &vibra_ir::Value::I32(7));
}

#[test]
fn formatter_uses_only_checker_binding_facts_for_safe_reordering() {
    let source = r#"
(defn answer () i32
  (choose 3i32 second: 11i32 first: 9i32))
(defn choose (fallback i32) i32
  labelled: (first i32 7i32 second i32 8i32)
  first)
"#;
    let checked = check_source("format-bindings.vib", source);
    assert!(checked.accepted(), "{:?}", checked.diagnostics());
    let bound = vibra_fmt::format_source_with_bindings(
        "format-bindings.vib",
        source,
        checked.application_bindings(),
    )
    .expect("formatted source");
    let formatted = bound.text();
    let first = formatted.find("first: 9i32").expect("first label");
    let second = formatted.find("second: 11i32").expect("second label");
    assert!(first < second, "{formatted}");
    let reparsed = check_source("format-bindings.vib", formatted);
    assert!(reparsed.accepted(), "{:?}", reparsed.diagnostics());
    let result = vibra_interp::run(reparsed.program().expect("reparsed program"))
        .expect("reparsed execution");
    assert_eq!(result.value(), &vibra_ir::Value::I32(9));
    let reformatted = vibra_fmt::format_source_with_bindings(
        "format-bindings.vib",
        formatted,
        reparsed.application_bindings(),
    )
    .expect("reformatted source")
    .text()
    .to_owned();
    assert_eq!(reformatted, formatted);
}

#[test]
fn probe_lambda_capture_and_return() {
    let source = r#"
(defn answer () i32
  ((make)))
(defn make () (fn () i32)
  (let value 41i32
    (lambda () i32 value)))
"#;
    let checked = check_source("closures.vib", source);
    assert!(checked.accepted(), "{:?}", checked.diagnostics());
    let result =
        vibra_interp::run(checked.program().expect("program")).expect("execution");
    assert_eq!(result.value(), &vibra_ir::Value::I32(41));
}

#[test]
fn probe_nested_let_capture_survives_the_outer_scope() {
    let source = r#"
(defn answer () i32
  ((make)))
(defn make () (fn () i32)
  (let outer 40i32
    (let middle 1i32
      (lambda () i32 outer))))
"#;
    let checked = check_source("nested-closures.vib", source);
    assert!(checked.accepted(), "{:?}", checked.diagnostics());
    let result =
        vibra_interp::run(checked.program().expect("program")).expect("execution");
    assert_eq!(result.value(), &vibra_ir::Value::I32(40));
}

#[test]
fn probe_nested_lambda_capture_uses_the_parent_environment() {
    let source = r#"
(defn answer () i32
  (((make))))
(defn make () (fn () (fn () i32))
  (let value 39i32
    (lambda () (fn () i32)
      (lambda () i32 value))))
"#;
    let checked = check_source("nested-lambdas.vib", source);
    assert!(checked.accepted(), "{:?}", checked.diagnostics());
    let result =
        vibra_interp::run(checked.program().expect("program")).expect("execution");
    assert_eq!(result.value(), &vibra_ir::Value::I32(39));
}

#[test]
fn nested_lambda_capture_uses_parent_capture_slot_types() {
    let source = r#"
(defn answer () f32
  (((make))))
(defn make () (fn () (fn () f32))
  (let first 1i32
    (let second 2.5f32
      (lambda () (fn () f32)
        (do
          (lambda () i32 first)
          (lambda () f32 second))))))
"#;
    let checked = check_source("nested-capture-types.vib", source);
    assert!(checked.accepted(), "{:?}", checked.diagnostics());
    let result =
        vibra_interp::run(checked.program().expect("program")).expect("execution");
    assert_eq!(result.value(), &vibra_ir::Value::F32(2.5f32.to_bits()));
}

#[test]
fn lambda_discard_parameters_keep_their_declared_slots() {
    let source = r#"
(defn answer () i32
  ((make) 1i32 2i32))
(defn make () (fn (i32 i32) i32)
  (lambda (- i32 value i32) i32 value))
"#;
    let checked = check_source("lambda-discard.vib", source);
    assert!(checked.accepted(), "{:?}", checked.diagnostics());
    let result =
        vibra_interp::run(checked.program().expect("program")).expect("execution");
    assert_eq!(result.value(), &vibra_ir::Value::I32(2));
}

#[test]
fn probe_non_callable_diagnostic() {
    let checked = check_source("not-callable.vib", "(defn answer () i32 (1i32 2i32))");
    assert!(!checked.accepted());
    assert!(
        checked
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code() == DiagnosticCode::TypeNotApplicable)
    );
}

#[test]
fn rejects_call_contract_violations_before_lowering() {
    for source in [
        "(defn answer () i32 (choose 1i32 extra: 2i32))\n(defn choose (value i32) i32 labelled: (known i32 0i32) value)",
        "(defn answer () i32 (choose 1i32 known: 2i32 known: 3i32))\n(defn choose (value i32) i32 labelled: (known i32 0i32) value)",
        "(defn answer () i32 (choose 1i32 known: true))\n(defn choose (value i32) i32 labelled: (known i32 0i32) value)",
    ] {
        let checked = check_source("bad-call.vib", source);
        assert!(!checked.accepted(), "{source}");
        assert!(checked.diagnostics().iter().any(|diagnostic| {
            diagnostic.code() == DiagnosticCode::TypeArgumentMismatch
        }));
        assert!(checked.program().is_none());
        assert!(checked.application_bindings().is_empty());
    }
    let checked = check_source(
        "bad-required-label.vib",
        "(defn answer () i32 (choose 1i32))\n(defn choose (value i32) i32 labelled: (required i32) value)",
    );
    assert!(!checked.accepted());
    assert!(checked.program().is_none());
}

#[test]
fn invalid_result_does_not_emit_argument_order_facts_or_warnings() {
    let source = r#"
(defn answer () str
  (choose 1i32 second: 11i32 first: 9i32))
(defn choose (fallback i32) i32
  labelled: (first i32 7i32 second i32 8i32)
  first)
"#;
    let checked = check_source("invalid-result.vib", source);
    assert!(!checked.accepted());
    assert!(checked
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.code() == DiagnosticCode::TypeArgumentMismatch));
    assert!(
        !checked
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code() == DiagnosticCode::StyleArgumentOrder)
    );
    assert!(checked.application_bindings().is_empty());
}

#[test]
fn rejects_nonempty_effects_in_function_values() {
    let checked = check_source(
        "effects.vib",
        "(defn answer () i32\n  effects: (io.stdout)\n  1i32)",
    );
    assert!(!checked.accepted());
    assert!(
        checked
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code() == DiagnosticCode::ToolUnavailable)
    );
}

#[test]
fn indirect_function_aliases_do_not_bypass_recursive_admission() {
    let source = r#"
(defn first () i32
  (let next second
    (next)))
(defn second () i32
  (let next first
    (next)))
"#;
    let checked = check_source("indirect-recursion.vib", source);
    assert!(!checked.accepted(), "recursive aliases must be rejected");
    assert!(
        checked
            .diagnostics()
            .iter()
            .any(|diagnostic| { diagnostic.code() == DiagnosticCode::ToolUnavailable })
    );
    assert!(checked.program().is_none());
}

#[test]
fn global_function_aliases_do_not_bypass_recursive_admission() {
    let source = r#"
(def recursive (fn () i32) loop)
(defn loop () i32
  (recursive))
"#;
    let checked = check_source("global-recursion.vib", source);
    assert!(!checked.accepted(), "global aliases must be rejected");
    assert!(
        checked
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code() == DiagnosticCode::ToolUnavailable)
    );
    assert!(checked.program().is_none());
}

#[test]
fn conditional_function_values_do_not_bypass_recursive_admission() {
    let source = r#"
(defn answer () i32
  ((if true answer other)))
(defn other () i32 1i32)
"#;
    let checked = check_source("conditional-recursion.vib", source);
    assert!(
        !checked.accepted(),
        "a conditional callee must not hide a recursive target"
    );
    assert!(
        checked
            .diagnostics()
            .iter()
            .any(|diagnostic| { diagnostic.code() == DiagnosticCode::ToolUnavailable })
    );
}

#[test]
fn higher_order_function_values_do_not_bypass_recursive_admission() {
    let source = r#"
(defn apply (f (fn () i32)) i32
  (f))
(defn answer () i32
  (apply answer))
"#;
    let checked = check_source("higher-order-recursion.vib", source);
    assert!(
        !checked.accepted(),
        "higher-order recursion must be rejected"
    );
    assert!(
        checked
            .diagnostics()
            .iter()
            .any(|diagnostic| { diagnostic.code() == DiagnosticCode::ToolUnavailable })
    );
}

#[test]
fn nonrecursive_higher_order_function_values_remain_admitted() {
    let source = r#"
(defn answer () i32
  (apply choose))
(defn apply (f (fn () i32)) i32
  (f))
(defn choose () i32 1i32)
"#;
    let checked = check_source("higher-order-value.vib", source);
    assert!(checked.accepted(), "{:?}", checked.diagnostics());
    let result =
        vibra_interp::run(checked.program().expect("program")).expect("execution");
    assert_eq!(result.value(), &vibra_ir::Value::I32(1));
}

#[test]
fn returned_function_values_do_not_bypass_recursive_admission() {
    let source = r#"
(defn forward () (fn () i32)
  answer)
(defn answer () i32
  ((forward)))
"#;
    let checked = check_source("returned-recursion.vib", source);
    assert!(
        !checked.accepted(),
        "returned recursive values must be rejected"
    );
    assert!(
        checked
            .diagnostics()
            .iter()
            .any(|diagnostic| { diagnostic.code() == DiagnosticCode::ToolUnavailable })
    );
}

#[test]
fn closure_return_values_do_not_bypass_recursive_admission() {
    let source = r#"
(defn answer () i32
  (((lambda () (fn () i32) answer))))
"#;
    let checked = check_source("closure-returned-recursion.vib", source);
    assert!(
        !checked.accepted(),
        "closures returning recursive values must be rejected"
    );
    assert!(
        checked
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code() == DiagnosticCode::ToolUnavailable)
    );
}

#[test]
fn omitted_labels_resolve_defaults_from_the_selected_callable() {
    let source = r#"
(defn answer () i32
  ((if false first second)))
(defn first () i32
  labelled: (value i32 1i32)
  value)
(defn second () i32
  labelled: (value i32 2i32)
  value)
"#;
    let checked = check_source("selected-default.vib", source);
    assert!(checked.accepted(), "{:?}", checked.diagnostics());
    let result =
        vibra_interp::run(checked.program().expect("program")).expect("execution");
    assert_eq!(result.value(), &vibra_ir::Value::I32(2));
}

#[test]
fn omitted_labels_through_a_written_function_type_use_value_defaults() {
    let source = r#"
(def selected (fn () i32 labelled: (value i32)) choose)
(defn answer () i32
  (selected))
(defn choose () i32
  labelled: (value i32 7i32)
  value)
"#;
    let checked = check_source("typed-default.vib", source);
    assert!(checked.accepted(), "{:?}", checked.diagnostics());
    let result =
        vibra_interp::run(checked.program().expect("program")).expect("execution");
    assert_eq!(result.value(), &vibra_ir::Value::I32(7));
}
