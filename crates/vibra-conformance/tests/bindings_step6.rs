//! Step 6 binding, control-flow, and immutable-value contract tests.

#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)]
#![allow(missing_docs)]

use std::path::{Path, PathBuf};

use vibra_conformance::{
    Case, InterpreterV1Handler, ProfileHandler, StaticV1TypeHandler,
};
use vibra_syntax::parse_data;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crate has a workspace root")
        .to_path_buf()
}

fn case(id: &str) -> Case {
    vibra_conformance::Corpus::discover(workspace_root().join("conformance/cases"))
        .expect("Step 6 corpus")
        .cases()
        .iter()
        .find(|case| case.manifest().id() == id)
        .cloned()
        .expect("Step 6 case")
}

#[test]
fn interpreter_runs_checked_bindings_and_selected_if_branch() {
    let case = case("V1-RUNTIME-bindings");
    let first = InterpreterV1Handler.run(&case).expect("interpreter");
    let second = InterpreterV1Handler.run(&case).expect("repeat interpreter");
    assert_eq!(first, second);
    assert_eq!(
        first.interpreter.expect("execution").result.as_deref(),
        Some("(record type: @i32 value: 2i32)\n")
    );
}

#[test]
fn initializer_cycle_is_a_static_rejection_before_interpretation() {
    let case = case("V1-TYPE-NAMES-binding-cycle");
    let observation = StaticV1TypeHandler.run(&case).expect("type handler");
    assert!(!observation.accepted);
    assert_eq!(observation.diagnostics.len(), 1);
    assert_eq!(
        observation.diagnostics[0].code(),
        vibra_diagnostics::DiagnosticCode::TypeInitializerCycle
    );
}

#[test]
fn checked_binding_observation_is_valid_vibon() {
    let case = case("V1-RUNTIME-bindings");
    let observation = StaticV1TypeHandler.run(&case).expect("type handler");
    let types = observation.types.expect("typed program observation");
    let document = parse_data(Path::new("types.vibon"), &types)
        .expect("typed binding observation loader");
    assert!(document.accepted(), "{:?}", document.diagnostics());
}

#[test]
fn all_discard_spellings_and_sibling_scope_are_checked() {
    let case = case("V1-TYPE-NAMES-binding-discards");
    let observation = StaticV1TypeHandler.run(&case).expect("type handler");
    assert!(observation.accepted);
    assert!(observation.diagnostics.is_empty());
}
