//! Reproducible local-store measurements; timings are evidence, not promises.

use std::collections::BTreeMap;
use std::fs;
use std::sync::{Arc, Barrier};
use std::time::Instant;

use vibra_workspace::codebase::import::prepare_import;
use vibra_workspace::codebase::store::{CodebaseStore, HeadExpectation, StoreAccess};

use crate::support::TempDir;

#[test]
#[ignore = "manual M1 local-store evidence; writes tmp/m1-store-measurements.vibon"]
fn measure_local_codebase_workload() {
    let root = TempDir::new("codebase-measurements");
    root.write("project.vibon", "(record format: @project.v1 package: (record name: \"store-measure\" version: \"0.1.0\") targets: (array (record name: @app kind: @bin root: \"src/app\" entry: @app.main.main effects: (array)) (record name: @math kind: @lib root: \"src/math\")) dependencies: (dict))\n");
    root.write(
        "src/app/main.vib",
        "(import many @math.many)\n(defn main () void (many.item-0) void)\n",
    );
    let body = (0..128)
        .map(|index| {
            format!("(defn item-{index} () u32 visibility: @public {index}u32)\n")
        })
        .collect::<String>();
    root.write("src/math/many.vib", &body);
    let mut metrics = BTreeMap::new();
    let started = Instant::now();
    let first = prepare_import(root.path()).expect("baseline workload checks");
    metrics.insert("prepare-first-us", started.elapsed().as_micros());
    let store =
        CodebaseStore::init(root.path(), ".vibra/codebase.sqlite").expect("initialize");
    let started = Instant::now();
    let published = store
        .publish("main", HeadExpectation::Absent, &first.graph, &first.index)
        .expect("publish");
    metrics.insert("publish-first-us", started.elapsed().as_micros());
    metrics.insert(
        "import-first-us",
        metrics["prepare-first-us"] + metrics["publish-first-us"],
    );
    let started = Instant::now();
    let reopened = CodebaseStore::open(
        root.path(),
        ".vibra/codebase.sqlite",
        StoreAccess::ReadOnly,
    )
    .expect("reopen");
    assert_eq!(reopened.head(None).expect("head"), published.revision);
    metrics.insert("reopen-new-connection-us", started.elapsed().as_micros());
    let executable = std::env::current_exe().expect("measurement test executable");
    let cli = executable
        .parent()
        .and_then(std::path::Path::parent)
        .expect("target debug directory")
        .join(if cfg!(windows) { "vibra.exe" } else { "vibra" });
    assert!(cli.is_file(), "build vibra-cli before running measurements");
    let started = Instant::now();
    let cold_process = std::process::Command::new(cli)
        .arg("--workspace")
        .arg(root.path())
        .args(["codebase", "head"])
        .output()
        .expect("launch independent cold process");
    metrics.insert("reopen-new-process-us", started.elapsed().as_micros());
    assert!(
        cold_process.status.success(),
        "{}",
        String::from_utf8_lossy(&cold_process.stderr)
    );
    assert_eq!(
        String::from_utf8(cold_process.stdout)
            .expect("UTF-8 head response")
            .trim(),
        format!("main -> {}", published.revision)
    );
    let mapping = first
        .graph
        .entities
        .iter()
        .find(|entity| entity.key.canonical().contains("/app.main.main\""))
        .expect("entry entity");
    let mut lookup = Vec::new();
    let mut reads = Vec::new();
    for _ in 0..10 {
        let started = Instant::now();
        reopened
            .lookup(&published.revision, &mapping.key.canonical())
            .expect("indexed lookup");
        lookup.push(started.elapsed().as_micros());
        let started = Instant::now();
        let read = reopened
            .read(&published.revision, mapping.object.as_str())
            .expect("addressed read with direct relations");
        assert!(!read.references.is_empty());
        reads.push(started.elapsed().as_micros());
    }
    lookup.sort_unstable();
    reads.sort_unstable();
    metrics.insert("lookup-median-us", lookup[5]);
    metrics.insert("lookup-max-us", lookup[9]);
    metrics.insert("read-median-us", reads[5]);
    metrics.insert("read-max-us", reads[9]);
    let started = Instant::now();
    let identical =
        prepare_import(root.path()).expect("capture and check unchanged import");
    metrics.insert("prepare-deduplicating-us", started.elapsed().as_micros());
    let started = Instant::now();
    let duplicate = store
        .publish(
            "main",
            HeadExpectation::Revision(published.revision.clone()),
            &identical.graph,
            &identical.index,
        )
        .expect("deduplicating import");
    assert!(!duplicate.published);
    metrics.insert("publish-deduplicating-us", started.elapsed().as_micros());
    metrics.insert(
        "import-deduplicating-us",
        metrics["prepare-deduplicating-us"] + metrics["publish-deduplicating-us"],
    );
    root.write(
        "src/math/many.vib",
        &body.replacen("@public 0u32", "@public 1u32", 1),
    );
    let started = Instant::now();
    let second = prepare_import(root.path()).expect("changed workload checks");
    metrics.insert("prepare-changed-us", started.elapsed().as_micros());
    let barrier = Arc::new(Barrier::new(2));
    let reader_barrier = Arc::clone(&barrier);
    let reader_store = CodebaseStore::open(
        root.path(),
        ".vibra/codebase.sqlite",
        StoreAccess::ReadOnly,
    )
    .expect("concurrent client");
    let old_revision = published.revision.clone();
    let old_object = mapping.object.as_str().to_owned();
    let reader = std::thread::spawn(move || {
        reader_barrier.wait();
        for _ in 0..20 {
            reader_store
                .read(&old_revision, &old_object)
                .expect("pinned read during publication");
        }
    });
    barrier.wait();
    let started = Instant::now();
    let candidate = store.publish(
        "main",
        HeadExpectation::Revision(published.revision.clone()),
        &second.graph,
        &second.index,
    );
    metrics.insert("publish-with-reader-us", started.elapsed().as_micros());
    reader.join().expect("reader completed");
    let changed =
        match candidate {
            Ok(changed) => {
                metrics.insert("writer-busy", 0);
                changed
            }
            Err(error) => {
                assert_eq!(error.code(), vibra_diagnostics::DiagnosticCode::StoreBusy);
                metrics.insert("writer-busy", 1);
                let started = Instant::now();
                let changed = store.publish(
                "main", HeadExpectation::Revision(published.revision),
                &second.graph, &second.index,
            ).expect("explicit retry with original expectation after reader completes");
                metrics.insert("publish-retry-us", started.elapsed().as_micros());
                changed
            }
        };
    assert!(changed.published);
    metrics.insert("object-count", first.graph.objects.len() as u128);
    metrics.insert("source-count", first.graph.sources.len() as u128);
    metrics.insert("relation-count", first.graph.edges.len() as u128);
    metrics.insert(
        "store-bytes",
        u128::from(
            fs::metadata(root.path().join(".vibra/codebase.sqlite"))
                .expect("store metadata")
                .len(),
        ),
    );
    let facts = metrics
        .iter()
        .map(|(key, value)| format!("@{key} {value}u64"))
        .collect::<Vec<_>>()
        .join(" ");
    let sqlite = rusqlite::Connection::open(root.path().join(".vibra/codebase.sqlite"))
        .expect("inspect actual SQLite configuration");
    let journal: String = sqlite
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .expect("journal setting");
    let synchronous: u32 = sqlite
        .pragma_query_value(None, "synchronous", |row| row.get(0))
        .expect("synchronization setting");
    assert_eq!(journal, "delete");
    assert_eq!(synchronous, 2);
    let artifact = format!(
        "(record format: @codebase-measurements.v1 profile: @default-unoptimized-test relation-read: @included-in-addressed-read stored-revisions: 2u64 platform: {} architecture: {} sqlite: {} binding: \"rusqlite 0.42.0\" journal: @{} synchronous: @full reopen: @new-process-and-new-connection-warm-filesystem-cache reopen-samples: 1u64 declarations: 128u64 publication-samples: 1u64 read-samples: 10u64 concurrent-reads: 20u64 metrics: (dict {facts}))\n",
        vibra_ir::canonical_string(std::env::consts::OS),
        vibra_ir::canonical_string(std::env::consts::ARCH),
        vibra_ir::canonical_string(rusqlite::version()),
        journal,
    );
    let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("workspace root");
    fs::create_dir_all(repository.join("tmp")).expect("measurement directory");
    fs::write(
        repository.join("tmp/m1-store-measurements.vibon"),
        &artifact,
    )
    .expect("measurement artifact");
    println!("{artifact}");
}
