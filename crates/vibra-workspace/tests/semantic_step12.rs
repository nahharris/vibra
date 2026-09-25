//! Workspace-level Step 12 checking and pure execution contracts.

#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use vibra_diagnostics::{ByteSpan, DiagnosticCode};
use vibra_workspace::{
    WorkspaceSnapshot,
    semantic::{CheckStatus, RunOutcome},
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

    fn with_project(label: &str, project: &str, sources: &[(&str, &str)]) -> Self {
        let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let root = fs::canonicalize(std::env::temp_dir())
            .expect("canonical temp directory")
            .join(format!(
                "vibra-step12-{label}-{}-{serial}",
                std::process::id()
            ));
        fs::create_dir_all(&root).expect("create project root");
        fs::write(root.join("project.vibon"), project).expect("write project marker");
        for (path, source) in sources {
            let destination = root.join(path);
            fs::create_dir_all(destination.parent().expect("source parent"))
                .expect("create module directory");
            fs::write(destination, source).expect("write module source");
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
fn resolves_imported_calls_and_constants_before_running_a_private_non_main_entry() {
    let project = TempProject::new(
        "multi-module",
        &[
            (
                "src/app/main.vib",
                "(import util @app.util)\n(defn execute () void (let - util.base (util.perform)))\n",
            ),
            (
                "src/app/util.vib",
                "(def base i32 7i32 visibility: @public)\n(defn perform () void visibility: @public (do))\n",
            ),
        ],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");

    let checked = vibra_workspace::semantic::check_all(&snapshot);

    let target = snapshot
        .project()
        .project()
        .targets()
        .first()
        .expect("binary target");

    assert_eq!(
        checked.status(),
        CheckStatus::Accepted,
        "{:?}",
        checked.diagnostics()
    );
    assert!(checked.diagnostics().is_empty());
    assert!(checked.program_for_target(target).is_some());
    let run = vibra_workspace::semantic::run_target(&snapshot, target);
    assert_eq!(run.check().status(), CheckStatus::Accepted);
    assert!(matches!(run.outcome(), Some(RunOutcome::Program(_))));
}

#[test]
fn higher_order_helper_before_the_entry_accepts_its_known_lambda_argument() {
    let project = TempProject::new(
        "higher-order-entry",
        &[(
            "src/app/main.vib",
            "(defn invoke (callback (fn () void)) void (callback))\n(defn execute () void (invoke (lambda () void (do))))\n",
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

    assert_eq!(
        run.check().status(),
        CheckStatus::Accepted,
        "{:?}",
        run.check().diagnostics()
    );
    assert!(matches!(run.outcome(), Some(RunOutcome::Program(_))));
}

#[test]
fn an_unused_imported_higher_order_helper_does_not_block_the_entry() {
    let project = TempProject::with_project(
        "unused-imported-higher-order-helper",
        "(record format: @project.v1 package: (record name: \"demo\" version: \"0.1.0\") targets: (array (record name: @app kind: @bin root: \"src/app\" entry: @app.main.execute effects: (array)) (record name: @util kind: @lib root: \"src/util\")) dependencies: (map))\n",
        &[
            (
                "src/app/main.vib",
                "(import util @util.api)\n(defn execute () void (do))\n",
            ),
            (
                "src/util/api.vib",
                "(defn apply (callback (fn () i32)) i32 visibility: @public (callback))\n(defn dead (callback (fn () i32)) i32 (apply callback))\n",
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

    let run = vibra_workspace::semantic::run_target(&snapshot, target);

    assert_eq!(
        run.check().status(),
        CheckStatus::Accepted,
        "{:?}",
        run.check().diagnostics()
    );
    assert!(matches!(run.outcome(), Some(RunOutcome::Program(_))));
}

#[test]
fn an_uninvoked_higher_order_closure_does_not_block_the_entry() {
    let project = TempProject::new(
        "uninvoked-higher-order-closure",
        &[(
            "src/app/main.vib",
            "(defn execute () void (let unused (lambda (f (fn () void)) void (f)) (do)))\n",
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

    assert_eq!(
        run.check().status(),
        CheckStatus::Accepted,
        "{:?}",
        run.check().diagnostics()
    );
    assert!(matches!(run.outcome(), Some(RunOutcome::Program(_))));
}

#[test]
fn an_invoked_higher_order_closure_uses_its_known_callback_argument() {
    let project = TempProject::new(
        "invoked-higher-order-closure",
        &[(
            "src/app/main.vib",
            "(defn execute () void (let invoke (lambda (callback (fn () void)) void (callback)) (invoke (lambda () void (do)))))\n",
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

    assert_eq!(
        run.check().status(),
        CheckStatus::Accepted,
        "{:?}",
        run.check().diagnostics()
    );
    assert!(matches!(run.outcome(), Some(RunOutcome::Program(_))));
}

#[test]
fn checking_a_library_without_an_entry_still_rejects_initializer_cycles() {
    let project = TempProject::with_project(
        "library-initializer-cycle",
        "(record format: @project.v1 package: (record name: \"demo\" version: \"0.1.0\") targets: (array (record name: @util kind: @lib root: \"src/util\")) dependencies: (map))\n",
        &[(
            "src/util/main.vib",
            "(def first i32 second)\n(def second i32 first)\n",
        )],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let target = snapshot
        .project()
        .project()
        .targets()
        .first()
        .expect("library target");

    let checked = vibra_workspace::semantic::check_target(&snapshot, target);

    assert_eq!(checked.status(), CheckStatus::Diagnostics);
    assert!(checked.diagnostics().iter().any(|diagnostic| {
        diagnostic.code() == DiagnosticCode::TypeInitializerCycle
            && diagnostic.source_id() == Some("src/util/main.vib")
    }));
}

#[test]
fn initializer_cycle_diagnostic_points_into_disjoint_cycle() {
    let source = "(def unrelated i32 1i32)\n(def first i32 second)\n(def second i32 first)\n(defn execute () void (do))\n";
    let project = TempProject::new(
        "initializer-cycle-span-after-acyclic-global",
        &[("src/app/main.vib", source)],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let target = snapshot
        .project()
        .project()
        .targets()
        .first()
        .expect("binary target");

    let checked = vibra_workspace::semantic::check_target(&snapshot, target);
    let diagnostic = checked
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.code() == DiagnosticCode::TypeInitializerCycle)
        .expect("initializer-cycle diagnostic");
    let cycle_start = source.find("(def first").expect("first cycle declaration");
    let cycle_end = source[cycle_start..]
        .find('\n')
        .map(|offset| cycle_start + offset)
        .expect("cycle declaration end");

    assert_eq!(diagnostic.source_id(), Some("src/app/main.vib"));
    assert_eq!(
        diagnostic.primary_span(),
        ByteSpan::new(cycle_start, cycle_end)
    );
}

#[test]
fn global_initializer_may_call_a_terminating_recursive_helper() {
    let project = TempProject::new(
        "recursive-helper-global-initializer",
        &[(
            "src/app/main.vib",
            "(def value i32 (helper false))\n(defn helper (again bool) i32 (if again (helper false) 1i32))\n(defn execute () void (let - value (do)))\n",
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

    assert_eq!(
        run.check().status(),
        CheckStatus::Accepted,
        "{:?}",
        run.check().diagnostics()
    );
    assert!(matches!(run.outcome(), Some(RunOutcome::Program(_))));
}

#[test]
fn library_initializer_cycles_flow_through_higher_order_helpers() {
    let project = TempProject::with_project(
        "library-higher-order-initializer-cycle",
        "(record format: @project.v1 package: (record name: \"demo\" version: \"0.1.0\") targets: (array (record name: @util kind: @lib root: \"src/util\")) dependencies: (map))\n",
        &[(
            "src/util/main.vib",
            "(def value i32 (apply read))\n(defn apply (f (fn () i32)) i32 (f))\n(defn read () i32 value)\n",
        )],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let target = snapshot
        .project()
        .project()
        .targets()
        .first()
        .expect("library target");

    let checked = vibra_workspace::semantic::check_target(&snapshot, target);

    assert_eq!(checked.status(), CheckStatus::Diagnostics);
    assert!(checked.diagnostics().iter().any(|diagnostic| {
        diagnostic.code() == DiagnosticCode::TypeInitializerCycle
            && diagnostic.source_id() == Some("src/util/main.vib")
    }));
}

#[test]
fn library_initializer_does_not_execute_an_uninvoked_closure_body() {
    let project = TempProject::with_project(
        "library-uninvoked-closure-initializer",
        "(record format: @project.v1 package: (record name: \"demo\" version: \"0.1.0\") targets: (array (record name: @util kind: @lib root: \"src/util\")) dependencies: (map))\n",
        &[(
            "src/util/main.vib",
            "(def value i32 (let unused (lambda () i32 (read)) 0i32))\n(defn read () i32 value)\n",
        )],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let target = snapshot
        .project()
        .project()
        .targets()
        .first()
        .expect("library target");

    let checked = vibra_workspace::semantic::check_target(&snapshot, target);

    assert_eq!(
        checked.status(),
        CheckStatus::Accepted,
        "{:?}",
        checked.diagnostics()
    );
}

#[test]
fn library_without_initializers_can_expose_a_higher_order_helper() {
    let project = TempProject::with_project(
        "library-higher-order-helper",
        "(record format: @project.v1 package: (record name: \"demo\" version: \"0.1.0\") targets: (array (record name: @util kind: @lib root: \"src/util\")) dependencies: (map))\n",
        &[(
            "src/util/main.vib",
            "(defn apply (f (fn () i32)) i32 (f))\n",
        )],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let target = snapshot
        .project()
        .project()
        .targets()
        .first()
        .expect("library target");

    let checked = vibra_workspace::semantic::check_target(&snapshot, target);

    assert_eq!(checked.status(), CheckStatus::Accepted);
    assert!(checked.diagnostics().is_empty());
}

#[test]
fn an_error_in_an_unreachable_declaration_blocks_the_target_program() {
    let project = TempProject::new(
        "unreachable-error",
        &[(
            "src/app/main.vib",
            "(defn execute () void (do))\n(defn unused () i32 \"wrong type\")\n",
        )],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");

    let checked = vibra_workspace::semantic::check_all(&snapshot);

    let target = snapshot
        .project()
        .project()
        .targets()
        .first()
        .expect("binary target");

    assert_eq!(checked.status(), CheckStatus::Diagnostics);
    assert!(checked.program_for_target(target).is_none());
    assert!(checked.diagnostics().iter().any(|diagnostic| {
        diagnostic.code() == DiagnosticCode::TypeArgumentMismatch
            && diagnostic.source_id() == Some("src/app/main.vib")
    }));
    let run = vibra_workspace::semantic::run_target(&snapshot, target);
    assert!(run.outcome().is_none());
    assert_eq!(run.check().status(), CheckStatus::Diagnostics);
}

#[test]
fn checks_recursion_and_closures_in_every_module_without_running_check() {
    let project = TempProject::new(
        "recursion-closure",
        &[(
            "src/app/main.vib",
            "(defn first () i32 (second))\n(defn second () i32 (first))\n(defn execute () void (let captured 9i32 (let callback (lambda () i32 captured) (let - (callback) (do)))))\n",
        )],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let target = snapshot
        .project()
        .project()
        .targets()
        .first()
        .expect("target");
    let checked = vibra_workspace::semantic::check_target(&snapshot, target);

    assert_eq!(
        checked.status(),
        CheckStatus::Accepted,
        "{:?}",
        checked.diagnostics()
    );
    assert!(checked.program_for_target(target).is_some());
}

#[test]
fn selected_target_checks_its_import_closure_and_ignores_an_unrelated_target() {
    let project_text = "(record format: @project.v1 package: (record name: \"demo\" version: \"0.1.0\") targets: (array (record name: @app kind: @bin root: \"src/app\" entry: @app.main.execute effects: (array)) (record name: @util kind: @lib root: \"src/util\") (record name: @other kind: @bin root: \"src/other\" entry: @other.main.execute effects: (array))) dependencies: (map))\n";
    let project = TempProject::with_project(
        "target-closure",
        project_text,
        &[
            (
                "src/app/main.vib",
                "(import util @util.helpers)\n(defn execute () void (util.perform))\n",
            ),
            (
                "src/util/helpers.vib",
                "(defn perform () void visibility: @public (do))\n",
            ),
            (
                "src/other/main.vib",
                "(defn execute () void (do))\n(defn unreachable () i32 \"bad\")\n",
            ),
        ],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let app = snapshot
        .project()
        .project()
        .targets()
        .iter()
        .find(|target| target.name().atom().value() == "app")
        .expect("app target");

    let selected = vibra_workspace::semantic::check_target(&snapshot, app);
    assert_eq!(
        selected.status(),
        CheckStatus::Accepted,
        "{:?}",
        selected.diagnostics()
    );
    assert!(selected.program_for_target(app).is_some());

    let all = vibra_workspace::semantic::check_all(&snapshot);
    assert_eq!(all.status(), CheckStatus::Diagnostics);
    assert!(all.diagnostics().iter().any(|diagnostic| {
        diagnostic.code() == DiagnosticCode::TypeArgumentMismatch
            && diagnostic.source_id() == Some("src/other/main.vib")
    }));
}

#[test]
fn an_error_in_a_transitively_imported_unit_blocks_execution() {
    let project_text = "(record format: @project.v1 package: (record name: \"demo\" version: \"0.1.0\") targets: (array (record name: @app kind: @bin root: \"src/app\" entry: @app.main.execute effects: (array)) (record name: @util kind: @lib root: \"src/util\")) dependencies: (map))\n";
    let project = TempProject::with_project(
        "closure-error",
        project_text,
        &[
            (
                "src/app/main.vib",
                "(import util @util.helpers)\n(defn execute () void (util.perform))\n",
            ),
            (
                "src/util/helpers.vib",
                "(defn perform () void visibility: @public (do))\n(defn unused () i32 \"bad\")\n",
            ),
        ],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let app = snapshot
        .project()
        .project()
        .targets()
        .first()
        .expect("app target");

    let run = vibra_workspace::semantic::run_target(&snapshot, app);

    assert_eq!(run.check().status(), CheckStatus::Diagnostics);
    assert!(run.outcome().is_none());
    assert!(run.check().diagnostics().iter().any(|diagnostic| {
        diagnostic.code() == DiagnosticCode::TypeArgumentMismatch
            && diagnostic.source_id() == Some("src/util/helpers.vib")
    }));
}

#[test]
fn verified_bootstrap_imports_keep_package_identity_and_execute_through_checked_ir() {
    let project = TempProject::new(
        "verified-stdlib",
        &[(
            "src/app/main.vib",
            "(import text @std.text)\n(defn execute () void (let - (text.length (text.concat \"a\" \"b\")) (do)))\n",
        )],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let target = snapshot
        .project()
        .project()
        .targets()
        .first()
        .expect("target");
    let verification = vibra_types::verify_bootstrap().expect("bootstrap verification");

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
    assert!(matches!(run.outcome(), Some(RunOutcome::Program(_))));

    let resolved = snapshot
        .resolve_with_bootstrap(&verification)
        .expect("resolved snapshot");
    let module = resolved
        .modules()
        .iter()
        .find(|module| module.source_id() == vibra_types::BOOTSTRAP_TEXT_SOURCE_ID)
        .expect("verified text module");
    assert_eq!(module.package().name(), "vibra-stdlib");
    assert_eq!(module.package().version(), "0.1.0");
}

#[test]
fn bootstrap_overlay_rejects_a_duplicate_project_source_identity() {
    let project_text = "(record format: @project.v1 package: (record name: \"demo\" version: \"0.1.0\") targets: (array (record name: @app kind: @bin root: \"src/app\" entry: @app.main.execute effects: (array)) (record name: @local kind: @lib root: \"stdlib/m2/src/std\")) dependencies: (map))\n";
    let project = TempProject::with_project(
        "bootstrap-source-id-collision",
        project_text,
        &[
            (
                "src/app/main.vib",
                "(import text @std.text)\n(defn execute () void (let - (text.length \"x\") (do)))\n",
            ),
            (
                "stdlib/m2/src/std/text.vib",
                "(defn local () i32 \"wrong type\")\n",
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
    let verification = vibra_types::verify_bootstrap().expect("bootstrap verification");

    let checked = vibra_workspace::semantic::check_target_with_bootstrap(
        &snapshot,
        target,
        Some(&verification),
    );

    assert_ne!(checked.status(), CheckStatus::Accepted);
    assert!(checked.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_atom() == "@module.source-id-collision"
    }));
}

#[test]
fn bootstrap_spelling_without_verification_is_unavailable_and_never_runs() {
    let project = TempProject::new(
        "unverified-stdlib",
        &[(
            "src/app/main.vib",
            "(import text @std.text)\n(defn execute () void (let - (text.length \"spoof\") (do)))\n",
        )],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let target = snapshot
        .project()
        .project()
        .targets()
        .first()
        .expect("target");

    let run = vibra_workspace::semantic::run_target(&snapshot, target);

    assert_eq!(run.check().status(), CheckStatus::Unavailable);
    assert!(run.outcome().is_none());
    assert!(run.check().diagnostics().iter().any(|diagnostic| {
        diagnostic.code() == DiagnosticCode::ToolUnavailable
            && diagnostic.source_id() == Some("src/app/main.vib")
    }));
}

#[test]
fn valid_deferred_result_entry_is_unavailable_without_syntax_or_signature_errors() {
    let project = TempProject::new(
        "deferred-result",
        &[(
            "src/app/main.vib",
            "(defn execute () (result void failure) (do))\n",
        )],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let target = snapshot
        .project()
        .project()
        .targets()
        .first()
        .expect("target");

    let checked = vibra_workspace::semantic::check_target(&snapshot, target);

    assert_eq!(checked.status(), CheckStatus::Unavailable);
    assert!(
        checked
            .diagnostics()
            .iter()
            .any(|diagnostic| { diagnostic.code() == DiagnosticCode::ToolUnavailable })
    );
    assert!(!checked.diagnostics().iter().any(|diagnostic| {
        matches!(
            diagnostic.code(),
            DiagnosticCode::SyntaxInvalidForm
                | DiagnosticCode::SyntaxInvalidAttribute
                | DiagnosticCode::ProjectInvalidEntrySignature
        )
    }));
}

#[test]
fn imported_std_modules_keep_the_verified_package_and_local_std_units_resolve_locally()
{
    let project_text = "(record format: @project.v1 package: (record name: \"demo\" version: \"0.1.0\") targets: (array (record name: @app kind: @bin root: \"src/app\" entry: @app.main.execute effects: (array)) (record name: @std kind: @lib root: \"src/std\")) dependencies: (map))\n";
    let project = TempProject::with_project(
        "exact-stdlib-overlay",
        project_text,
        &[
            (
                "src/app/main.vib",
                "(import text @std.text)\n(import assertions @std.assert)\n(import extra @std.extra)\n(defn execute () void (let - (text.length (extra.suffix \"a\")) (do)))\n",
            ),
            (
                "src/std/extra.vib",
                "(defn suffix (value str) str visibility: @public value)\n",
            ),
        ],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let target = snapshot
        .project()
        .project()
        .targets()
        .first()
        .expect("target");
    let verification = vibra_types::verify_bootstrap().expect("bootstrap verification");

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
    let resolved = snapshot
        .resolve_with_bootstrap(&verification)
        .expect("resolved snapshot");
    let imports = resolved
        .imports()
        .iter()
        .filter(|import| import.source_id() == "src/app/main.vib")
        .map(|import| {
            (
                import.written(),
                import.module().expect("resolved import").package().name(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(imports.get("std.text"), Some(&"vibra-stdlib"));
    assert_eq!(imports.get("std.assert"), Some(&"vibra-stdlib"));
    assert_eq!(imports.get("std.extra"), Some(&"demo"));
}

#[test]
fn an_exact_local_std_text_target_cannot_satisfy_the_reserved_bootstrap_import() {
    let project_text = "(record format: @project.v1 package: (record name: \"demo\" version: \"0.1.0\") targets: (array (record name: @app kind: @bin root: \"src/app\" entry: @app.main.execute effects: (array)) (record name: @std kind: @lib root: \"src/std\")) dependencies: (map))\n";
    let project = TempProject::with_project(
        "local-std-text",
        project_text,
        &[
            (
                "src/app/main.vib",
                "(import text @std.text)\n(defn execute () void (let - (text.length \"local\") (do)))\n",
            ),
            (
                "src/std/text.vib",
                "(defn length (value str) u64 visibility: @public 5u64)\n",
            ),
        ],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let target = snapshot
        .project()
        .project()
        .targets()
        .first()
        .expect("target");
    let resolved = snapshot.resolve().expect("resolved snapshot");
    assert!(resolved.imports()[0].module().is_none());

    let checked = vibra_workspace::semantic::check_target(&snapshot, target);

    assert_eq!(checked.status(), CheckStatus::Unavailable);
    assert!(checked.program_for_target(target).is_none());
    assert!(checked.diagnostics().iter().any(|diagnostic| {
        diagnostic.code() == DiagnosticCode::ToolUnavailable
            && diagnostic.source_id() == Some("src/app/main.vib")
    }));
}

#[test]
fn an_unrelated_reserved_bootstrap_import_blocks_an_explicit_target() {
    let project_text = "(record format: @project.v1 package: (record name: \"demo\" version: \"0.1.0\") targets: (array (record name: @app kind: @bin root: \"src/app\" entry: @app.main.execute effects: (array)) (record name: @other kind: @lib root: \"src/other\")) dependencies: (map))\n";
    let project = TempProject::with_project(
        "global-bootstrap-requirement",
        project_text,
        &[
            ("src/app/main.vib", "(defn execute () void (do))\n"),
            (
                "src/other/module.vib",
                "(import text @std.text)\n(defn broken () i32 \"wrong type\")\n",
            ),
        ],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let target = snapshot
        .project()
        .project()
        .targets()
        .iter()
        .find(|target| target.name().atom().value() == "app")
        .expect("app target");

    let checked = vibra_workspace::semantic::check_target(&snapshot, target);

    assert_eq!(checked.status(), CheckStatus::Unavailable);
    assert!(checked.diagnostics().iter().any(|diagnostic| {
        diagnostic.code() == DiagnosticCode::ToolUnavailable
            && diagnostic.source_id() == Some("src/other/module.vib")
    }));
    assert!(!checked.diagnostics().iter().any(|diagnostic| {
        diagnostic.code() == DiagnosticCode::TypeArgumentMismatch
            && diagnostic.source_id() == Some("src/other/module.vib")
    }));
}

#[test]
fn variadic_entry_parameter_is_rejected_at_the_entry_span() {
    let project = TempProject::new(
        "variadic-entry",
        &[(
            "src/app/main.vib",
            "(defn execute () void variadic: (rest (array i32)) (do))\n",
        )],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let target = snapshot
        .project()
        .project()
        .targets()
        .first()
        .expect("target");

    let checked = vibra_workspace::semantic::check_target(&snapshot, target);

    assert!(checked.diagnostics().iter().any(|diagnostic| {
        diagnostic.code() == DiagnosticCode::ProjectInvalidEntrySignature
            && diagnostic.source_id() == Some("project.vibon")
            && diagnostic.primary_span() == target.entry().expect("entry").span()
    }));
}

#[test]
fn ordinary_dependency_diagnostics_apply_even_to_an_explicit_target() {
    let project_text = "(record format: @project.v1 package: (record name: \"demo\" version: \"0.1.0\") targets: (array (record name: @app kind: @bin root: \"src/app\" entry: @app.main.execute effects: (array))) dependencies: (map @remote (record kind: @git git: \"https://example.com/repo.git\" rev: \"0123456789abcdef0123456789abcdef01234567\")))\n";
    let project = TempProject::with_project(
        "global-dependency-diagnostic",
        project_text,
        &[("src/app/main.vib", "(defn execute () void (do))\n")],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let target = snapshot
        .project()
        .project()
        .targets()
        .first()
        .expect("target");

    let checked = vibra_workspace::semantic::check_target(&snapshot, target);

    assert_eq!(checked.status(), CheckStatus::Unavailable);
    assert!(checked.diagnostics().iter().any(|diagnostic| {
        diagnostic.code() == DiagnosticCode::ToolUnavailable
            && diagnostic.source_id() == Some("project.vibon")
    }));
}

#[test]
fn retired_syntax_and_unsupported_valid_declarations_keep_distinct_diagnostics() {
    let retired = TempProject::new(
        "retired-form",
        &[("src/app/main.vib", "(defn execute () void (return))\n")],
    );
    let retired_snapshot =
        WorkspaceSnapshot::load(retired.path()).expect("retired snapshot");
    let target = retired_snapshot
        .project()
        .project()
        .targets()
        .first()
        .expect("target");
    let checked = vibra_workspace::semantic::check_target(&retired_snapshot, target);
    assert_eq!(checked.status(), CheckStatus::Diagnostics);
    assert!(
        checked.diagnostics().iter().any(|diagnostic| {
            diagnostic.code() == DiagnosticCode::SyntaxRetiredForm
        })
    );
    assert!(
        !checked
            .diagnostics()
            .iter()
            .any(|diagnostic| { diagnostic.code() == DiagnosticCode::ToolUnavailable })
    );

    let unsupported = TempProject::new(
        "unsupported-declaration",
        &[(
            "src/app/main.vib",
            "(deftype box (record value i32))\n(defn execute () void (do))\n",
        )],
    );
    let unsupported_snapshot =
        WorkspaceSnapshot::load(unsupported.path()).expect("unsupported snapshot");
    let target = unsupported_snapshot
        .project()
        .project()
        .targets()
        .first()
        .expect("target");
    let checked =
        vibra_workspace::semantic::check_target(&unsupported_snapshot, target);
    assert_eq!(checked.status(), CheckStatus::Unavailable);
    assert!(checked.diagnostics().iter().any(|diagnostic| {
        diagnostic.code() == DiagnosticCode::ToolUnavailable
            && diagnostic.source_id() == Some("src/app/main.vib")
    }));
}

#[test]
fn recognized_host_provider_is_unavailable_and_unknown_provider_is_rejected() {
    for (label, provider, expected) in [
        ("host-provider", "host", DiagnosticCode::ToolUnavailable),
        (
            "unknown-provider",
            "wasm",
            DiagnosticCode::ExternalUnknownSymbol,
        ),
        (
            "copied-compiler-provider",
            "compiler",
            DiagnosticCode::ToolUnavailable,
        ),
    ] {
        let project = TempProject::new(
            label,
            &[(
                "src/app/main.vib",
                &format!(
                    "(defn execute () void external: @{} symbol: \"sample.operation\")\n",
                    provider
                ),
            )],
        );
        let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
        let target = snapshot
            .project()
            .project()
            .targets()
            .first()
            .expect("target");

        let checked = vibra_workspace::semantic::check_target(&snapshot, target);

        assert!(checked.diagnostics().iter().any(|diagnostic| {
            diagnostic.code() == expected
                && diagnostic.source_id() == Some("src/app/main.vib")
        }));
    }
}

#[test]
fn initializer_cycles_keep_the_source_diagnostic() {
    let project = TempProject::new(
        "initializer-cycle",
        &[(
            "src/app/main.vib",
            "(def first i32 second)\n(def second i32 first)\n(defn execute () void (do))\n",
        )],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let target = snapshot
        .project()
        .project()
        .targets()
        .first()
        .expect("target");

    let checked = vibra_workspace::semantic::check_target(&snapshot, target);

    assert_eq!(checked.status(), CheckStatus::Diagnostics);
    assert!(checked.diagnostics().iter().any(|diagnostic| {
        diagnostic.code() == DiagnosticCode::TypeInitializerCycle
    }));
    assert!(
        !checked
            .diagnostics()
            .iter()
            .any(|diagnostic| { diagnostic.code() == DiagnosticCode::ToolUnavailable })
    );
}
