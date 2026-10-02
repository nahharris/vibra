//! Typing of `let`, `let-else`, `return`, and `never`
//! (`docs/roadmap/pre-m4/01-bindings-return-never.md`, sections C, D, and E).

#![allow(clippy::expect_used, clippy::indexing_slicing, missing_docs)]

use vibra_diagnostics::DiagnosticCode as C;
use vibra_types::check_source;

const PRELUDE: &str = "(defn spin () never (spin))\n(defn use (value i32) void (do))\n(defn attempt () (result void str) (result.ok))\n(deftype shape (enum empty void circle u32))\n(deftype number (union i32 f32))\n(defn none-of () (option t) where: (t any) (option.none))\n(defn pick (a t) t where: (t any) a)\n";

fn codes(source: &str) -> Vec<C> {
    let checked = check_source("pre-m4.vib", &format!("{PRELUDE}{source}"));
    checked
        .diagnostics()
        .iter()
        .map(vibra_diagnostics::Diagnostic::code)
        .collect()
}

fn ok(source: &str) {
    assert_eq!(codes(source), Vec::<C>::new(), "{source}");
}

#[test]
fn a_final_let_is_void() {
    ok("(defn f () void (let a 1i32))");
    ok("(defn f () void (do))");
    assert_eq!(codes("(defn f () i32 (let a 1i32))"), vec![C::TypeMismatch]);
}

#[test]
fn let_patterns_must_be_irrefutable_and_values_have_no_expected_type() {
    assert_eq!(
        codes("(defn f (v shape) u32 (let (shape.circle r) v) r)"),
        vec![C::PatternRefutableBinding]
    );
    assert_eq!(
        codes("(defn f () i32 (let a 1) a)"),
        vec![C::TypeAmbiguousInference]
    );
}

#[test]
fn a_bound_or_discarded_result_is_handled() {
    ok("(defn f () void (let r (attempt)) (let - (attempt)))");
    assert_eq!(
        codes("(defn f () void (attempt) (let - 1i32))"),
        vec![C::TypeUnhandledFallible]
    );
}

#[test]
fn let_else_needs_a_refutable_pattern_and_a_never_fallback() {
    ok("(defn f (o (option i32)) i32 (let-else (option.some v) o (return 0i32)) v)");
    ok("(defn f (o (option i32)) i32 (let-else (option.some v) o (spin)) v)");
    ok(
        "(defn f (o (option i32) c bool) i32 (let-else (option.some v) o (if c (spin) (spin))) v)",
    );
    ok(
        "(defn f (o (option i32)) i32 (let-else (option.some v) o (do (use 1i32) (return 0i32))) v)",
    );
    ok("(defn f (o (option i32)) void (let-else (option.some v) o (return void)))");
    ok("(defn f (n number) i32 (let-else (as i32 v) n (return 0i32)) v)");
    ok(
        "(defn f (o (option never)) i32 (let-else (option.some v) o (return 0i32)) 1i32)",
    );
    for pattern in ["x", "-", "(tupleof a b)"] {
        let source = format!(
            "(defn f (p (tuple i32 i32)) i32 (let-else {pattern} p (return 0i32)) 0i32)"
        );
        assert_eq!(
            codes(&source),
            vec![C::PatternIrrefutableLetElse],
            "{pattern}"
        );
    }
    assert_eq!(
        codes("(defn f (o (option i32)) i32 (let-else (option.some v) o 0i32) v)"),
        vec![C::TypeMismatch]
    );
}

#[test]
fn return_exits_the_innermost_function_at_its_written_result_type() {
    ok("(defn f (c bool) i32 (if c (return 1i32) void) 2i32)");
    ok(
        "(deftype holder (record n i32) (defn m (value self c bool) i32 (if c (return 1i32) void) 2i32))",
    );
    ok(
        "(defn f (c bool) str (let g (lambda () i32 (if c (return 1i32) void) 2i32)) \"t\")",
    );
    ok("(defn f (c bool) void (if c (return void) void) void)");
    ok("(defn f (c bool) number (if c (return 1i32) void) 2i32)");
    assert_eq!(
        codes("(defn f (c bool) i32 (if c (return \"x\") void) 1i32)"),
        vec![C::TypeMismatch]
    );
}

#[test]
fn return_outside_a_function_is_invalid() {
    assert_eq!(
        codes("(def x i32 (return 1i32))"),
        vec![C::TypeInvalidReturn]
    );
}

#[test]
fn a_return_in_tail_position_is_redundant() {
    for source in [
        "(defn f () i32 (return 1i32))",
        "(defn f (c bool) i32 (if c (return 1i32) 2i32))",
        "(defn f (v bool) i32 (match v true (return 1i32) false 2i32))",
        "(defn f () i32 (do (use 1i32) (return 2i32)))",
        "(defn f () (fn () i32) (lambda () i32 (return 1i32)))",
    ] {
        assert_eq!(codes(source), vec![C::TypeRedundantReturn], "{source}");
    }
    assert_eq!(
        codes("(defn f (c bool) i32 (if c (return 1i32) (return 2i32)))"),
        vec![C::TypeRedundantReturn, C::TypeRedundantReturn]
    );
    ok("(defn f (o (option i32)) void (let-else (option.some v) o (return void)))");
}

#[test]
fn a_return_nested_in_a_return_reports_the_position_error_once() {
    assert_eq!(
        codes("(defn f () i32 (return (return 1i32)))"),
        vec![C::TypeUnreachableCode, C::TypeRedundantReturn]
    );
    assert_eq!(
        codes("(defn f (c bool) i32 (if c (return (spin)) void) 1i32)"),
        vec![C::TypeUnreachableCode]
    );
}

#[test]
fn a_never_function_needs_a_never_final_expression() {
    ok("(defn g () never (g))");
    assert_eq!(codes("(defn g () never 1i32)"), vec![C::TypeMismatch]);
    assert_eq!(codes("(defn g () never)"), vec![C::TypeMismatch]);
    // D18: the operand mismatches `never`, and the final `return` is redundant.
    let mut found = codes("(defn g () never (return 1i32))");
    found.sort_by_key(|code| code.as_atom());
    assert_eq!(found, vec![C::TypeMismatch, C::TypeRedundantReturn]);
}

#[test]
fn never_is_admitted_at_expected_types_and_skipped_in_joins() {
    ok("(defn f () i32 (spin))");
    ok("(defn f () (option i32) (spin))");
    ok("(defn f (c bool) i32 (if c 1i32 (spin)))");
    ok("(defn f (c bool) never (if c (spin) (spin)))");
    ok(
        "(defn f (v (option i32)) i32 (match v (option.some n) n (option.none) (spin)))",
    );
    ok("(defn f (c bool) number (if c 1i32 (spin)))");
    assert_eq!(
        codes("(defn f (c bool) i32 (if c 1i32 2.0f32))"),
        vec![C::TypeMismatch]
    );
}

#[test]
fn never_is_unreachable_code_outside_its_admitted_positions() {
    for source in [
        "(defn f () i32 (use (spin)) 1i32)",
        "(defn f () i32 (let a (spin)) 1i32)",
        "(defn f () i32 (if (spin) 1i32 2i32))",
        "(defn f () i32 (match (spin) - 1i32))",
        "(defn f () (result i32 str) (try (spin)))",
        "(defn f () i32 (tupleof (spin)) 1i32)",
        "(defn f () i32 (try (return 1i32)))",
    ] {
        assert_eq!(codes(source), vec![C::TypeUnreachableCode], "{source}");
    }
    assert_eq!(codes("(def x i32 (spin))"), vec![C::TypeUnreachableCode]);
    assert_eq!(
        codes("(defn f () i32 (spin) 1i32)"),
        vec![C::TypeUnreachableCode]
    );
}

#[test]
fn never_is_written_wherever_a_type_is_and_is_never_inferred() {
    ok("(defn f (x never) i32 1i32)");
    ok("(defn f () (result i32 never) (result.ok 1i32))");
    ok("(defn f () (array never) (array.of))");
    ok("(defn f () (fn () never) (lambda () never (spin)))");
    ok("(defn f () (option never) (as (option never) (option.none)))");
    ok("(defn f () (option never) (none-of types: (never)))");
    assert_eq!(
        codes("(defn f () i32 (pick (spin)) 1i32)"),
        vec![C::TypeUnreachableCode, C::TypeAmbiguousInference]
    );
    assert_eq!(
        codes("(defn f () i32 (none-of) 1i32)"),
        vec![C::TypeAmbiguousInference]
    );
}

#[test]
fn never_satisfies_any_and_nothing_else() {
    let tag = "(defint tag (defn name (value self) i32))\n(deftype any-box (record item t) where: (t any))\n(deftype tag-box (record item t) where: (t tag))\n";
    assert_eq!(
        codes(&format!("{tag}(defn f (b (any-box never)) i32 1i32)")),
        Vec::<C>::new()
    );
    assert_eq!(
        codes(&format!("{tag}(defn f (b (tag-box never)) i32 1i32)")),
        vec![C::TypeUnsatisfiedBound]
    );
    assert_eq!(
        codes("(def k (dict never i32) (dict.of))"),
        vec![C::TypeInvalidDictKey]
    );
    assert_eq!(
        codes(
            "(defint named (defn label (value self) i32) (impl never (defn label (value never) i32 1i32)))"
        ),
        vec![C::NameWrongEntityKind]
    );
}

#[test]
fn uninhabited_types_need_no_arm() {
    ok("(defn f (r (result i32 never)) i32 (match r (result.ok v) v))");
    ok("(defn f (r (result i32 never)) i32 (let (result.ok value) r) value)");
    assert_eq!(
        codes(
            "(defn f (r (result i32 never)) i32 (match r (result.ok v) v (result.err e) 0i32))"
        ),
        vec![C::PatternUnreachableArm]
    );
    assert_eq!(
        codes("(defn f (t (tuple i32 never)) i32 (match t - 0i32))"),
        vec![C::PatternUnreachableArm]
    );
    assert_eq!(
        codes(
            "(deftype mixed (enum a never b i32 c i32))\n(defn f (m mixed) i32 (match m (mixed.b x) x))"
        ),
        vec![C::PatternNonExhaustive]
    );
}

#[test]
fn inhabitedness_is_structural() {
    let types = "(deftype dead never)\n(deftype dead-record (record n i32 gone never))\n(deftype dead-enum (enum a never b never))\n(deftype dead-union (union dead never))\n(deftype live-enum (enum a never b void))\n";
    for (name, uninhabited) in [
        ("dead", true),
        ("dead-record", true),
        ("dead-enum", true),
        ("dead-union", true),
        ("live-enum", false),
        ("(array never)", false),
        ("(option never)", false),
        ("(fn () never)", false),
    ] {
        let source = format!("{types}(defn f (v {name}) i32 (match v - 0i32))");
        let expected = if uninhabited {
            vec![C::PatternUnreachableArm]
        } else {
            Vec::new()
        };
        assert_eq!(codes(&source), expected, "{name}");
    }
}

#[test]
fn a_result_that_cannot_fail_is_not_fallible() {
    let attempts = "(defn a1 () (result void never) (result.ok))\n(defn a2 () (result void str) (result.ok))\n(defn a3 () (result never str) (result.err \"x\"))\n";
    assert_eq!(
        codes(&format!("{attempts}(defn f () i32 (a1) 1i32)")),
        Vec::<C>::new()
    );
    assert_eq!(
        codes(&format!("{attempts}(defn f () i32 (a2) 1i32)")),
        vec![C::TypeUnhandledFallible]
    );
    assert_eq!(
        codes(&format!("{attempts}(defn f () i32 (a3) 1i32)")),
        vec![C::TypeUnhandledFallible]
    );
    // `try` keeps requiring the same error type.
    assert_eq!(
        codes(&format!(
            "{attempts}(defn f () (result void never) (try (a1)) (result.ok))"
        )),
        Vec::<C>::new()
    );
    assert_eq!(
        codes(&format!("{attempts}(defn f () i32 (try (a1)))")),
        vec![C::TypeInvalidTry]
    );
}

#[test]
fn inhabitedness_terminates_on_recursive_types() {
    // E23: a recursive declared type is examined without a fixed-point search.
    let source = "(deftype tree (record kids (array tree)))\n(defn f (v tree) i32 (match v - 0i32))";
    assert_eq!(codes(source), Vec::<C>::new());
}
