//! The `interpret` cases of the committed corpus, observed beyond their
//! results.
//!
//! `docs/spec/05-tooling.md`, "One source alone", checks an `interpret`
//! source as a one-module workspace with the embedded standard library, so
//! every such module set holds the functions of `@std.bool`. The check is
//! the one `interpret` runs before it executes the entry.
//! `docs/spec/06-runtime.md`, "Tail calls", requires a call in tail position
//! to replace its caller's activation, so the tail case runs at depth one.

use vibra_conformance::{
    Case, ConformanceOperation, Corpus, INTERPRET_BUDGET, corpus_root,
};
use vibra_interp::Execution;
use vibra_types::{check_source, interpret};

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
    let holds = |case: &Case| {
        let id = &case.manifest().id;
        let source_id = case
            .manifest()
            .inputs
            .source
            .as_deref()
            .expect("one source");
        let source = case.read_file(source_id).expect("the source reads");
        let check = check_source(source_id, &source);
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
    };
    // The checks are independent: they run on as many threads as the host
    // has, each over its share of the cases.
    let threads = std::thread::available_parallelism().map_or(1, usize::from);
    let share = cases.len().div_ceil(threads);
    std::thread::scope(|scope| {
        for chunk in cases.chunks(share) {
            scope.spawn(move || chunk.iter().for_each(holds));
        }
    });
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

/// `main` transfers to `even`; each of the 100000 steps transfers between
/// `even` and `odd` through the functions their record argument holds.
#[test]
fn the_tail_call_through_a_field_runs_at_depth_one() {
    let run = execution("V2-RUNTIME-tail-through-value");
    assert_eq!(run.instrumentation.max_activation_depth, 1);
    assert_eq!(run.instrumentation.tail_transfer_count, 100_001);
}

/// `main` transfers to its closure, which transfers to itself through the
/// record parameter that holds it for each of the 100000 steps.
#[test]
fn the_tail_call_through_a_closure_runs_at_depth_one() {
    let run = execution("V2-RUNTIME-tail-through-closure");
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

/// `docs/spec/02-type-system.md`, "Bare guards": a bare `where` is an
/// obligation discharged where the value is made, with no run-time check. A
/// value that crosses an interface value is not checked again: the run of
/// `V2-CONTRACT-ingress-interface-value` takes the same reductions as the same
/// source with its `where` section written as spaces.
#[test]
fn a_value_through_an_interface_value_is_not_checked_again() {
    let id = "V2-CONTRACT-ingress-interface-value";
    let cases = interpret_cases();
    let case = cases
        .iter()
        .find(|case| case.manifest().id == id)
        .expect("the corpus has the case");
    let source = case.read_file("main.vib").expect("the source reads");
    let section = "where isPositive ";
    let plain = source.replacen(section, &" ".repeat(section.len()), 1);
    assert_ne!(source, plain);
    let run = |text: &str| match interpret("main.vib", text, INTERPRET_BUDGET).run {
        Some(Ok(execution)) => execution,
        other => panic!("the source did not run to a result: {other:?}"),
    };
    let guarded = run(&source);
    let unguarded = run(&plain);
    assert_eq!(
        guarded.instrumentation.reduction_count,
        unguarded.instrumentation.reduction_count
    );
    assert_eq!(guarded.canonical, unguarded.canonical);
}
