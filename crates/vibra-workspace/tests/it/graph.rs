//! The `@sourceGraph.v1` observation
//! (`docs/spec/07-diagnostics-and-conformance.md`, "Persistent observations").

use vibra_diagnostics::{ByteSpan, DiagnosticCode};
use vibra_workspace::SourceGraph;

use crate::project::BINARY;
use crate::support::{LIBRARY, TempDir, capture, faults};

#[test]
fn the_graph_holds_exact_bytes_and_an_empty_tests_unit() {
    let project = TempDir::project(
        "graph",
        LIBRARY,
        &[("src/app/b.vib", "b"), ("src/app/a.vib", "proc")],
    );
    let snapshot = capture(project.path()).expect("captured");
    let graph = SourceGraph::build(&snapshot, &[]).expect("built");
    assert_eq!(
        graph.canonical_text(),
        "{format: @sourceGraph.v1 units: [{name: @app kind: @lib modules: [{path: @app.a sourceId: \"src/app/a.vib\" bytesHex: \"70726f63\"} {path: @app.b sourceId: \"src/app/b.vib\" bytesHex: \"62\"}]} {name: @tests kind: @tests modules: []}] dependencies: []}"
    );
}

#[test]
fn a_binary_target_is_recorded_with_its_kind() {
    let project =
        TempDir::project("graph-bin", BINARY, &[("src/hello/main.vib", "proc")]);
    let snapshot = capture(project.path()).expect("captured");
    let graph = SourceGraph::build(&snapshot, &[]).expect("built");
    assert_eq!(
        graph.canonical_text(),
        "{format: @sourceGraph.v1 units: [{name: @hello kind: @bin modules: [{path: @hello.main sourceId: \"src/hello/main.vib\" bytesHex: \"70726f63\"}]} {name: @tests kind: @tests modules: []}] dependencies: []}"
    );
}

#[test]
fn a_dependency_is_unavailable_with_the_span_of_its_entry() {
    let document = LIBRARY.replace(
        "dependencies: [:]",
        "dependencies: [\"local\": {kind: @path path: \"../local\" target: @local}]",
    );
    let project = TempDir::project("graph-dep", &document, &[("src/app/", "")]);
    let snapshot = capture(project.path()).expect("captured");
    let graph = SourceGraph::build(&snapshot, &[]).expect("built");
    let start = document.find("\"local\"").expect("key");
    let end = document.find("@local}").expect("value") + "@local}".len();
    let [dependency] = graph.dependencies() else {
        panic!("one dependency");
    };
    assert_eq!(dependency.span(), ByteSpan::new(start, end));
    assert!(graph.canonical_text().ends_with(&format!(
        "dependencies: [{{alias: \"local\" kind: @path source: \"../local\" target: @local status: @unavailable span: {{sourceId: \"project.vibon\" start: {start}u64 end: {end}u64}}}}]}}"
    )));
}

#[test]
fn a_module_with_an_embedded_source_identity_collides() {
    let project = TempDir::project("graph-embedded", LIBRARY, &[("src/app/a.vib", "")]);
    let snapshot = capture(project.path()).expect("captured");
    let diagnostics =
        SourceGraph::build(&snapshot, &["src/app/a.vib"]).expect_err("collides");
    assert_eq!(
        faults(&diagnostics),
        [(
            DiagnosticCode::ModuleSourceIdCollision,
            Some("src/app/a.vib".to_owned())
        )]
    );
}
