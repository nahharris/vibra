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
fn a_git_dependency_is_unavailable_with_the_span_of_its_entry() {
    let rev = "0123456789abcdef0123456789abcdef01234567";
    let document = LIBRARY.replace(
        "dependencies: [:]",
        &format!(
            "dependencies: [\"money\": {{kind: @git git: \"https://example.com/money.git\" rev: \"{rev}\"}}]"
        ),
    );
    let project = TempDir::project("graph-dep", &document, &[("src/app/", "")]);
    let snapshot = capture(project.path()).expect("captured");
    let graph = SourceGraph::build(&snapshot, &[]).expect("built");
    let start = document.find("\"money\"").expect("key");
    let end = document.find(&format!("{rev}\"}}")).expect("value") + rev.len() + 2;
    let [dependency] = graph.dependencies() else {
        panic!("one dependency");
    };
    assert_eq!(dependency.span(), ByteSpan::new(start, end));
    assert!(graph.canonical_text().ends_with(&format!(
        "dependencies: [{{name: \"money\" kind: @git source: \"https://example.com/money.git\" status: @unavailable span: {{sourceId: \"project.vibon\" start: {start}u64 end: {end}u64}}}}]}}"
    )));
}

/// A path dependency's unit is in the graph with the kind `@dependency`,
/// and its record is `@captured`.
#[test]
fn a_path_dependency_is_captured_as_a_dependency_unit() {
    let document = LIBRARY.replace(
        "dependencies: [:]",
        "dependencies: [\"money\": {kind: @path path: \"lib/money\"}]",
    );
    let project = TempDir::project(
        "graph-path-dep",
        &document,
        &[
            ("src/app/", ""),
            (
                "lib/money/project.vibon",
                "{format: @project.v2 package: {name: \"money\" version: \"1.0.0\"} targets: [{name: @money kind: @lib root: \"src/money\"}] dependencies: [:]}",
            ),
            ("lib/money/src/money/amount.vib", "b"),
        ],
    );
    let snapshot = capture(project.path()).expect("captured");
    let graph = SourceGraph::build(&snapshot, &[]).expect("built");
    let text = graph.canonical_text();
    assert!(
        text.contains("{name: @money kind: @dependency modules: [{path: @money.amount sourceId: \"lib/money/src/money/amount.vib\" bytesHex: \"62\"}]}"),
        "{text}"
    );
    assert!(
        text.contains(
            "{name: \"money\" kind: @path source: \"lib/money\" status: @captured"
        ),
        "{text}"
    );
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

/// 04 "Discovery and the source snapshot": an embedded module's source
/// identity is its module atom and a project module's is a path ending in
/// `.vib`, so they never meet, even for a project module named like a
/// library one.
#[test]
fn a_project_module_never_shares_an_embedded_source_identity() {
    let project = TempDir::project("graph-atom", LIBRARY, &[("src/app/io.vib", "")]);
    let snapshot = capture(project.path()).expect("captured");
    let library = vibra_types::load_stdlib().expect("the embedded library");
    let embedded = library
        .modules()
        .iter()
        .map(|module| module.source_id())
        .collect::<Vec<_>>();
    assert!(!embedded.is_empty());
    for module in library.modules() {
        assert_eq!(module.source_id(), module.path());
        assert!(
            !module.source_id().ends_with(".vib"),
            "{}",
            module.source_id()
        );
    }
    let graph = SourceGraph::build(&snapshot, &embedded).expect("no collision");
    assert!(
        graph
            .canonical_text()
            .contains("{path: @app.io sourceId: \"src/app/io.vib\""),
        "{}",
        graph.canonical_text()
    );
}
