#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use super::*;

use crate::codebase::import::prepare_import;
use crate::codebase::store::{HeadExpectation, StoreAccess};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

const CHILD_ROOT: &str = "VIBRA_EXPORT_INTERRUPTION_ROOT";
const CHILD_REVISION: &str = "VIBRA_EXPORT_INTERRUPTION_REVISION";
const CHILD_DESTINATION: &str = "VIBRA_EXPORT_INTERRUPTION_DESTINATION";
const CHILD_PLAN: &str = "VIBRA_EXPORT_INTERRUPTION_PLAN";
const CHILD_CHECKPOINT: &str = "VIBRA_EXPORT_INTERRUPTION_CHECKPOINT";
const CHILD_EXIT_CODE: i32 = 74;

static NEXT_WORKSPACE: AtomicU64 = AtomicU64::new(0);

struct TestWorkspace(PathBuf);

impl TestWorkspace {
    fn new() -> Self {
        let serial = NEXT_WORKSPACE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir()
            .canonicalize()
            .expect("canonical temporary directory")
            .join(format!(
                "vibra-export-interruption-{}-{serial}",
                std::process::id()
            ));
        std::fs::create_dir(&root).expect("create isolated workspace");
        Self(root)
    }

    fn write(&self, relative: &str, text: &str) {
        let path = self.0.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create source parent");
        }
        std::fs::write(path, text).expect("write source fixture");
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestWorkspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn export_process_interruption_leaves_absent_or_exact_complete_tree() {
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        let root = PathBuf::from(root);
        let revision = std::env::var(CHILD_REVISION).expect("child export revision");
        let destination =
            std::env::var(CHILD_DESTINATION).expect("child export destination");
        let plan = std::env::var(CHILD_PLAN).expect("child export plan");
        let selected = checkpoint_from_env();
        let store =
            CodebaseStore::open(&root, ".vibra/codebase.sqlite", StoreAccess::ReadOnly)
                .expect("open child source store");
        let _ = store.apply_export_with_checkpoint(
            &revision,
            &destination,
            &plan,
            |checkpoint| exit_at_checkpoint(checkpoint, selected),
        );
        panic!("child export returned without exiting at its checkpoint");
    }

    let workspace = TestWorkspace::new();
    workspace.write(
        "project.vibon",
        "(record format: @project.v1 package: (record name: \"export-interruption\" version: \"0.1.0\") targets: (array (record name: @app kind: @bin root: \"src/app\" entry: @app.main.main effects: (array))) dependencies: (dict))\n",
    );
    workspace.write("src/app/main.vib", "(defn main () void void)\n");
    std::fs::create_dir(workspace.path().join("out"))
        .expect("create export destination parent");
    let prepared =
        prepare_import(workspace.path()).expect("prepare one tiny local graph");
    let expected_files = prepared
        .graph
        .sources
        .iter()
        .map(|source| (source.origin.clone(), source.text.clone()))
        .collect::<BTreeMap<_, _>>();
    let store = CodebaseStore::init(workspace.path(), ".vibra/codebase.sqlite")
        .expect("initialize codebase store");
    store
        .publish(
            "main",
            HeadExpectation::Absent,
            &prepared.graph,
            &prepared.index,
        )
        .expect("publish prepared graph");
    let revision = store.head(Some("main")).expect("read published head");
    let expected_store = store.verify().expect("verify source store");

    for (phase, expected_present) in [("before", false), ("after", true)] {
        let destination = format!("out/{phase}");
        let plan = store
            .preview_export(&revision, &destination)
            .expect("preview export");
        let status = Command::new(std::env::current_exe().expect("test binary path"))
            .args([
                "--exact",
                "codebase::export::tests::export_process_interruption_leaves_absent_or_exact_complete_tree",
                "--nocapture",
            ])
            .env(CHILD_ROOT, workspace.path())
            .env(CHILD_REVISION, &revision)
            .env(CHILD_DESTINATION, &destination)
            .env(CHILD_PLAN, &plan.plan)
            .env(CHILD_CHECKPOINT, phase)
            .status()
            .expect("spawn export interruption child");
        assert_eq!(status.code(), Some(CHILD_EXIT_CODE));

        let destination_path = workspace.path().join(&destination);
        if expected_present {
            assert!(destination_path.is_dir());
        } else {
            assert!(!destination_path.exists());
        }
        assert_eq!(
            store.head(Some("main")).expect("recheck source head"),
            revision
        );
        assert_eq!(
            store.verify().expect("reverify source store"),
            expected_store
        );
        if expected_present {
            assert_exact_export(&destination_path, &expected_files);
        }
    }
}

fn assert_exact_export(root: &Path, expected: &BTreeMap<String, Vec<u8>>) {
    let mut actual_paths = Vec::new();
    collect_file_paths(root, root, &mut actual_paths);
    actual_paths.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
    let expected_paths = expected.keys().cloned().collect::<Vec<_>>();
    assert_eq!(actual_paths, expected_paths);
    for (path, bytes) in expected {
        assert_eq!(
            std::fs::read(root.join(path))
                .expect("read exported source")
                .as_slice(),
            bytes.as_slice(),
            "exported bytes for {path}"
        );
    }
}

fn collect_file_paths(root: &Path, directory: &Path, output: &mut Vec<String>) {
    for entry in std::fs::read_dir(directory).expect("read exported directory") {
        let entry = entry.expect("read exported directory entry");
        let path = entry.path();
        if entry.file_type().expect("inspect exported entry").is_dir() {
            collect_file_paths(root, &path, output);
        } else {
            output.push(
                path.strip_prefix(root)
                    .expect("export path is beneath destination")
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
}

fn checkpoint_from_env() -> PublicationCheckpoint {
    match std::env::var(CHILD_CHECKPOINT).as_deref() {
        Ok("before") => PublicationCheckpoint::BeforePublish,
        Ok("after") => PublicationCheckpoint::AfterPublish,
        other => panic!("invalid child checkpoint: {other:?}"),
    }
}

fn exit_at_checkpoint(current: PublicationCheckpoint, selected: PublicationCheckpoint) {
    if current == selected {
        std::process::exit(CHILD_EXIT_CODE);
    }
}
