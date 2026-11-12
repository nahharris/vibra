//! Independent client connections observe complete revisions during backup.

use crate::support::TempDir;

#[test]
fn writer_contention_returns_busy_and_preserves_the_original_expectation() {
    let root = TempDir::new("codebase-busy-publisher");
    root.write("project.vibon", "(record format: @project.v1 package: (record name: \"busy-case\" version: \"0.1.0\") targets: (array (record name: @app kind: @bin root: \"src/app\" entry: @app.main.main effects: (array))) dependencies: (dict))\n");
    root.write("src/app/main.vib", "(defn main () void void)\n");
    let prepared = prepare_import(root.path()).expect("staged checked candidate");
    let store =
        CodebaseStore::init(root.path(), ".vibra/codebase.sqlite").expect("initialize");
    let lock = rusqlite::Connection::open(root.path().join(".vibra/codebase.sqlite"))
        .expect("independent writer");
    lock.execute_batch("BEGIN IMMEDIATE")
        .expect("hold writer lock");
    let error = store
        .publish(
            "main",
            HeadExpectation::Absent,
            &prepared.graph,
            &prepared.index,
        )
        .expect_err("lock contention must not publish");
    assert_eq!(error.code(), vibra_diagnostics::DiagnosticCode::StoreBusy);
    lock.execute_batch("ROLLBACK").expect("release writer");
    assert_eq!(
        store
            .verify()
            .expect("nothing was published")
            .revision_count,
        0
    );
    let published = store
        .publish(
            "main",
            HeadExpectation::Absent,
            &prepared.graph,
            &prepared.index,
        )
        .expect("explicit retry with the original expectation");
    assert!(published.published);
    assert_eq!(
        store.head(None).expect("committed head"),
        published.revision
    );
}
use std::sync::{Arc, Barrier};
use vibra_workspace::codebase::import::prepare_import;
use vibra_workspace::codebase::store::{CodebaseStore, HeadExpectation, StoreAccess};

#[test]
fn backup_overlapping_publication_restores_one_complete_committed_snapshot() {
    let root = TempDir::new("codebase-overlapping-backup");
    root.write("project.vibon", "(record format: @project.v1 package: (record name: \"backup-race\" version: \"0.1.0\") targets: (array (record name: @app kind: @bin root: \"src/app\" entry: @app.main.main effects: (array))) dependencies: (dict))\n");
    root.write("src/app/main.vib", "(defn main () void (do 0u32 void))\n");
    let first = prepare_import(root.path()).expect("initial checked graph");
    let store =
        CodebaseStore::init(root.path(), ".vibra/codebase.sqlite").expect("initialize");
    let initial = store
        .publish("main", HeadExpectation::Absent, &first.graph, &first.index)
        .expect("first publication");
    root.write("src/app/main.vib", "(defn main () void (do 1u32 void))\n");
    let second = prepare_import(root.path()).expect("changed checked graph");
    let barrier = Arc::new(Barrier::new(2));
    let backup_barrier = Arc::clone(&barrier);
    let reader = CodebaseStore::open(
        root.path(),
        ".vibra/codebase.sqlite",
        StoreAccess::ReadOnly,
    )
    .expect("independent backup client");
    let backup = std::thread::spawn(move || {
        backup_barrier.wait();
        reader
            .backup("overlapping.sqlite")
            .expect("consistent online backup")
    });
    barrier.wait();
    let published = store
        .publish(
            "main",
            HeadExpectation::Revision(initial.revision.clone()),
            &second.graph,
            &second.index,
        )
        .expect("second publication");
    let backup_report = backup.join().expect("backup worker");
    assert!(matches!(backup_report.revision_count, 1 | 2));
    CodebaseStore::restore(
        root.path(),
        "overlapping.sqlite",
        ".vibra/independent.sqlite",
    )
    .expect("restore captured snapshot");
    let restored = CodebaseStore::open(
        root.path(),
        ".vibra/independent.sqlite",
        StoreAccess::ReadOnly,
    )
    .expect("open independent restore");
    let head = restored.head(None).expect("restored head");
    let expected = if head == initial.revision {
        &first.graph
    } else {
        assert_eq!(head, published.revision);
        &second.graph
    };
    assert_eq!(
        restored
            .verify()
            .expect("valid restored snapshot")
            .revision_count,
        backup_report.revision_count
    );
    let attachments = restored
        .revision_sources(&head)
        .expect("restored attachments");
    for source in &expected.sources {
        assert!(
            attachments
                .iter()
                .any(|actual| actual.source_id == source.source_id
                    && actual.text == source.text)
        );
    }
    for object in expected.objects.iter().filter(|object| {
        object.origins.iter().any(|origin| {
            expected
                .sources
                .iter()
                .any(|source| source.source_id == origin.source_id)
        })
    }) {
        restored
            .read(&head, object.address.as_str())
            .expect("all referenced objects present");
    }
}
