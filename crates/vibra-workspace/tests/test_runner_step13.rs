//! Public workspace test-runner contracts for Step 13.

#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use vibra_types::load_stdlib;
use vibra_workspace::{
    WorkspaceSnapshot,
    semantic::{TestItemStatus, TestSelector, TestSuiteStatus, run_tests},
};

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

struct TempProject(PathBuf);

impl TempProject {
    fn new(label: &str, sources: &[(&str, &str)]) -> Self {
        let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let root = fs::canonicalize(std::env::temp_dir())
            .expect("canonical temp directory")
            .join(format!(
                "vibra-step13-{label}-{}-{serial}",
                std::process::id()
            ));
        fs::create_dir_all(&root).expect("create project root");
        fs::write(
            root.join("project.vibon"),
            "(record format: @project.v1 package: (record name: \"demo\" version: \"0.1.0\") targets: (array (record name: @app kind: @bin root: \"src/app\" entry: @app.main.execute effects: (array))) dependencies: (dict))\n",
        )
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

fn verified_bootstrap() -> vibra_types::Stdlib {
    load_stdlib().expect("embedded standard library")
}

#[test]
fn selected_tests_use_verified_assertions_and_record_structured_failure() {
    let project = TempProject::new(
        "assertions",
        &[
            ("src/app/main.vib", "(defn execute () void void)\n"),
            (
                "tests/math/add.vib",
                "(import assert @std.assert)\n(test \"fails\" (assert.equal \"expected\" \"actual\"))\n(test \"passes-after-failure\" (assert.equal 4i32 4i32))\n",
            ),
        ],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let verification = verified_bootstrap();

    let result = run_tests(&snapshot, None, Some(&verification));

    assert_eq!(result.status(), TestSuiteStatus::TestFailed);
    assert_eq!(result.selected(), 2);
    assert_eq!(result.passed(), 1);
    assert_eq!(result.failed(), 1);
    let failure = result.items()[0].failure().expect("assertion failure");
    assert_eq!(result.items()[0].status(), TestItemStatus::AssertionFailed);
    assert_eq!(result.items()[1].status(), TestItemStatus::Passed);
    assert_eq!(
        result.items()[1].name(),
        "@tests.math.add::\"passes-after-failure\""
    );
    assert_eq!(failure.assertion(), "@std.assert.equal");
    assert_eq!(failure.expected(), "\"expected\"");
    assert_eq!(failure.actual(), "\"actual\"");
    assert_eq!(failure.source_id(), "tests/math/add.vib");
    assert!(
        result
            .items()
            .iter()
            .all(|item| item.audit_trace().is_empty())
    );
}

#[test]
fn unknown_explicit_selector_is_invalid_while_omitted_empty_suite_succeeds() {
    let empty = TempProject::new(
        "empty-suite",
        &[("src/app/main.vib", "(defn execute () void void)\n")],
    );
    let snapshot = WorkspaceSnapshot::load(empty.path()).expect("snapshot");
    let verification = verified_bootstrap();

    let result = run_tests(&snapshot, None, Some(&verification));
    assert_eq!(result.status(), TestSuiteStatus::Ok);
    assert_eq!(result.selected(), 0);
    assert!(result.items().is_empty());

    let selector =
        TestSelector::parse("@tests.math::\"missing\"").expect("canonical selector");
    let missing = run_tests(&snapshot, Some(&selector), Some(&verification));
    assert_eq!(missing.status(), TestSuiteStatus::InvalidInput);
    assert!(missing.items().is_empty());
    assert!(missing.diagnostics().is_empty());
}

#[test]
fn duplicate_decoded_names_in_one_module_make_all_selected_items_invalid() {
    let project = TempProject::new(
        "duplicates",
        &[
            ("src/app/main.vib", "(defn execute () void void)\n"),
            (
                "tests/math.vib",
                "(import assert @std.assert)\n(test \"same\" (assert.true true))\n(test \"same\" (assert.true true))\n",
            ),
        ],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let verification = verified_bootstrap();

    let result = run_tests(&snapshot, None, Some(&verification));

    assert_eq!(result.status(), TestSuiteStatus::Diagnostics);
    assert_eq!(result.selected(), 2);
    assert!(
        result
            .items()
            .iter()
            .all(|item| item.status() == TestItemStatus::Invalid)
    );
    assert!(result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code() == vibra_diagnostics::DiagnosticCode::NameRedeclaration
    }));
}

#[test]
fn unavailable_helper_is_attributed_only_to_tests_that_reference_it() {
    let project = TempProject::new(
        "dependency-scoped-unavailable",
        &[
            ("src/app/main.vib", "(defn execute () void void)\n"),
            (
                "tests/helpers.vib",
                "(defn unavailable () i32 visibility: @public effects: (io.stdout) 1i32)\n",
            ),
            (
                "tests/math.vib",
                "(import assert @std.assert)\n(import helpers @tests.helpers)\n(test \"depends\" (do (helpers.unavailable) void))\n(test \"independent\" (assert.true true))\n",
            ),
        ],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let verification = verified_bootstrap();

    let result = run_tests(&snapshot, None, Some(&verification));

    assert_eq!(result.status(), TestSuiteStatus::Unavailable);
    assert_eq!(result.selected(), 2);
    assert_eq!(result.items()[0].status(), TestItemStatus::Unavailable);
    assert!(result.items()[0].diagnostics().iter().any(|diagnostic| {
        diagnostic.code() == vibra_diagnostics::DiagnosticCode::ToolUnavailable
    }));
    assert_eq!(result.items()[1].status(), TestItemStatus::Passed);
    assert!(result.items()[1].diagnostics().is_empty());
}

#[test]
fn unsupported_test_form_is_unavailable_at_its_reference() {
    let project = TempProject::new(
        "unsupported-form",
        &[
            ("src/app/main.vib", "(defn execute () void void)\n"),
            (
                "tests/math.vib",
                "(import assert @std.assert)\n(test \"generic\" (let - (lambda () void effects: (audit) void)) (assert.true true))\n",
            ),
        ],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let verification = verified_bootstrap();

    let result = run_tests(&snapshot, None, Some(&verification));

    assert_eq!(result.status(), TestSuiteStatus::Unavailable);
    assert_eq!(result.items()[0].status(), TestItemStatus::Unavailable);
    assert!(result.items()[0].diagnostics().iter().any(|diagnostic| {
        diagnostic.code() == vibra_diagnostics::DiagnosticCode::ToolUnavailable
    }));
}

#[test]
fn missing_verified_assertion_bootstrap_is_unavailable_not_a_type_error() {
    let project = TempProject::new(
        "missing-assert-bootstrap",
        &[
            ("src/app/main.vib", "(defn execute () void void)\n"),
            (
                "tests/math.vib",
                "(import assert @std.assert)\n(test \"needs-bootstrap\" (assert.true true))\n",
            ),
        ],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");

    let result = run_tests(&snapshot, None, None);

    assert_eq!(result.status(), TestSuiteStatus::Unavailable);
    assert_eq!(result.items()[0].status(), TestItemStatus::Unavailable);
    assert!(result.items()[0].diagnostics().iter().any(|diagnostic| {
        diagnostic.code() == vibra_diagnostics::DiagnosticCode::ToolUnavailable
    }));
}

#[test]
fn ordinary_type_errors_in_selected_import_closure_block_all_tests() {
    let project = TempProject::new(
        "static-type-error",
        &[
            ("src/app/main.vib", "(defn execute () void void)\n"),
            (
                "tests/math.vib",
                "(import assert @std.assert)\n(test \"broken\" (assert.true 1i32))\n(test \"would-pass\" (assert.true true))\n",
            ),
        ],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let verification = verified_bootstrap();

    let result = run_tests(&snapshot, None, Some(&verification));

    assert_eq!(result.status(), TestSuiteStatus::Diagnostics);
    assert_eq!(result.selected(), 2);
    assert!(
        result
            .items()
            .iter()
            .all(|item| item.status() == TestItemStatus::Invalid)
    );
    assert!(result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code() == vibra_diagnostics::DiagnosticCode::TypeArgumentMismatch
    }));
}

#[test]
fn tests_can_call_public_target_helpers_through_ordinary_imports() {
    let project = TempProject::new(
        "target-public-import",
        &[
            ("src/app/main.vib", "(defn execute () void void)\n"),
            (
                "src/app/api.vib",
                "(defn answer () i32 visibility: @public 42i32)\n",
            ),
            (
                "tests/math.vib",
                "(import assert @std.assert)\n(import api @app.api)\n(test \"uses-public-target\" (assert.equal (api.answer) 42i32))\n",
            ),
        ],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let verification = verified_bootstrap();

    let result = run_tests(&snapshot, None, Some(&verification));

    assert_eq!(result.status(), TestSuiteStatus::Ok);
    assert_eq!(result.passed(), 1);
    assert_eq!(result.items()[0].status(), TestItemStatus::Passed);
}

#[test]
fn missing_required_assertion_import_is_an_ordinary_invalidating_error() {
    let source = "(test \"missing-import\" void)\n";
    let project = TempProject::new(
        "missing-required-assert-import",
        &[
            ("src/app/main.vib", "(defn execute () void void)\n"),
            ("tests/math.vib", source),
        ],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let verification = verified_bootstrap();

    let result = run_tests(&snapshot, None, Some(&verification));

    assert_eq!(result.status(), TestSuiteStatus::Diagnostics);
    assert_eq!(result.items().len(), 1);
    assert_eq!(result.items()[0].status(), TestItemStatus::Invalid);
    let [diagnostic] = result.diagnostics() else {
        panic!("one missing import diagnostic");
    };
    assert_eq!(
        diagnostic.code(),
        vibra_diagnostics::DiagnosticCode::ModuleMissingRequiredImport
    );
    assert_eq!(diagnostic.source_id(), Some("tests/math.vib"));
    let start = source.find("\"missing-import\"").expect("name literal");
    assert_eq!(
        diagnostic.primary_span(),
        vibra_diagnostics::ByteSpan::new(start, start + "\"missing-import\"".len())
    );
}

#[test]
fn warnings_are_reported_without_blocking_test_execution() {
    let project = TempProject::new(
        "nonblocking-warning",
        &[
            ("src/app/main.vib", "(defn execute () void void)\n"),
            (
                "tests/math.vib",
                "(import assert @std.assert)\n(defn choose (fallback i32) i32 labelled: (first i32 7i32 second i32 8i32) first)\n(test \"warning-still-runs\" (assert.equal (choose 3i32 second: 11i32 first: 9i32) 9i32))\n",
            ),
        ],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let verification = verified_bootstrap();

    let result = run_tests(&snapshot, None, Some(&verification));

    assert_eq!(result.status(), TestSuiteStatus::Ok);
    assert_eq!(result.passed(), 1);
    assert!(result.diagnostics().iter().any(|diagnostic| {
        diagnostic.code() == vibra_diagnostics::DiagnosticCode::StyleArgumentOrder
            && diagnostic.level() == vibra_diagnostics::Level::Warning
    }));
    assert!(result.items()[0].diagnostics().iter().any(|diagnostic| {
        diagnostic.code() == vibra_diagnostics::DiagnosticCode::StyleArgumentOrder
            && diagnostic.level() == vibra_diagnostics::Level::Warning
    }));
}

#[test]
fn a_test_that_exhausts_its_budget_ends_the_run_with_the_host_event_and_no_counts() {
    use vibra_workspace::semantic::{MemoryBudget, run_tests_with_budget};

    let project = TempProject::new(
        "memory-exhausted",
        &[
            ("src/app/main.vib", "(defn execute () void void)\n"),
            (
                "tests/deep/forever.vib",
                "(import assert @std.assert)
(defn forever (count u64) u64\n  (match (u64.add-checked (forever count) 1u64)\n    (result.ok total) total\n    (result.err -) 0u64))\n(test \"first\" (assert.true true))\n(test \"never ends\" (assert.equal (forever 0u64) 0u64))\n",
            ),
        ],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let verification = verified_bootstrap();
    let budget = MemoryBudget::new(2 * 1024 * 1024);

    let result = run_tests_with_budget(&snapshot, None, Some(&verification), budget);

    assert_eq!(result.status(), TestSuiteStatus::OperationalFailure);
    assert_eq!(result.selected(), 0, "a host event reports no counts");
    assert_eq!(result.passed(), 0);
    assert_eq!(result.failed(), 0);
    assert!(result.items().is_empty());
    let [diagnostic] = result.diagnostics() else {
        panic!("one unlocated diagnostic: {:?}", result.diagnostics());
    };
    assert_eq!(
        diagnostic.code(),
        vibra_diagnostics::DiagnosticCode::RuntimeMemoryExhausted
    );
    assert_eq!(diagnostic.source_id(), None);
    assert_eq!(
        diagnostic.primary_span(),
        vibra_diagnostics::ByteSpan::empty_at(0)
    );

    // The same suite under the default budget stops the same way, and a
    // following suite without the recursion runs normally in this process.
    let result = run_tests(&snapshot, None, Some(&verification));
    assert_eq!(result.status(), TestSuiteStatus::OperationalFailure);
    let passing = TempProject::new(
        "memory-recovered",
        &[
            ("src/app/main.vib", "(defn execute () void void)\n"),
            (
                "tests/deep/fine.vib",
                "(import assert @std.assert)\n(test \"first\" (assert.true true))\n",
            ),
        ],
    );
    let snapshot = WorkspaceSnapshot::load(passing.path()).expect("snapshot");
    let result = run_tests(&snapshot, None, Some(&verification));
    assert_eq!(result.status(), TestSuiteStatus::Ok);
    assert_eq!(result.passed(), 1);
}

#[test]
fn a_test_body_ending_in_a_source_call_reuses_its_activation() {
    let project = TempProject::new(
        "tail-test-body",
        &[
            ("src/app/main.vib", "(defn execute () void void)\n"),
            (
                "tests/tail/loop.vib",
                "(import assert @std.assert)
(defn finish () void (do))\n(defn spin (count u64) void\n  (if (u64.equal count 0u64) (finish) (spin (lower count))))\n(defn lower (count u64) u64\n  (match (u64.sub-checked count 1u64)\n    (result.ok value) value\n    (result.err -) 0u64))\n(test \"one call\" (finish))\n(test \"a loop of a hundred thousand tail calls\" (spin 100000u64))\n",
            ),
        ],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let verification = verified_bootstrap();

    let result = run_tests(&snapshot, None, Some(&verification));

    assert_eq!(
        result.status(),
        TestSuiteStatus::Ok,
        "{:?}",
        result.diagnostics()
    );
    assert_eq!(result.selected(), 2);
    assert_eq!(result.passed(), 2);
}
