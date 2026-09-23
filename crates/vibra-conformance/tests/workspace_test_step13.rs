#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

//! Manifest requirements for the Step 13 workspace-test operation.

use std::path::PathBuf;

use vibra_conformance::{
    CaseManifest, ConformanceOperation, Corpus, InterpreterV1WorkspaceTestHandler,
    ProfileHandler,
};

#[test]
fn workspace_test_requires_one_result_snapshot_and_no_suite_audit_snapshot() {
    let valid = CaseManifest::from_str(
        r#"
id = "V1-RUNTIME-workspace-test"
rule = "V1-RUNTIME"
profile = "interpreter-v1"
operation = "workspace-test"

[inputs]
project = "tree/project.vibon"
tree = "tree"

[expect]
accepted = true
interpreter = { result = "result.vibon" }
"#,
    )
    .expect("workspace-test uses one result snapshot");
    assert_eq!(valid.operation(), ConformanceOperation::WorkspaceTest);
    assert_eq!(valid.operation().as_str(), "workspace-test");

    let rejected = CaseManifest::from_str(
        r#"
id = "V1-RUNTIME-workspace-test-rejected"
rule = "V1-RUNTIME"
profile = "interpreter-v1"
operation = "workspace-test"

[inputs]
project = "tree/project.vibon"
tree = "tree"

[expect]
accepted = false
interpreter = { result = "result.vibon" }

[[expect.diagnostics]]
code = "@tool.unavailable"
level = "@error"
source = "tree/tests/math.vib"
span = [0, 0]
"#,
    )
    .expect("rejected workspace-test still records the canonical result");
    assert_eq!(rejected.operation(), ConformanceOperation::WorkspaceTest);

    for invalid in [
        r#"
id = "V1-RUNTIME-workspace-test-missing-snapshot"
rule = "V1-RUNTIME"
profile = "interpreter-v1"
operation = "workspace-test"
[inputs]
project = "tree/project.vibon"
tree = "tree"
[expect]
accepted = false
"#,
        r#"
id = "V1-RUNTIME-workspace-test-audit-snapshot"
rule = "V1-RUNTIME"
profile = "interpreter-v1"
operation = "workspace-test"
[inputs]
project = "tree/project.vibon"
tree = "tree"
[expect]
accepted = true
interpreter = { result = "result.vibon", audit_trace = "audit.vibon" }
"#,
        r#"
id = "V1-RUNTIME-workspace-test-wrong-profile"
rule = "V1-RUNTIME"
profile = "static-v1"
operation = "workspace-test"
[inputs]
project = "tree/project.vibon"
tree = "tree"
[expect]
accepted = true
interpreter = { result = "result.vibon" }
"#,
        r#"
id = "V1-RUNTIME-workspace-test-wrong-binding"
rule = "V1-RUNTIME"
profile = "interpreter-v1"
operation = "workspace-test"
[inputs]
project = "other/project.vibon"
tree = "tree"
[expect]
accepted = true
interpreter = { result = "result.vibon" }
"#,
        r#"
id = "V1-RUNTIME-workspace-test-source-input"
rule = "V1-RUNTIME"
profile = "interpreter-v1"
operation = "workspace-test"
[inputs]
source = "source.vib"
project = "tree/project.vibon"
tree = "tree"
[expect]
accepted = true
interpreter = { result = "result.vibon" }
"#,
    ] {
        assert!(CaseManifest::from_str(invalid).is_err());
    }
}

#[test]
fn workspace_test_handler_compares_authored_outcomes_and_empty_item_traces() {
    let corpus_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("conformance/cases");
    let corpus = Corpus::discover(corpus_root).expect("repository corpus");
    let handler = InterpreterV1WorkspaceTestHandler;
    for case_id in [
        "V1-RUNTIME-workspace-test-passing",
        "V1-RUNTIME-workspace-test-failure",
        "V1-PROJECT-workspace-test-invalid-missing-assert-import",
        "V1-RUNTIME-workspace-test-unavailable-assertion",
        "V1-RUNTIME-workspace-test-empty",
    ] {
        let case = corpus
            .cases()
            .iter()
            .find(|case| case.manifest().id() == case_id)
            .unwrap_or_else(|| panic!("missing case {case_id}"));
        assert!(handler.can_run(case), "{case_id}");
        let observation = handler.run(case).expect("workspace-test handler");
        assert_eq!(
            observation.accepted,
            case.manifest().expectations.accepted,
            "{case_id}"
        );
        let expected_path = case
            .manifest()
            .expectations
            .interpreter
            .as_ref()
            .and_then(|execution| execution.result.as_deref())
            .expect("workspace-test result snapshot");
        let expected = case.read_file(expected_path).expect("read result snapshot");
        let actual = observation.interpreter.expect("test-run observation");
        assert_eq!(
            actual.result.as_deref(),
            Some(expected.as_str()),
            "{case_id}"
        );
        assert!(actual.audit_trace.is_empty(), "{case_id}");
    }
}
