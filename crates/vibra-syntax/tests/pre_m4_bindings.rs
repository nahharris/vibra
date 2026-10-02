//! Pre-M4 binding sequences, `let-else`, `return`, and `never` at the reader
//! and contextual-AST level (`docs/roadmap/pre-m4/01-bindings-return-never.md`,
//! sections A and B16/B17).

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    missing_docs
)]

use std::path::Path;

use vibra_diagnostics::DiagnosticCode;
use vibra_syntax::{Declaration, ExpressionKind, parse_source, query_position};

fn codes(source: &str) -> Vec<DiagnosticCode> {
    let document =
        parse_source(Path::new("pre-m4.vib"), source).expect("source loader");
    document
        .diagnostics()
        .iter()
        .map(vibra_diagnostics::Diagnostic::code)
        .collect()
}

fn accepted(source: &str) {
    let document =
        parse_source(Path::new("pre-m4.vib"), source).expect("source loader");
    assert!(
        document.accepted(),
        "{source}: {:?}",
        document.diagnostics()
    );
}

#[test]
fn let_forms_are_body_elements_with_one_or_more_pairs() {
    // A01-A05.
    for source in [
        "(defn f () i32 (let a 1i32) a)",
        "(defn f () i32 (let a 1i32 b a) b)",
        "(defn f () i32 (let - (g) (tupleof x y) pair - x) x)",
        "(defn f (o (option i32)) i32 (let-else (option.some v) o (return 0i32)) v)",
        "(defn f () i32 (do (let a 1i32) a))",
        "(defn f () (fn () i32) (lambda () i32 (let a 1i32) a))",
        "(deftype t (record n i32) (defn m (value self) i32 (let a 1i32) a))",
        "(defint i (defn d (value self) i32 (let a 1i32) a))",
        "(test \"t\" (let a 1i32) (g a))",
    ] {
        accepted(source);
    }
}

#[test]
fn a_let_has_pairs_in_the_ast() {
    let document =
        parse_source(Path::new("pairs.vib"), "(defn f () i32 (let a 1i32 b a) b)")
            .expect("source loader");
    let ast = document.ast().expect("AST");
    let Declaration::Defn(function) = &ast.declarations()[0] else {
        panic!("expected defn")
    };
    let [binding, _] = function.expressions() else {
        panic!("expected a let and a value")
    };
    let ExpressionKind::Let { bindings } = binding.kind() else {
        panic!("expected let")
    };
    assert_eq!(bindings.len(), 2);
}

#[test]
fn a_binding_form_anywhere_else_is_misplaced() {
    // A06-A11.
    for source in [
        "(defn f () i32 (g (let a 1i32)))",
        "(defn f () i32 (if (let a true) 1i32 2i32))",
        "(defn f () i32 (if true (let a 1i32) 2i32))",
        "(defn f () i32 (if true 1i32 (let a 2i32)))",
        "(defn f () i32 (match (let a 1i32) - 0i32))",
        "(defn f () i32 (match 1i32 - (let a 1i32)))",
        "(def x i32 (let a 1i32))",
        "(defn f () i32 (let b (let a 1i32)) b)",
        "(defn f () i32 (let-else x (let a 1i32) (return 0i32)) 0i32)",
        "(defn f () i32 (let-else x y (let a 1i32)) 0i32)",
        "(defn f () i32 (do (return (let a 1i32)) 0i32))",
        "(defn f () i32 (try (let a x)))",
        "(defn f () i32 (as i32 (let a 1i32)))",
        "(defn f () i32 (tupleof (let a 1i32)))",
        "(defn f () i32 (g (let-else x y (return 0i32))))",
    ] {
        assert_eq!(
            codes(source),
            vec![DiagnosticCode::SyntaxMisplacedBinding],
            "{source}"
        );
    }
}

#[test]
fn a_misplaced_form_is_accepted_once_wrapped_in_do() {
    // A12.
    accepted("(defn f (flag bool) i32 (if flag (do (let a 1i32) a) 2i32))");
    accepted("(defn f (x i32) i32 (match x - (do (let a 1i32) a)))");
}

#[test]
fn binding_and_return_arity_is_checked() {
    // A13-A18.
    for source in [
        "(defn f () i32 (let) 0i32)",
        "(defn f () i32 (let a) 0i32)",
        "(defn f () i32 (let a 1i32 b) 0i32)",
        "(defn f () i32 (let-else p v) 0i32)",
        "(defn f () i32 (let-else p v f g) 0i32)",
        "(defn f () i32 (return))",
        "(defn f () i32 (return a b))",
    ] {
        assert_eq!(
            codes(source),
            vec![DiagnosticCode::SyntaxInvalidForm],
            "{source}"
        );
    }
}

#[test]
fn the_old_let_shape_has_no_bridge() {
    // A19: a trailing form is read under the pair grammar.
    assert_eq!(
        codes("(defn f (x i32) i32 (let y 1i32 (add y x)) 0i32)"),
        vec![DiagnosticCode::SyntaxInvalidForm]
    );
}

#[test]
fn a_rejected_form_leaves_the_next_declaration_readable() {
    // A20-A21: one diagnostic, and the second declaration still parses.
    for source in [
        "(defn bad () i32 (g (let a 1i32)))\n(defn good () i32 (let a 1i32) a)",
        "(defn bad () i32 (let a 1i32 b) 0i32)\n(defn good () i32 (let a 1i32) a)",
    ] {
        assert_eq!(codes(source).len(), 1, "{source}");
    }
}

#[test]
fn return_is_no_longer_retired_but_the_loop_forms_are() {
    // A23.
    accepted("(defn f () void (return void))");
    for head in ["while", "for", "break", "continue"] {
        assert_eq!(
            codes(&format!("(defn f () void ({head} true))")),
            vec![DiagnosticCode::SyntaxRetiredForm],
            "{head}"
        );
    }
}

#[test]
fn never_is_reserved_as_a_declaration_and_a_value_spelling() {
    // B16-B17.
    assert_eq!(
        codes("(deftype never i32)"),
        vec![DiagnosticCode::NameReservedDeclaration]
    );
    assert_eq!(
        codes("(defint never)"),
        vec![DiagnosticCode::NameReservedDeclaration]
    );
    assert_eq!(
        codes("(deftype g (record n i32) where: (never any))"),
        vec![DiagnosticCode::NameReservedDeclaration]
    );
    assert_eq!(
        codes("(def never i32 1i32)"),
        vec![DiagnosticCode::NameReservedValueSpelling]
    );
}

#[test]
fn the_structural_query_offers_binding_forms_only_at_body_elements() {
    let source = "(defn f () i32 (g 1i32) (h 2i32))";
    let document = parse_source(Path::new("query.vib"), source).expect("source loader");
    let body = source.find("(g 1i32)").expect("body element") + 1;
    let operand = source.find("1i32").expect("operand");
    let permitted = |offset| {
        query_position(&document, offset)
            .expect("query")
            .permitted_forms()
            .expect("forms")
            .to_vec()
    };
    let at_body = permitted(body);
    let at_operand = permitted(operand);
    assert!(at_body.iter().any(|form| form == "let"));
    assert!(at_body.iter().any(|form| form == "let-else"));
    assert!(
        !at_operand
            .iter()
            .any(|form| form == "let" || form == "let-else")
    );
    assert!(at_operand.iter().any(|form| form == "return"));
}
