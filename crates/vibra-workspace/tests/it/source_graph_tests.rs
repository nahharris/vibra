//! Step 13 project-wide test-unit snapshot and graph contracts.

use std::fs;

use vibra_diagnostics::DiagnosticCode;
use vibra_workspace::WorkspaceSnapshot;

use crate::support::{cleanup, lib_project, lib_target, plain_directory, write};

#[test]
fn absent_test_root_still_contributes_reserved_empty_tests_unit() {
    let root = plain_directory("empty-unit");
    write(
        &root.join("project.vibon"),
        lib_project(&lib_target("hello", "src"), ""),
    );
    fs::create_dir_all(root.join("src")).expect("target root");

    let snapshot = WorkspaceSnapshot::load(root.join("project.vibon"))
        .expect("optional tests root may be absent");
    let graph = snapshot.source_graph().expect("source graph");
    let tests = graph
        .units()
        .iter()
        .find(|unit| unit.name() == "tests")
        .expect("the reserved tests unit is always present");

    assert!(tests.modules().is_empty());
    assert_eq!(tests.kind(), vibra_workspace::project::TargetKind::Lib);
    cleanup(&root);
}

#[test]
fn test_root_modules_have_test_relative_ids_and_are_in_the_snapshot_revision() {
    let root = plain_directory("modules");
    write(
        &root.join("project.vibon"),
        lib_project(&lib_target("hello", "src"), ""),
    );
    fs::create_dir_all(root.join("src")).expect("target root");
    write(&root.join("src/main.vib"), b"(defn main () void void)");
    let absent = WorkspaceSnapshot::load(root.join("project.vibon"))
        .expect("snapshot without tests");
    let absent_revision = absent.revision().clone();
    write(&root.join("tests/z.vib"), b"(test \"z\" void)");
    write(&root.join("tests/math/add.vib"), b"(test \"add\" void)");

    let snapshot = WorkspaceSnapshot::load(root.join("project.vibon"))
        .expect("snapshot with tests");
    let graph = snapshot.source_graph().expect("source graph");
    let tests = graph
        .units()
        .iter()
        .find(|unit| unit.name() == "tests")
        .expect("test unit");
    assert_eq!(
        tests
            .modules()
            .iter()
            .map(|module| (module.id().as_atom(), module.source_id().to_owned()))
            .collect::<Vec<_>>(),
        vec![
            (
                "@tests.math.add".to_owned(),
                "tests/math/add.vib".to_owned()
            ),
            ("@tests.z".to_owned(), "tests/z.vib".to_owned()),
        ]
    );
    assert_ne!(snapshot.revision(), &absent_revision);
    cleanup(&root);
}

#[test]
fn non_directory_test_root_is_rejected_at_the_project_relative_root_span() {
    let root = plain_directory("root-file");
    write(
        &root.join("project.vibon"),
        lib_project(&lib_target("hello", "src"), ""),
    );
    fs::create_dir_all(root.join("src")).expect("target root");
    write(&root.join("tests"), b"not a directory");

    let error = WorkspaceSnapshot::load(root.join("project.vibon"))
        .expect_err("a file cannot be the tests root");
    let diagnostic = &error.diagnostics()[0];
    assert_eq!(diagnostic.code(), DiagnosticCode::ProjectInvalidTargetRoot);
    assert_eq!(diagnostic.source_id(), Some("tests"));
    assert_eq!(diagnostic.primary_span().start(), 0);
    assert_eq!(diagnostic.primary_span().end(), 0);
    cleanup(&root);
}

#[test]
fn reserved_tests_target_name_is_rejected_before_source_capture() {
    let root = plain_directory("reserved-target");
    write(
        &root.join("project.vibon"),
        lib_project(&lib_target("tests", "src"), ""),
    );
    fs::create_dir_all(root.join("src")).expect("target root");

    let error = WorkspaceSnapshot::load(root.join("project.vibon"))
        .expect_err("the target unit name is reserved");
    assert_eq!(
        error.diagnostics()[0].code(),
        DiagnosticCode::ProjectReservedUnitName
    );
    cleanup(&root);
}

#[test]
fn test_root_equal_to_target_root_is_rejected_at_the_target_root_field() {
    let root = plain_directory("root-overlap");
    write(
        &root.join("project.vibon"),
        lib_project(&lib_target("hello", "tests"), ""),
    );
    fs::create_dir_all(root.join("tests")).expect("shared root");

    let error = WorkspaceSnapshot::load(root.join("project.vibon"))
        .expect_err("target and test roots must be disjoint");
    assert_eq!(
        error.diagnostics()[0].code(),
        DiagnosticCode::ProjectOverlappingTargetRoots
    );
    assert_eq!(error.diagnostics()[0].source_id(), Some("project.vibon"));
    cleanup(&root);
}
