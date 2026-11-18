//! The in-process harness of the host tests: run one command line through
//! `vibra_cli::run` and read what it wrote.

use std::ffi::OsString;
use std::path::Path;

use serde_json::Value;

/// Everything one run produced.
pub(crate) struct Outcome {
    pub(crate) code: i32,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

/// Runs `arguments` in the current directory.
pub(crate) fn run(arguments: &[&str]) -> Outcome {
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let code = vibra_cli::run(
        arguments.iter().map(OsString::from),
        Path::new("."),
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
    let outcome = run(arguments);
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
