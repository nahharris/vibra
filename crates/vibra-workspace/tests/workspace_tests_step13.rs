//! Workspace scope boundaries for the Step 13 test unit.

#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use vibra_diagnostics::DiagnosticCode;
use vibra_workspace::{
    WorkspaceSnapshot,
    semantic::{CheckStatus, TestSelector},
};

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

struct TempProject(PathBuf);

impl TempProject {
    fn new(label: &str, sources: &[(&str, &str)]) -> Self {
        Self::with_project(
            label,
            "(record format: @project.v1 package: (record name: \"demo\" version: \"0.1.0\") targets: (array (record name: @app kind: @bin root: \"src/app\" entry: @app.main.execute effects: (array))) dependencies: (map))\n",
            sources,
        )
    }

    fn with_project(
        label: &str,
        project_document: &str,
        sources: &[(&str, &str)],
    ) -> Self {
        let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let root = fs::canonicalize(std::env::temp_dir())
            .expect("canonical temp directory")
            .join(format!(
                "vibra-step13-{label}-{}-{serial}",
                std::process::id()
            ));
        fs::create_dir_all(&root).expect("create project root");
        fs::write(root.join("project.vibon"), project_document)
            .expect("write project marker");
        for (relative, source) in sources {
            let destination = root.join(relative);
            fs::create_dir_all(destination.parent().expect("source parent"))
                .expect("create source parent");
            fs::write(destination, source).expect("write source");
        }
        Self(root)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn target_imports_cannot_resolve_reserved_test_modules() {
    let project = TempProject::new(
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
    let project = TempProject::new(
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
    let project = TempProject::new(
        "target-assertion-import",
        &[(
            "src/app/main.vib",
            "(import assert @std.assert)\n(defn execute () void void)\n",
        )],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let verification =
        vibra_types::verify_bootstrap().expect("signed bootstrap verification");

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
    let project = TempProject::new(
        "target-assertion-reference",
        &[(
            "src/app/main.vib",
            "(import assert @std.assert)\n(defn execute () void (assert.true true))\n",
        )],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let verification =
        vibra_types::verify_bootstrap().expect("signed bootstrap verification");

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
    let project = TempProject::with_project(
        "local-assert-spoof",
        "(record format: @project.v1 package: (record name: \"demo\" version: \"0.1.0\") targets: (array (record name: @std kind: @bin root: \"src/std\" entry: @std.assert.execute effects: (array))) dependencies: (map))\n",
        &[(
            "src/std/assert.vib",
            "(defn equal-i32 (left i32 right i32) void void)\n(defn execute () void (equal-i32 1i32 2i32))\n",
        )],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let verification =
        vibra_types::verify_bootstrap().expect("signed bootstrap verification");
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
    let project = TempProject::new(
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
fn test_bootstrap_query_scopes_verified_assertions_to_selected_test_closure() {
    let project = TempProject::new(
        "test-bootstrap-query",
        &[
            ("src/app/main.vib", "(defn execute () void void)\n"),
            (
                "tests/helpers.vib",
                "(import assert @std.assert)\n(defn check () void visibility: @public (assert.true true))\n",
            ),
            (
                "tests/math.vib",
                "(import helpers @tests.helpers)\n(test \"uses-helper\" (helpers.check))\n",
            ),
            ("tests/plain.vib", "(test \"no-bootstrap\" void)\n"),
        ],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let helper_test = TestSelector::parse("@tests.math::\"uses-helper\"")
        .expect("canonical test selector");
    let plain_test = TestSelector::parse("@tests.plain::\"no-bootstrap\"")
        .expect("canonical test selector");

    assert!(
        snapshot
            .requires_test_bootstrap_verification(None)
            .expect("all tests")
    );
    assert!(
        snapshot
            .requires_test_bootstrap_verification(Some(&helper_test))
            .expect("selected test closure")
    );
    assert!(
        !snapshot
            .requires_test_bootstrap_verification(Some(&plain_test))
            .expect("selected test closure")
    );
    assert!(
        snapshot
            .requires_bootstrap_verification()
            .expect("whole-snapshot query includes tests")
    );
}

#[test]
fn test_import_bootstrap_requires_verified_check_and_run_without_becoming_target_work()
{
    let project = TempProject::new(
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

    assert!(
        snapshot
            .requires_bootstrap_verification()
            .expect("whole-snapshot bootstrap query"),
        "test import must request signed bootstrap verification"
    );

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

    let verification =
        vibra_types::verify_bootstrap().expect("signed bootstrap verification");
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
