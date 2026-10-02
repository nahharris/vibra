//! Scope of `let` and `let-else` bindings (`docs/roadmap/pre-m4/01-bindings-return-never.md`,
//! section B).

#![allow(clippy::expect_used, clippy::indexing_slicing, missing_docs)]

use vibra_diagnostics::DiagnosticCode;
use vibra_resolve::{ResolveInput, Resolver};

fn codes(source: &str) -> Vec<DiagnosticCode> {
    let snapshot = Resolver::resolve(ResolveInput::single_module(
        "demo",
        "1.0.0",
        "app",
        "main",
        source.as_bytes(),
    ));
    snapshot
        .diagnostics()
        .iter()
        .map(vibra_diagnostics::Diagnostic::code)
        .collect()
}

fn unknown() -> Vec<DiagnosticCode> {
    vec![DiagnosticCode::NameUnknownSymbol]
}

#[test]
fn a_binding_reaches_later_pairs_and_later_elements_only() {
    // B01-B04, B13.
    assert_eq!(codes("(defn f () i32 (let a a) a)"), unknown());
    assert_eq!(
        codes("(defn f () i32 (use a) (let a 1i32) a)\n(defn use (v i32) void (do))"),
        unknown()
    );
    assert_eq!(codes("(defn f () i32 (do (let b 1i32)) b)"), unknown());
    assert_eq!(
        codes("(defn f () i32 (let f2 (lambda () i32 g)) (let g 1i32) g)"),
        unknown()
    );
    assert!(codes("(defn f () i32 (let a 1i32 b a) (use a b) b)\n(defn use (x i32 y i32) void (do))").is_empty());
    assert!(
        codes("(defn f () i32 (let g 1i32) (let h (lambda () i32 g)) (h))").is_empty()
    );
}

#[test]
fn a_binding_that_outlasts_its_form_makes_a_later_repeat_a_redeclaration() {
    // B05-B08.
    let redeclaration = vec![DiagnosticCode::NameRedeclaration];
    assert_eq!(
        codes("(defn f () i32 (let a 1i32) (let a 2i32) a)"),
        redeclaration
    );
    assert_eq!(
        codes("(defn f () i32 (let a 1i32 a 2i32) a)"),
        redeclaration
    );
    assert_eq!(
        codes("(defn f () i32 (let d 1i32) (do (let d 2i32) d))"),
        redeclaration
    );
    assert!(
        codes("(defn f () i32 (do (let c 1i32) c) (do (let c 2i32) c))").is_empty()
    );
    // The first binder is the related span of the later one.
    let snapshot = Resolver::resolve(ResolveInput::single_module(
        "demo",
        "1.0.0",
        "app",
        "main",
        b"(defn f () i32 (let a 1i32) (let a 2i32) a)",
    ));
    let diagnostic = snapshot.diagnostics().first().expect("redeclaration");
    assert_eq!(diagnostic.related().len(), 1);
}

#[test]
fn a_let_else_pattern_binds_after_the_form_and_not_in_its_value_or_fallback() {
    // B09-B12.
    assert_eq!(
        codes(
            "(defn f (o (option i32)) i32 (let-else (option.some x) x (return 0i32)) 0i32)"
        ),
        unknown()
    );
    assert_eq!(
        codes(
            "(defn f (o (option i32)) i32 (let-else (option.some y) o (return y)) y)"
        ),
        unknown()
    );
    assert!(codes(
        "(defn f (o (option i32)) i32 (let-else (option.some z) o (do (let z 1i32) (return z))) z)"
    )
    .is_empty());
    assert!(
        codes(
            "(defn f (o (option i32)) i32 (let-else (option.some w) o (return 0i32)) w)"
        )
        .is_empty()
    );
}

#[test]
fn discard_pairs_repeat_freely() {
    // B14.
    assert!(codes("(defn f () i32 (let - 1i32 @- 2i32 -: 3i32) 0i32)").is_empty());
}
