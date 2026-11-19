//! The unavailable outcome of a known command no step has landed.

use serde_json::json;
use vibra_cli::inventory::LINES;

use crate::support::{envelope, run};

#[test]
fn unavailable_command() {
    let (code, document) = envelope(&["--format", "json", "check"]);
    assert_eq!(code, 4);
    assert_eq!(document["command"], "check");
    assert_eq!(document["status"], "unsupported");
    assert_eq!(document["result"], "@command.unavailable");
    assert_eq!(document["workspaceRevision"], json!(null));
    assert_eq!(document["payload"], json!({}));
    let diagnostics = document["diagnostics"].as_array().expect("diagnostics");
    assert_eq!(diagnostics.len(), 1);
    let diagnostic = &diagnostics[0];
    assert_eq!(diagnostic["code"], "@tool.unavailable");
    assert_eq!(diagnostic["level"], "@error");
    // No span: a failure with no location.
    assert_eq!(
        diagnostic["primarySpan"],
        json!({
            "sourceId": null, "start": 0, "end": 0,
            "startPosition": {"line": 1, "column": 1},
            "endPosition": {"line": 1, "column": 1}
        })
    );
    assert_eq!(diagnostic["related"], json!([]));
    assert_eq!(diagnostic["fixes"], json!([]));
    let notes = diagnostic["notes"].as_array().expect("notes");
    assert!(
        notes
            .iter()
            .any(|note| note.as_str().is_some_and(|text| text.contains("`check`"))),
        "a note names the command: {notes:?}"
    );
}

/// A well-formed command line for `line`: its words, a spelling of the first
/// operand when the line fixes them, and a placeholder for every other
/// required operand.
fn sample(line: &vibra_cli::inventory::Line) -> Vec<String> {
    let mut arguments = vec!["--format".to_owned(), "json".to_owned()];
    arguments.extend(line.words.iter().map(|word| (*word).to_owned()));
    for position in 0..line.required {
        arguments.push(match (position, line.first_operand.first()) {
            (0, Some(kind)) => (*kind).to_owned(),
            _ => "x".to_owned(),
        });
    }
    arguments
}

#[test]
fn unavailable_each() {
    let mut checked = 0;
    for line in LINES.iter().filter(|line| !line.supported) {
        let arguments = sample(line);
        let arguments = arguments.iter().map(String::as_str).collect::<Vec<_>>();
        let (code, document) = envelope(&arguments);
        assert_eq!(code, 4, "{arguments:?}");
        assert_eq!(document["command"], line.name().as_str(), "{arguments:?}");
        assert_eq!(document["status"], "unsupported");
        assert_eq!(document["result"], "@command.unavailable");
        assert_eq!(document["diagnostics"][0]["code"], "@tool.unavailable");
        assert_eq!(document["payload"], json!({}));
        checked += 1;
    }
    assert_eq!(checked, 17, "every line of the inventory but `help`");
}

#[test]
fn well_formed_lines_with_operands_and_options_are_unavailable() {
    let lines: [&[&str]; 21] = [
        &["project", "init"],
        &["project", "init", "dest"],
        &["project", "inspect"],
        &["project", "add", "target", "(record name: @app)", "--write"],
        &["project", "remove", "dependency", "@lib", "--write"],
        &["project", "sync"],
        &["fmt", "src/a.vib", "--write"],
        &["check", "@app"],
        &["lint", "@app", "--write"],
        &["test", "@tests.math::\"adds one\""],
        &["run"],
        &["run", "@app", "--", "-x", "--", "y"],
        &["eval", "@app.main.f", "--arg", "1", "--arg", "2"],
        &["build", "@app", "--target", "x86_64-pc-windows-msvc"],
        &["query", "@app.main.f", "--include", "m", "--expand", "r"],
        &["edit", "fix", "@app.main.f", "--write"],
        &["edit", "rename", "@app.main.f", "g", "--write"],
        &["edit", "imports", "src/a.vib", "--write"],
        &["mcp"],
        &["mcp", "--read-only", "x"],
        &["check", "--write-nothing"],
    ];
    for line in &lines[..20] {
        let mut arguments = vec!["--format", "json"];
        arguments.extend_from_slice(line);
        let (code, document) = envelope(&arguments);
        assert_eq!(code, 4, "{arguments:?}");
        assert_eq!(document["result"], "@command.unavailable");
    }
    // The last row is not well formed: it names an option no line has.
    let mut arguments = vec!["--format", "json"];
    arguments.extend_from_slice(lines[20]);
    assert_eq!(run(&arguments).code, 2);
}

#[test]
fn unavailable_in_human_format_is_one_line_on_standard_error() {
    let outcome = run(&["fmt", "a.vib"]);
    assert_eq!(outcome.code, 4);
    assert!(outcome.stdout.is_empty());
    assert_eq!(outcome.stderr.lines().count(), 1, "{:?}", outcome.stderr);
    assert!(outcome.stderr.starts_with("@error @tool.unavailable:"));
    assert!(outcome.stderr.contains("`fmt`"));
}

#[test]
fn the_program_arguments_of_run_are_every_token_after_the_separator() {
    let invocation = vibra_cli::args::parse(
        &["run", "@app", "--", "-x", "--", "--write"].map(Into::into),
        std::path::Path::new("."),
    )
    .expect("a well-formed line");
    assert_eq!(invocation.operands, ["@app"]);
    assert_eq!(invocation.program_arguments, ["-x", "--", "--write"]);
    assert!(invocation.options.is_empty());
    let bare = vibra_cli::args::parse(
        &["run", "--"].map(Into::into),
        std::path::Path::new("."),
    )
    .expect("a well-formed line");
    assert!(bare.program_arguments.is_empty());
}
