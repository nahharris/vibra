use super::{
    CodebaseStore, HeadExpectation, StoreAccess, StoreError,
    ensure_supported_record_format,
};
use crate::codebase::import::prepare_import;
use sha2::Digest;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;

struct TestWorkspace(PathBuf);

impl TestWorkspace {
    fn new() -> Self {
        let serial = super::NEXT_TEMP_FILE.fetch_add(1, super::Ordering::Relaxed);
        let path = std::env::temp_dir()
            .canonicalize()
            .expect("canonical temporary directory")
            .join(format!(
                "vibra-codebase-reindex-race-{}-{serial}",
                std::process::id()
            ));
        std::fs::create_dir(&path).expect("create isolated workspace");
        Self(path)
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
fn reindex_rejects_a_publication_that_changes_its_snapshot_before_replacement() {
    let workspace = TestWorkspace::new();
    workspace.write(
            "project.vibon",
            "(record format: @project.v1 package: (record name: \"reindex-race\" version: \"0.1.0\") targets: (array (record name: @app kind: @bin root: \"src/app\" entry: @app.main.main effects: (array))) dependencies: (dict))\n",
        );
    workspace.write("src/app/main.vib", "(defn main () void void)\n");
    let first = prepare_import(workspace.path()).expect("prepare initial graph");
    let publisher = CodebaseStore::init(workspace.path(), ".vibra/codebase.sqlite")
        .expect("initialize store");
    let first_receipt = publisher
        .publish("main", HeadExpectation::Absent, &first.graph, &first.index)
        .expect("publish initial revision");

    let reindex = CodebaseStore::open(
        workspace.path(),
        ".vibra/codebase.sqlite",
        StoreAccess::ReadWrite,
    )
    .expect("open reindex store before starting worker");
    enum ReindexProgress {
        Checkpoint,
        FailedBeforeCheckpoint(String),
    }
    let (progress_sender, progress_receiver) = mpsc::channel();
    let (continue_sender, continue_receiver) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let checkpoint_sender = progress_sender.clone();
        let result = reindex.reindex_with_before_publish(|| {
            let _ = checkpoint_sender.send(ReindexProgress::Checkpoint);
            let _ = continue_receiver.recv();
        });
        if let Err(error) = &result {
            let _ = progress_sender
                .send(ReindexProgress::FailedBeforeCheckpoint(error.to_string()));
        }
        result
    });

    match progress_receiver.recv() {
        Ok(ReindexProgress::Checkpoint) => {}
        Ok(ReindexProgress::FailedBeforeCheckpoint(error)) => {
            panic!("reindex failed before its prepublication checkpoint: {error}");
        }
        Err(mpsc::RecvError) => {
            panic!("reindex worker exited before its prepublication checkpoint");
        }
    }
    workspace.write(
        "src/app/main.vib",
        "(defn helper () void void)\n(defn main () void (helper) void)\n",
    );
    let second = prepare_import(workspace.path()).expect("prepare concurrent revision");
    let second_receipt = publisher
        .publish(
            "main",
            HeadExpectation::Revision(first_receipt.revision.clone()),
            &second.graph,
            &second.index,
        )
        .expect("publish while reindex is rebuilding");
    continue_sender
        .send(())
        .expect("release reindex checkpoint");

    let result = worker.join().expect("reindex worker completes");
    assert!(
        matches!(result, Err(StoreError::StaleHead(_))),
        "{result:?}"
    );
    assert_eq!(
        publisher.head(None).expect("head stays on new revision"),
        second_receipt.revision
    );
    assert_eq!(
        publisher
            .verify()
            .expect("indexes remain consistent")
            .revision_count,
        2
    );
}

#[test]
fn generated_compiler_registry_records_match_the_canonical_trust_fingerprint() {
    let host_snapshot = vibra_ir::external::host::registry_snapshot();
    let minor = host_snapshot
        .split_whitespace()
        .nth(2)
        .and_then(|version| version.strip_prefix("1."))
        .and_then(|minor| minor.parse::<u32>().ok())
        .expect("host registry minor version");
    let host_payload = super::canonical_vibon_frame(&format!(
            "(record format: @codebase-registry.v1 provider: @host version: {} minor: {}u32 snapshot: {})\n",
            super::canonical_string("vibra_v1"),
            minor,
            super::canonical_string(&host_snapshot),
        ))
        .expect("canonical host record");
    let host_fingerprint = super::format_address(&super::hash_with_domain(
        b"vibra-codebase-registry-v1\0",
        host_payload.as_bytes(),
    ));
    let host_record = format!(
        "(record kind: @registry fingerprint: {} registry: {})",
        super::canonical_string(&host_fingerprint),
        host_payload.trim_end(),
    );

    let roles = vibra_ir::external::RoleTypes::new(None);
    let mut compiler_rows = vibra_ir::external::CompilerIntrinsic::all()
            .into_iter()
            .map(|intrinsic| {
                let parameters = intrinsic
                    .type_parameters()
                    .iter()
                    .map(|parameter| super::canonical_string(parameter))
                    .collect::<Vec<_>>();
                let signature = vibra_ir::canonical_type(&vibra_ir::Type::Function(Box::new(
                    intrinsic.signature(&roles),
                )));
                (
                    intrinsic.symbol().to_owned(),
                    format!(
                        "(record symbol: {} type-parameters: (array{}) signature: {} native: {})",
                        super::canonical_string(intrinsic.symbol()),
                        if parameters.is_empty() {
                            String::new()
                        } else {
                            format!(" {}", parameters.join(" "))
                        },
                        super::canonical_string(&signature),
                        intrinsic.is_native(),
                    ),
                )
            })
            .collect::<Vec<_>>();
    compiler_rows.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    let raw_rows = compiler_rows
        .iter()
        .map(|(_, row)| row.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let compiler_payload = super::canonical_vibon_frame(&format!(
            "(record format: @codebase-registry.v1 provider: @compiler version: {} rows: (array {}))\n",
            super::canonical_string("compiler-v1"),
            raw_rows,
        ))
        .expect("canonical compiler record");
    let compiler_fingerprint = super::format_address(&super::hash_with_domain(
        b"vibra-codebase-registry-v1\0",
        compiler_payload.as_bytes(),
    ));
    let compiler_record = format!(
        "(record kind: @registry fingerprint: {} registry: {})",
        super::canonical_string(&compiler_fingerprint),
        compiler_payload.trim_end(),
    );

    let stdlib = super::embedded_stdlib().expect("embedded standard library");
    let mut modules = stdlib
        .modules()
        .iter()
        .map(|module| {
            let identity = format!(
                "@{}@{}/{}",
                stdlib.package().name(),
                stdlib.package().version(),
                module.atom(),
            );
            let digest: [u8; 32] = sha2::Sha256::digest(module.bytes()).into();
            format!(
                "(record module: {} source-hash: {})",
                super::canonical_string(&identity),
                super::canonical_string(&super::format_address(&digest)),
            )
        })
        .collect::<Vec<_>>();
    modules.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
    let stdlib_record = format!(
        "(record kind: @stdlib package-name: {} package-version: {} manifest: {} modules: {})",
        super::canonical_string(stdlib.package().name()),
        super::canonical_string(stdlib.package().version()),
        super::canonical_string(stdlib.manifest_hash()),
        vibra_ir::canonical_array(&modules),
    );

    let trusted = [compiler_record, host_record, stdlib_record]
        .into_iter()
        .map(|record| {
            let frame = super::canonical_vibon_frame(&format!("{record}\n"))
                .expect("canonical trusted record");
            vibra_syntax::parse_data("trusted-record.vibon", &frame)
                .expect("parse trusted record")
                .into_data()
                .expect("decoded trusted record")
        })
        .collect::<Vec<_>>();

    super::validate_trusted_records(&trusted)
        .expect("generated registry records match current trust data");
}

#[test]
fn trusted_external_edges_require_matching_provider_and_known_symbol() {
    let workspace = TestWorkspace::new();
    workspace.write(
        "project.vibon",
        "(record format: @project.v1 package: (record name: \"trusted-edge-check\" version: \"0.1.0\") targets: (array (record name: @app kind: @bin root: \"src/app\" entry: @app.main.main effects: (array))) dependencies: (dict))\n",
    );
    workspace.write(
        "src/app/main.vib",
        "(defn main () void (do (u32.add 1u32 2u32) void))\n",
    );
    let prepared =
        prepare_import(workspace.path()).expect("prepare trusted-edge fixture");
    let revision_frame = crate::codebase::graph::validate_revision_payload(
        prepared.graph.revision.address.as_str(),
        &prepared.graph.revision.payload,
    )
    .expect("validated revision frame");
    let fingerprint_for = |provider: &str| {
        super::array_field(&revision_frame, "trusted")
            .expect("trusted array")
            .iter()
            .find_map(|entry| {
                let kind = super::data_atom(super::required_field(entry, "kind").ok()?)
                    .ok()?;
                if kind != "registry" {
                    return None;
                }
                let registry = super::required_field(entry, "registry").ok()?;
                let actual =
                    super::data_atom(super::required_field(registry, "provider").ok()?)
                        .ok()?;
                (actual == provider).then(|| {
                    super::data_string(
                        super::required_field(entry, "fingerprint")
                            .expect("fingerprint"),
                    )
                    .expect("fingerprint string")
                })
            })
            .expect("provider registry fingerprint")
    };
    let compiler = fingerprint_for("compiler");
    let host = fingerprint_for("host");

    super::validate_trusted_edge_identity(
        "@compiler:u32.add",
        &compiler,
        &revision_frame,
    )
    .expect("known compiler symbol matches compiler registry");
    assert!(
        super::validate_trusted_edge_identity(
            "@compiler:u32.add",
            &host,
            &revision_frame,
        )
        .is_err()
    );
    assert!(
        super::validate_trusted_edge_identity(
            "@host:no-such-symbol",
            &host,
            &revision_frame,
        )
        .is_err()
    );

    let component_payload = super::canonical_vibon_frame(&format!(
        "(record format: @codebase-component.v1 members: (array (record references: (array (record part: (array) kind: @value-reference target: (record kind: @trusted-external identity: {} registry: {}))))))\n",
        super::canonical_string("@compiler:u32.add"),
        super::canonical_string(&host),
    ))
    .expect("canonical malformed trusted edge component");
    let component =
        crate::codebase::graph::parse_canonical_frame(component_payload.as_bytes())
            .expect("parse malformed trusted edge component");
    let connection =
        rusqlite::Connection::open_in_memory().expect("in-memory connection");
    assert!(
        super::read_member_references(
            &connection,
            &component,
            "",
            &[0; 32],
            0,
            &std::collections::BTreeSet::new(),
            &revision_frame,
        )
        .is_err()
    );

    let mut candidate = prepared.graph.clone();
    let edge = candidate
        .edges
        .iter_mut()
        .find(|edge| {
            matches!(
                &edge.target,
                crate::codebase::graph::EdgeTarget::TrustedExternal { identity, .. }
                    if identity == "@compiler:u32.add"
            )
        })
        .expect("compiler edge in candidate graph");
    edge.target = crate::codebase::graph::EdgeTarget::TrustedExternal {
        identity: "@compiler:u32.add".to_owned(),
        registry: host,
    };
    assert!(super::validate_graph(&candidate).is_err());
}

#[test]
fn definition_node_schema_accepts_atom_values_and_closed_lowercase_bytes() {
    let parse_node = |text: &str| {
        let frame = super::canonical_vibon_frame(text).expect("canonical node frame");
        crate::codebase::graph::parse_canonical_frame(frame.as_bytes())
            .expect("parse canonical node")
    };
    let atom_value = parse_node("(record kind: @atom-value value: \"ready\")");
    super::validate_definition_node(&atom_value).expect("atom-value node is accepted");
    let bytes = parse_node("(record kind: @bytes value: \"00af\")");
    super::validate_definition_node(&bytes).expect("lowercase byte hex is accepted");

    for malformed in [
        "(record kind: @atom-value value: \"@ready\")",
        "(record kind: @atom-value value: \"ready\" extra: @void)",
        "(record kind: @bytes value: \"0\")",
        "(record kind: @bytes value: \"0G\")",
        "(record kind: @bytes value: \"AF\")",
        "(record kind: @bytes value: \"00\" extra: @void)",
    ] {
        let node = parse_node(malformed);
        assert!(
            super::validate_definition_node(&node).is_err(),
            "malformed definition node was accepted: {malformed}"
        );
    }
}

#[test]
fn definition_node_leaf_values_follow_their_closed_scalar_grammar() {
    let parse_node = |text: &str| {
        let frame = super::canonical_vibon_frame(text).expect("canonical node frame");
        crate::codebase::graph::parse_canonical_frame(frame.as_bytes())
            .expect("parse canonical node")
    };

    for valid in [
        "(record kind: @atom value: \"body\")",
        "(record kind: @label name: \"label\")",
        "(record kind: @string value: \"text\")",
        "(record kind: @character value: \"λ\")",
        "(record kind: @integer type: @i8 value: \"-8\")",
        "(record kind: @integer type: @u64 value: \"18446744073709551615\")",
        "(record kind: @float type: @f32 value: \"1.25\")",
        "(record kind: @float type: @f64 value: \"nan\")",
        "(record kind: @reference identity: \"@demo@0.1.0/app.main.run\")",
        "(record kind: @reference identity: \"@compiler:u32.add\")",
        "(record kind: @reference identity: \"@host:clock.now\")",
        "(record kind: @binder binder-kind: @value index: 0u64)",
        "(record kind: @binder binder-kind: @type index: 12u64)",
    ] {
        let node = parse_node(valid);
        super::validate_definition_node(&node).unwrap_or_else(|error| {
            panic!("valid definition node rejected: {valid}: {error}")
        });
    }

    for malformed in [
        "(record kind: @atom value: @body)",
        "(record kind: @label name: 1)",
        "(record kind: @string value: @text)",
        "(record kind: @character value: \"\")",
        "(record kind: @character value: \"ab\")",
        "(record kind: @integer type: @str value: \"1\")",
        "(record kind: @integer type: @u8 value: \"256\")",
        "(record kind: @integer type: @i32 value: \"01\")",
        "(record kind: @integer type: @u8 value: \"-1\")",
        "(record kind: @float type: @i32 value: \"1.0\")",
        "(record kind: @float type: @f32 value: \"1e+2\")",
        "(record kind: @float type: @f32 value: \"1.0f32\")",
        "(record kind: @reference identity: \"not-an-identity\")",
        "(record kind: @reference identity: \"@host:\")",
        "(record kind: @binder binder-kind: @expression index: 0u64)",
        "(record kind: @binder binder-kind: @value index: 0)",
        "(record kind: @binder binder-kind: @type index: -1u64)",
        "(record kind: @list items: (array (record kind: @integer type: @i8 value: \"128\")))",
    ] {
        let node = parse_node(malformed);
        assert!(
            super::validate_definition_node(&node).is_err(),
            "malformed definition node was accepted: {malformed}"
        );
    }
}

#[test]
fn deeply_nested_definition_lists_are_validated_with_a_bounded_stack() {
    let depth = 512;
    let mut text = String::with_capacity(depth * 44 + 40);
    for _ in 0..depth {
        text.push_str("(record kind: @list items: (array ");
    }
    text.push_str("(record kind: @atom value: \"leaf\")");
    for _ in 0..depth {
        text.push_str("))");
    }
    text.push('\n');

    let document = vibra_syntax::parse_data("deep-definition.vibon", &text)
        .expect("valid VIBON extension");
    assert!(document.accepted(), "deep definition parses as VIBON");
    let node = document.into_data().expect("decoded deep definition");
    let worker = std::thread::Builder::new()
        .stack_size(64 * 1024)
        .spawn(move || super::validate_definition_node(&node))
        .expect("start bounded-stack validator");
    worker
        .join()
        .expect("definition validation must not overflow the host stack")
        .expect("deep valid list definition is accepted");
}

#[test]
fn recognized_future_record_formats_are_unsupported_versions() {
    assert!(matches!(
        ensure_supported_record_format(
            "checked-component.v2",
            "checked-component.v1",
            "checked component",
        ),
        Err(StoreError::UnsupportedVersion {
            found: 2,
            supported: 1,
        })
    ));
}

#[test]
fn an_existing_revision_can_be_selected_by_multiple_heads_and_reselected() {
    let workspace = TestWorkspace::new();
    workspace.write(
            "project.vibon",
            "(record format: @project.v1 package: (record name: \"head-reuse\" version: \"0.1.0\") targets: (array (record name: @app kind: @bin root: \"src/app\" entry: @app.main.main effects: (array))) dependencies: (dict))\n",
        );
    workspace.write("src/app/main.vib", "(defn main () void void)\n");
    let first = prepare_import(workspace.path()).expect("prepare first revision");
    let store = CodebaseStore::init(workspace.path(), ".vibra/codebase.sqlite")
        .expect("initialize store");
    let first_receipt = store
        .publish("main", HeadExpectation::Absent, &first.graph, &first.index)
        .expect("publish first revision");

    workspace.write(
        "src/app/main.vib",
        "(defn helper () void void)\n(defn main () void (helper) void)\n",
    );
    let second = prepare_import(workspace.path()).expect("prepare second revision");
    let second_receipt = store
        .publish(
            "main",
            HeadExpectation::Revision(first_receipt.revision.clone()),
            &second.graph,
            &second.index,
        )
        .expect("publish second revision");

    store
        .publish(
            "archive",
            HeadExpectation::Absent,
            &first.graph,
            &first.index,
        )
        .expect("select old revision under another head");
    store
        .publish(
            "main",
            HeadExpectation::Revision(second_receipt.revision.clone()),
            &first.graph,
            &first.index,
        )
        .expect("move main back to stored revision");

    assert_eq!(
        store.head(Some("main")).expect("main head"),
        first_receipt.revision
    );
    assert_eq!(
        store.head(Some("archive")).expect("archive head"),
        first_receipt.revision
    );
    assert_eq!(
        store
            .verify()
            .expect("verify reused revision")
            .revision_count,
        2
    );
}

#[test]
fn a_failed_head_write_rolls_back_the_entire_publication() {
    let workspace = TestWorkspace::new();
    workspace.write(
            "project.vibon",
            "(record format: @project.v1 package: (record name: \"publish-abort\" version: \"0.1.0\") targets: (array (record name: @app kind: @bin root: \"src/app\" entry: @app.main.main effects: (array))) dependencies: (dict))\n",
        );
    workspace.write("src/app/main.vib", "(defn main () void void)\n");
    let prepared =
        prepare_import(workspace.path()).expect("prepare candidate revision");
    let store = CodebaseStore::init(workspace.path(), ".vibra/codebase.sqlite")
        .expect("initialize store");
    let fixture = store.connect().expect("connect corruption fixture");
    fixture
        .execute_batch(
            "CREATE TRIGGER fail_test_publication BEFORE INSERT ON heads
                 WHEN NEW.name='abort'
                 BEGIN SELECT RAISE(ABORT, 'injected publication interruption'); END;",
        )
        .expect("install aborting trigger");
    drop(fixture);

    let result = store.publish(
        "abort",
        HeadExpectation::Absent,
        &prepared.graph,
        &prepared.index,
    );

    assert!(
        result.is_err(),
        "injected head write must abort: {result:?}"
    );
    assert!(matches!(
        store.head(Some("abort")),
        Err(StoreError::UnknownHead(_))
    ));
    assert_eq!(
        store
            .verify()
            .expect("verify rolled-back store")
            .revision_count,
        0
    );
    let check = store.connect().expect("inspect rolled-back rows");
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
    ] {
        let count = check
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get::<_, i64>(0)
            })
            .expect("count table rows");
        assert_eq!(count, 0, "publication left rows in {table}");
    }
}

#[test]
fn publish_refuses_to_advance_a_store_with_a_dangling_head() {
    let workspace = TestWorkspace::new();
    workspace.write(
        "project.vibon",
        "(record format: @project.v1 package: (record name: \"damaged-authority\" version: \"0.1.0\") targets: (array (record name: @app kind: @bin root: \"src/app\" entry: @app.main.main effects: (array))) dependencies: (dict))\n",
    );
    workspace.write("src/app/main.vib", "(defn main () void void)\n");
    let store = CodebaseStore::init(workspace.path(), ".vibra/codebase.sqlite")
        .expect("initialize store before corruption");
    let missing_revision = format!("sha256:{}", "f".repeat(64));
    let corruption =
        rusqlite::Connection::open(workspace.path().join(".vibra/codebase.sqlite"))
            .expect("open store corruption fixture");
    corruption
        .pragma_update(None, "foreign_keys", false)
        .expect("disable fixture foreign-key enforcement");
    corruption
        .execute(
            "INSERT INTO heads(name,revision_address) VALUES (?1,?2)",
            rusqlite::params!["main", missing_revision],
        )
        .expect("insert dangling head with foreign keys disabled");
    drop(corruption);

    let prepared =
        prepare_import(workspace.path()).expect("prepare candidate revision");
    let result = store.publish(
        "main",
        HeadExpectation::Absent,
        &prepared.graph,
        &prepared.index,
    );
    assert!(matches!(result, Err(StoreError::Corrupt(_))), "{result:?}");

    let check =
        rusqlite::Connection::open(workspace.path().join(".vibra/codebase.sqlite"))
            .expect("inspect authority after rejected publication");
    let revision_count: i64 = check
        .query_row("SELECT count(*) FROM revisions", [], |row| row.get(0))
        .expect("count published revisions");
    let head: String = check
        .query_row(
            "SELECT revision_address FROM heads WHERE name='main'",
            [],
            |row| row.get(0),
        )
        .expect("read preexisting dangling head");
    assert_eq!(revision_count, 0, "corrupt store accepted a revision");
    assert_eq!(head, missing_revision, "corrupt head was replaced");
}

#[test]
fn verify_detects_each_derived_index_mismatch() {
    let workspace = TestWorkspace::new();
    workspace.write(
        "project.vibon",
        "(record format: @project.v1 package: (record name: \"derived-index-check\" version: \"0.1.0\") targets: (array (record name: @app kind: @bin root: \"src/app\" entry: @app.main.main effects: (array))) dependencies: (dict))\n",
    );
    workspace.write(
        "src/app/main.vib",
        "(defn main () void (do (u32.add 1u32 2u32) void))\n",
    );
    let prepared = prepare_import(workspace.path()).expect("prepare checked revision");
    assert!(
        !prepared.graph.edges.is_empty(),
        "fixture must contain edges"
    );
    assert!(
        prepared
            .graph
            .objects
            .iter()
            .any(|object| !object.origins.is_empty()),
        "fixture must contain origins"
    );
    let store = CodebaseStore::init(workspace.path(), ".vibra/codebase.sqlite")
        .expect("initialize store");
    let receipt = store
        .publish(
            "main",
            HeadExpectation::Absent,
            &prepared.graph,
            &prepared.index,
        )
        .expect("publish valid graph");
    let compiler_edge = prepared
        .graph
        .edges
        .iter()
        .find(|edge| {
            matches!(
                &edge.target,
                crate::codebase::graph::EdgeTarget::TrustedExternal { identity, .. }
                    if identity == "@compiler:u32.add"
            )
        })
        .expect("compiler edge in checked graph");
    let connection = store.connect().expect("connect for derived edge fixture");
    let deleted = connection
        .execute(
            "DELETE FROM revision_edges WHERE revision_address=?1 AND edge_index=0",
            [receipt.revision.as_str()],
        )
        .expect("delete one cached edge");
    assert_eq!(deleted, 1, "fixture did not delete an edge row");
    drop(connection);

    let read = store
        .read(&receipt.revision, compiler_edge.owner.as_str())
        .expect("read direct references from the canonical component frame");
    assert!(
        read.references.iter().any(|reference| matches!(
            &reference.target,
            super::ReadTarget::TrustedExternal { identity, .. }
                if identity == "@compiler:u32.add"
        )),
        "read trusted the deleted revision_edges cache: {:?}",
        read.references
    );
    assert!(matches!(store.verify(), Err(StoreError::Corrupt(_))));
    store.reindex().expect("repair the missing cached edge");

    let connection = store
        .connect()
        .expect("connect for remaining derived indexes");

    for (savepoint, mutation) in [
        (
            "missing_origin",
            "DELETE FROM revision_origins WHERE revision_address=?1 AND origin_index=0",
        ),
        (
            "changed_cached_index",
            "UPDATE index_documents SET payload_vibon=X'00' WHERE revision_address=?1",
        ),
    ] {
        connection
            .execute_batch(&format!("SAVEPOINT {savepoint}"))
            .expect("create isolated corruption savepoint");
        let changed = connection
            .execute(mutation, [receipt.revision.as_str()])
            .expect("corrupt one derived index");
        assert_eq!(changed, 1, "fixture did not alter {savepoint}");
        let result = super::verify_rebuilt_indexes(&connection);
        assert!(
            matches!(result, Err(StoreError::Corrupt(_))),
            "verification accepted {savepoint}: {result:?}"
        );
        connection
            .execute_batch(&format!("ROLLBACK TO {savepoint}; RELEASE {savepoint}"))
            .expect("restore valid derived index rows");
    }
}

#[test]
fn verify_rejects_dangling_foreign_key_rows() {
    let workspace = TestWorkspace::new();
    let store = CodebaseStore::init(workspace.path(), ".vibra/codebase.sqlite")
        .expect("initialize empty store");
    let connection =
        rusqlite::Connection::open(workspace.path().join(".vibra/codebase.sqlite"))
            .expect("open store for foreign-key fixture");
    connection
        .pragma_update(None, "foreign_keys", false)
        .expect("disable fixture foreign-key enforcement");
    let absent_revision = format!("sha256:{}", "1".repeat(64));
    let absent_source = format!("sha256:{}", "2".repeat(64));
    connection
        .execute(
            "INSERT INTO revision_sources(revision_address,source_id,source_address,source_hash,origin) VALUES (?1,'orphan',?2,?2,'src/orphan.vib')",
            rusqlite::params![absent_revision, absent_source],
        )
        .expect("insert dangling foreign keys with checks disabled");
    drop(connection);

    assert!(matches!(store.verify(), Err(StoreError::Corrupt(_))));
}

#[test]
fn read_only_open_rejects_a_dangling_named_head() {
    let workspace = TestWorkspace::new();
    let store = CodebaseStore::init(workspace.path(), ".vibra/codebase.sqlite")
        .expect("initialize empty store");
    drop(store);
    let connection =
        rusqlite::Connection::open(workspace.path().join(".vibra/codebase.sqlite"))
            .expect("open store for dangling-head fixture");
    connection
        .pragma_update(None, "foreign_keys", false)
        .expect("disable fixture foreign-key enforcement");
    connection
        .execute(
            "INSERT INTO heads(name,revision_address) VALUES ('main',?1)",
            [format!("sha256:{}", "3".repeat(64))],
        )
        .expect("insert dangling head");
    drop(connection);

    let opened = CodebaseStore::open(
        workspace.path(),
        ".vibra/codebase.sqlite",
        StoreAccess::ReadOnly,
    );
    assert!(matches!(opened, Err(StoreError::Corrupt(_))), "{opened:?}");
}

#[test]
fn read_only_open_does_not_create_sidecars_for_a_wal_header_database() {
    let workspace = TestWorkspace::new();
    let store = CodebaseStore::init(workspace.path(), ".vibra/codebase.sqlite")
        .expect("initialize rollback-journal store");
    drop(store);
    let path = workspace.path().join(".vibra/codebase.sqlite");
    let connection =
        rusqlite::Connection::open(&path).expect("open journal-mode fixture");
    let mode: String = connection
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .expect("read initial journal mode");
    assert_eq!(mode.to_ascii_lowercase(), "delete");
    let mode: String = connection
        .query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))
        .expect("switch fixture to WAL mode");
    assert_eq!(mode.to_ascii_lowercase(), "wal");
    drop(connection);

    let entries = |directory: &Path| {
        std::fs::read_dir(directory)
            .expect("read store directory")
            .map(|entry| entry.expect("directory entry").file_name())
            .collect::<std::collections::BTreeSet<_>>()
    };
    let before = entries(&workspace.path().join(".vibra"));
    assert_eq!(
        before.len(),
        1,
        "closing the last WAL connection removes sidecars"
    );

    let opened = CodebaseStore::open(
        workspace.path(),
        ".vibra/codebase.sqlite",
        StoreAccess::ReadOnly,
    );
    assert!(
        matches!(opened, Err(StoreError::UnsupportedFormat(_))),
        "{opened:?}"
    );
    let after = entries(&workspace.path().join(".vibra"));
    assert_eq!(
        before, after,
        "read-only open changed directory entries: {opened:?}"
    );
}

#[test]
fn reindex_refuses_a_source_attachment_with_mismatched_hashes() {
    let workspace = TestWorkspace::new();
    let store = CodebaseStore::init(workspace.path(), ".vibra/codebase.sqlite")
        .expect("initialize empty store");
    let payload = super::canonical_vibon_frame(
        "(record format: @source.v1 text: \"unattached bytes\")\n",
    )
    .expect("canonical source wrapper");
    let connection =
        rusqlite::Connection::open(workspace.path().join(".vibra/codebase.sqlite"))
            .expect("open store for source corruption fixture");
    connection
        .execute(
            "INSERT INTO source_attachments(address,raw_hash,payload_vibon) VALUES (?1,?2,?3)",
            rusqlite::params![
                format!("sha256:{}", "a".repeat(64)),
                format!("sha256:{}", "b".repeat(64)),
                payload.as_bytes(),
            ],
        )
        .expect("insert source with false content addresses");
    drop(connection);

    assert!(matches!(store.reindex(), Err(StoreError::Corrupt(_))));
    assert!(matches!(store.verify(), Err(StoreError::Corrupt(_))));
}

#[test]
fn apply_export_rejects_malformed_plan_addresses_at_the_service_boundary() {
    let workspace = TestWorkspace::new();
    let store = CodebaseStore::init(workspace.path(), ".vibra/codebase.sqlite")
        .expect("initialize empty store");
    let result = store.apply_export(
        &format!("sha256:{}", "0".repeat(64)),
        "exported",
        "garbage",
    );
    assert!(
        matches!(result, Err(StoreError::InvalidInput(_))),
        "{result:?}"
    );
}

#[test]
fn read_only_journal_recovery_error_guides_write_enabled_reindex() {
    let error = rusqlite::Error::SqliteFailure(
        rusqlite::ffi::Error {
            code: rusqlite::ffi::ErrorCode::ReadOnly,
            extended_code: rusqlite::ffi::SQLITE_READONLY_ROLLBACK,
        },
        Some("attempt to write a readonly database".to_owned()),
    );
    let mapped = super::map_sqlite_error(error);
    let StoreError::Io(message) = mapped else {
        panic!("rollback journal recovery must remain an I/O failure: {mapped:?}");
    };
    assert!(message.contains("vibra codebase reindex"), "{message}");
    assert!(message.contains("read-only operation"), "{message}");
}

#[test]
fn process_interruption_before_commit_recovers_without_partial_publication() {
    const CHILD_ROOT_ENV: &str = "VIBRA_CODEBASE_CRASH_TEST_ROOT";
    const CHILD_EXIT_CODE: i32 = 77;

    if let Some(root) = std::env::var_os(CHILD_ROOT_ENV) {
        let root = PathBuf::from(root);
        let prepared = prepare_import(&root).expect("prepare child candidate");
        let store = CodebaseStore::open(
            &root,
            ".vibra/codebase.sqlite",
            StoreAccess::ReadWrite,
        )
        .expect("open child store");
        let _ = store.publish_with_before_commit(
            "main",
            HeadExpectation::Absent,
            &prepared.graph,
            &prepared.index,
            || std::process::exit(CHILD_EXIT_CODE),
        );
        panic!("child publication returned without exiting before commit");
    }

    let workspace = TestWorkspace::new();
    workspace.write(
        "project.vibon",
        "(record format: @project.v1 package: (record name: \"crash-recovery\" version: \"0.1.0\") targets: (array (record name: @app kind: @bin root: \"src/app\" entry: @app.main.main effects: (array))) dependencies: (dict))\n",
    );
    workspace.write("src/app/main.vib", "(defn main () void void)\n");
    let store = CodebaseStore::init(workspace.path(), ".vibra/codebase.sqlite")
        .expect("initialize crash-test store");
    drop(store);

    let status = Command::new(std::env::current_exe().expect("test binary path"))
        .args([
            "--exact",
            "codebase::store::tests::process_interruption_before_commit_recovers_without_partial_publication",
            "--nocapture",
        ])
        .env(CHILD_ROOT_ENV, workspace.path())
        .status()
        .expect("spawn crash-test child");
    assert_eq!(status.code(), Some(CHILD_EXIT_CODE));
    assert!(
        workspace
            .path()
            .join(".vibra/codebase.sqlite-journal")
            .is_file(),
        "forced cache spill should leave a hot rollback journal"
    );

    match CodebaseStore::open(
        workspace.path(),
        ".vibra/codebase.sqlite",
        StoreAccess::ReadOnly,
    ) {
        Ok(read_only) => {
            assert_eq!(
                read_only
                    .verify()
                    .expect("verify read-only recovery")
                    .revision_count,
                0
            );
        }
        Err(StoreError::Io(message))
            if message.to_ascii_lowercase().contains("readonly")
                || message.to_ascii_lowercase().contains("read-only") =>
        {
            // SQLite cannot roll back a hot journal through a read-only VFS.
            // Opening with write access performs its documented recovery.
            let recovered = CodebaseStore::open(
                workspace.path(),
                ".vibra/codebase.sqlite",
                StoreAccess::ReadWrite,
            )
            .expect("write-enabled open recovers the hot journal");
            recovered
                .reindex()
                .expect("reindex after write-enabled journal recovery");
            assert_eq!(
                recovered
                    .verify()
                    .expect("verify recovered store")
                    .revision_count,
                0
            );
            drop(recovered);
        }
        Err(error) => panic!("unexpected read-only recovery error: {error}"),
    }

    let reopened = CodebaseStore::open(
        workspace.path(),
        ".vibra/codebase.sqlite",
        StoreAccess::ReadOnly,
    )
    .expect("read-only reopen after recovery");
    assert_eq!(
        reopened
            .verify()
            .expect("verify after recovery")
            .revision_count,
        0
    );
}
