//! Step 9 expression, pattern, and binding-aware formatter tests.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::path::Path;

use vibra_diagnostics::DiagnosticCode;
use vibra_fmt::{FormatError, format_source, format_source_with_bindings};
use vibra_syntax::{
    ApplicationBinding, BindingFacts, Declaration, ExpressionKind, VariadicBinding,
    parse_source,
};

#[test]
fn formatter_renders_nested_step9_forms_and_is_idempotent() {
    let source = r#"
(defn run (value (tuple i32 i32)) i32
  (let (tuple left right) value
    (match left
      (as i32 n) (if true n right)
      (option.none) (try right))))
"#;
    let formatted =
        format_source(Path::new("expressions.vib"), source).expect("source mode");
    assert!(formatted.contains("(match"));
    assert!(formatted.contains("(as i32 n)"));
    assert!(formatted.contains("(try right)"));
    assert_eq!(
        format_source(Path::new("expressions.vib"), &formatted)
            .expect("idempotent formatting"),
        formatted
    );
    assert!(
        parse_source(Path::new("expressions.vib"), &formatted)
            .expect("source loader")
            .accepted()
    );
}

#[test]
fn formatter_keeps_multiline_match_arms_as_pattern_result_units() {
    let source = r#"
(defn choose (value i32) i32
  (match value
    (constructor.with.a.long.name (tuple first second third fourth fifth sixth))
      (if true first second)
    (another.constructor.with.a.long.name (array first second third fourth fifth sixth))
      (try value)))
"#;
    let formatted = format_source(Path::new("match.vib"), source).expect("source mode");
    let lines = formatted.lines().collect::<Vec<_>>();
    let first_pattern = lines
        .iter()
        .position(|line| line.contains("constructor.with.a.long.name"))
        .expect("first pattern line");
    let second_pattern = lines
        .iter()
        .position(|line| line.contains("another.constructor.with.a.long.name"))
        .expect("second pattern line");
    assert!(lines[first_pattern].contains("(if true first second)"));
    assert!(lines[second_pattern].contains("(try value)"));
    assert_eq!(
        format_source(Path::new("match.vib"), &formatted)
            .expect("idempotent formatting"),
        formatted
    );
}

#[test]
fn formatter_preserves_comments_when_match_arms_move_together() {
    let source = r#"
(defn choose (value i32) i32
  (match value
    ; first arm
    (option.some) 1i32 ; first result
    ; second arm
    (option.none) 2i32))
"#;
    let formatted =
        format_source(Path::new("match-comments.vib"), source).expect("source mode");
    assert!(formatted.contains("; first arm"));
    assert!(formatted.contains("; first result"));
    assert!(formatted.contains("; second arm"));
    let lines = formatted.lines().collect::<Vec<_>>();
    let first_result = lines
        .iter()
        .position(|line| line.contains("(option.some) 1i32"))
        .expect("first result");
    let first_result_comment = lines
        .iter()
        .position(|line| line.contains("; first result"))
        .expect("first result comment");
    let second_arm = lines
        .iter()
        .position(|line| line.contains("(option.none) 2i32"))
        .expect("second arm");
    assert!(first_result < first_result_comment);
    assert!(first_result_comment < second_arm);
    assert_eq!(
        format_source(Path::new("match-comments.vib"), &formatted)
            .expect("idempotent formatting"),
        formatted
    );
}

#[test]
fn formatter_normalizes_arguments_only_with_authoritative_facts() {
    let source = "(defn call () i32 (call b: 2i32 1i32 rest1 rest2))";
    let document =
        parse_source(Path::new("bindings.vib"), source).expect("source loader");
    assert!(document.accepted(), "{:?}", document.diagnostics());
    let ast = document.ast().expect("declaration AST");
    let Declaration::Defn(function) = &ast.declarations()[0] else {
        panic!("expected defn")
    };
    let [expression] = function.expressions() else {
        panic!("expected one expression")
    };
    let ExpressionKind::Application(application) = expression.kind() else {
        panic!("expected application")
    };
    let facts =
        BindingFacts::new(1, vec!["b".to_owned()], Some(VariadicBinding::Array));
    let binding = ApplicationBinding::new(application.span(), facts);
    let formatted =
        format_source_with_bindings(Path::new("bindings.vib"), source, &[binding])
            .expect("binding facts");
    assert_eq!(
        formatted.text(),
        "(defn call () i32 (call 1i32 b: 2i32 rest1 rest2))\n"
    );
    assert!(
        formatted
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code() == DiagnosticCode::StyleArgumentOrder)
    );
}

#[test]
fn formatter_orders_labels_by_the_supplied_signature_and_accepts_map_tails() {
    let source = "(defn call () i32 (call b: 2i32 a: 1i32 0i32))";
    let document =
        parse_source(Path::new("bindings.vib"), source).expect("source loader");
    let ast = document.ast().expect("declaration AST");
    let Declaration::Defn(function) = &ast.declarations()[0] else {
        panic!("expected defn")
    };
    let [expression] = function.expressions() else {
        panic!("expected one expression")
    };
    let ExpressionKind::Application(application) = expression.kind() else {
        panic!("expected application")
    };
    let binding = ApplicationBinding::new(
        application.span(),
        BindingFacts::new(1, vec!["a".to_owned(), "b".to_owned()], None),
    );
    let formatted =
        format_source_with_bindings(Path::new("bindings.vib"), source, &[binding])
            .expect("binding facts");
    assert_eq!(
        formatted.text(),
        "(defn call () i32 (call 0i32 a: 1i32 b: 2i32))\n"
    );

    let map_source = "(defn call () i32 (call 0i32 key1 value1 key2 value2))";
    let map_document =
        parse_source(Path::new("bindings.vib"), map_source).expect("source loader");
    let map_ast = map_document.ast().expect("declaration AST");
    let Declaration::Defn(map_function) = &map_ast.declarations()[0] else {
        panic!("expected defn")
    };
    let [map_expression] = map_function.expressions() else {
        panic!("expected one expression")
    };
    let ExpressionKind::Application(map_application) = map_expression.kind() else {
        panic!("expected application")
    };
    let map_binding = ApplicationBinding::new(
        map_application.span(),
        BindingFacts::new(1, Vec::new(), Some(VariadicBinding::Map)),
    );
    let map_formatted = format_source_with_bindings(
        Path::new("bindings.vib"),
        map_source,
        &[map_binding],
    )
    .expect("map binding facts");
    assert_eq!(
        map_formatted.text(),
        "(defn call () i32 (call 0i32 key1 value1 key2 value2))\n"
    );
}

#[test]
fn binding_facts_apply_through_the_comment_preserving_path() {
    let source = r#"
(defn call () i32
  ; keep this comment
  (call b: 2i32 1i32))
"#;
    let document = parse_source(Path::new("bindings-comments.vib"), source)
        .expect("source loader");
    let ast = document.ast().expect("declaration AST");
    let Declaration::Defn(function) = &ast.declarations()[0] else {
        panic!("expected defn")
    };
    let [expression] = function.expressions() else {
        panic!("expected one expression")
    };
    let ExpressionKind::Application(application) = expression.kind() else {
        panic!("expected application")
    };
    let binding = ApplicationBinding::new(
        application.span(),
        BindingFacts::new(1, vec!["b".to_owned()], None),
    );
    let formatted = format_source_with_bindings(
        Path::new("bindings-comments.vib"),
        source,
        &[binding],
    )
    .expect("binding facts");
    assert!(formatted.text().contains("; keep this comment"));
    assert!(formatted.text().contains("(call 1i32 b: 2i32)"));
    assert!(
        formatted
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code() == DiagnosticCode::StyleArgumentOrder)
    );
}

#[test]
fn formatter_rejects_contradictory_binding_facts() {
    let cases = [
        (
            "(defn call () i32 (call unknown: 1i32))",
            BindingFacts::new(0, vec!["known".to_owned()], None),
        ),
        (
            "(defn call () i32 (call b: 1i32 b: 2i32))",
            BindingFacts::new(0, vec!["b".to_owned()], None),
        ),
        (
            "(defn call () i32 (call 1i32 2i32 3i32 4i32))",
            BindingFacts::new(1, Vec::new(), Some(VariadicBinding::Map)),
        ),
    ];
    for (source, facts) in cases {
        let document =
            parse_source(Path::new("bindings.vib"), source).expect("source loader");
        assert!(document.accepted(), "{:?}", document.diagnostics());
        let ast = document.ast().expect("declaration AST");
        let Declaration::Defn(function) = &ast.declarations()[0] else {
            panic!("expected defn")
        };
        let [expression] = function.expressions() else {
            panic!("expected one expression")
        };
        let ExpressionKind::Application(application) = expression.kind() else {
            panic!("expected application")
        };
        let binding = ApplicationBinding::new(application.span(), facts);
        let error =
            format_source_with_bindings(Path::new("bindings.vib"), source, &[binding])
                .expect_err("contradictory binding facts must fail");
        assert!(matches!(error, FormatError::Binding(_)));
    }
}

#[test]
fn formatter_preserves_written_order_without_binding_facts() {
    let source = "(defn call () i32 (call b: 2i32 1i32))";
    let formatted =
        format_source(Path::new("bindings.vib"), source).expect("source mode");
    assert_eq!(formatted, "(defn call () i32 (call b: 2i32 1i32))\n");
}

#[test]
fn formatter_moves_types_group_before_operands_and_reports_style() {
    let source = "(defn call () i32 (call 1i32 types: (i32)))";
    let formatted = format_source_with_bindings(Path::new("bindings.vib"), source, &[])
        .expect("source mode");
    assert_eq!(
        formatted.text(),
        "(defn call () i32 (call types: (i32) 1i32))\n"
    );
    assert_eq!(
        formatted
            .diagnostics()
            .iter()
            .filter(|diagnostic| diagnostic.code() == DiagnosticCode::StyleArgumentOrder)
            .count(),
        1
    );
}
