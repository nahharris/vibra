//! Target roots and the source snapshot
//! (`docs/spec/04-programs-and-packages.md`, "Discovery and the source
//! snapshot").

use std::fs;

use vibra_diagnostics::{ByteSpan, DiagnosticCode};
use vibra_workspace::UnitKind;

use crate::support::{
    LIBRARY, TempDir, capture, failure, faults, link_directory, link_file,
};

fn project_fault(code: DiagnosticCode) -> (DiagnosticCode, Option<String>) {
    (code, Some("project.vibon".to_owned()))
}

fn module_fault(
    code: DiagnosticCode,
    source: &str,
) -> (DiagnosticCode, Option<String>) {
    (code, Some(source.to_owned()))
}

fn two_targets(first: &str, second: &str) -> String {
    format!(
        "{{format: @project.v2 package: {{name: \"hello\" version: \"0.1.0\"}} targets: [{{name: @app kind: @lib root: \"{first}\"}} {{name: @inner kind: @lib root: \"{second}\"}}] dependencies: [:]}}"
    )
}

#[test]
fn modules_are_sorted_by_path_and_the_tests_unit_holds_its_root() {
    let project = TempDir::project(
        "sorted",
        LIBRARY,
        &[
            ("src/app/b.vib", "b"),
            ("src/app/a.vib", "a"),
            ("src/app/notes.txt", "not a module"),
            ("src/app/dir/c.vib", "c"),
            ("tests/t.vib", "t"),
        ],
    );
    let snapshot = capture(project.path()).expect("captured");
    let units = snapshot
        .units()
        .iter()
        .map(|unit| {
            (
                unit.name().to_owned(),
                unit.kind(),
                unit.modules()
                    .iter()
                    .map(|module| {
                        (module.source_id().to_owned(), module.path().to_owned())
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
    let pairs = |items: &[(&str, &str)]| {
        items
            .iter()
            .map(|(id, path)| ((*id).to_owned(), (*path).to_owned()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        units,
        [
            (
                "@app".to_owned(),
                UnitKind::Lib,
                pairs(&[
                    ("src/app/a.vib", "@app.a"),
                    ("src/app/b.vib", "@app.b"),
                    ("src/app/dir/c.vib", "@app.dir.c"),
                ])
            ),
            (
                "@tests".to_owned(),
                UnitKind::Tests,
                pairs(&[("tests/t.vib", "@tests.t")])
            ),
        ]
    );
}

#[test]
fn two_snapshots_of_one_tree_are_identical() {
    let project = TempDir::project(
        "repeat",
        LIBRARY,
        &[
            ("src/app/a.vib", "x\r\ny"),
            ("src/app/z/q.vib", "\u{feff}q"),
        ],
    );
    let first = capture(project.path()).expect("first");
    let second = capture(project.path()).expect("second");
    assert_eq!(first, second);
    let bytes = |snapshot: &vibra_workspace::SourceSnapshot| {
        snapshot
            .modules()
            .flat_map(|module| module.bytes().to_vec())
            .collect::<Vec<_>>()
    };
    assert_eq!(bytes(&first), bytes(&second));
    assert_eq!(bytes(&first), "x\r\ny\u{feff}q".as_bytes());
}

#[test]
fn a_project_without_tests_has_an_empty_tests_unit() {
    let project = TempDir::project("no-tests", LIBRARY, &[("src/app/a.vib", "")]);
    let snapshot = capture(project.path()).expect("captured");
    let tests = snapshot.units().last().expect("the tests unit");
    assert_eq!((tests.name(), tests.kind()), ("@tests", UnitKind::Tests));
    assert!(tests.modules().is_empty());
}

#[test]
fn an_invalid_segment_is_reported_at_offset_zero_of_that_source() {
    let project = TempDir::project(
        "segment",
        LIBRARY,
        &[
            ("src/app/my-mod.vib", "x"),
            ("src/app/fn.vib", "x"),
            ("src/app/9a/x.vib", "x"),
        ],
    );
    let diagnostics = failure(capture(project.path()));
    assert_eq!(
        faults(&diagnostics),
        [
            module_fault(DiagnosticCode::ModuleInvalidSegment, "src/app/9a/x.vib"),
            module_fault(DiagnosticCode::ModuleInvalidSegment, "src/app/fn.vib"),
            module_fault(DiagnosticCode::ModuleInvalidSegment, "src/app/my-mod.vib"),
        ]
    );
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.primary_span() == ByteSpan::empty_at(0))
    );
}

#[test]
fn a_module_beside_a_directory_of_its_name_collides() {
    let project = TempDir::project(
        "file-dir",
        LIBRARY,
        &[("src/app/text.vib", "x"), ("src/app/text/inner.vib", "y")],
    );
    assert_eq!(
        faults(&failure(capture(project.path()))),
        [module_fault(
            DiagnosticCode::ModuleFileDirectoryCollision,
            "src/app/text.vib"
        )]
    );
}

#[test]
fn paths_that_differ_only_by_case_collide_on_a_case_sensitive_host() {
    let project = TempDir::project("case", LIBRARY, &[("src/app/A.vib", "upper")]);
    project.write("src/app/a.vib", "lower");
    let sensitive = fs::read(project.join("src/app/A.vib")).expect("read") == b"upper";
    if sensitive {
        assert_eq!(
            faults(&failure(capture(project.path()))),
            [module_fault(
                DiagnosticCode::ModuleCaseCollision,
                "src/app/a.vib"
            )]
        );
    } else {
        // The host folds case: the tree holds one module, and the collision
        // rule is the snapshot module's unit test.
        capture(project.path()).expect("one module");
    }
}

#[test]
fn target_roots_are_checked_and_reserved_unit_names_refused() {
    let cases: [(&str, &str, DiagnosticCode); 6] = [
        (
            "root: \"src/app\"",
            "root: \"src/nothere\"",
            DiagnosticCode::ProjectInvalidTargetRoot,
        ),
        (
            "root: \"src/app\"",
            "root: \"./src\"",
            DiagnosticCode::ProjectInvalidTargetRoot,
        ),
        (
            "root: \"src/app\"",
            "root: \"src/../src/app\"",
            DiagnosticCode::ProjectInvalidTargetRoot,
        ),
        (
            "root: \"src/app\"",
            "root: \"\"",
            DiagnosticCode::ProjectInvalidTargetRoot,
        ),
        (
            "name: @app",
            "name: @tests",
            DiagnosticCode::ProjectReservedUnitName,
        ),
        (
            "name: @app",
            "name: @std",
            DiagnosticCode::ProjectReservedUnitName,
        ),
    ];
    for (from, to, code) in cases {
        let document = LIBRARY.replace(from, to);
        let project = TempDir::project("roots", &document, &[("src/app/a.vib", "")]);
        let diagnostics = failure(capture(project.path()));
        assert_eq!(faults(&diagnostics), [project_fault(code)], "{to}");
        let value = to.split_once(": ").expect("a field").1;
        let start = document.find(value).expect("written");
        assert_eq!(
            diagnostics[0].primary_span(),
            ByteSpan::new(start, start + value.len()),
            "{to}"
        );
    }
}

#[test]
fn a_root_that_is_a_file_or_a_link_or_outside_is_invalid() {
    let project = TempDir::project("root-file", LIBRARY, &[("src/app", "a file")]);
    assert_eq!(
        faults(&failure(capture(project.path()))),
        [project_fault(DiagnosticCode::ProjectInvalidTargetRoot)]
    );
    let outside = TempDir::new("root-outside");
    let project = TempDir::project("root-link", LIBRARY, &[("src/", "")]);
    link_directory(outside.path(), &project.join("src/app"));
    assert_eq!(
        faults(&failure(capture(project.path()))),
        [project_fault(DiagnosticCode::ProjectInvalidTargetRoot)]
    );
}

#[test]
fn a_reserved_dependency_name_is_refused_at_its_key() {
    let document = LIBRARY.replace(
        "dependencies: [:]",
        "dependencies: [\"std\": {kind: @path path: \"../std\"}]",
    );
    let project = TempDir::project("dep-std", &document, &[("src/app/a.vib", "")]);
    let diagnostics = failure(capture(project.path()));
    assert_eq!(
        faults(&diagnostics),
        [project_fault(DiagnosticCode::ProjectReservedUnitName)]
    );
    let start = document.find("\"std\"").expect("key");
    assert_eq!(
        diagnostics[0].primary_span(),
        ByteSpan::new(start, start + 5)
    );
}

#[test]
fn overlapping_roots_are_reported_on_the_later_target() {
    let document = two_targets("src/app", "src/app/inner");
    let project =
        TempDir::project("overlap", &document, &[("src/app/inner/a.vib", "")]);
    let diagnostics = failure(capture(project.path()));
    assert_eq!(
        faults(&diagnostics),
        [project_fault(DiagnosticCode::ProjectOverlappingTargetRoots)]
    );
    let start = document.find("\"src/app/inner\"").expect("root");
    assert_eq!(
        diagnostics[0].primary_span(),
        ByteSpan::new(start, start + "\"src/app/inner\"".len())
    );
    assert!(diagnostics[0].related().is_empty());
    let project = TempDir::project(
        "overlap-tests",
        &LIBRARY.replace("src/app", "tests/app"),
        &[("tests/app/", "")],
    );
    assert_eq!(
        faults(&failure(capture(project.path()))),
        [project_fault(DiagnosticCode::ProjectOverlappingTargetRoots)]
    );
}

#[test]
fn root_checks_finish_before_any_directory_is_listed() {
    let document = two_targets("src/app", "src/nothere");
    let project = TempDir::project("before", &document, &[("src/app/my-mod.vib", "")]);
    assert_eq!(
        faults(&failure(capture(project.path()))),
        [project_fault(DiagnosticCode::ProjectInvalidTargetRoot)]
    );
}

#[test]
fn a_link_that_leaves_its_root_escapes() {
    let project = TempDir::project(
        "escape",
        &two_targets("src/app", "src/other"),
        &[("src/app/", ""), ("src/other/x.vib", "")],
    );
    link_directory(&project.join("src/other"), &project.join("src/app/away"));
    assert_eq!(
        faults(&failure(capture(project.path()))),
        [module_fault(
            DiagnosticCode::ModulePathEscape,
            "src/app/away"
        )]
    );
    let outside = TempDir::new("escape-outside");
    let project = TempDir::project("escape-out", LIBRARY, &[("src/app/", "")]);
    link_directory(outside.path(), &project.join("src/app/out"));
    assert_eq!(
        faults(&failure(capture(project.path()))),
        [module_fault(
            DiagnosticCode::ModulePathEscape,
            "src/app/out"
        )]
    );
}

#[test]
fn a_linked_directory_inside_its_root_is_walked_once() {
    let project = TempDir::project("inside", LIBRARY, &[("src/app/real/m.vib", "m")]);
    link_directory(
        &project.join("src/app/real"),
        &project.join("src/app/alias"),
    );
    let snapshot = capture(project.path()).expect("captured");
    let ids = snapshot
        .modules()
        .map(|module| module.source_id().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["src/app/alias/m.vib"]);
}

#[test]
fn a_linked_file_is_admitted_once_at_its_first_sorted_path() {
    let project = TempDir::project("file-link", LIBRARY, &[("src/app/zeta.vib", "z")]);
    link_file(
        &project.join("src/app/zeta.vib"),
        &project.join("src/app/alpha.vib"),
    );
    let snapshot = capture(project.path()).expect("captured");
    let ids = snapshot
        .modules()
        .map(|module| module.source_id().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["src/app/alpha.vib"]);
}

#[test]
fn a_dangling_link_is_a_module_io_error() {
    let project = TempDir::project("dangling", LIBRARY, &[("src/app/gone.vib", "")]);
    link_file(
        &project.join("src/app/gone.vib"),
        &project.join("src/app/link.vib"),
    );
    fs::remove_file(project.join("src/app/gone.vib")).expect("remove the target");
    assert_eq!(
        faults(&failure(capture(project.path()))),
        [module_fault(
            DiagnosticCode::ModuleIoError,
            "src/app/link.vib"
        )]
    );
}
