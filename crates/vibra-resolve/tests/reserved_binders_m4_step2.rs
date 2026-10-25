//! A lexical binder spelled as a reserved word (`docs/spec/01-source-language.md`,
//! "Reader", and `docs/spec/07-diagnostics-and-conformance.md`, "Binding,
//! `return`, and `never` diagnostics"; M4 ledger D14.1).

#![allow(clippy::expect_used, clippy::indexing_slicing, missing_docs)]

use vibra_diagnostics::{ByteSpan, DiagnosticCode};
use vibra_resolve::{ResolveInput, Resolver};

fn diagnostics(source: &str) -> Vec<(DiagnosticCode, ByteSpan)> {
    Resolver::resolve(ResolveInput::single_module(
        "demo",
        "1.0.0",
        "app",
        "main",
        source.as_bytes(),
    ))
    .diagnostics()
    .iter()
    .map(|diagnostic| (diagnostic.code(), diagnostic.primary_span()))
    .collect()
}

/// The one diagnostic `source` reports, which must be
/// `@name.reserved-declaration` at `binder` inside the first `anchor`.
fn rejected(source: &str, anchor: &str, binder: &str) {
    // The binder is the first spelling that is not a qualified path's head.
    fn binder_offset(anchor: &str, binder: &str) -> usize {
        anchor
            .match_indices(binder)
            .map(|(index, _)| index)
            .find(|index| !anchor[index + binder.len()..].starts_with('.'))
            .expect("binder")
    }
    let start = source.find(anchor).expect("anchor") + binder_offset(anchor, binder);
    assert_eq!(
        diagnostics(source),
        vec![(
            DiagnosticCode::NameReservedDeclaration,
            ByteSpan::new(start, start + binder.len())
        )],
        "{source}"
    );
}

/// Every symbol spelling the rule names that every binder site admits as a pure
/// name: keywords, `any`, and the closed vocabulary's types. The vocabulary's
/// values, `true` and `false`, are constant patterns at a pattern site and
/// reserved pure names elsewhere, so they have their own test below.
const SPELLINGS: &[&str] = &[
    "do", "let", "let-else", "if", "match", "return", "as", "try", "lambda", "tupleof",
    "recordof", "enumof", "any", "never", "bool", "char", "str", "bytes", "atom",
    "option", "result", "iter", "array", "dict", "i8", "i16", "i32", "i64", "u8",
    "u16", "u32", "u64", "f32", "f64",
];

#[test]
fn a_parameter_spelled_as_a_reserved_word_is_rejected_and_still_binds() {
    for spelling in SPELLINGS {
        let source = format!("(defn f ({spelling} i32) i32 {spelling})");
        rejected(&source, &format!("({spelling} i32)"), spelling);
    }
}

#[test]
fn a_let_binder_is_rejected_and_the_next_valid_binder_resolves() {
    for spelling in SPELLINGS {
        let source =
            format!("(defn f () i32 (let {spelling} 1i32) (let ok {spelling}) ok)");
        rejected(&source, &format!(" {spelling} 1i32"), spelling);
    }
}

#[test]
fn a_let_else_binder_is_rejected() {
    for spelling in SPELLINGS {
        let source = format!(
            "(defn f (o (option i32)) i32 (let-else (option.some {spelling}) o (return 0i32)) {spelling})"
        );
        rejected(&source, &format!("(option.some {spelling}"), spelling);
    }
}

#[test]
fn a_match_arm_binder_is_rejected() {
    for spelling in SPELLINGS {
        let source = format!(
            "(defn f (o (option i32)) i32 (match o (option.some {spelling}) {spelling} (option.none) 0i32))"
        );
        rejected(&source, &format!("(option.some {spelling}"), spelling);
    }
}

#[test]
fn lambda_labelled_and_variadic_binders_are_rejected() {
    for spelling in SPELLINGS {
        let lambda = format!(
            "(defn f () i32 (let g (lambda ({spelling} i32) i32 {spelling})) (g 1i32))"
        );
        rejected(&lambda, &format!("({spelling} i32)"), spelling);
        let labelled =
            format!("(defn f () i32\n  labelled: ({spelling} i32 0i32)\n  {spelling})");
        rejected(&labelled, &format!("({spelling} i32 0i32)"), spelling);
        let variadic =
            format!("(defn f () i32\n  variadic: ({spelling} (array i32))\n  0i32)");
        rejected(&variadic, &format!("({spelling} (array i32))"), spelling);
    }
}

#[test]
fn a_nested_pattern_binder_is_rejected_at_its_own_name() {
    let source = "(defn f (p (tuple i32 i32)) i32 (let (tupleof a if) p) a)";
    rejected(source, "(tupleof a if)", "if");
}

#[test]
fn ordinary_binders_and_discards_are_accepted() {
    let source = "(defn f (value i32 - i32) i32 (let first 1i32 - 2i32) (let second 3i32) value)";
    assert_eq!(diagnostics(source), vec![], "{source}");
}
