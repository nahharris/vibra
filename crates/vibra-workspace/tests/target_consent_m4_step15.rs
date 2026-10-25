//! M4 Step 15 contracts that no single corpus case expresses: the target
//! `effects` array is resolved and admitted per target, a bad atom or an
//! unadmitted target never stops a sibling, the entry is never run unless it
//! is admitted, a test is consent to its own row, and `fmt` writes the array in
//! canonical order.

#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use vibra_diagnostics::{DiagnosticCode, Level};
use vibra_workspace::{
    WorkspaceSnapshot,
    semantic::{CheckStatus, TestItemStatus, TestSuiteStatus, run_tests},
};

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

struct TempProject(PathBuf);

impl TempProject {
    fn new(label: &str, project: &str, sources: &[(&str, &str)]) -> Self {
        let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let root = fs::canonicalize(std::env::temp_dir())
            .expect("canonical temp directory")
            .join(format!(
                "vibra-step15-{label}-{}-{serial}",
                std::process::id()
            ));
        fs::create_dir_all(&root).expect("create project root");
        fs::write(root.join("project.vibon"), project).expect("write project marker");
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

const OPS: &str = "(deffect write\n  visibility: @public\n  (defn text (target str) void visibility: @public (do)))\n\n(deffect audit\n  visibility: @public\n  (defn record (target str) void\n    visibility: @public\n    effects: (write)\n    (write.text target)))\n";

const ENTRY: &str = "(import log @ops.log)\n\n(defn execute () void\n  effects: (log.audit log.write)\n  (log.audit.record \"x\"))\n";

fn two_binaries(first_array: &str, second_array: &str) -> String {
    format!(
        "(record format: @project.v1 package: (record name: \"demo\" version: \"0.1.0\") targets: (array (record name: @first kind: @bin root: \"src/first\" entry: @first.main.execute effects: (array{first_array})) (record name: @second kind: @bin root: \"src/second\" entry: @second.main.execute effects: (array{second_array})) (record name: @ops kind: @lib root: \"src/ops\")) dependencies: (dict))\n"
    )
}

fn sources() -> Vec<(&'static str, &'static str)> {
    vec![
        ("src/first/main.vib", ENTRY),
        ("src/second/main.vib", ENTRY),
        ("src/ops/log.vib", OPS),
    ]
}

#[test]
fn a_bad_atom_or_an_unadmitted_target_does_not_stop_its_sibling() {
    let project = TempProject::new(
        "sibling",
        &two_binaries(
            " @ops.log.missing @ops.log.audit",
            " @ops.log.audit @ops.log.write",
        ),
        &sources(),
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let targets = snapshot.project().project().targets();
    let first = &targets[0];
    let second = &targets[1];

    // The whole workspace reports the broken target's atom and its omission.
    let everything = vibra_workspace::semantic::check_all(&snapshot);
    assert_eq!(everything.status(), CheckStatus::Diagnostics);
    let codes = everything
        .diagnostics()
        .iter()
        .map(|diagnostic| {
            (diagnostic.code(), diagnostic.source_id().map(str::to_owned))
        })
        .collect::<Vec<_>>();
    assert!(
        codes.contains(&(
            DiagnosticCode::NameUnknownSymbol,
            Some("project.vibon".to_owned())
        )),
        "{codes:?}"
    );
    assert!(
        codes.contains(&(
            DiagnosticCode::EffectOutsideCeiling,
            Some("project.vibon".to_owned())
        )),
        "{codes:?}"
    );

    // Checking the sibling by name sees none of it and still runs.
    let sibling = vibra_workspace::semantic::run_target(&snapshot, second);
    assert_eq!(
        sibling.check().status(),
        CheckStatus::Accepted,
        "{:?}",
        sibling.check().diagnostics()
    );
    assert!(sibling.outcome().is_some());

    // The broken target is not admitted and is never run.
    let broken = vibra_workspace::semantic::run_target(&snapshot, first);
    assert_eq!(broken.check().status(), CheckStatus::Diagnostics);
    assert!(broken.outcome().is_none());
}

#[test]
fn an_omitted_root_is_reported_at_the_entry_value_with_every_hop_related() {
    let project = TempProject::new(
        "witness",
        &two_binaries(" @ops.log.audit", " @ops.log.audit @ops.log.write"),
        &sources(),
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let first = &snapshot.project().project().targets()[0];

    let checked = vibra_workspace::semantic::check_target(&snapshot, first);

    let diagnostics = checked
        .diagnostics()
        .iter()
        .filter(|diagnostic| diagnostic.code() == DiagnosticCode::EffectOutsideCeiling)
        .collect::<Vec<_>>();
    assert_eq!(diagnostics.len(), 1, "{:?}", checked.diagnostics());
    let diagnostic = diagnostics[0];
    assert_eq!(diagnostic.source_id(), Some("project.vibon"));
    let entry = first.entry().expect("entry").span();
    assert_eq!(diagnostic.primary_span(), entry);
    assert!(diagnostic.notes().is_empty());
    let hops = diagnostic
        .related()
        .iter()
        .map(|related| (related.source_id.as_deref(), related.message.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(
        hops,
        [
            (
                Some("src/first/main.vib"),
                "performs @ops.log.write via @ops.log.audit.record"
            ),
            (
                Some("src/ops/log.vib"),
                "performs @ops.log.write via @ops.log.write.text"
            ),
        ]
    );
}

#[test]
fn an_unused_array_root_is_a_warning_and_the_target_still_runs() {
    let unused = TempProject::new(
        "unused-root",
        "(record format: @project.v1 package: (record name: \"demo\" version: \"0.1.0\") targets: (array (record name: @app kind: @bin root: \"src/app\" entry: @app.main.execute effects: (array @ops.log.write)) (record name: @ops kind: @lib root: \"src/ops\")) dependencies: (dict))\n",
        &[
            ("src/app/main.vib", "(defn execute () void void)\n"),
            ("src/ops/log.vib", OPS),
        ],
    );
    let snapshot = WorkspaceSnapshot::load(unused.path()).expect("snapshot");
    let app = &snapshot.project().project().targets()[0];

    let run = vibra_workspace::semantic::run_target(&snapshot, app);

    assert_eq!(run.check().status(), CheckStatus::Accepted);
    let warnings = run
        .check()
        .diagnostics()
        .iter()
        .map(|diagnostic| (diagnostic.code(), diagnostic.level()))
        .collect::<Vec<_>>();
    assert_eq!(
        warnings,
        [(DiagnosticCode::ContractUnusedEffect, Level::Warning)]
    );
    assert!(run.outcome().is_some());
}

#[test]
fn a_test_is_consent_to_its_own_row_with_an_empty_target_array() {
    let project = TempProject::new(
        "test-consent",
        "(record format: @project.v1 package: (record name: \"demo\" version: \"0.1.0\") targets: (array (record name: @app kind: @bin root: \"src/app\" entry: @app.main.execute effects: (array)) (record name: @ops kind: @lib root: \"src/ops\")) dependencies: (dict))\n",
        &[
            ("src/app/main.vib", "(defn execute () void void)\n"),
            ("src/ops/log.vib", OPS),
            (
                "tests/consent.vib",
                "(import assert @std.assert)\n(import log @ops.log)\n\n(test \"an effectful test writes its row\"\n  effects: (log.audit log.write)\n  (log.audit.record \"x\")\n  (assert.true true))\n\n(test \"a pure test omits it\"\n  (assert.true true))\n\n(test \"a test over its row is rejected\"\n  effects: (log.audit)\n  (log.audit.record \"x\")\n  (assert.true true))\n",
            ),
        ],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let verification = vibra_types::load_stdlib().expect("embedded standard library");

    let result = run_tests(&snapshot, None, Some(&verification));

    // The over-ceiling test is a source error, so no test executes.
    assert_eq!(result.status(), TestSuiteStatus::Diagnostics);
    assert!(
        result.diagnostics().iter().any(|diagnostic| {
            diagnostic.code() == DiagnosticCode::EffectOutsideCeiling
                && diagnostic.source_id() == Some("tests/consent.vib")
        }),
        "{:?}",
        result.diagnostics()
    );
    assert!(
        !result
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.source_id() == Some("project.vibon")),
        "the target array takes no part in a test: {:?}",
        result.diagnostics()
    );
}

#[test]
fn effectful_tests_with_complete_rows_run_under_an_empty_target_array() {
    let project = TempProject::new(
        "test-consent-ok",
        "(record format: @project.v1 package: (record name: \"demo\" version: \"0.1.0\") targets: (array (record name: @app kind: @bin root: \"src/app\" entry: @app.main.execute effects: (array)) (record name: @ops kind: @lib root: \"src/ops\")) dependencies: (dict))\n",
        &[
            ("src/app/main.vib", "(defn execute () void void)\n"),
            ("src/ops/log.vib", OPS),
            (
                "tests/consent.vib",
                "(import assert @std.assert)\n(import log @ops.log)\n\n(test \"an effectful test writes its row\"\n  effects: (log.audit log.write)\n  (log.audit.record \"x\")\n  (assert.true true))\n\n(test \"a pure test omits it\"\n  (assert.true true))\n",
            ),
        ],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");
    let verification = vibra_types::load_stdlib().expect("embedded standard library");

    let result = run_tests(&snapshot, None, Some(&verification));

    assert_eq!(
        result.status(),
        TestSuiteStatus::Ok,
        "{:?}",
        result.diagnostics()
    );
    assert_eq!(result.selected(), 2);
    assert_eq!(result.passed(), 2);
    assert!(
        result
            .items()
            .iter()
            .all(|item| item.status() == TestItemStatus::Passed)
    );
}

#[test]
fn the_array_is_written_in_canonical_order_by_the_formatter() {
    let document = "(record format: @project.v1 package: (record name: \"demo\" version: \"0.1.0\") targets: (array (record effects: (array @ops.log.write @ops.log.audit) entry: @app.main.execute root: \"src/app\" kind: @bin name: @app)) dependencies: (dict))\n";
    let project = TempProject::new(
        "fmt",
        document,
        &[("src/app/main.vib", "(defn execute () void void)\n")],
    );
    let snapshot = WorkspaceSnapshot::load(project.path()).expect("snapshot");

    let formatted = snapshot.project().project().canonical_vibon();

    assert!(
        formatted.contains(
            "(record name: @app kind: @bin root: \"src/app\" entry: @app.main.execute effects: (array @ops.log.audit @ops.log.write))"
        ),
        "{formatted}"
    );
}
