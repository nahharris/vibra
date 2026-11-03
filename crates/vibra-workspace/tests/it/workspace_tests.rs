//! Workspace scope boundaries for the Step 13 test unit.

use vibra_diagnostics::DiagnosticCode;
use vibra_workspace::{WorkspaceSnapshot, semantic::CheckStatus};

use crate::support::TempDir;

#[test]
fn target_imports_cannot_resolve_reserved_test_modules() {
    let project = TempDir::demo(
        "target-import",
        &[
            (
                "src/app/main.vib",
                "(import private @tests.helper)\n(defn execute () void void)\n",
            ),
            (
                "tests/helper.vib",
                "(defn hidden () void visibility: @public (do))\n",
            ),
        ],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");

    let resolved = snapshot.resolve().expect("resolution result");

    assert!(resolved.diagnostics().iter().any(|diagnostic| {
        diagnostic.code() == DiagnosticCode::ModuleUnknownPath
            && diagnostic.source_id() == Some("src/app/main.vib")
    }));
}

#[test]
fn target_check_ignores_unrelated_test_body_errors() {
    let project = TempDir::demo(
        "check-scope",
        &[
            ("src/app/main.vib", "(defn execute () void void)\n"),
            ("tests/broken.vib", "(test \"broken\" missing)\n"),
        ],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");

    let checked = vibra_workspace::semantic::check_all(&snapshot);

    assert_eq!(
        checked.status(),
        CheckStatus::Accepted,
        "{:?}",
        checked.diagnostics()
    );
    assert!(checked.diagnostics().is_empty());
}

#[test]
fn target_may_import_assertion_module_without_referencing_it() {
    let project = TempDir::demo(
        "target-assertion-import",
        &[(
            "src/app/main.vib",
            "(import assert @std.assert)\n(defn execute () void void)\n",
        )],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let verification = vibra_types::load_stdlib().expect("embedded standard library");

    let resolved = snapshot
        .resolve_with_bootstrap(&verification)
        .expect("resolution result");

    assert!(
        resolved.diagnostics().is_empty(),
        "{:?}",
        resolved.diagnostics()
    );
    let checked = vibra_workspace::semantic::check_all_with_bootstrap(
        &snapshot,
        Some(&verification),
    );
    assert_eq!(
        checked.status(),
        CheckStatus::Accepted,
        "{:?}",
        checked.diagnostics()
    );
    let target = snapshot
        .project()
        .project()
        .targets()
        .first()
        .expect("binary target");
    let run = vibra_workspace::semantic::run_target_with_bootstrap(
        &snapshot,
        target,
        Some(&verification),
    );
    assert!(run.outcome().is_some(), "{:?}", run.check().diagnostics());
}

#[test]
fn target_reference_to_assertion_is_unavailable_at_the_reference() {
    let project = TempDir::demo(
        "target-assertion-reference",
        &[(
            "src/app/main.vib",
            "(import assert @std.assert)\n(defn execute () void (assert.true true))\n",
        )],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let verification = vibra_types::load_stdlib().expect("embedded standard library");

    let checked = vibra_workspace::semantic::check_all_with_bootstrap(
        &snapshot,
        Some(&verification),
    );

    assert_eq!(
        checked.status(),
        CheckStatus::Unavailable,
        "{:?}",
        checked.diagnostics()
    );
    let expected_start =
        "(import assert @std.assert)\n(defn execute () void ".len() + 1;
    assert!(
        checked.diagnostics().iter().any(|diagnostic| {
            diagnostic.code() == DiagnosticCode::ToolUnavailable
                && diagnostic.source_id() == Some("src/app/main.vib")
                && diagnostic.primary_span()
                    == vibra_diagnostics::ByteSpan::new(
                        expected_start,
                        expected_start + 11,
                    )
        }),
        "{:?}",
        checked.diagnostics()
    );
}

#[test]
fn local_std_assert_functions_are_not_promoted_to_trusted_assertions() {
    let project = TempDir::project(
        "local-assert-spoof",
        "(record format: @project.v1 package: (record name: \"demo\" version: \"0.1.0\") targets: (array (record name: @std kind: @bin root: \"src/std\" entry: @std.assert.execute effects: (array))) dependencies: (dict))\n",
        &[(
            "src/std/assert.vib",
            "(defn equal (left i32 right i32) void void)\n(defn execute () void (equal 1i32 2i32))\n",
        )],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let verification = vibra_types::load_stdlib().expect("embedded standard library");
    let target = snapshot
        .project()
        .project()
        .targets()
        .first()
        .expect("local std binary target");

    let run = vibra_workspace::semantic::run_target_with_bootstrap(
        &snapshot,
        target,
        Some(&verification),
    );

    assert_eq!(run.check().status(), CheckStatus::Accepted);
    assert!(matches!(
        run.outcome(),
        Some(vibra_workspace::semantic::RunOutcome::Program(_))
    ));
}

#[test]
fn test_declaration_outside_tests_is_unavailable_and_not_runnable() {
    let project = TempDir::demo(
        "test-declaration-in-target",
        &[(
            "src/app/main.vib",
            "(defn execute () void void)\n(test \"hidden\" void)\n",
        )],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let target = snapshot
        .project()
        .project()
        .targets()
        .first()
        .expect("binary target");

    let run = vibra_workspace::semantic::run_target(&snapshot, target);

    assert_eq!(run.check().status(), CheckStatus::Unavailable);
    assert!(run.check().diagnostics().iter().any(|diagnostic| {
        diagnostic.code() == DiagnosticCode::ToolUnavailable
            && diagnostic.source_id() == Some("src/app/main.vib")
    }));
    assert!(run.outcome().is_none());
}

#[test]
fn test_import_bootstrap_requires_verified_check_and_run_without_becoming_target_work()
{
    let project = TempDir::demo(
        "test-import-requires-global-bootstrap",
        &[
            ("src/app/main.vib", "(defn execute () void void)\n"),
            (
                "tests/math.vib",
                "(import assert @std.assert)\n(test \"works\" (assert.true true))\n",
            ),
        ],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");

    let target = snapshot
        .project()
        .project()
        .targets()
        .first()
        .expect("binary target");
    let unverified_check = vibra_workspace::semantic::check_target(&snapshot, target);
    assert_eq!(unverified_check.status(), CheckStatus::Unavailable);
    assert!(unverified_check.diagnostics().iter().any(|diagnostic| {
        diagnostic.code() == DiagnosticCode::ToolUnavailable
            && diagnostic.source_id() == Some("tests/math.vib")
    }));
    let unverified_run = vibra_workspace::semantic::run_target(&snapshot, target);
    assert_eq!(unverified_run.check().status(), CheckStatus::Unavailable);
    assert!(unverified_run.outcome().is_none());

    let verification = vibra_types::load_stdlib().expect("embedded standard library");
    let checked = vibra_workspace::semantic::check_target_with_bootstrap(
        &snapshot,
        target,
        Some(&verification),
    );
    assert_eq!(
        checked.status(),
        CheckStatus::Accepted,
        "{:?}",
        checked.diagnostics()
    );
    let run = vibra_workspace::semantic::run_target_with_bootstrap(
        &snapshot,
        target,
        Some(&verification),
    );
    assert_eq!(run.check().status(), CheckStatus::Accepted);
    assert!(matches!(
        run.outcome(),
        Some(vibra_workspace::semantic::RunOutcome::Program(_))
    ));
}
