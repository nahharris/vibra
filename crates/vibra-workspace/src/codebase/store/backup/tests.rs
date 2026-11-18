#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use super::*;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

const CHILD_ROOT: &str = "VIBRA_BACKUP_INTERRUPTION_ROOT";
const CHILD_SOURCE: &str = "VIBRA_BACKUP_INTERRUPTION_SOURCE";
const CHILD_DESTINATION: &str = "VIBRA_BACKUP_INTERRUPTION_DESTINATION";
const CHILD_CHECKPOINT: &str = "VIBRA_BACKUP_INTERRUPTION_CHECKPOINT";
const CHILD_EXIT_CODE: i32 = 73;

static NEXT_WORKSPACE: AtomicU64 = AtomicU64::new(0);

struct TestWorkspace(PathBuf);

impl TestWorkspace {
    fn new() -> Self {
        let serial = NEXT_WORKSPACE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir()
            .canonicalize()
            .expect("canonical temporary directory")
            .join(format!(
                "vibra-backup-interruption-{}-{serial}",
                std::process::id()
            ));
        std::fs::create_dir(&root).expect("create isolated workspace");
        Self(root)
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
fn backup_process_interruption_leaves_absent_or_complete_verified_copy() {
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        let root = PathBuf::from(root);
        let destination =
            std::env::var(CHILD_DESTINATION).expect("child backup destination");
        let selected = checkpoint_from_env();
        let store =
            CodebaseStore::open(&root, ".vibra/codebase.sqlite", StoreAccess::ReadOnly)
                .expect("open original child store");
        let _ = store.backup_with_checkpoint(destination, |checkpoint| {
            exit_at_checkpoint(checkpoint, selected);
        });
        panic!("child backup returned without exiting at its checkpoint");
    }

    let workspace = TestWorkspace::new();
    std::fs::create_dir(workspace.path().join("backups"))
        .expect("create backup destination parent");
    let original = CodebaseStore::init(workspace.path(), ".vibra/codebase.sqlite")
        .expect("initialize empty source store");
    let expected = original.verify().expect("verify original store");
    drop(original);

    for (phase, expected_present) in [("before", false), ("after", true)] {
        let destination = format!("backups/{phase}.sqlite");
        let status = Command::new(std::env::current_exe().expect("test binary path"))
            .args([
                "--exact",
                "codebase::store::backup::tests::backup_process_interruption_leaves_absent_or_complete_verified_copy",
                "--nocapture",
            ])
            .env(CHILD_ROOT, workspace.path())
            .env(CHILD_DESTINATION, &destination)
            .env(CHILD_CHECKPOINT, phase)
            .status()
            .expect("spawn backup interruption child");
        assert_eq!(status.code(), Some(CHILD_EXIT_CODE));

        let destination_path = workspace.path().join(&destination);
        if expected_present {
            assert!(destination_path.is_file());
        } else {
            assert!(!destination_path.exists());
        }
        let original = CodebaseStore::open(
            workspace.path(),
            ".vibra/codebase.sqlite",
            StoreAccess::ReadOnly,
        )
        .expect("reopen original store");
        assert_eq!(original.verify().expect("verify original store"), expected);
        if expected_present {
            let copied = CodebaseStore::open(
                workspace.path(),
                &destination,
                StoreAccess::ReadOnly,
            )
            .expect("open published backup");
            assert_eq!(copied.verify().expect("verify published backup"), expected);
        }
    }
}

#[test]
fn restore_process_interruption_leaves_absent_or_complete_verified_store() {
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        let root = PathBuf::from(root);
        let source = std::env::var(CHILD_SOURCE).expect("child restore source");
        let destination =
            std::env::var(CHILD_DESTINATION).expect("child restore destination");
        let selected = checkpoint_from_env();
        let _ = CodebaseStore::restore_with_checkpoint(
            &root,
            source,
            destination,
            |checkpoint| exit_at_checkpoint(checkpoint, selected),
        );
        panic!("child restore returned without exiting at its checkpoint");
    }

    let workspace = TestWorkspace::new();
    std::fs::create_dir(workspace.path().join("backups"))
        .expect("create backup destination parent");
    std::fs::create_dir(workspace.path().join("restores"))
        .expect("create restore destination parent");
    let original = CodebaseStore::init(workspace.path(), ".vibra/codebase.sqlite")
        .expect("initialize empty source store");
    let expected = original.verify().expect("verify original store");
    original
        .backup("backups/source.sqlite")
        .expect("create restore source backup");
    drop(original);

    for (phase, expected_present) in [("before", false), ("after", true)] {
        let destination = format!("restores/{phase}.sqlite");
        let status = Command::new(std::env::current_exe().expect("test binary path"))
            .args([
                "--exact",
                "codebase::store::backup::tests::restore_process_interruption_leaves_absent_or_complete_verified_store",
                "--nocapture",
            ])
            .env(CHILD_ROOT, workspace.path())
            .env(CHILD_SOURCE, "backups/source.sqlite")
            .env(CHILD_DESTINATION, &destination)
            .env(CHILD_CHECKPOINT, phase)
            .status()
            .expect("spawn restore interruption child");
        assert_eq!(status.code(), Some(CHILD_EXIT_CODE));

        let destination_path = workspace.path().join(&destination);
        assert_eq!(destination_path.is_file(), expected_present);
        let source = CodebaseStore::open(
            workspace.path(),
            "backups/source.sqlite",
            StoreAccess::ReadOnly,
        )
        .expect("reopen restore source backup");
        assert_eq!(source.verify().expect("verify source backup"), expected);
        let original = CodebaseStore::open(
            workspace.path(),
            ".vibra/codebase.sqlite",
            StoreAccess::ReadOnly,
        )
        .expect("reopen original store");
        assert_eq!(original.verify().expect("verify original store"), expected);
        if expected_present {
            let restored = CodebaseStore::open(
                workspace.path(),
                &destination,
                StoreAccess::ReadOnly,
            )
            .expect("open published restore");
            assert_eq!(
                restored.verify().expect("verify published restore"),
                expected
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
