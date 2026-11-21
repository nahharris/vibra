//! The in-process harness of the host tests: run one command line through
//! `vibra_cli::run` and read what it wrote.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

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

/// A counter, not the clock, makes a name unique within the process.
static NEXT: AtomicU64 = AtomicU64::new(0);

/// The library project of the host tests, with the root `src/app`.
pub(crate) const LIBRARY: &str = "{format: @project.v2 package: {name: \"hello\" version: \"0.1.0\"} targets: [{name: @app kind: @lib root: \"src/app\"}] dependencies: [:]}";

/// A temporary directory, removed on drop.
pub(crate) struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// A fresh directory under the canonical temporary directory.
    pub(crate) fn new(label: &str) -> Self {
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        let parent =
            fs::canonicalize(std::env::temp_dir()).expect("temporary directory");
        let path =
            parent.join(format!("vibra-cli-{label}-{}-{serial}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create the directory");
        Self { path }
    }

    /// A project whose `project.vibon` holds `document`, with `files`, each
    /// a relative path and its content.
    pub(crate) fn project(label: &str, document: &str, files: &[(&str, &str)]) -> Self {
        let directory = Self::new(label);
        directory.write("project.vibon", document);
        for (path, contents) in files {
            directory.write(path, contents);
        }
        directory
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.path.join(relative)).expect("a readable fixture")
    }

    pub(crate) fn write(&self, relative: &str, contents: &str) {
        let path = self.path.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent directory");
        }
        fs::write(path, contents).expect("fixture file");
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
