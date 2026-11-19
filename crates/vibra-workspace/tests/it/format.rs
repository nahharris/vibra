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
fn a_captured_module_is_formatted_and_checked() {
    let root = TempDir::project("fmt-module", LIBRARY, &[("src/app/main.vib", MESSY)]);
    let plan = plan(&root, "src/app/main.vib");
    assert_eq!(plan.path(), "src/app/main.vib");
    assert!(plan.changed());
    assert_eq!(plan.text(), Some(CANONICAL));
    assert!(plan.diagnostics().is_empty(), "{:?}", plan.diagnostics());
    assert!(plan.revision().is_some());
    // A preview writes nothing.
    assert_eq!(
        std::fs::read_to_string(root.join("src/app/main.vib")).expect("file"),
        MESSY
    );
}

#[test]
fn the_candidate_text_is_checked_in_place_of_the_captured_bytes() {
    let source = "fn  f returns r:I32 do return \"a\" endfn";
    let root =
        TempDir::project("fmt-semantic", LIBRARY, &[("src/app/main.vib", source)]);
    let plan = plan(&root, "src/app/main.vib");
    let formatted = "fn f\n  returns r: I32\n  do\n    return \"a\"\nendfn\n";
    assert_eq!(plan.text(), Some(formatted));
    assert_eq!(codes(&plan), [DiagnosticCode::TypeMismatch]);
    let diagnostic = &plan.diagnostics()[0];
    assert_eq!(diagnostic.source_id(), Some("src/app/main.vib"));
    // The span is in the candidate text: the literal `"a"`.
    let start = formatted.find("\"a\"").expect("literal");
    assert_eq!(diagnostic.primary_span().start(), start);
    assert_eq!(diagnostic.primary_span().end(), start + 3);
    assert!(plan.has_errors());
    assert_eq!(plan.write(), Ok(false), "diagnostics prevent writing");
    assert_eq!(
        std::fs::read_to_string(root.join("src/app/main.vib")).expect("file"),
        source
    );
}

#[test]
fn a_recovered_file_is_kept_byte_for_byte() {
    let source = "fn f takes {x: I32 returns r: I32 do return x endfn";
    let root =
        TempDir::project("fmt-recovered", LIBRARY, &[("src/app/main.vib", source)]);
    let plan = plan(&root, "src/app/main.vib");
    assert_eq!(plan.text(), Some(source));
    assert!(!plan.changed());
    assert_eq!(codes(&plan), [DiagnosticCode::SyntaxUnmatchedDelimiter]);
}

#[test]
fn a_file_outside_every_unit_is_formatted_by_syntax_alone() {
    let root = TempDir::project(
        "fmt-outside",
        LIBRARY,
        &[
            ("src/app/main.vib", CANONICAL),
            (
                "notes/scratch.vib",
                "fn  f returns r:I32 do return nowhere endfn",
            ),
        ],
    );
    let plan = plan(&root, "notes/scratch.vib");
    assert!(plan.diagnostics().is_empty(), "{:?}", plan.diagnostics());
    assert_eq!(
        plan.text(),
        Some("fn f\n  returns r: I32\n  do\n    return nowhere\nendfn\n")
    );
}

#[test]
fn a_module_the_checker_does_not_support_is_formatted_by_syntax_alone() {
    let root = TempDir::project(
        "fmt-unavailable",
        LIBRARY,
        &[(
            "src/app/main.vib",
            "proc  run returns r:Void do return void endproc",
        )],
    );
    let plan = plan(&root, "src/app/main.vib");
    assert!(plan.diagnostics().is_empty(), "{:?}", plan.diagnostics());
    assert!(plan.changed());
}

#[test]
fn vibon_keeps_comments_spellings_and_order() {
    let project = "{format:@project.v2 // the marker\n  package: {name: \"hello\"   version: \"0.1.0\"}\n  dependencies: [:] targets: [{name: @app kind: @lib root: \"src/app\" }]}\n";
    let root =
        TempDir::project("fmt-vibon", project, &[("src/app/main.vib", CANONICAL)]);
    let plan = plan(&root, "project.vibon");
    assert!(plan.diagnostics().is_empty(), "{:?}", plan.diagnostics());
    let text = plan.text().expect("text");
    assert!(text.contains("// the marker"), "{text}");
    assert!(
        text.find("dependencies").expect("dependencies")
            < text.find("targets").expect("targets"),
        "{text}"
    );
}

#[test]
fn write_replaces_a_changed_file_and_a_second_plan_is_unchanged() {
    let root = TempDir::project("fmt-write", LIBRARY, &[("src/app/main.vib", MESSY)]);
    let first = plan(&root, "src/app/main.vib");
    assert_eq!(first.write(), Ok(true));
    assert_eq!(
        std::fs::read_to_string(root.join("src/app/main.vib")).expect("file"),
        CANONICAL
    );
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
