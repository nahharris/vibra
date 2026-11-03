//! Helpers shared by the `vibra-cli` integration tests: one temporary
//! directory with one process-wide counter, one way to run the built binary,
//! and the command-result envelope checks.

use std::fs;
use std::io::{ErrorKind, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use jsonschema::Validator;
use serde_json::Value;

/// A counter, not the clock, makes a name unique within the process: the clock
/// of some platforms is too coarse to tell two parallel tests apart.
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

/// A fresh directory under the system temporary directory, removed on drop.
/// Directories made with [`TempDir::sibling_dir`] are removed with it.
pub(crate) struct TempDir {
    path: PathBuf,
    siblings: Vec<PathBuf>,
}

impl TempDir {
    pub(crate) fn new(label: &str) -> Self {
        for _ in 0..1024 {
            let nonce = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("vibra-cli-{label}-{}-{nonce}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => {
                    return Self {
                        path,
                        siblings: Vec::new(),
                    };
                }
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
                Err(error) => panic!("create a unique temporary directory: {error}"),
            }
        }
        panic!("could not create a unique temporary directory");
    }

    /// A fresh project: `project.vibon` holds `manifest`, and each of `sources`
    /// is a project-relative path and its content. `src/app` always exists.
    pub(crate) fn project(
        label: &str,
        manifest: &str,
        sources: &[(&str, &str)],
    ) -> Self {
        let directory = Self::new(label);
        fs::create_dir_all(directory.path.join("src/app"))
            .expect("create project source root");
        directory.write("project.vibon", manifest);
        for (relative, source) in sources {
            directory.write(relative, source);
        }
        directory
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// Writes `content` at the project-relative `relative`, creating parents.
    pub(crate) fn write(&self, relative: &str, content: &str) {
        let path = self.path.join(relative);
        fs::create_dir_all(path.parent().expect("fixture file has a parent"))
            .expect("create fixture directory");
        fs::write(path, content).expect("write fixture file");
    }

    /// A directory beside this one, removed when this one is.
    pub(crate) fn sibling_dir(&mut self, suffix: &str) -> PathBuf {
        let name = self
            .path
            .file_name()
            .expect("temporary root has a name")
            .to_string_lossy();
        let path = self.path.with_file_name(format!("{name}-{suffix}"));
        fs::create_dir_all(&path).expect("create outside directory");
        self.siblings.push(path.clone());
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
        for path in &self.siblings {
            let _ = fs::remove_dir_all(path);
        }
    }
}

/// A `project.vibon` document with one package and the given `targets` and
/// `dependencies` (each already spelled as vibon).
pub(crate) fn manifest(package: &str, targets: &str, dependencies: &str) -> String {
    format!(
        "(record format: @project.v1 package: (record name: \"{package}\" version: \"0.1.0\") targets: (array {targets}) dependencies: {dependencies})\n"
    )
}

/// A binary target named `app` rooted at `src/app` whose entry is `entry`.
pub(crate) fn app_target(entry: &str, effects: &str) -> String {
    format!(
        "(record name: @app kind: @bin root: \"src/app\" entry: @app.main.{entry} effects: (array{}))",
        if effects.is_empty() {
            String::new()
        } else {
            format!(" {effects}")
        }
    )
}

/// The built `vibra` binary, with no arguments.
pub(crate) fn vibra() -> Command {
    Command::new(env!("CARGO_BIN_EXE_vibra"))
}

pub(crate) fn run(args: &[&str]) -> Output {
    vibra()
        .args(args)
        .output()
        .expect("run the built vibra binary")
}

pub(crate) fn run_in(args: &[&str], current_dir: &Path) -> Output {
    vibra()
        .current_dir(current_dir)
        .args(args)
        .output()
        .expect("run the built vibra binary in the requested directory")
}

/// Runs the binary over `workspace` with every stream piped: `input` is written
/// to its standard input, or the input is empty (`/dev/null`) when `None`.
pub(crate) fn run_workspace(
    workspace: &Path,
    arguments: &[&str],
    input: Option<&[u8]>,
) -> Output {
    let mut child = vibra()
        .args(["--workspace", &workspace.to_string_lossy()])
        .args(arguments)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("vibra binary");
    if let Some(input) = input {
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(input)
            .expect("write input");
    }
    child.wait_with_output().expect("output")
}

pub(crate) fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("utf-8 output")
}

/// A path spelled with `/` separators on every platform.
pub(crate) fn spelled(path: &Path) -> String {
    path.to_str().expect("utf-8 path").replace('\\', "/")
}

pub(crate) fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout is one JSON document")
}

pub(crate) fn assert_envelope_schema(envelope: &Value) {
    let schema: Value = serde_json::from_str(vibra_schema::COMMAND_RESULT_SCHEMA)
        .expect("checked-in command result schema is valid JSON");
    let validator = Validator::new(&schema).expect("command result schema compiles");
    assert!(
        validator.is_valid(envelope),
        "envelope violates command-result schema: {}",
        serde_json::to_string_pretty(envelope).expect("envelope serializes")
    );
}
