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

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    missing_docs
)]

use std::path::{Path, PathBuf};

use vibra_types::check_source;

/// Smaller than the 1 MiB that Windows gives a process's main thread.
const SMALL_STACK: usize = 512 * 1024;

fn case_input(case: &str) -> String {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/cases")
        .join(case)
        .join("input.vib");
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
