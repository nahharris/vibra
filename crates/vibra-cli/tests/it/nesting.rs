//! The source nesting limit of `m1Default` (`docs/spec/05-tooling.md`,
//! "Limits") through the built binary: every command answers a document
//! nested past the limit with its document, never a stack overflow.
//!
//! Each run is a child process, so an overflow ends that child with its
//! exception code and fails the assertion, not the test process. The
//! expectations are counted by hand from the definition of a level: a
//! module value's initializer is nested 0 levels, each `do … enddo` and each
//! bracket form encloses its contents one level more.

use std::path::Path;
use std::process::Command;

use serde_json::Value;

use crate::support::{BINARY, ENTRY, LIBRARY, TempDir, VIBRA, codes, single_document};

/// The limit `m1Default` publishes.
const LIMIT: usize = 1_024;

/// What one run of the binary produced.
struct Ran {
    code: i32,
    stdout: String,
}

fn vibra(directory: &Path, arguments: &[&str]) -> Ran {
    let output = Command::new(VIBRA)
        .args(arguments)
        .current_dir(directory)
        .output()
        .expect("the binary runs");
    Ran {
        code: output.status.code().expect("an exit status"),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
    }
}

/// The one envelope of `vibra --format json` with `arguments` in
/// `directory`, checked against its schema, and the exit code.
fn json(directory: &Path, arguments: &[&str]) -> (i32, Value) {
    let mut line = vec!["--format", "json"];
    line.extend_from_slice(arguments);
    let ran = vibra(directory, &line);
    assert!(
        (0..=6).contains(&ran.code),
        "`vibra {}` ended with {:#x}, not a result's exit code",
        arguments.join(" "),
        ran.code
    );
    let value = single_document(&ran.stdout);
    vibra_schema::validate_command_result(&value)
        .unwrap_or_else(|violation| panic!("{arguments:?}: {violation}"));
    (ran.code, value)
}

/// A module value of type `I32` whose initializer is `depth` nested `do`
/// blocks around `1i32`, which is nested `depth` levels.
fn blocks(depth: usize) -> String {
    format!(
        "let x: I32 be {}1i32{} endlet\n",
        "do ".repeat(depth),
        " enddo".repeat(depth)
    )
}

/// A module value whose initializer is `depth` nested lists around `1i32`,
/// which is nested `depth` levels.
fn lists(depth: usize) -> String {
    format!(
        "let x: I32 be {}1i32{} endlet\n",
        "[".repeat(depth),
        "]".repeat(depth)
    )
}

fn library(label: &str, module: &str) -> TempDir {
    TempDir::project(label, LIBRARY, &[("src/app/main.vib", module)])
}

/// Asserts that `value` reports the nesting limit alone, at `start` of
/// `source_id`, with the status `unknown`.
fn assert_limit_alone(value: &Value, source_id: &str, start: usize) {
    assert_eq!(codes(value), ["@tool.limitExceeded"], "{value}");
    let span = &value["diagnostics"][0]["primarySpan"];
    assert_eq!(span["sourceId"], source_id);
    assert_eq!(span["start"].as_u64(), Some(start as u64));
    assert_eq!(value["status"], "unknown");
    assert_eq!(
        value["limits"]["reached"],
        serde_json::json!(["sourceNesting"])
    );
    assert_eq!(value["limits"]["complete"], false);
}

#[test]
fn check_passes_at_exactly_the_limit_of_do_blocks() {
    let directory = library("nesting-at-limit", &blocks(LIMIT));
    let (code, value) = json(directory.path(), &["check"]);
    assert_eq!(value["result"], "@command.ok", "{value}");
    assert_eq!(code, 0);
    assert_eq!(value["status"], "passed");
    assert_eq!(value["limits"]["reached"], serde_json::json!([]));
}

#[test]
fn a_project_file_past_the_limit_is_the_limit_alone() {
    // A field the closed record does not name: the limit decides before
    // the schema would.
    let prefix = "{format: @project.v2 package: {name: \"hello\" version: \"0.1.0\"} targets: [{name: @app kind: @lib root: \"src/app\"}] dependencies: [:] extra: ";
    let document = format!("{prefix}{}1u8{}}}", "[".repeat(LIMIT), "]".repeat(LIMIT));
    let directory =
        TempDir::project("nesting-project", &document, &[("src/app/main.vib", "")]);
    let (code, value) = json(directory.path(), &["check"]);
    assert_eq!(code, 1);
    // The record encloses `extra` one level; 1,024 lists bring `1u8` to
    // 1,025.
    assert_limit_alone(&value, "project.vibon", prefix.len() + LIMIT);
    assert_eq!(value["workspaceRevision"], Value::Null);
}

#[test]
fn fmt_formats_a_captured_module_far_past_the_limit() {
    let module = blocks(3_000);
    let directory = library("nesting-fmt", &module);
    let (code, value) = json(directory.path(), &["fmt", "src/app/main.vib"]);
    assert_eq!(value["result"], "@command.ok", "{}", &value["diagnostics"]);
    assert_eq!(code, 0);
    assert_eq!(value["diagnostics"], serde_json::json!([]));
    let text = value["payload"]["text"]
        .as_str()
        .expect("the formatted text");
    assert!(text.len() > 3_000 * 3_000, "the nesting is laid out");
}

#[test]
fn every_command_answers_a_module_past_the_limit_with_its_document() {
    let mut module = ENTRY.to_owned();
    module.push('\n');
    module.push_str(&blocks(LIMIT + 1));
    let directory = TempDir::project(
        "nesting-commands",
        BINARY,
        &[
            ("src/app/main.vib", &module),
            ("tests/main.vib", "test \"one\" do endtest\n"),
        ],
    );
    let start = module.find("1i32").expect("the literal");
    for (line, result) in [
        (&["check"][..], "@command.diagnostics"),
        (&["test"][..], "@command.diagnostics"),
        (&["run"][..], "@command.diagnostics"),
    ] {
        let (code, value) = json(directory.path(), line);
        assert_eq!(value["result"], result, "{line:?}: {value}");
        assert_eq!(code, 1, "{line:?}");
        assert_limit_alone(&value, "src/app/main.vib", start);
    }
    for line in [
        &["build"][..],
        &["eval", "@app.main.x"][..],
        &["project", "inspect"][..],
    ] {
        let (code, value) = json(directory.path(), line);
        assert_eq!(value["result"], "@command.unavailable", "{line:?}: {value}");
        assert_eq!(code, 4, "{line:?}");
    }
}

#[test]
fn check_answers_a_million_nested_brackets() {
    let directory = library("nesting-million", &lists(1_000_000));
    let (code, value) = json(directory.path(), &["check"]);
    assert_eq!(code, 1);
    assert_limit_alone(&value, "src/app/main.vib", 14 + LIMIT + 1);
}

/// `vibra check` answers each recursion probe of `vibra-workspace` with its
/// document, at a size whose check finishes in seconds in a debug build
/// (the probe table of the report has the sizes past those): every probe is
/// checked clean, except the `if` chain, whose 10,000 `if`s each nest in the
/// one before and are the limit alone.
#[test]
fn check_answers_each_probe_with_a_document() {
    const SIZES: &[(&str, usize)] = &[
        ("typeChain", 1_000),
        ("callChain", 100),
        ("procChain", 10_000),
        ("moduleLetChain", 1_000),
        ("importChain", 100),
        ("sequentialLets", 1_000),
        ("wideList", 10_000),
        ("ifChain", 10_000),
        ("matchArms", 1_000),
        ("dependencyChain", 100),
    ];
    for (name, size) in SIZES {
        let files = crate::probes::probe_files(name, *size);
        let project = files
            .iter()
            .find(|(path, _)| path == "project.vibon")
            .map(|(_, text)| text.as_str())
            .expect("a project file");
        let rest = files
            .iter()
            .filter(|(path, _)| path != "project.vibon")
            .map(|(path, text)| (path.as_str(), text.as_str()))
            .collect::<Vec<_>>();
        let directory = TempDir::project(&format!("probe-{name}"), project, &rest);
        let (code, value) = json(directory.path(), &["check"]);
        if *name == "ifChain" {
            assert_eq!(code, 1, "{name}");
            assert_eq!(codes(&value), ["@tool.limitExceeded"], "{name}");
        } else {
            assert_eq!((code, codes(&value)), (0, Vec::new()), "{name}: {value}");
        }
    }
}
