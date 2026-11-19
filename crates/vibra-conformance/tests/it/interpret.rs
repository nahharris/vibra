//! The `interpret` cases of the committed corpus, observed beyond their
//! results.
//!
//! `docs/spec/05-tooling.md`, "One source alone", checks an `interpret`
//! source as a one-module workspace with the embedded standard library, so
//! every such module set holds the functions of `@std.bool`.
//! `docs/spec/06-runtime.md`, "Tail calls", requires a call in tail position
//! to replace its caller's activation, so the tail case runs at depth one.

use vibra_conformance::{
    Case, ConformanceOperation, Corpus, INTERPRET_BUDGET, corpus_root,
};
use vibra_interp::Execution;
use vibra_types::interpret;

fn interpret_cases() -> Vec<Case> {
    let corpus = Corpus::discover(corpus_root()).expect("the corpus loads");
    corpus
        .cases()
        .iter()
        .filter(|case| case.manifest().operation == ConformanceOperation::Interpret)
        .cloned()
        .collect()
}

fn execution(id: &str) -> Execution {
    let cases = interpret_cases();
    let case = cases
        .iter()
        .find(|case| case.manifest().id == id)
        .unwrap_or_else(|| panic!("the corpus has `{id}`"));
    let source_id = case
        .manifest()
        .inputs
        .source
        .as_deref()
        .expect("one source");
    let source = case.read_file(source_id).expect("the source reads");
    match interpret(source_id, &source, INTERPRET_BUDGET).run {
        Some(Ok(execution)) => execution,
        other => panic!("`{id}` did not run to a result: {other:?}"),
    }
}

#[test]
fn every_interpret_module_set_holds_the_functions_of_std_bool() {
    let cases = interpret_cases();
    assert!(!cases.is_empty(), "the corpus has interpret cases");
    for case in &cases {
        let id = &case.manifest().id;
        let source_id = case
            .manifest()
            .inputs
            .source
            .as_deref()
            .expect("one source");
        let source = case.read_file(source_id).expect("the source reads");
        let check = interpret(source_id, &source, INTERPRET_BUDGET).check;
        assert!(check.module_set().is_some(), "`{id}` has a module set");
        for member in ["not", "and", "or", "xor"] {
            let name = format!("std.bool.Bool.{member}");
            assert!(
                check
                    .functions()
                    .iter()
                    .any(|(function, _)| *function == name),
                "the module set of `{id}` lacks `{name}`"
            );
        }
    }
}

/// `main` returns `count(100000i32 0i32)` and `count` returns a call to
/// itself until `n` is zero: one transfer from `main` and one for each of
/// the 100000 steps, all at the depth of one activation.
#[test]
fn the_tail_loop_runs_at_depth_one() {
    let run = execution("V2-RUNTIME-tail-loop");
    assert_eq!(run.instrumentation.max_activation_depth, 1);
    assert_eq!(run.instrumentation.tail_transfer_count, 100_001);
}

/// `main` transfers to `sum(100000i32)`, which nests 100000 non-tail calls
/// down to `sum(0i32)`: 100001 activations alive at once.
#[test]
fn the_non_tail_recursion_keeps_one_activation_per_call() {
    let run = execution("V2-RUNTIME-non-tail-depth");
    assert_eq!(run.instrumentation.max_activation_depth, 100_001);
    assert_eq!(run.instrumentation.tail_transfer_count, 1);
}
