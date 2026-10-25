//! M4 Step 19: the interpreter classifies an entry's result by its variant, not
//! by the content of the error, and the Wasm backend's classification agrees.

#![allow(clippy::expect_used, clippy::indexing_slicing, missing_docs)]

use vibra_interp::Interpreter;
use vibra_types::check_source;

fn failed(source: &str) -> bool {
    let checked = check_source("entry.vib", source);
    assert!(checked.accepted(), "{source}: {:?}", checked.diagnostics());
    Interpreter::run(checked.program().expect("program"))
        .expect("execution")
        .program_failed()
}

#[test]
fn only_an_err_result_is_a_program_failure() {
    assert!(failed(
        "(deftype failure (record code i32))\n(defn main () (result void failure) (result.err (failure code: 1i32)))"
    ));
    assert!(!failed("(deftype failure (record code i32))\n(defn main () (result void failure) (result.ok))"));
    assert!(!failed("(defn main () void (do))"));
}

#[test]
fn the_error_content_is_never_inspected() {
    // An error whose text says `ok` is still an err.
    assert!(failed(
        "(deftype outcome (record text str))\n(defn main () (result void outcome) (result.err (outcome text: \"ok\")))"
    ));
}
