//! Step 10 semantic workspace-query contracts.

#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use vibra_workspace::{WorkspaceSnapshot, query::SemanticFactStatus};

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

fn temporary_directory(label: &str) -> PathBuf {
    let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let parent = fs::canonicalize(std::env::temp_dir()).expect("canonical temp parent");
    let root = parent.join(format!(
        "vibra-step10-{label}-{serial}-{}",
        std::process::id()
    ));
    fs::create_dir_all(&root).expect("temporary workspace");
    root
}

fn write_workspace(root: &Path, source: &str) {
    fs::create_dir_all(root.join("src")).expect("source directory");
    fs::write(
        root.join("project.vibon"),
        "(record format: @project.v1 package: (record name: \"hello\" version: \"0.1.0\") targets: (array (record name: @hello kind: @lib root: \"src\")) dependencies: (map))",
    )
    .expect("project marker");
    fs::write(root.join("src/main.vib"), source).expect("source module");
}

#[test]
fn semantic_query_joins_structural_type_scope_and_application_facts() {
    let root = temporary_directory("facts");
    let source =
        "(defn choose (value i32) i32 value)\n(defn answer () i32 (choose 1i32))";
    write_workspace(&root, source);
    let workspace = WorkspaceSnapshot::load(&root).expect("workspace snapshot");

    let literal = workspace
        .query_position("src/main.vib", source.find("1i32").expect("literal"))
        .expect("literal query");
    assert_eq!(literal.role().value().map(String::as_str), Some("@literal"));
    assert_eq!(
        literal.context().value().map(String::as_str),
        Some("argument")
    );
    assert_eq!(literal.expected_type().status(), SemanticFactStatus::Exact);
    assert_eq!(
        literal.expected_type().value().map(|value| value.name()),
        Some("i32")
    );
    assert_eq!(
        literal.observed_type().value().map(|value| value.name()),
        Some("i32")
    );
    assert_eq!(literal.identity().status(), SemanticFactStatus::Unavailable);
    assert!(
        literal
            .declaration_candidates()
            .value()
            .is_some_and(Vec::is_empty)
    );

    let application = workspace
        .query_position("src/main.vib", source.find("(choose").expect("application"))
        .expect("application query");
    assert_eq!(
        application.role().value().map(String::as_str),
        Some("@application")
    );
    let contract = application
        .application()
        .value()
        .expect("function contract");
    assert_eq!(contract.positional()[0].name(), "i32");
    assert_eq!(contract.result_type().name(), "i32");
    assert_eq!(
        contract.callee().expect("callee identity").kind(),
        "function"
    );

    let body_name = source.rfind("value").expect("body name");
    let body = workspace
        .query_position("src/main.vib", body_name)
        .expect("body query");
    assert_eq!(
        body.role().value().map(String::as_str),
        Some("@local-binding")
    );
    assert_eq!(body.identity().value().expect("binder").kind(), "binder");
    assert!(
        body.visible_locals()
            .value()
            .expect("locals")
            .iter()
            .any(|local| local.name() == "value")
    );
    assert!(body.node_id().starts_with("src/main.vib#"));
    assert!(body.workspace_revision().as_str().starts_with("sha256:"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn semantic_query_keeps_discards_unidentified_and_binds_labelled_slots() {
    let root = temporary_directory("labels");
    let source = r#"(defn answer () str
  (let - "ignored"
    "done"))
(defn use-local () str
  (let f choose
    (f "head" second: 11i32 first: "tail")))
(defn choose (fallback str) str
  labelled: (first str "first" second i32 8i32)
  first)
"#;
    write_workspace(&root, source);
    let workspace = WorkspaceSnapshot::load(&root).expect("workspace snapshot");

    let discard_offset = source.find("let -").expect("discard") + 4;
    let discard = workspace
        .query_position("src/main.vib", discard_offset)
        .expect("discard query");
    assert_eq!(discard.role().value().map(String::as_str), Some("@discard"));
    assert_eq!(
        discard.context().value().map(String::as_str),
        Some("let-value")
    );
    assert_eq!(discard.identity().status(), SemanticFactStatus::Unavailable);
    assert_eq!(
        discard.expected_type().status(),
        SemanticFactStatus::Unavailable
    );
    assert_eq!(
        discard.observed_type().status(),
        SemanticFactStatus::Unavailable
    );
    assert_eq!(
        discard.application().status(),
        SemanticFactStatus::Unavailable
    );

    let tail_offset = source.find("\"tail\"").expect("labelled tail");
    let tail = workspace
        .query_position("src/main.vib", tail_offset)
        .expect("labelled argument query");
    assert_eq!(
        tail.expected_type().value().map(|value| value.name()),
        Some("str")
    );

    let second_offset = source.find("11i32").expect("labelled integer");
    let second = workspace
        .query_position("src/main.vib", second_offset)
        .expect("second labelled argument query");
    assert_eq!(
        second.expected_type().value().map(|value| value.name()),
        Some("i32")
    );

    let application_offset = source.find("(f \"head\"").expect("local call");
    let application = workspace
        .query_position("src/main.vib", application_offset)
        .expect("local application query");
    assert_eq!(
        application.application().status(),
        SemanticFactStatus::Exact
    );
    assert_eq!(
        application
            .application()
            .value()
            .expect("local contract")
            .callee(),
        None
    );
    assert_eq!(
        application
            .application()
            .value()
            .expect("local contract")
            .result_type()
            .name(),
        "str"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn semantic_query_retains_literal_type_next_to_recovered_syntax() {
    let root = temporary_directory("recovery");
    let source =
        "(defn broken () i32 (let (bind x) 1i32 1i32))\n(defn good () i32 2i32)";
    write_workspace(&root, source);
    let workspace = WorkspaceSnapshot::load(&root).expect("workspace snapshot");
    let offset = source.rfind("2i32").expect("valid neighboring literal");
    let query = workspace
        .query_position("src/main.vib", offset)
        .expect("query remains available");
    assert_eq!(query.structural().status(), vibra_syntax::FactStatus::Exact);
    assert_eq!(
        query.observed_type().value().map(|value| value.name()),
        Some("i32")
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn semantic_query_rejects_structural_boundaries_without_panicking() {
    let root = temporary_directory("boundaries");
    let source = "(defn answer () str \"🌱\")";
    write_workspace(&root, source);
    let workspace = WorkspaceSnapshot::load(&root).expect("workspace snapshot");
    assert!(
        workspace
            .query_position("src/main.vib", source.len().saturating_add(1))
            .is_err()
    );
    let emoji = source.find('🌱').expect("emoji");
    assert!(workspace.query_position("src/main.vib", emoji + 1).is_err());
    assert!(matches!(
        workspace.query_position("missing.vib", 0),
        Err(vibra_workspace::query::WorkspaceQueryError::UnknownSource(
            _
        ))
    ));
    let _ = fs::remove_dir_all(root);
}
