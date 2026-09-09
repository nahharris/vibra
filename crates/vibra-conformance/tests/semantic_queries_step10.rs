//! The tooling-v1 handler must produce real semantic query snapshots.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};

use vibra_conformance::{
    ConformanceProfile, ConformanceRunner, Corpus, ProfileDispatcher,
    ToolingV1QueryHandler,
};
use vibra_diagnostics::LineIndex;
use vibra_schema::WorkspacePositionQueryDocument;
use vibra_workspace::WorkspaceSnapshot;

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

#[test]
fn the_tooling_handler_passes_real_snapshots_and_rejects_wrong_facts() {
    let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let parent = fs::canonicalize(std::env::temp_dir()).expect("canonical temp parent");
    let root = parent.join(format!(
        "vibra-conformance-semantic-query-{serial}-{}",
        std::process::id()
    ));
    let tree = root.join("V1-TOOL-query-synthetic").join("tree");
    fs::create_dir_all(tree.join("src")).expect("tree source directory");
    let case = tree.parent().expect("case directory").to_path_buf();
    fs::write(
        case.join("case.toml"),
        r#"id = "V1-TOOL-query-synthetic"
rule = "V1-TOOL-workspace-position-query"
profile = "tooling-v1"
operation = "query"

[inputs]
project = "tree/project.vibon"
tree = "tree"

[expect]
accepted = true

[[expect.queries]]
input = "tree/src/main.vib"
offset = 64
snapshot = "query.json"
"#,
    )
    .expect("write manifest");
    fs::write(
        tree.join("project.vibon"),
        "(record format: @project.v1 package: (record name: \"hello\" version: \"0.1.0\") targets: (array (record name: @hello kind: @lib root: \"src\")) dependencies: (map))",
    )
    .expect("write project marker");
    let source =
        "(defn choose (value i32) i32 value)\n(defn answer () i32 (choose 1i32))";
    fs::write(tree.join("src/main.vib"), source).expect("write source module");

    let workspace =
        WorkspaceSnapshot::load_confined(&tree).expect("workspace snapshot");
    let query = workspace
        .query_position("src/main.vib", 64)
        .expect("semantic query");
    let rendered = WorkspacePositionQueryDocument::render_with_source(
        &query,
        &LineIndex::new(source),
    );
    fs::write(
        case.join("query.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&rendered).expect("serialize query")
        ),
    )
    .expect("write query snapshot");

    let corpus = Corpus::discover(&root).expect("temporary corpus");
    let runner = ConformanceRunner::new(
        ProfileDispatcher::new()
            .with_handler(ConformanceProfile::ToolingV1, ToolingV1QueryHandler),
    );
    let report = runner.run(&corpus);
    assert!(report.is_success(), "semantic query report: {report:?}");

    let mut wrong = serde_json::to_value(&rendered).expect("query JSON");
    wrong["expectedType"]["value"]["name"] = serde_json::json!("str");
    fs::write(
        case.join("query.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&wrong).expect("serialize wrong query")
        ),
    )
    .expect("write wrong query snapshot");
    let failed = runner.run(&corpus);
    assert_eq!(failed.failed(), 1);
    let reason = format!("{:?}", failed.cases());
    assert!(reason.contains("query snapshot mismatch"), "{reason}");

    let _ = fs::remove_dir_all(root);
}
