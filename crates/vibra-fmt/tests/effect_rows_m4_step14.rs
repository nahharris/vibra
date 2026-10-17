//! M4 Step 14: an effect row is written sorted by the canonical identity of its
//! roots (`docs/spec/03-effects.md`, "Function effect rows"). The formatter
//! never resolves a symbol, so it reorders a row only for the checker's fact
//! and keeps every lexical symbol as written.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use vibra_diagnostics::ByteSpan;
use vibra_fmt::{format_source, format_source_with_facts};
use vibra_syntax::EffectRowOrder;

fn at(source: &str, needle: &str) -> ByteSpan {
    let start = source.find(needle).expect("needle in source");
    ByteSpan::new(start, start + needle.len())
}

fn format_with(source: &str, rows: &[EffectRowOrder]) -> String {
    format_source_with_facts("rows.vib", source, &[], rows)
        .expect("source mode")
        .text()
        .to_owned()
}

#[test]
fn a_row_with_a_fact_is_written_in_the_canonical_order_keeping_each_symbol() {
    let source = "(defn f () void effects: (io.stdout fs.read env.read) (do))";
    let rows = [EffectRowOrder::new(at(source, "io.stdout"), vec![2, 1, 0])];
    assert_eq!(
        format_with(source, &rows),
        "(defn f () void effects: (env.read fs.read io.stdout) (do))\n"
    );
}

#[test]
fn a_row_across_lines_is_one_line_in_that_order() {
    let source = "(defn f () void\n  effects: (zebra\n            apple)\n  (do))";
    let rows = [EffectRowOrder::new(at(source, "zebra"), vec![1, 0])];
    assert_eq!(
        format_with(source, &rows),
        "(defn f () void effects: (apple zebra) (do))\n"
    );
}

#[test]
fn a_row_without_a_fact_keeps_its_written_order() {
    let source = "(defn f () void effects: (zebra apple) (do))";
    assert_eq!(
        format_with(source, &[]),
        "(defn f () void effects: (zebra apple) (do))\n"
    );
    assert_eq!(
        format_source("rows.vib", source).expect("source"),
        format_with(source, &[])
    );
}

#[test]
fn a_fact_that_does_not_fit_its_row_is_ignored() {
    let source = "(defn f () void effects: (zebra apple) (do))";
    let wrong_length = [EffectRowOrder::new(at(source, "zebra"), vec![2, 1, 0])];
    assert_eq!(
        format_with(source, &wrong_length),
        "(defn f () void effects: (zebra apple) (do))\n"
    );
    let not_a_permutation = [EffectRowOrder::new(at(source, "zebra"), vec![0, 0])];
    assert_eq!(
        format_with(source, &not_a_permutation),
        "(defn f () void effects: (zebra apple) (do))\n"
    );
}

#[test]
fn a_row_with_a_comment_is_left_as_written() {
    let source =
        "(defn f () void\n  effects: (zebra ; first\n            apple)\n  (do))";
    let rows = [EffectRowOrder::new(at(source, "zebra"), vec![1, 0])];
    let formatted = format_with(source, &rows);
    let zebra = formatted.find("zebra").expect("zebra");
    let apple = formatted.find("apple").expect("apple");
    assert!(zebra < apple, "{formatted}");
    assert!(formatted.contains("; first"), "{formatted}");
}

#[test]
fn rows_of_functions_lambdas_tests_and_function_types_are_ordered_and_idempotent() {
    let source = "(test \"t\" effects: (b a) (do))\n(defn f (g (fn () void effects: (d c))) void effects: (f2 e) (let - (lambda () void effects: (z y) (do))) (do))";
    let rows = [
        EffectRowOrder::new(at(source, "b a"), vec![1, 0]),
        EffectRowOrder::new(at(source, "d c"), vec![1, 0]),
        EffectRowOrder::new(at(source, "f2 e"), vec![1, 0]),
        EffectRowOrder::new(at(source, "z y"), vec![1, 0]),
    ];
    let once = format_with(source, &rows);
    assert!(once.contains("effects: (a b)"), "{once}");
    assert!(once.contains("effects: (c d)"), "{once}");
    assert!(once.contains("effects: (e f2)"), "{once}");
    assert!(once.contains("effects: (y z)"), "{once}");
    // The checker's fact for the formatted text is the identity order, so a
    // second pass changes nothing.
    let identity = [
        EffectRowOrder::new(at(&once, "a b"), vec![0, 1]),
        EffectRowOrder::new(at(&once, "c d"), vec![0, 1]),
        EffectRowOrder::new(at(&once, "e f2"), vec![0, 1]),
        EffectRowOrder::new(at(&once, "y z"), vec![0, 1]),
    ];
    assert_eq!(format_with(&once, &identity), once);
}
