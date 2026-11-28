//! The format plan: path admission, the captured-module rule, syntax-only
//! formatting, recovery, and the atomic write.

use vibra_diagnostics::DiagnosticCode;
use vibra_workspace::{Discovery, WorkspaceCapture, plan_format};

use crate::support::{LIBRARY, TempDir, link_directory};

const MESSY: &str = "fn  id takes {x:I32} returns r:I32 do return x endfn";
const CANONICAL: &str =
    "fn id\n  takes {x: I32}\n  returns r: I32\n  do\n    return x\nendfn\n";

fn plan(root: &TempDir, path: &str) -> vibra_workspace::FormatPlan {
    let capture = WorkspaceCapture::capture(root.path(), Discovery::Exact);
    plan_format(root.path(), &capture, path).expect("an admitted extension")
}

fn codes(plan: &vibra_workspace::FormatPlan) -> Vec<DiagnosticCode> {
    plan.diagnostics()
        .iter()
        .map(vibra_diagnostics::Diagnostic::code)
        .collect()
}

#[test]
fn a_file_that_is_not_utf8_plans_no_edit() {
    let root =
        TempDir::project("fmt-encoding", LIBRARY, &[("src/app/main.vib", CANONICAL)]);
    for (path, bytes) in [
        (
            "src/app/main.vib",
            &b"fn f\xff returns r: I32 do return 1i32 endfn\n"[..],
        ),
        ("data.vibon", &b"{name: \"\xff\"}\n"[..]),
    ] {
        std::fs::write(root.join(path), bytes).expect("written");
        let plan = plan(&root, path);
        assert_eq!(
            codes(&plan),
            [DiagnosticCode::SyntaxInvalidEncoding],
            "{path}"
        );
        assert_eq!(plan.text(), None, "{path}: no lossy decoding");
        assert!(!plan.changed(), "{path}");
        assert_eq!(plan.write(), Ok(false), "{path}");
        assert_eq!(
            std::fs::read(root.join(path)).expect("file"),
            bytes,
            "{path}"
        );
    }
}

#[test]
fn write_replaces_a_changed_file_and_a_second_plan_is_unchanged() {
    let root = TempDir::project("fmt-write", LIBRARY, &[("src/app/main.vib", MESSY)]);
    let first = plan(&root, "src/app/main.vib");
    assert_eq!(first.write(), Ok(true));
    assert_eq!(root.read("src/app/main.vib"), CANONICAL);
    let leftovers = std::fs::read_dir(root.join("src/app"))
        .expect("directory")
        .count();
    assert_eq!(leftovers, 1, "no temporary file is left");
    let second = plan(&root, "src/app/main.vib");
    assert!(!second.changed());
    assert_eq!(second.write(), Ok(false));
    assert_ne!(first.revision(), second.revision());
}

#[test]
fn paths_are_confined_and_files_must_exist() {
    let outer = TempDir::new("fmt-outer");
    outer.write("other/a.vib", MESSY);
    let root = TempDir::project("fmt-escape", LIBRARY, &[("src/app/main.vib", MESSY)]);
    let escape = plan(&root, "../other/a.vib");
    assert_eq!(codes(&escape), [DiagnosticCode::ModulePathEscape]);
    assert_eq!(escape.text(), None);
    let missing = plan(&root, "src/app/none.vib");
    assert_eq!(codes(&missing), [DiagnosticCode::ModuleIoError]);
    assert_eq!(
        missing.diagnostics()[0].source_id(),
        Some("src/app/none.vib")
    );
    assert_eq!(missing.diagnostics()[0].primary_span().start(), 0);
    link_directory(&outer.join("other"), &root.join("linked"));
    let linked = plan(&root, "linked/a.vib");
    assert_eq!(codes(&linked), [DiagnosticCode::ModulePathEscape]);
    let linked_missing = plan(&root, "linked/none.vib");
    assert_eq!(codes(&linked_missing), [DiagnosticCode::ModulePathEscape]);
}

#[test]
fn another_extension_is_invalid_input() {
    let root =
        TempDir::project("fmt-extension", LIBRARY, &[("src/app/main.vib", MESSY)]);
    root.write("README.md", "# readme\n");
    let capture = WorkspaceCapture::capture(root.path(), Discovery::Exact);
    assert!(plan_format(root.path(), &capture, "README.md").is_err());
    assert!(plan_format(root.path(), &capture, "../x.md").is_err());
}

#[test]
fn without_a_project_a_document_is_formatted_with_no_revision() {
    let root = TempDir::new("fmt-no-project");
    root.write("a.vib", MESSY);
    let capture = WorkspaceCapture::capture(root.path(), Discovery::Exact);
    assert_eq!(
        capture.diagnostics()[0].code(),
        DiagnosticCode::ProjectNotFound
    );
    let plan = plan_format(root.path(), &capture, "a.vib").expect("admitted");
    assert_eq!(plan.revision(), None);
    assert_eq!(plan.text(), Some(CANONICAL));
}
