//! The entry and the admission ceiling of a binary target, checked through
//! a captured project. Expectations come from the programs chapter,
//! "Targets" ("Its first part MUST be the target's own name"; "The entry's
//! row MUST be contained in it"), the effects chapter, "Target admission",
//! and the tooling chapter, the `check` rule of "Command surface".

use vibra_diagnostics::{ByteSpan, DiagnosticCode, Level};
use vibra_workspace::{WorkspaceCheck, check_snapshot};

use crate::support::{TempDir, capture};

const EXIT: &str = "use @std.process.ExitCode enduse\n\n";

/// A project of one binary target `@app` with `entry` and `effects`, a
/// second binary target `@tool` when `tool` is set, and an empty `grants`.
fn project(entry: &str, effects: &str, tool: bool) -> String {
    let second = if tool {
        " {name: @tool kind: @bin root: \"src/tool\" entry: @tool.main.main effects: [] grants: [:]}"
    } else {
        ""
    };
    format!(
        "{{format: @project.v2 package: {{name: \"hello\" version: \"0.1.0\"}} targets: \
         [{{name: @app kind: @bin root: \"src/app\" entry: {entry} effects: {effects} grants: [:]}}{second}] \
         dependencies: [:]}}"
    )
}

/// The span of the first `needle` in `text`.
fn at(text: &str, needle: &str) -> ByteSpan {
    let start = text.find(needle).expect("the needle");
    ByteSpan::new(start, start + needle.len())
}

fn check(
    document: &str,
    files: &[(&str, &str)],
    target: Option<&str>,
) -> WorkspaceCheck {
    let tree = TempDir::project("entry", document, files);
    let snapshot = capture(tree.path()).expect("the project captures");
    check_snapshot(&snapshot, target)
}

/// The code, source, and span of each diagnostic.
fn found(check: &WorkspaceCheck) -> Vec<(DiagnosticCode, Option<String>, ByteSpan)> {
    check
        .diagnostics()
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.code(),
                diagnostic.source_id().map(str::to_owned),
                diagnostic.primary_span(),
            )
        })
        .collect()
}

fn main_proc() -> String {
    format!("{EXIT}proc main returns code: ExitCode do return ExitCode(0u8) endproc\n")
}

#[test]
fn a_checked_entry_hands_admission_its_row_and_ceiling() {
    let source = format!(
        "{EXIT}use @std.io.StdoutText enduse\n\n\
         proc main returns code: ExitCode performs {{StdoutText}} do return ExitCode(0u8) endproc\n"
    );
    let document = project("@app.main.main", "[@std.io.StdoutText]", false);
    let result = check(&document, &[("src/app/main.vib", &source)], None);
    assert!(!result.has_errors(), "{:?}", result.diagnostics());
    let [facts] = result.entries() else {
        panic!("one entry: {:?}", result.entries());
    };
    assert_eq!(facts.target(), "@app");
    assert_eq!(facts.signature().function(), "app.main.main");
    assert!(facts.leaves().is_empty());
    assert_eq!(facts.row(), facts.ceiling());
    assert_eq!(
        facts
            .ceiling_entries()
            .iter()
            .map(|(root, span)| (root.path().to_owned(), *span))
            .collect::<Vec<_>>(),
        [(
            "std.io.StdoutText".to_owned(),
            at(&document, "@std.io.StdoutText")
        )]
    );
}

#[test]
fn an_entry_outside_its_target_is_not_resolved_at_all() {
    // `@tool.main.main` exists, and is still outside `@app`; a prefix of
    // the name is not the name.
    let tool = main_proc();
    for entry in ["@tool.main.main", "@appx.main.main"] {
        let document = project(entry, "[]", true);
        let result = check(
            &document,
            &[
                ("src/app/main.vib", &main_proc()),
                ("src/tool/main.vib", &tool),
            ],
            Some("@app"),
        );
        assert_eq!(
            found(&result),
            [(
                DiagnosticCode::ProjectEntryOutsideTarget,
                Some("project.vibon".to_owned()),
                at(&document, entry)
            )],
            "{entry}"
        );
        assert!(result.entries().is_empty());
    }
}

#[test]
fn project_file_diagnostics_come_first_in_span_order() {
    // The entry is wrong, the list repeats, and the source has an error of
    // its own: the project file's two come first, ordered by span.
    let source = format!(
        "{EXIT}proc main returns code: ExitCode do return ExitCode(0u8) endproc\n\n\
         fn broken returns r: I32 do return true endfn\n"
    );
    let document = project(
        "@app.main.nope",
        "[@std.io.StdoutText @std.io.StdoutText]",
        false,
    );
    let result = check(&document, &[("src/app/main.vib", &source)], None);
    let codes = found(&result)
        .into_iter()
        .map(|(code, source, _)| (code, source))
        .collect::<Vec<_>>();
    let project_file = Some("project.vibon".to_owned());
    assert_eq!(
        codes.get(..2),
        Some(
            [
                (DiagnosticCode::NameUnknownSymbol, project_file.clone()),
                (DiagnosticCode::EffectDuplicateRoot, project_file),
            ]
            .as_slice()
        )
    );
    assert!(codes.get(2..).is_some_and(|rest| {
        !rest.is_empty()
            && rest
                .iter()
                .all(|(_, source)| source.as_deref() == Some("src/app/main.vib"))
    }));
}

#[test]
fn each_interface_outside_the_ceiling_is_reported_once() {
    let source = format!(
        "{EXIT}use @std.io.StdoutText enduse\n\nuse @std.io.StderrText enduse\n\n\
         proc main returns code: ExitCode performs {{StdoutText StderrText}} do return ExitCode(0u8) endproc\n"
    );
    let document = project("@app.main.main", "[]", false);
    let result = check(&document, &[("src/app/main.vib", &source)], None);
    let effects = ByteSpan::new(
        at(&document, "effects: []").start() + 9,
        at(&document, "effects: []").end(),
    );
    let related = result
        .diagnostics()
        .iter()
        .filter(|diagnostic| diagnostic.source_id() == Some("project.vibon"))
        .map(|diagnostic| {
            assert_eq!(diagnostic.code(), DiagnosticCode::EffectOutsideCeiling);
            assert_eq!(diagnostic.primary_span(), effects);
            diagnostic
                .related()
                .iter()
                .map(|related| (related.source_id.clone(), related.span))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let performs = |name: &str| {
        let row = source.find("performs").expect("the row");
        let start = row
            + source
                .get(row..)
                .and_then(|rest| rest.find(name))
                .expect("the entry");
        vec![(
            Some("src/app/main.vib".to_owned()),
            ByteSpan::new(start, start + name.len()),
        )]
    };
    assert_eq!(related, [performs("StdoutText"), performs("StderrText")]);
    assert!(result.entries().is_empty());
}

#[test]
fn a_recipe_keeps_a_leafless_entry_unavailable() {
    // The entry takes no grant, and `grants` still holds a recipe nobody
    // checks yet: the target stays unavailable, and no unused effect is
    // decided while a recipe could be a startup step.
    let document = "{format: @project.v2 package: {name: \"hello\" version: \"0.1.0\"} targets: \
                    [{name: @app kind: @bin root: \"src/app\" entry: @app.main.main \
                    effects: [@std.io.StdoutText] \
                    grants: [@out: {provider: @host.io.stdout resource: {kind: @processStream} after: []}]}] \
                    dependencies: [:]}";
    let result = check(document, &[("src/app/main.vib", &main_proc())], None);
    let grants = at(
        document,
        "[@out: {provider: @host.io.stdout resource: {kind: @processStream} after: []}]",
    );
    assert_eq!(
        found(&result),
        [(
            DiagnosticCode::ToolUnavailable,
            Some("project.vibon".to_owned()),
            grants
        )]
    );
    assert!(result.only_unavailable());
}

#[test]
fn a_named_target_checks_its_own_entry_only() {
    let broken =
        format!("{EXIT}fn main returns code: ExitCode do return ExitCode(0u8) endfn\n");
    let document = project("@app.main.main", "[]", true);
    let tool_main = main_proc();
    let files_ok = [
        ("src/app/main.vib", broken.as_str()),
        ("src/tool/main.vib", tool_main.as_str()),
    ];
    let tool = check(&document, &files_ok, Some("@tool"));
    assert!(!tool.has_errors(), "{:?}", tool.diagnostics());
    assert_eq!(
        tool.entries()
            .iter()
            .map(|facts| facts.target())
            .collect::<Vec<_>>(),
        ["@tool"]
    );
    let all = check(&document, &files_ok, None);
    assert_eq!(
        found(&all),
        [(
            DiagnosticCode::ProjectInvalidEntrySignature,
            Some("project.vibon".to_owned()),
            at(&document, "@app.main.main")
        )]
    );
    assert_eq!(
        all.entries()
            .iter()
            .map(|facts| facts.target())
            .collect::<Vec<_>>(),
        ["@tool"]
    );
    assert!(
        all.diagnostics()
            .iter()
            .all(|diagnostic| diagnostic.level() == Level::Error)
    );
}
