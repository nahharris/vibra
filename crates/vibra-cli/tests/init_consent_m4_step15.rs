#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

//! M4 Step 15: the `project init` template carries the empty target consent
//! array and is admitted by the same rule as every other binary target.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use vibra_workspace::WorkspaceSnapshot;
use vibra_workspace::semantic::{CheckStatus, RunOutcome};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let nonce = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "vibra-cli-step15-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create test root");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn the_generated_project_declares_an_empty_array_and_is_admitted_without_a_diagnostic()
{
    let root = TempDir::new("init");
    let workspace_path = root.path().join("hello");
    fs::create_dir(&workspace_path).expect("create hello workspace");
    let workspace = workspace_path.to_string_lossy().into_owned();
    let output = Command::new(env!("CARGO_BIN_EXE_vibra"))
        .args([
            "--format",
            "json",
            "--workspace",
            &workspace,
            "project",
            "init",
        ])
        .output()
        .expect("run the built vibra binary");
    assert_eq!(output.status.code(), Some(0));

    let text = fs::read_to_string(workspace_path.join("project.vibon"))
        .expect("generated project document");
    assert!(
        text.contains("entry: @hello.main.main effects: (array))"),
        "the generated target keeps the required empty array: {text}"
    );

    let snapshot = WorkspaceSnapshot::load_confined(&workspace_path)
        .expect("generated project decodes");
    let target = &snapshot.project().project().targets()[0];
    assert_eq!(target.effects().map(<[_]>::len), Some(0));

    let verification = vibra_types::load_stdlib().expect("embedded standard library");
    let run = vibra_workspace::semantic::run_target_with_bootstrap(
        &snapshot,
        target,
        Some(&verification),
    );
    assert_eq!(run.check().status(), CheckStatus::Accepted);
    assert!(
        run.check().diagnostics().is_empty(),
        "{:?}",
        run.check().diagnostics()
    );
    assert!(matches!(run.outcome(), Some(RunOutcome::Program(_))));
}
