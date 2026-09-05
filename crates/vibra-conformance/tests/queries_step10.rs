//! Synthetic proof that structural query snapshots are real runner outputs.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use vibra_conformance::{
    ConformanceProfile, ConformanceRunner, Corpus, ProfileDispatcher, ReaderV1Handler,
};
use vibra_diagnostics::LineIndex;
use vibra_schema::SourcePositionQueryDocument;
use vibra_syntax::parse_source;

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

#[test]
fn the_runner_passes_real_query_snapshots_and_rejects_wrong_ones() {
    let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "vibra-conformance-query-step10-{serial}-{}",
        std::process::id()
    ));
    let case = root.join("V1-TOOL-query-synthetic");
    std::fs::create_dir_all(&case).expect("create temporary case");
    std::fs::write(
        case.join("case.toml"),
        r#"id = "V1-TOOL-query-synthetic"
rule = "V1-TOOL-query-synthetic"
profile = "reader-v1"

[inputs]
source = "input.vib"

[expect]
accepted = true

[[expect.queries]]
input = "input.vib"
offset = 16
snapshot = "query.json"
"#,
    )
    .expect("write temporary manifest");
    let source = "; 🌱\n(defn f (value str) str value)";
    std::fs::write(case.join("input.vib"), source).expect("write temporary source");
    let document =
        parse_source(PathBuf::from("input.vib"), source).expect("source loader");
    let query = document.query_position(16).expect("query result");
    let rendered = SourcePositionQueryDocument::render(&query, &LineIndex::new(source));
    let snapshot = format!(
        "{}\n",
        serde_json::to_string_pretty(&rendered).expect("query serializes")
    );
    std::fs::write(case.join("query.json"), snapshot).expect("write expected query");

    let corpus = Corpus::discover(&root).expect("temporary corpus");
    let dispatcher = ProfileDispatcher::new()
        .with_handler(ConformanceProfile::ReaderV1, ReaderV1Handler);
    let runner = ConformanceRunner::new(dispatcher);
    let report = runner.run(&corpus);
    assert!(report.is_success(), "query report: {report:?}");

    std::fs::write(case.join("query.json"), "{}\n").expect("write wrong query");
    let failed = runner.run(&corpus);
    assert_eq!(failed.failed(), 1);
    let reason = format!("{:?}", failed.cases());
    assert!(reason.contains("query snapshot mismatch"), "{reason}");

    let _ = std::fs::remove_dir_all(root);
}
