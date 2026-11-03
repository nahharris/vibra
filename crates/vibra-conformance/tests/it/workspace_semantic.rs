//! Manifest requirements for the workspace-check and workspace-run operations.
//! The handlers' results are the expectations of the corpus cases
//! `V1-PROJECT-workspace-check-*` and `V1-RUNTIME-workspace-run-*`.

use vibra_conformance::{CaseManifest, ConformanceOperation};

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
