//! M4 Step 21b: the indirect call-flow analysis is total.
//!
//! A continuation-passing loop wraps the continuation it was given in a new
//! closure on every round. The analysis used to carry the summary of each
//! captured value inside the closure summary, so each wrap built a deeper
//! summary, the domain had no finite height, and the analysis either reported
//! `@tool.unavailable` or exhausted the host stack. A closure is now an
//! identity, and what its expression captures is one summary per expression,
//! so the domain is finite and the analysis cannot fail to converge on a
//! checked program.

use crate::support::corpus_root;
use std::path::PathBuf;

use vibra_types::check_source;

/// Smaller than the 1 MiB that Windows gives a process's main thread.
const SMALL_STACK: usize = 768 * 1024;

fn case_input(case: &str) -> String {
    let path: PathBuf = corpus_root().join(case).join("input.vib");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// Checks `source` on a thread with `stack` bytes and reports whether it was
/// accepted, with the diagnostics when it was not.
fn check_on_small_stack(source: String, stack: usize) -> Result<(), String> {
    std::thread::Builder::new()
        .stack_size(stack)
        .spawn(move || {
            let checked = check_source("input.vib", &source);
            if checked.accepted() && checked.program().is_some() {
                Ok(())
            } else {
                Err(format!("{:?}", checked.diagnostics()))
            }
        })
        .expect("thread")
        .join()
        .expect("the checker must not abort the thread")
}

#[test]
fn every_continuation_chain_case_checks_on_a_small_stack() {
    for case in [
        "V1-RUNTIME-continuation-chain",
        "V1-RUNTIME-continuation-chain-non-tail",
        "V1-RUNTIME-continuation-chain-mutual",
    ] {
        if let Err(reason) = check_on_small_stack(case_input(case), SMALL_STACK) {
            panic!("{case}: {reason}");
        }
    }
}

#[test]
fn a_wrapping_closure_that_captures_a_function_value_checks() {
    // The smallest shape: the closure captures its own parameter, which holds
    // the closure the previous round made.
    let source = "(defn main () u64 (round 3u64 (lambda (total u64) u64 total)))\n\
(defn round (count u64 next (fn (u64) u64)) u64\n\
  (if (u64.equal count 0u64) (next 0u64)\n\
    (round (lower count) (lambda (total u64) u64 (next total)))))\n\
(defn lower (count u64) u64\n\
  (match (u64.sub-checked count 1u64) (result.ok value) value (result.err -) 0u64))\n";
    if let Err(reason) = check_on_small_stack(source.to_owned(), SMALL_STACK) {
        panic!("{reason}");
    }
}

/// The chain cases build a chain of a hundred thousand closures (a hundred and
/// fifty thousand for the mutual one). The corpus runs them at that size in both
/// backends; here the size is cut by a factor of seventeen and the stack by the
/// same, so a release that recursed on the host stack would overflow at the
/// same number of bytes a level.
const ROUNDS: u64 = 6_000;
const INTERPRETER_STACK: usize = 32 * 1024;

/// Runs the case, with its rounds cut to [`ROUNDS`], in the reference
/// interpreter on a thread with `stack` bytes and returns its canonical result.
/// The case is checked on this thread, which has the stack the checker needs.
fn interpret_on_small_stack(case: &str, stack: usize) -> String {
    let source = case_input(case).replace("100000u64", &format!("{ROUNDS}u64"));
    let checked = check_source("input.vib", &source);
    let program = checked.program().expect("a checked program").clone();
    std::thread::Builder::new()
        .stack_size(stack)
        .spawn(move || {
            let world = vibra_host::HostWorld::default();
            let mut host = vibra_host::WorldProvider::new(&world);
            vibra_interp::Interpreter::run_with_host(
                &program,
                vibra_interp::MemoryBudget::new(
                    vibra_conformance::INSTANCE_MEMORY_LIMIT_BYTES,
                ),
                &mut host,
            )
            .expect("the run must end in a result")
            .canonical_result()
        })
        .expect("thread")
        .join()
        .expect("the interpreter must not abort the thread")
}

#[test]
fn a_long_closure_chain_runs_and_is_released_on_a_small_stack() {
    for (case, expected) in [
        ("V1-RUNTIME-continuation-chain", ROUNDS),
        ("V1-RUNTIME-continuation-chain-non-tail", ROUNDS),
        ("V1-RUNTIME-continuation-chain-mutual", ROUNDS * 3 / 2),
    ] {
        let result = interpret_on_small_stack(case, INTERPRETER_STACK);
        assert!(
            result.contains(&format!("{expected}u64")),
            "{case}: {result}"
        );
    }
}
