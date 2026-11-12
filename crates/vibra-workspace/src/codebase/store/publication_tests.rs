//! Abrupt termination at transaction boundaries, including a lost success reply.
use super::*;
use crate::codebase::import::prepare_import;
use std::process::Command;

const CHILD_ROOT: &str = "VIBRA_PUBLICATION_BOUNDARY_ROOT";
const CHILD_PHASE: &str = "VIBRA_PUBLICATION_BOUNDARY_PHASE";
const EXIT_CODE: i32 = 79;
const TEST: &str = "codebase::store::publication_tests::process_interruption_at_each_publication_boundary_is_atomic";

#[test]
fn process_interruption_at_each_publication_boundary_is_atomic() {
    let phases = [
        PublicationCheckpoint::BeforeTransaction,
        PublicationCheckpoint::ObjectsWritten,
        PublicationCheckpoint::IndexesWritten,
        PublicationCheckpoint::BeforeCommit,
        PublicationCheckpoint::Committed,
    ];
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        let root = PathBuf::from(root);
        let requested = std::env::var(CHILD_PHASE).expect("requested fault boundary");
        let prepared = prepare_import(&root).expect("stage checked child candidate");
        std::fs::write(
            root.join("expected-revision"),
            prepared.graph.revision.address.as_str(),
        )
        .expect("record candidate identity before publication");
        let store = CodebaseStore::open(
            &root,
            ".vibra/codebase.sqlite",
            StoreAccess::ReadWrite,
        )
        .expect("open child store");
        let _ = store.publish_with_options(
            "main",
            HeadExpectation::Absent,
            &prepared.graph,
            &prepared.index,
            Some(1),
            |phase| {
                if format!("{phase:?}") == requested {
                    std::process::exit(EXIT_CODE);
                }
            },
        );
        panic!("child returned without reaching {requested}");
    }
    for phase in phases {
        let serial = NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir()
            .canonicalize()
            .expect("canonical temporary directory")
            .join(format!(
                "vibra-publication-boundary-{}-{serial}",
                std::process::id()
            ));
        std::fs::create_dir(&root).expect("create isolated child workspace");
        let root = TestRoot(root);
        std::fs::create_dir_all(root.0.join("src/app"))
            .expect("create source directory");
        std::fs::write(root.0.join("project.vibon"), "(record format: @project.v1 package: (record name: \"publication-boundary\" version: \"0.1.0\") targets: (array (record name: @app kind: @bin root: \"src/app\" entry: @app.main.main effects: (array))) dependencies: (dict))\n").expect("write manifest");
        std::fs::write(
            root.0.join("src/app/main.vib"),
            "(defn main () void void)\n",
        )
        .expect("write source");
        drop(
            CodebaseStore::init(&root.0, ".vibra/codebase.sqlite")
                .expect("initialize store"),
        );
        let status = Command::new(std::env::current_exe().expect("test executable"))
            .args(["--exact", TEST, "--nocapture"])
            .env(CHILD_ROOT, &root.0)
            .env(CHILD_PHASE, format!("{phase:?}"))
            .status()
            .expect("launch child process");
        assert_eq!(status.code(), Some(EXIT_CODE), "{phase:?}");
        // Write-enabled open explicitly performs any required hot-journal recovery.
        let store = CodebaseStore::open(
            &root.0,
            ".vibra/codebase.sqlite",
            StoreAccess::ReadWrite,
        )
        .expect("recover interrupted transaction");
        let expected = std::fs::read_to_string(root.0.join("expected-revision"))
            .expect("candidate address");
        let committed = phase == PublicationCheckpoint::Committed;
        assert_eq!(
            store
                .verify()
                .expect("complete recovered authority")
                .revision_count,
            u64::from(committed)
        );
        if committed {
            assert_eq!(
                store
                    .head(None)
                    .expect("resolve lost reply through named head"),
                expected
            );
            let prepared = prepare_import(&root.0).expect("stage identical retry");
            let duplicate = store
                .publish(
                    "main",
                    HeadExpectation::Revision(expected.clone()),
                    &prepared.graph,
                    &prepared.index,
                )
                .expect("explicit retry after resolving uncertain response");
            assert!(!duplicate.published);
            assert_eq!(duplicate.revision, expected);
            assert_eq!(
                store.verify().expect("no duplicate history").revision_count,
                1
            );
        } else {
            assert!(
                matches!(store.head(None), Err(StoreError::UnknownHead(_))),
                "{phase:?}"
            );
            let connection = store.connect().expect("inspect rollback");
            for table in [
                "components",
                "objects",
                "source_attachments",
                "revisions",
                "revision_entities",
                "revision_objects",
                "revision_sources",
                "revision_edges",
                "revision_origins",
                "index_documents",
                "heads",
            ] {
                let count: i64 = connection
                    .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                        row.get(0)
                    })
                    .expect("count complete rollback rows");
                assert_eq!(count, 0, "partial {table} rows at {phase:?}");
            }
        }
    }
}

struct TestRoot(PathBuf);
impl Drop for TestRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
