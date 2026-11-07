//! What the corpus cannot say about interface dispatch in the WebAssembly
//! backend (milestone 4 Step 9): a contract call in tail position reuses its
//! frame and one that is not does not, under a memory limit.
//!
//! The rest of the Step 9 matrix is corpus cases that run in both backends and
//! hold the live size to the module values:
//! `V1-RUNTIME-interface-containers`, `V1-RUNTIME-generic-dispatch`,
//! `V1-RUNTIME-interface-dispatch`, `V1-RUNTIME-tail-contract-member-interface-value`,
//! and `V1-RUNTIME-entry-outcome-trap-holding-err`.

use crate::support::runner_with;
use vibra_ir::Type;
use vibra_types::check_source;
use vibra_wasm_run::{Observed, Outcome};

fn run_limited(source: &str, limit: usize) -> Observed {
    let result = check_source("input.vib", source);
    assert!(result.accepted(), "{:?}\n{source}", result.diagnostics());
    let program = result.program().expect("an accepted program");
    let ty: Type = program.entry().signature().result();
    let module = vibra_wasm::emit(program).expect("the program lowers");
    let heads = |head: u32| module.descriptors().type_of(head).cloned();
    runner_with(limit)
        .run_observed_with(module.bytes(), &ty, program.types(), &heads)
        .expect("the module runs")
}

fn tail_source(n: u32, tail: bool) -> String {
    let call = "(walker.walk (as walker (counter left: (lower (value @left)))))";
    let step = if tail {
        call.to_owned()
    } else {
        format!("(do {call} 7u64)")
    };
    format!(
        "(defn main () u64 (walker.walk (as walker (counter left: {n}u64))))\n\
(defint walker (defn walk (value self) u64))\n\
(deftype counter (record left u64) (impl walker (defn walk (value self) u64 (if (equatable.equal (value @left) 0u64) 7u64 {step}))))\n\
(defn lower (c u64) u64 (match (u64.sub-checked c 1u64) (result.ok v) v (result.err -) 0u64))"
    )
}

#[test]
fn a_tail_contract_call_reuses_the_frame_and_a_non_tail_one_does_not() {
    let limit = 256 * 1024;
    let done = run_limited(&tail_source(100_000, true), limit);
    assert!(matches!(done, Observed::Completed { .. }), "{done:?}");
    let control = run_limited(&tail_source(100_000, false), limit);
    assert!(
        matches!(control, Observed::Stopped(Outcome::MemoryExhausted)),
        "{control:?}"
    );
}
