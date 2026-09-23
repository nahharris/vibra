#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

//! Host checks for the Step 12 workspace conformance boundary.

use std::path::PathBuf;

use vibra_conformance::{
    CaseManifest, ConformanceOperation, Corpus, InterpreterV1WorkspaceRunHandler,
    ProfileHandler, StaticV1WorkspaceCheckHandler,
};

#[test]
fn workspace_operations_require_their_profile_and_confined_project_binding() {
    let check = CaseManifest::from_str(
        r#"
id = "V1-PROJECT-workspace-check"
rule = "V1-PROJECT"
profile = "static-v1"
operation = "workspace-check"

[inputs]
project = "tree/project.vibon"
tree = "tree"

[expect]
accepted = true
"#,
    )
    .expect("valid workspace-check manifest");
    assert_eq!(check.operation(), ConformanceOperation::WorkspaceCheck);

    let run = CaseManifest::from_str(
        r#"
id = "V1-RUNTIME-workspace-run"
rule = "V1-RUNTIME"
profile = "interpreter-v1"
operation = "workspace-run"

[inputs]
project = "tree/project.vibon"
tree = "tree"

[expect]
accepted = true
interpreter = { result = "result.vibon", audit_trace = "audit.vibon" }
"#,
    )
    .expect("valid workspace-run manifest");
    assert_eq!(run.operation(), ConformanceOperation::WorkspaceRun);

    let rejected_run = CaseManifest::from_str(
        r#"
id = "V1-RUNTIME-workspace-run-rejected"
rule = "V1-RUNTIME"
profile = "interpreter-v1"
operation = "workspace-run"

[inputs]
project = "tree/project.vibon"
tree = "tree"

[expect]
accepted = false

[[expect.diagnostics]]
code = "@tool.unavailable"
level = "@error"
source = "tree/src/app/main.vib"
span = [0, 0]
"#,
    )
    .expect("rejected workspace-run does not need execution snapshots");
    assert_eq!(rejected_run.operation(), ConformanceOperation::WorkspaceRun);

    for invalid in [
        r#"
id = "V1-PROJECT-workspace-check-profile"
rule = "V1-PROJECT"
profile = "interpreter-v1"
operation = "workspace-check"
[inputs]
project = "tree/project.vibon"
tree = "tree"
[expect]
accepted = true
"#,
        r#"
id = "V1-RUNTIME-workspace-run-binding"
rule = "V1-RUNTIME"
profile = "interpreter-v1"
operation = "workspace-run"
[inputs]
project = "outside/project.vibon"
tree = "tree"
[expect]
accepted = true
interpreter = { result = "result.vibon", audit_trace = "audit.vibon" }
"#,
        r#"
id = "V1-RUNTIME-workspace-run-snapshots"
rule = "V1-RUNTIME"
profile = "interpreter-v1"
operation = "workspace-run"
[inputs]
project = "tree/project.vibon"
tree = "tree"
[expect]
accepted = true
"#,
    ] {
        assert!(CaseManifest::from_str(invalid).is_err());
    }
}

#[test]
fn workspace_check_only_checks_and_workspace_run_records_the_pure_result() {
    let corpus_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("conformance/cases");
    let corpus = Corpus::discover(corpus_root).expect("repository corpus");
    let check_case = corpus
        .cases()
        .iter()
        .find(|case| {
            case.manifest().id() == "V1-PROJECT-workspace-check-import-closure"
        })
        .expect("workspace-check case");
    let checked = StaticV1WorkspaceCheckHandler
        .run(check_case)
        .expect("workspace check handler");
    assert!(checked.accepted);
    assert!(checked.diagnostics.is_empty());
    assert!(checked.interpreter.is_none());

    let run_case = corpus
        .cases()
        .iter()
        .find(|case| case.manifest().id() == "V1-RUNTIME-workspace-run")
        .expect("workspace-run case");
    let ran = InterpreterV1WorkspaceRunHandler
        .run(run_case)
        .expect("workspace run handler");
    assert!(ran.accepted);
    assert!(ran.diagnostics.is_empty());
    let execution = ran.interpreter.expect("pure execution observation");
    assert_eq!(
        execution.result.as_deref(),
        Some("(record type: @void value: void)\n")
    );
    assert!(execution.audit_trace.is_empty());
}

#[test]
fn workspace_cases_cover_higher_order_entry_selection_and_library_cycles() {
    let corpus_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("conformance/cases");
    let corpus = Corpus::discover(corpus_root).expect("repository corpus");

    let entry_case = corpus
        .cases()
        .iter()
        .find(|case| {
            case.manifest().id() == "V1-RUNTIME-workspace-run-higher-order-entry"
        })
        .expect("higher-order workspace-run case");
    let ran = InterpreterV1WorkspaceRunHandler
        .run(entry_case)
        .expect("higher-order workspace run");
    assert!(ran.accepted, "{:?}", ran.diagnostics);
    assert_eq!(
        ran.interpreter
            .expect("pure execution observation")
            .result
            .as_deref(),
        Some("(record type: @void value: void)\n")
    );

    let unused_helper_case = corpus
        .cases()
        .iter()
        .find(|case| {
            case.manifest().id()
                == "V1-RUNTIME-workspace-run-unused-imported-higher-order-helper"
        })
        .expect("unused imported higher-order helper case");
    let ran = InterpreterV1WorkspaceRunHandler
        .run(unused_helper_case)
        .expect("workspace run with unused higher-order import");
    assert!(ran.accepted, "{:?}", ran.diagnostics);
    assert!(ran.interpreter.is_some());

    let closure_case = corpus
        .cases()
        .iter()
        .find(|case| {
            case.manifest().id()
                == "V1-RUNTIME-workspace-run-uninvoked-higher-order-closure"
        })
        .expect("uninvoked higher-order closure case");
    let ran = InterpreterV1WorkspaceRunHandler
        .run(closure_case)
        .expect("workspace run with an uninvoked closure");
    assert!(ran.accepted, "{:?}", ran.diagnostics);
    assert!(ran.interpreter.is_some());

    let cycle_case = corpus
        .cases()
        .iter()
        .find(|case| {
            case.manifest().id()
                == "V1-PROJECT-workspace-check-higher-order-initializer-cycle"
        })
        .expect("library initializer cycle case");
    let checked = StaticV1WorkspaceCheckHandler
        .run(cycle_case)
        .expect("library workspace check");
    assert!(!checked.accepted);
    assert!(checked.diagnostics.iter().any(|diagnostic| {
        diagnostic.code() == vibra_diagnostics::DiagnosticCode::TypeInitializerCycle
            && diagnostic.source_id() == Some("src/util/main.vib")
    }));

    let uninvoked_closure_case = corpus
        .cases()
        .iter()
        .find(|case| {
            case.manifest().id()
                == "V1-PROJECT-workspace-check-uninvoked-closure-initializer"
        })
        .expect("uninvoked closure initializer case");
    let checked = StaticV1WorkspaceCheckHandler
        .run(uninvoked_closure_case)
        .expect("library check with an uninvoked closure");
    assert!(checked.accepted, "{:?}", checked.diagnostics);
    assert!(checked.diagnostics.is_empty());
}
