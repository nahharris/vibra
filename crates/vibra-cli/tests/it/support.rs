//! The in-process harness of the host tests: run one command line through
//! `vibra_cli::run` and read what it wrote; and the fixtures and readers
//! several modules share.

use std::ffi::OsString;
use std::path::Path;

use serde_json::Value;

/// The built `vibra`.
pub(crate) const VIBRA: &str = env!("CARGO_BIN_EXE_vibra");

/// Everything one run produced.
pub(crate) struct Outcome {
    pub(crate) code: i32,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

/// Runs `arguments` in the current directory.
pub(crate) fn run(arguments: &[&str]) -> Outcome {
    run_in(Path::new("."), arguments)
}

/// Runs `arguments` with `directory` as the current directory.
pub(crate) fn run_in(directory: &Path, arguments: &[&str]) -> Outcome {
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let code = vibra_cli::run(
        arguments.iter().map(OsString::from),
        directory,
        &mut stdout,
        &mut stderr,
    );
    Outcome {
        code,
        stdout: String::from_utf8(stdout).expect("standard output is UTF-8"),
        stderr: String::from_utf8(stderr).expect("standard error is UTF-8"),
    }
}

/// The one envelope an invocation wrote, checked against the published
/// schema, and the exit code the document's result maps to.
pub(crate) fn envelope(arguments: &[&str]) -> (i32, Value) {
    envelope_in(Path::new("."), arguments)
}

/// The one envelope of `arguments` run in `directory` in JSON format,
/// checked against the published schema, and the exit code.
pub(crate) fn envelope_in(directory: &Path, arguments: &[&str]) -> (i32, Value) {
    let outcome = run_in(directory, arguments);
    let value = single_document(&outcome.stdout);
    vibra_schema::validate_command_result(&value).unwrap_or_else(|violation| {
        panic!("the envelope of {arguments:?} violates its schema: {violation}")
    });
    (outcome.code, value)
}

/// Parses `text` as exactly one JSON value followed by one newline.
pub(crate) fn single_document(text: &str) -> Value {
    let body = text
        .strip_suffix('\n')
        .unwrap_or_else(|| panic!("no final newline in {text:?}"));
    assert!(!body.contains('\n'), "more than one line in {text:?}");
    serde_json::from_str(body).unwrap_or_else(|error| panic!("{error}: {text:?}"))
}

/// The codes of the diagnostics of `document`, in order.
pub(crate) fn codes(document: &Value) -> Vec<String> {
    document["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .map(|diagnostic| diagnostic["code"].as_str().expect("a code").to_owned())
        .collect()
}

/// Every file under `directory`, as a path relative to it with `/`.
pub(crate) fn files(directory: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in std::fs::read_dir(&next).expect("a readable directory") {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let relative = path.strip_prefix(directory).expect("beneath it");
                found.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    found.sort();
    found
}

pub(crate) use crate::workspace_support::{LIBRARY, TempDir};

/// A binary project whose entry is `@app.main.main`, with `src/app/main.vib`.
pub(crate) const BINARY: &str = "{format: @project.v2 package: {name: \"hello\" version: \"0.1.0\"} targets: [{name: @app kind: @bin root: \"src/app\" entry: @app.main.main effects: [] grants: [:]}] dependencies: [:]}";

/// The entry of [`BINARY`]: it returns success.
pub(crate) const ENTRY: &str = "use @std.process.ExitCode enduse\n\nproc main returns code: ExitCode do return ExitCode(0u8) endproc\n";
