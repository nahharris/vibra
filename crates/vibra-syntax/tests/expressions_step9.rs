//! Step 9 expression, pattern, and application-shape tests.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::path::Path;

use vibra_diagnostics::DiagnosticCode;
use vibra_syntax::{Declaration, ExpressionKind, PatternKind, TypeExpr, parse_source};

#[test]
fn nested_expressions_and_patterns_have_step9_structure() {
    let source = r#"
(defn run (value (tuple i32 i32)) i32
  (let (tuple left right) value
    (match left
      (as i32 n) (if true n right)
      (option.none) (try right))))
"#;
    let document =
        parse_source(Path::new("expressions.vib"), source).expect("source loader");
    assert!(document.accepted(), "{:?}", document.diagnostics());
    let ast = document.ast().expect("declaration AST");
    let Declaration::Defn(function) = &ast.declarations()[0] else {
        panic!("expected defn")
    };
    let [expression] = function.expressions() else {
        panic!("expected one function expression")
    };
    let ExpressionKind::Let {
        pattern,
        value,
        body,
    } = expression.kind()
    else {
        panic!("expected let expression")
    };
    assert!(matches!(pattern.kind(), PatternKind::Tuple(values) if values.len() == 2));
    assert!(matches!(value.kind(), ExpressionKind::Name(_)));
    let [match_expression] = body.as_slice() else {
        panic!("expected one let body expression")
    };
    let ExpressionKind::Match { scrutinee, arms } = match_expression.kind() else {
        panic!("expected match expression")
    };
    assert!(matches!(scrutinee.kind(), ExpressionKind::Name(_)));
    assert_eq!(arms.len(), 2);
    assert!(matches!(
        arms[0].pattern().kind(),
        PatternKind::As {
            value_type: TypeExpr::Name(_),
            pattern: _
        }
    ));
    assert!(matches!(arms[0].result().kind(), ExpressionKind::If { .. }));
    assert!(matches!(
        arms[1].pattern().kind(),
        PatternKind::Constructor { arguments, .. } if arguments.is_empty()
    ));
    assert!(matches!(arms[1].result().kind(), ExpressionKind::Try(_)));
}

#[test]
fn applications_accept_arbitrary_callees_and_contextual_type_arguments() {
    let source = r#"
(defn calls () i32
  ((choose-function true) 1i32 label: 2i32 types: (i32))
  (f))
"#;
    let document =
        parse_source(Path::new("applications.vib"), source).expect("source loader");
    assert!(document.accepted(), "{:?}", document.diagnostics());
    let ast = document.ast().expect("declaration AST");
    let Declaration::Defn(function) = &ast.declarations()[0] else {
        panic!("expected defn")
    };
    let [first, second] = function.expressions() else {
        panic!("expected two body expressions")
    };
    let ExpressionKind::Application(application) = first.kind() else {
        panic!("expected application")
    };
    assert!(matches!(
        application.callee().kind(),
        ExpressionKind::Application(_)
    ));
    assert!(matches!(
        application.type_arguments(),
        Some([TypeExpr::Name(_)])
    ));
    assert!(application.type_arguments_after_operands());
    assert_eq!(application.arguments().len(), 2);
    assert_eq!(application.arguments()[0].label(), None);
    assert_eq!(
        application.arguments()[1]
            .label()
            .expect("labelled argument")
            .value(),
        "label"
    );
    assert!(
        matches!(second.kind(), ExpressionKind::Application(application) if application.arguments().is_empty())
    );
}

#[test]
fn empty_do_and_multiple_body_expressions_are_valid() {
    let source = "(defn sequence () void (do) (do 0i32 (try 0i32)))";
    let document = parse_source(Path::new("do.vib"), source).expect("source loader");
    assert!(document.accepted(), "{:?}", document.diagnostics());
    let ast = document.ast().expect("declaration AST");
    let Declaration::Defn(function) = &ast.declarations()[0] else {
        panic!("expected defn")
    };
    let [empty, sequence] = function.expressions() else {
        panic!("expected two body expressions")
    };
    assert!(matches!(empty.kind(), ExpressionKind::Do(values) if values.is_empty()));
    assert!(matches!(sequence.kind(), ExpressionKind::Do(values) if values.len() == 2));
}

#[test]
fn lambda_attributes_and_nested_discard_patterns_are_structured() {
    let source = r#"
(defn make () (fn () i32)
  (lambda () i32
    labelled: (level i32 0i32)
    variadic: (rest (array i32))
    effects: ()
    0i32))
(defn ignore (value i32) i32
  (let - value
    (let @- value
      (let -: value value))))
"#;
    let document =
        parse_source(Path::new("patterns.vib"), source).expect("source loader");
    assert!(document.accepted(), "{:?}", document.diagnostics());
    let ast = document.ast().expect("declaration AST");
    let Declaration::Defn(make) = &ast.declarations()[0] else {
        panic!("expected make")
    };
    let [lambda] = make.expressions() else {
        panic!("expected lambda")
    };
    let ExpressionKind::Lambda(lambda) = lambda.kind() else {
        panic!("expected lambda expression")
    };
    assert_eq!(lambda.attributes().items().len(), 3);
    assert_eq!(lambda.body().len(), 1);

    let Declaration::Defn(ignore) = &ast.declarations()[1] else {
        panic!("expected ignore")
    };
    let [outer] = ignore.expressions() else {
        panic!("expected outer let")
    };
    let ExpressionKind::Let { pattern, body, .. } = outer.kind() else {
        panic!("expected let")
    };
    assert!(matches!(pattern.kind(), PatternKind::Binding(name) if name.is_discard()));
    let [middle] = body.as_slice() else {
        panic!("expected middle let")
    };
    let ExpressionKind::Let { pattern, body, .. } = middle.kind() else {
        panic!("expected middle let")
    };
    assert!(matches!(pattern.kind(), PatternKind::Binding(name) if name.is_discard()));
    let [inner] = body.as_slice() else {
        panic!("expected inner let")
    };
    let ExpressionKind::Let { pattern, .. } = inner.kind() else {
        panic!("expected inner let")
    };
    assert!(matches!(pattern.kind(), PatternKind::Binding(name) if name.is_discard()));
}

#[test]
fn malformed_step9_forms_report_existing_syntax_diagnostics() {
    let cases = [
        ("(defn bad () i32 ())", DiagnosticCode::SyntaxInvalidForm),
        (
            "(defn bad () i32 (as i32))",
            DiagnosticCode::SyntaxInvalidForm,
        ),
        (
            "(defn bad (value i32) i32 (let (as i32) value value))",
            DiagnosticCode::SyntaxInvalidForm,
        ),
        (
            "(defn bad () i32 (match true false))",
            DiagnosticCode::SyntaxInvalidForm,
        ),
        (
            "(defn bad () i32 (if true 0i32))",
            DiagnosticCode::SyntaxInvalidForm,
        ),
        (
            "(defn bad () i32 (try 0i32 1i32))",
            DiagnosticCode::SyntaxInvalidForm,
        ),
        ("(defn bad () i32 -)", DiagnosticCode::SyntaxInvalidForm),
        (
            "(defn bad () i32 (tuple i32))",
            DiagnosticCode::SyntaxInvalidForm,
        ),
        (
            "(defn bad () i32 (f types: (i32) types: (str)))",
            DiagnosticCode::SyntaxDuplicateAttribute,
        ),
    ];
    for (source, code) in cases {
        let document = parse_source(Path::new("invalid-expressions.vib"), source)
            .expect("source loader");
        assert!(!document.accepted(), "accepted malformed source: {source}");
        assert!(
            document
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code() == code),
            "missing {code:?} for {source}: {:?}",
            document.diagnostics()
        );
    }
}

#[test]
fn retired_expression_forms_have_explicit_diagnostics() {
    for head in [
        "while", "for", "break", "continue", "return", "bind", "case",
    ] {
        let source = format!("(defn bad () void ({head} true))");
        let document =
            parse_source(Path::new("retired.vib"), &source).expect("source loader");
        assert!(!document.accepted(), "accepted retired source: {source}");
        assert!(
            document.diagnostics().iter().any(
                |diagnostic| diagnostic.code() == DiagnosticCode::SyntaxRetiredForm
            ),
            "missing retired-form diagnostic for {source}: {:?}",
            document.diagnostics()
        );
    }
}
