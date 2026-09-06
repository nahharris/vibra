#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

//! Step 3 workspace discovery, snapshot, and graph tests.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use vibra_diagnostics::DiagnosticCode;
use vibra_workspace::{WorkspaceSnapshot, source_graph::SourceGraph};

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

    let graph = snapshot
        .source_graph()
        .expect("matching snapshot provenance");
    assert!(graph.accepted());
    assert!(graph.lookup("hello", &["nested", "a"]).is_some());
    assert_eq!(
        graph.lookup("hello", &["z"]).expect("z module").bytes(),
        b"z\n"
    );
    cleanup(&root);
}

#[test]
fn discovers_nearest_project_from_nested_directory_and_file() {
    let root = temporary_directory("nested-discovery");
    write(
        &root.join("project.vibon"),
        project(&target("hello", "src"), ""),
    );
    write(&root.join("src/deep/input.txt"), b"ignored");

    let from_directory = WorkspaceSnapshot::load(root.join("src/deep"))
        .expect("nested directory discovery");
    assert_eq!(
        from_directory.project().root(),
        fs::canonicalize(&root).expect("canonical temporary root")
    );

    let from_file = WorkspaceSnapshot::load(root.join("src/deep/input.txt"))
        .expect("file-parent discovery");
    assert_eq!(
        from_file.project().project_path(),
        fs::canonicalize(root.join("project.vibon")).expect("canonical marker")
    );
    cleanup(&root);
}

#[test]
fn missing_discovery_does_not_search_siblings_or_legacy_markers() {
    let root = temporary_directory("not-found");
    write(
        &root.join("project.vib"),
        project(&target("hello", "src"), ""),
    );
    write(
        &root.join("sibling/project.vibon"),
        project(&target("hello", "src"), ""),
    );
    let error = WorkspaceSnapshot::load(root.join("missing"))
        .expect_err("missing discovery must stop");
    assert_eq!(
        error.diagnostics()[0].code(),
        DiagnosticCode::ProjectNotFound
    );
    cleanup(&root);
}

#[test]
fn malformed_nearest_project_does_not_fall_back_to_an_ancestor() {
    let root = temporary_directory("malformed-nearest");
    write(
        &root.join("project.vibon"),
        project(&target("hello", "src"), ""),
    );
    write(&root.join("src/child/project.vibon"), b"(record broken");
    let error = WorkspaceSnapshot::load(root.join("src/child"))
        .expect_err("nearest malformed marker must be terminal");
    assert_ne!(
        error.diagnostics()[0].code(),
        DiagnosticCode::ProjectNotFound
    );
    assert_eq!(error.diagnostics()[0].source_id(), Some("project.vibon"));
    cleanup(&root);
}

#[test]
fn confined_loader_reports_io_for_a_non_directory_root() {
    let root = temporary_directory("confined-io");
    let marker = root.join("project.vibon");
    write(&marker, project(&target("hello", "src"), ""));
    let error = WorkspaceSnapshot::load_confined(&marker)
        .expect_err("a regular file cannot be a confined root");
    assert_eq!(
        error.diagnostics()[0].code(),
        DiagnosticCode::ProjectIoError
    );
    cleanup(&root);

    let root = temporary_directory("confined-no-ancestor");
    write(
        &root.join("project.vibon"),
        project(&target("hello", "src"), ""),
    );
    fs::create_dir_all(root.join("tree/src")).expect("confined tree");
    let error = WorkspaceSnapshot::load_confined(root.join("tree"))
        .expect_err("confined loading must not search an ancestor marker");
    assert_eq!(
        error.diagnostics()[0].code(),
        DiagnosticCode::ProjectNotFound
    );
    cleanup(&root);
}

#[test]
fn rejects_invalid_source_segments_before_reading_source_bytes() {
    let root = temporary_directory("invalid-segment");
    write(
        &root.join("project.vibon"),
        project(&target("hello", "src"), ""),
    );
    write(&root.join("src/Bad.vib"), b"(not valid source");
    let error = WorkspaceSnapshot::load(root.join("project.vibon"))
        .expect_err("invalid segment must be rejected");
    assert_eq!(
        error.diagnostics()[0].code(),
        DiagnosticCode::ModuleInvalidSegment
    );
    cleanup(&root);
}

#[test]
fn sibling_prefix_roots_are_distinct_but_equal_and_reversed_nested_roots_fail() {
    let sibling = temporary_directory("sibling-roots");
    let sibling_targets =
        format!("{} {}", target("short", "src"), target("long", "src-long"));
    write(
        &sibling.join("project.vibon"),
        project(&sibling_targets, ""),
    );
    write(&sibling.join("src/main.vib"), b"main");
    write(&sibling.join("src-long/main.vib"), b"main");
    assert!(WorkspaceSnapshot::load(sibling.join("project.vibon")).is_ok());
    cleanup(&sibling);

    let equal = temporary_directory("equal-roots");
    write(
        &equal.join("project.vibon"),
        project(
            &format!("{} {}", target("one", "src"), target("two", "src")),
            "",
        ),
    );
    write(&equal.join("src/main.vib"), b"main");
    let error = WorkspaceSnapshot::load(equal.join("project.vibon"))
        .expect_err("equal roots must fail");
    assert_eq!(
        error.diagnostics()[0].code(),
        DiagnosticCode::ProjectOverlappingTargetRoots
    );
    cleanup(&equal);

    let reversed = temporary_directory("reversed-roots");
    write(
        &reversed.join("project.vibon"),
        project(
            &format!("{} {}", target("inner", "src/sub"), target("outer", "src")),
            "",
        ),
    );
    write(&reversed.join("src/sub/main.vib"), b"main");
    let error = WorkspaceSnapshot::load(reversed.join("project.vibon"))
        .expect_err("reversed nested roots must fail");
    assert_eq!(
        error.diagnostics()[0].code(),
        DiagnosticCode::ProjectOverlappingTargetRoots
    );
    cleanup(&reversed);
}

#[test]
fn interior_dot_target_roots_are_rejected_before_path_normalization() {
    let root = temporary_directory("dot-root");
    write(
        &root.join("project.vibon"),
        project(&target("hello", "src/./nested"), ""),
    );
    write(&root.join("src/nested/main.vib"), b"main");
    let error = WorkspaceSnapshot::load(root.join("project.vibon"))
        .expect_err("dot component must be rejected");
    assert_eq!(
        error.diagnostics()[0].code(),
        DiagnosticCode::ProjectInvalidTargetRoot
    );
    cleanup(&root);
}

#[test]
fn same_basename_modules_remain_distinct_across_units_and_no_index_is_implicit() {
    let root = temporary_directory("units");
    let targets = format!("{} {}", target("one", "one"), target("two", "two"));
    write(&root.join("project.vibon"), project(&targets, ""));
    write(&root.join("one/main.vib"), b"one");
    write(&root.join("two/main.vib"), b"two");
    let first =
        WorkspaceSnapshot::load(root.join("project.vibon")).expect("distinct units");
    let graph = first.source_graph().expect("matching snapshot provenance");
    assert!(graph.lookup("one", &["main"]).is_some());
    assert!(graph.lookup("two", &["main"]).is_some());
    cleanup(&root);

    let no_index = temporary_directory("no-index");
    write(
        &no_index.join("project.vibon"),
        project(&target("hello", "src"), ""),
    );
    write(&no_index.join("src/lib/main.vib"), b"main");
    let snapshot =
        WorkspaceSnapshot::load(no_index.join("project.vibon")).expect("nested module");
    let graph = snapshot
        .source_graph()
        .expect("matching snapshot provenance");
    assert!(graph.lookup("hello", &["lib", "main"]).is_some());
    assert!(graph.lookup("hello", &["lib"]).is_none());
    cleanup(&no_index);
}

#[test]
fn graph_rejects_a_project_from_another_snapshot() {
    let first_root = temporary_directory("provenance-a");
    let second_root = temporary_directory("provenance-b");
    for root in [&first_root, &second_root] {
        write(
            &root.join("project.vibon"),
            project(&target("hello", "src"), ""),
        );
        write(&root.join("src/main.vib"), b"main");
    }
    let first = WorkspaceSnapshot::load(first_root.join("project.vibon"))
        .expect("first snapshot");
    let second = WorkspaceSnapshot::load(second_root.join("project.vibon"))
        .expect("second snapshot");
    let error = SourceGraph::build(second.project(), first.source().clone())
        .expect_err("project and snapshot provenance must match");
    assert!(error.to_string().contains("provenance"));
    cleanup(&first_root);
    cleanup(&second_root);
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
    let graph = snapshot
        .source_graph()
        .expect("matching snapshot provenance");
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
#[cfg_attr(
    windows,
    ignore = "symlink privileges are unavailable on the ordinary Windows host"
)]
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

    result.expect("symlink fixture unavailable; CI must provide equivalent evidence");
    let error =
        WorkspaceSnapshot::load(root.join("project.vibon")).expect_err("escape link");
    assert_eq!(
        error.diagnostics()[0].code(),
        DiagnosticCode::ModulePathEscape
    );
    cleanup(&root);
}

#[test]
#[cfg_attr(
    windows,
    ignore = "symlink privileges are unavailable on the ordinary Windows host"
)]
fn rejects_an_escaping_directory_link() {
    let root = temporary_directory("directory-link-escape");
    write(
        &root.join("project.vibon"),
        project(&target("hello", "src"), ""),
    );
    fs::create_dir_all(root.join("src")).expect("source root");
    fs::create_dir_all(root.join("outside")).expect("outside directory");
    let result = create_dir_symlink(root.join("outside"), root.join("src/escape"));
    result.expect("directory symlink fixture unavailable; CI must provide evidence");
    let error = WorkspaceSnapshot::load(root.join("project.vibon"))
        .expect_err("escaping directory link");
    assert_eq!(
        error.diagnostics()[0].code(),
        DiagnosticCode::ModulePathEscape
    );
    cleanup(&root);
}

#[test]
#[cfg_attr(
    windows,
    ignore = "symlink privileges are unavailable on the ordinary Windows host"
)]
fn in_root_directory_aliases_are_visited_once() {
    let root = temporary_directory("directory-alias");
    write(
        &root.join("project.vibon"),
        project(&target("hello", "src"), ""),
    );
    write(&root.join("src/real/main.vib"), b"main");
    let result = create_dir_symlink(root.join("src/real"), root.join("src/alias"));
    result.expect("directory symlink fixture unavailable; CI must provide evidence");
    let snapshot =
        WorkspaceSnapshot::load(root.join("project.vibon")).expect("in-root alias");
    let documents = snapshot.source().documents().collect::<Vec<_>>();
    assert_eq!(documents.len(), 1);
    assert_eq!(documents[0].source_id(), "src/alias/main.vib");
    cleanup(&root);
}

#[test]
#[cfg_attr(
    windows,
    ignore = "symlink privileges are unavailable on the ordinary Windows host"
)]
fn in_root_directory_cycles_are_path_escape_diagnostics() {
    let root = temporary_directory("directory-cycle");
    write(
        &root.join("project.vibon"),
        project(&target("hello", "src"), ""),
    );
    fs::create_dir_all(root.join("src/dir")).expect("source directory");
    let result = create_dir_symlink(root.join("src"), root.join("src/dir/cycle"));
    result.expect("directory symlink fixture unavailable; CI must provide evidence");
    let error =
        WorkspaceSnapshot::load(root.join("project.vibon")).expect_err("in-root cycle");
    assert_eq!(
        error.diagnostics()[0].code(),
        DiagnosticCode::ModulePathEscape
    );
    cleanup(&root);
}

#[test]
#[cfg_attr(
    windows,
    ignore = "symlink privileges are unavailable on the ordinary Windows host"
)]
fn aliased_file_claims_still_participate_in_layout_collision_checks() {
    let root = temporary_directory("file-alias-collision");
    write(
        &root.join("project.vibon"),
        project(&target("hello", "src"), ""),
    );
    write(&root.join("src/a.vib"), b"a");
    write(&root.join("src/z/other.vib"), b"other");
    let result = create_file_symlink(root.join("src/a.vib"), root.join("src/z.vib"));
    result.expect("file symlink fixture unavailable; CI must provide evidence");
    let error = WorkspaceSnapshot::load(root.join("project.vibon"))
        .expect_err("aliased file claim must collide with directory");
    assert_eq!(
        error.diagnostics()[0].code(),
        DiagnosticCode::ModuleFileDirectoryCollision
    );
    assert_eq!(error.diagnostics()[0].source_id(), Some("src/z.vib"));
    cleanup(&root);
}

fn create_dir_symlink(target: PathBuf, link: PathBuf) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir(target, link)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (target, link);
        Err(std::io::Error::other("directory symlinks unsupported"))
    }
}

fn create_file_symlink(target: PathBuf, link: PathBuf) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_file(target, link)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (target, link);
        Err(std::io::Error::other("file symlinks unsupported"))
    }
}
