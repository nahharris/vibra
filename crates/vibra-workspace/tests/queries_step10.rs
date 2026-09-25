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
    assert_eq!(
        discard.declaration_candidates().status(),
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

#[test]
fn semantic_query_does_not_promote_expected_to_observed_for_unresolved_application() {
    let root = temporary_directory("unresolved-call");
    let source = "(defn answer () i32 (missing))";
    write_workspace(&root, source);
    let workspace = WorkspaceSnapshot::load(&root).expect("workspace snapshot");
    let query = workspace
        .query_position("src/main.vib", source.find("(missing").expect("call"))
        .expect("query");
    assert_eq!(
        query.expected_type().value().map(|value| value.name()),
        Some("i32")
    );
    assert_eq!(
        query.observed_type().status(),
        SemanticFactStatus::Unavailable
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn semantic_query_does_not_bind_qualified_name_to_unqualified_local() {
    let root = temporary_directory("qualified-local");
    let source = "(defn answer () i32 (let x 1i32 x.bad))";
    write_workspace(&root, source);
    let workspace = WorkspaceSnapshot::load(&root).expect("workspace snapshot");
    let query = workspace
        .query_position(
            "src/main.vib",
            source.find("x.bad").expect("qualified name"),
        )
        .expect("query");
    assert_ne!(
        query.role().value().map(String::as_str),
        Some("@local-binding")
    );
    assert_eq!(query.identity().status(), SemanticFactStatus::Unavailable);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn semantic_query_keeps_trivia_facts_unavailable() {
    let root = temporary_directory("trivia");
    let source = "(defn answer () i32 (let x 1i32 x))";
    write_workspace(&root, source);
    let workspace = WorkspaceSnapshot::load(&root).expect("workspace snapshot");
    let offset = source.find("1i32").expect("literal") - 1;
    let query = workspace
        .query_position("src/main.vib", offset)
        .expect("trivia query");
    assert_eq!(
        query.structural().category(),
        vibra_syntax::GrammarCategory::Trivia
    );
    assert_eq!(
        query.observed_type().status(),
        SemanticFactStatus::Unavailable
    );
    assert_eq!(
        query.declaration_candidates().status(),
        SemanticFactStatus::Unavailable
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn semantic_query_collects_lambda_labelled_binders() {
    let root = temporary_directory("lambda-label");
    let source = "(defn answer () i32 (lambda () i32 labelled: (x i32 1i32) x))";
    write_workspace(&root, source);
    let workspace = WorkspaceSnapshot::load(&root).expect("workspace snapshot");
    let query = workspace
        .query_position("src/main.vib", source.rfind('x').expect("lambda body"))
        .expect("query");
    assert_eq!(
        query.role().value().map(String::as_str),
        Some("@local-binding")
    );
    assert_eq!(query.identity().value().expect("binder").kind(), "binder");
    assert!(
        query
            .visible_locals()
            .value()
            .expect("locals")
            .iter()
            .any(|local| local.name() == "x")
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn semantic_query_does_not_bind_labelled_parameter_type() {
    let root = temporary_directory("label-type");
    let source = "(defn answer () i32 (lambda () i32 labelled: (x i32 1i32) x))";
    write_workspace(&root, source);
    let workspace = WorkspaceSnapshot::load(&root).expect("workspace snapshot");
    let offset = source.find("x i32").expect("labelled parameter") + 2;
    let query = workspace
        .query_position("src/main.vib", offset)
        .expect("query");
    assert_ne!(
        query.role().value().map(String::as_str),
        Some("@local-binding")
    );
    assert_eq!(query.identity().status(), SemanticFactStatus::Unavailable);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn semantic_query_collects_match_arm_bindings() {
    let root = temporary_directory("match-scope");
    let source = r#"(defn answer (value i32) i32
  (match value
    (as i32 n) n
    - 0i32))"#;
    write_workspace(&root, source);
    let workspace = WorkspaceSnapshot::load(&root).expect("workspace snapshot");
    let query = workspace
        .query_position("src/main.vib", source.rfind("n").expect("arm body"))
        .expect("query");
    assert_eq!(
        query.role().value().map(String::as_str),
        Some("@local-binding")
    );
    assert_eq!(query.identity().value().expect("binder").kind(), "binder");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn semantic_query_does_not_publish_unsupported_variadic_contracts() {
    let root = temporary_directory("variadic-contract");
    let source = r#"(defn f (value i32) i32
  variadic: (rest (array i32))
  value)
(defn answer () i32 (f 1i32))"#;
    write_workspace(&root, source);
    let workspace = WorkspaceSnapshot::load(&root).expect("workspace snapshot");
    let query = workspace
        .query_position("src/main.vib", source.rfind("(f").expect("call"))
        .expect("query");
    assert_eq!(
        query.application().status(),
        SemanticFactStatus::Unavailable
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn semantic_query_resolves_import_target_identity() {
    let root = temporary_directory("import-target");
    fs::create_dir_all(root.join("src")).expect("source directory");
    fs::write(
        root.join("project.vibon"),
        "(record format: @project.v1 package: (record name: \"app\" version: \"0.1.0\") targets: (array (record name: @app kind: @lib root: \"src\")) dependencies: (map))",
    )
    .expect("project marker");
    fs::write(
        root.join("src/main.vib"),
        "(import helper @app.helper)\n(defn answer () i32 1i32)",
    )
    .expect("main source");
    fs::write(root.join("src/helper.vib"), "(defn provided () i32 1i32)")
        .expect("helper source");
    let workspace = WorkspaceSnapshot::load(&root).expect("workspace snapshot");
    let source = fs::read_to_string(root.join("src/main.vib")).expect("main source");
    let query = workspace
        .query_position(
            "src/main.vib",
            source.find("@app.helper").expect("import target"),
        )
        .expect("query");
    assert_eq!(
        query.role().value().map(String::as_str),
        Some("@entity-reference")
    );
    assert_eq!(
        query.identity().value().expect("module identity").kind(),
        "module"
    );
    let _ = fs::remove_dir_all(root);
}
