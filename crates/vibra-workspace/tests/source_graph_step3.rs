#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

//! Step 3 workspace discovery, snapshot, and graph tests.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use vibra_diagnostics::DiagnosticCode;
use vibra_workspace::WorkspaceSnapshot;

fn project(targets: &str, dependencies: &str) -> String {
    format!(
        "(record format: @project.v1 package: (record name: \"hello\" version: \"0.1.0\") targets: (array {targets}) dependencies: (map {dependencies}))"
    )
}

fn target(name: &str, root: &str) -> String {
    format!("(record name: @{name} kind: @lib root: \"{root}\")")
}

fn temporary_directory(label: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "vibra-step3-{label}-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("temporary directory");
    path
}

fn write(path: &Path, contents: impl AsRef<[u8]>) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("parent directory");
    }
    fs::write(path, contents).expect("fixture file");
}

fn cleanup(path: &Path) {
    let _ = fs::remove_dir_all(path);
}

#[test]
fn captures_sorted_exact_bytes_and_stable_module_ids() {
    let root = temporary_directory("sorted");
    write(
        &root.join("project.vibon"),
        project(&target("hello", "src"), ""),
    );
    write(&root.join("src/z.vib"), b"z\n");
    write(&root.join("src/nested/a.vib"), b"a\0\n");
    write(&root.join("src/ignored.vibon"), b"(record)");

    let snapshot =
        WorkspaceSnapshot::load(root.join("project.vibon")).expect("snapshot");
    let documents = snapshot.source().documents().collect::<Vec<_>>();
    assert_eq!(
        documents
            .iter()
            .map(|document| document.source_id())
            .collect::<Vec<_>>(),
        vec!["src/nested/a.vib", "src/z.vib"]
    );
    assert_eq!(documents[0].bytes(), b"a\0\n");
    assert_eq!(documents[1].bytes(), b"z\n");

    let graph = snapshot.source_graph();
    assert!(graph.accepted());
    assert!(graph.lookup("hello", &["nested", "a"]).is_some());
    assert_eq!(
        graph.lookup("hello", &["z"]).expect("z module").bytes(),
        b"z\n"
    );
    cleanup(&root);
}

#[test]
fn rejects_overlapping_roots_before_reading_malformed_modules() {
    let root = temporary_directory("overlap");
    let targets = format!("{} {}", target("outer", "src"), target("inner", "src/sub"));
    write(&root.join("project.vibon"), project(&targets, ""));
    write(&root.join("src/broken.vib"), b"(not valid source");
    write(&root.join("src/sub/also-broken.vib"), b"(not valid source");

    let error =
        WorkspaceSnapshot::load(root.join("project.vibon")).expect_err("overlap");
    assert_eq!(
        error.diagnostics()[0].code(),
        DiagnosticCode::ProjectOverlappingTargetRoots
    );
    cleanup(&root);
}

#[test]
fn rejects_file_directory_collision_before_reading_malformed_modules() {
    let root = temporary_directory("collision");
    write(
        &root.join("project.vibon"),
        project(&target("hello", "src"), ""),
    );
    write(&root.join("src/text.vib"), b"(not valid source");
    write(&root.join("src/text/other.vib"), b"(also not valid source");

    let error =
        WorkspaceSnapshot::load(root.join("project.vibon")).expect_err("collision");
    assert_eq!(
        error.diagnostics()[0].code(),
        DiagnosticCode::ModuleFileDirectoryCollision
    );
    cleanup(&root);
}

#[test]
fn retains_dependencies_without_opening_or_resolving_them() {
    let root = temporary_directory("dependency");
    let dependency = "@remote (record kind: @path path: \"../outside\" target: @core)";
    write(
        &root.join("project.vibon"),
        project(&target("hello", "src"), dependency),
    );
    write(&root.join("src/main.vib"), b"(defn main)");

    let snapshot =
        WorkspaceSnapshot::load(root.join("project.vibon")).expect("snapshot");
    let graph = snapshot.source_graph();
    assert_eq!(graph.dependencies().len(), 1);
    assert_eq!(graph.dependencies()[0].alias(), "remote");
    assert_eq!(graph.dependencies()[0].target(), Some("core"));
    assert_eq!(
        graph.diagnostics()[0].code(),
        DiagnosticCode::ToolUnavailable
    );
    cleanup(&root);
}

#[test]
fn rejects_missing_and_absolute_target_roots_with_root_diagnostics() {
    let missing = temporary_directory("missing-root");
    write(
        &missing.join("project.vibon"),
        project(&target("hello", "missing"), ""),
    );
    let error = WorkspaceSnapshot::load(missing.join("project.vibon"))
        .expect_err("missing root");
    assert_eq!(
        error.diagnostics()[0].code(),
        DiagnosticCode::ProjectInvalidTargetRoot
    );
    cleanup(&missing);

    let absolute = temporary_directory("absolute-root");
    let absolute_value = if cfg!(windows) {
        "C:/outside"
    } else {
        "/outside"
    };
    write(
        &absolute.join("project.vibon"),
        project(&target("hello", absolute_value), ""),
    );
    let error = WorkspaceSnapshot::load(absolute.join("project.vibon"))
        .expect_err("absolute root");
    assert_eq!(
        error.diagnostics()[0].code(),
        DiagnosticCode::ProjectInvalidTargetRoot
    );
    cleanup(&absolute);
}

#[test]
fn rejects_an_escaping_source_link_when_platform_fixture_is_available() {
    let root = temporary_directory("symlink");
    write(
        &root.join("project.vibon"),
        project(&target("hello", "src"), ""),
    );
    write(&root.join("outside.vib"), b"outside");
    fs::create_dir_all(root.join("src")).expect("source root");

    #[cfg(unix)]
    let result = std::os::unix::fs::symlink(
        root.join("outside.vib"),
        root.join("src/escape.vib"),
    );
    #[cfg(windows)]
    let result = std::os::windows::fs::symlink_file(
        root.join("outside.vib"),
        root.join("src/escape.vib"),
    );
    #[cfg(not(any(unix, windows)))]
    let result: Result<(), std::io::Error> =
        Err(std::io::Error::other("unsupported platform"));

    if let Err(error) = result {
        eprintln!(
            "symlink fixture unavailable; CI must provide equivalent evidence: {error}"
        );
        cleanup(&root);
        return;
    }
    let error =
        WorkspaceSnapshot::load(root.join("project.vibon")).expect_err("escape link");
    assert_eq!(
        error.diagnostics()[0].code(),
        DiagnosticCode::ModulePathEscape
    );
    cleanup(&root);
}
