//! Durable local-store lifecycle tests.

use std::ffi::OsStr;
use std::path::Path;
#[cfg(windows)]
use std::process::Command;

use vibra_workspace::codebase::import::prepare_import;
use vibra_workspace::codebase::is_valid_head_name;
use vibra_workspace::codebase::store::{
    CodebaseStore, HeadExpectation, StoreAccess, StoreError,
};
use vibra_workspace::confined_fs::ConfinedDir;

use crate::support::TempDir;

const STORE_PATH: &str = ".vibra/codebase.sqlite";

#[test]
fn head_names_follow_the_shared_kebab_case_grammar() {
    assert!(is_valid_head_name("main"));
    assert!(is_valid_head_name("release-2"));
    assert!(is_valid_head_name("a0"));
    assert!(!is_valid_head_name("2release"));
    assert!(!is_valid_head_name("release-"));
    assert!(!is_valid_head_name("release--next"));
    assert!(!is_valid_head_name("Release"));
}

#[test]
fn explicit_init_creates_a_store_that_can_be_opened_read_only() {
    let root = TempDir::new("codebase-store-init");

    let store = CodebaseStore::init(root.path(), Path::new(STORE_PATH));
    assert!(store.is_ok(), "store initialization failed: {store:?}");
    assert!(root.path().join(STORE_PATH).is_file());

    let reopened =
        CodebaseStore::open(root.path(), Path::new(STORE_PATH), StoreAccess::ReadOnly);
    assert!(reopened.is_ok(), "read-only reopen failed: {reopened:?}");
}

#[test]
fn opening_an_absent_store_read_only_does_not_create_its_parent() {
    let root = TempDir::new("codebase-store-absent");

    let opened =
        CodebaseStore::open(root.path(), Path::new(STORE_PATH), StoreAccess::ReadOnly);

    assert!(matches!(opened, Err(StoreError::NotFound(_))));
    assert!(!root.path().join(".vibra").exists());
}

#[test]
fn exclusive_directory_publish_preserves_a_concurrently_created_empty_destination() {
    let root = TempDir::new("codebase-store-exclusive-rename");
    let parent = ConfinedDir::open(root.path()).expect("confined parent");
    parent.create_dir(OsStr::new("stage")).expect("stage dir");
    parent
        .create_dir(OsStr::new("destination"))
        .expect("destination dir");
    let stage = parent.open_dir("stage").expect("open stage");
    stage
        .write_new_file(OsStr::new("marker"), b"complete", None)
        .expect("stage marker");

    let result =
        parent.rename_to_new(OsStr::new("stage"), &parent, OsStr::new("destination"));

    assert!(
        result.is_err(),
        "exclusive rename must not replace an existing destination"
    );
    assert!(root.path().join("stage/marker").is_file());
    assert!(root.path().join("destination").is_dir());
    assert!(!root.path().join("destination/marker").exists());
}

#[test]
fn exclusive_directory_publish_makes_a_complete_tree_visible() {
    let root = TempDir::new("codebase-store-exclusive-rename-success");
    let parent = ConfinedDir::open(root.path()).expect("confined parent");
    parent.create_dir(OsStr::new("stage")).expect("stage dir");
    let stage = parent.open_dir("stage").expect("open stage");
    stage
        .write_new_file(OsStr::new("marker"), b"complete", None)
        .expect("stage marker");
    drop(stage);

    parent
        .rename_to_new(OsStr::new("stage"), &parent, OsStr::new("destination"))
        .expect("publish complete tree");

    assert!(!root.path().join("stage").exists());
    assert_eq!(
        std::fs::read(root.path().join("destination/marker"))
            .expect("published marker"),
        b"complete"
    );
}

#[test]
fn read_only_open_rejects_an_unknown_schema_version_without_migration() {
    let root = TempDir::new("codebase-store-version");
    CodebaseStore::init(root.path(), Path::new(STORE_PATH)).expect("initialize store");
    let database = rusqlite::Connection::open(root.path().join(STORE_PATH))
        .expect("open store for corruption fixture");
    database
        .pragma_update(None, "user_version", 99_i32)
        .expect("set unknown schema version");
    drop(database);

    let opened =
        CodebaseStore::open(root.path(), Path::new(STORE_PATH), StoreAccess::ReadOnly);

    assert!(matches!(
        opened,
        Err(StoreError::UnsupportedVersion { found: 99, .. })
    ));
}

#[test]
fn read_only_open_classifies_missing_required_tables_as_store_corruption() {
    let root = TempDir::new("codebase-store-missing-table");
    CodebaseStore::init(root.path(), Path::new(STORE_PATH)).expect("initialize store");
    let database = rusqlite::Connection::open(root.path().join(STORE_PATH))
        .expect("open store for corruption fixture");
    database
        .execute_batch("DROP TABLE revision_edges")
        .expect("drop required table");
    drop(database);

    let opened =
        CodebaseStore::open(root.path(), Path::new(STORE_PATH), StoreAccess::ReadOnly);

    assert!(matches!(opened, Err(StoreError::Corrupt(_))), "{opened:?}");
}

#[test]
fn backup_and_restore_publish_independent_store_files_without_overwrite() {
    let root = TempDir::new("codebase-store-backup-restore");
    CodebaseStore::init(root.path(), Path::new(STORE_PATH)).expect("initialize store");
    let read_only =
        CodebaseStore::open(root.path(), Path::new(STORE_PATH), StoreAccess::ReadOnly)
            .expect("open read-only source");

    let backup = read_only
        .backup(Path::new("standalone-backup.sqlite"))
        .expect("create backup");
    assert_eq!(backup.revision_count, 0);
    assert!(root.path().join("standalone-backup.sqlite").is_file());
    let restored = CodebaseStore::restore(
        root.path(),
        Path::new("standalone-backup.sqlite"),
        Path::new(".vibra/restored.sqlite"),
    )
    .expect("restore independent store");
    assert_eq!(restored.revision_count, 0);
    let reopened = CodebaseStore::open(
        root.path(),
        Path::new(".vibra/restored.sqlite"),
        StoreAccess::ReadOnly,
    )
    .expect("open restored store");
    assert_eq!(
        reopened
            .verify()
            .expect("verify restored store")
            .revision_count,
        0
    );

    let second_restore = CodebaseStore::restore(
        root.path(),
        Path::new("standalone-backup.sqlite"),
        Path::new(".vibra/restored.sqlite"),
    );
    assert!(matches!(second_restore, Err(StoreError::UnsafePath(_))));
}

#[test]
fn restore_rejects_corrupt_and_unsupported_sources_without_publishing() {
    let root = TempDir::new("codebase-restore-invalid-inputs");
    std::fs::create_dir(root.path().join("restores"))
        .expect("create restore destination parent");
    std::fs::write(root.path().join("corrupt.sqlite"), b"not a SQLite database")
        .expect("write corrupt backup fixture");

    let corrupt = CodebaseStore::restore(
        root.path(),
        Path::new("corrupt.sqlite"),
        Path::new("restores/corrupt.sqlite"),
    );
    assert!(
        matches!(corrupt, Err(StoreError::Corrupt(_))),
        "{corrupt:?}"
    );
    assert!(!root.path().join("restores/corrupt.sqlite").exists());

    CodebaseStore::init(root.path(), Path::new("future.sqlite"))
        .expect("initialize unsupported-version fixture");
    let future = rusqlite::Connection::open(root.path().join("future.sqlite"))
        .expect("open unsupported-version fixture");
    future
        .pragma_update(None, "user_version", 99_i32)
        .expect("set unsupported schema version");
    drop(future);

    let unsupported = CodebaseStore::restore(
        root.path(),
        Path::new("future.sqlite"),
        Path::new("restores/future.sqlite"),
    );
    assert!(
        matches!(
            unsupported,
            Err(StoreError::UnsupportedVersion { found: 99, .. })
        ),
        "{unsupported:?}"
    );
    assert!(!root.path().join("restores/future.sqlite").exists());
}

#[cfg(any(unix, windows))]
#[test]
fn export_refuses_a_destination_parent_replaced_by_a_directory_link() {
    #[cfg(windows)]
    let root = TempDir::plain("codebase-export-linked-parent");
    #[cfg(not(windows))]
    let root = TempDir::new("codebase-export-linked-parent");
    root.write(
        "project.vibon",
        "(record format: @project.v1 package: (record name: \"export-link\" version: \"0.1.0\") targets: (array (record name: @app kind: @bin root: \"src/app\" entry: @app.main.main effects: (array))) dependencies: (dict))\n",
    );
    root.write("src/app/main.vib", "(defn main () void void)\n");
    let prepared = prepare_import(root.path()).expect("prepare valid export fixture");
    let store = CodebaseStore::init(root.path(), Path::new(STORE_PATH))
        .expect("initialize export fixture store");
    let receipt = store
        .publish(
            "main",
            HeadExpectation::Absent,
            &prepared.graph,
            &prepared.index,
        )
        .expect("publish export fixture revision");

    for destination in ["../escape", "alias/../../escape"] {
        let result = store.preview_export(&receipt.revision, Path::new(destination));
        assert!(
            matches!(result, Err(StoreError::UnsafePath(_))),
            "{result:?}"
        );
    }

    let link = root.path().join("alias");
    let outside = root.path().join("outside");
    std::fs::create_dir(&link).expect("create preview destination parent");
    std::fs::create_dir(&outside).expect("create link target");
    std::fs::write(outside.join("sentinel"), b"preserve")
        .expect("write link target sentinel");
    let plan = store
        .preview_export(&receipt.revision, Path::new("alias/exported"))
        .expect("preview export into ordinary parent");

    std::fs::remove_dir(&link).expect("remove ordinary destination parent");
    create_directory_link(&outside, &link)
        .expect("create directory link fixture; CI must provide link support");
    let result =
        store.apply_export(&receipt.revision, Path::new("alias/exported"), &plan.plan);
    assert!(
        result.is_err(),
        "export followed destination link: {result:?}"
    );
    assert!(!outside.join("exported").exists());
    assert_eq!(
        std::fs::read(outside.join("sentinel")).expect("read preserved sentinel"),
        b"preserve"
    );
    remove_directory_link(&link).expect("remove directory link fixture");
}

#[cfg(any(unix, windows))]
fn create_directory_link(target: &Path, link: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)
    }
    #[cfg(windows)]
    {
        let output = Command::new("cmd")
            .args(["/D", "/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()?;
        if output.status.success() {
            Ok(())
        } else {
            Err(std::io::Error::other(format!(
                "junction creation failed: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )))
        }
    }
}

#[cfg(any(unix, windows))]
fn remove_directory_link(link: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::fs::remove_file(link)
    }
    #[cfg(windows)]
    {
        std::fs::remove_dir(link)
    }
}
