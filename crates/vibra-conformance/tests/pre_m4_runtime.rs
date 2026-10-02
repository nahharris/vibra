//! Evaluation of `let`, `let-else`, `return`, and `never` through the real
//! checker and interpreter (`docs/roadmap/pre-m4/01-bindings-return-never.md`,
//! section F).

#![allow(clippy::expect_used, clippy::indexing_slicing, missing_docs)]

use vibra_interp::Interpreter;
use vibra_types::check_source;

fn run(source: &str) -> vibra_interp::Execution {
    let checked = check_source("pre-m4.vib", source);
    assert!(checked.accepted(), "{source}: {:?}", checked.diagnostics());
    Interpreter::run(checked.program().expect("program")).expect("execution")
}

fn answer(source: &str) -> String {
    run(source).canonical_result().trim_end().to_owned()
}

#[test]
fn let_binds_in_order_for_the_rest_of_the_sequence() {
    // F01.
    assert_eq!(
        answer("(defn answer () i32 (let a 1i32 b a) (do (let c b) c))"),
        "(record type: @i32 value: 1i32)"
    );
}

#[test]
fn let_else_takes_the_match_path_or_the_fallback() {
    // F02, F03.
    let source = |argument: &str| {
        format!(
            "(defn answer () i32 (pick {argument}))\n(defn pick (o (option i32)) i32 (let-else (option.some v) o (return 100i32)) v)"
        )
    };
    assert_eq!(
        answer(&source("(option.some 5i32)")),
        "(record type: @i32 value: 5i32)"
    );
    assert_eq!(
        answer(&source("(option.none)")),
        "(record type: @i32 value: 100i32)"
    );
}

#[test]
fn return_skips_the_rest_and_a_lambda_return_leaves_only_the_lambda() {
    // F04, F05.
    let source = |flag: &str| {
        format!(
            "(defn answer () i32 (early {flag}))\n(defn early (c bool) i32 (if c (return 1i32) void) 2i32)"
        )
    };
    assert_eq!(answer(&source("true")), "(record type: @i32 value: 1i32)");
    assert_eq!(answer(&source("false")), "(record type: @i32 value: 2i32)");
    assert_eq!(
        answer(
            "(defn answer () i32 (let f (lambda () i32 (if true (return 10i32) void) 20i32)) (let - (f)) 7i32)"
        ),
        "(record type: @i32 value: 7i32)"
    );
}

#[test]
fn the_operand_of_return_widens_at_the_written_result_type() {
    // F06.
    let observed = answer(
        "(deftype number (union i32 f32))\n(defn answer () number (early true))\n(defn early (c bool) number (if c (return 1i32) void) 2i32)",
    );
    assert!(observed.contains("kind: @union"), "{observed}");
}

fn walk(count: usize) -> vibra_interp::Execution {
    let items = vec!["1i32"; count].join(" ");
    run(&format!(
        "(defn answer () i32 (walk (array.of {items})))\n(defn walk (items (array i32)) i32\n  (let-else (option.some rest) (array.slice items 1u64 (array.length items)) (return 7i32))\n  (let-else (option.none) (items 0u64) (return (walk rest)))\n  7i32)"
    ))
}

#[test]
fn the_operand_of_return_is_a_tail_call() {
    // F07: the entry call and one transfer per element, and no growth in live
    // activations.
    let short = walk(10);
    let long = walk(2_000);
    assert_eq!(short.tail_transfer_count(), 11);
    assert_eq!(long.tail_transfer_count(), 2_001);
    assert_eq!(short.max_activation_depth(), long.max_activation_depth());
}

#[test]
fn a_diverging_call_on_an_untaken_branch_is_never_made() {
    // F10.
    assert_eq!(
        answer(
            "(defn answer () i32 (if false (spin) 3i32))\n(defn spin () never (spin))"
        ),
        "(record type: @i32 value: 3i32)"
    );
}
