//! Temporary project trees, links, and the documents the tests write. The
//! tests of `vibra-cli` include this file with `#[path]` for `TempDir` and
//! `LIBRARY`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use vibra_diagnostics::{Diagnostic, DiagnosticCode};
use vibra_workspace::{SourceSnapshot, discover_project_at};

/// A counter, not the clock, makes a name unique within the process.
static NEXT: AtomicU64 = AtomicU64::new(0);

/// The library project of the tests, with the root `src/app`.
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
            parent.join(format!("vibra-ws-{label}-{}-{serial}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create the directory");
        Self { path }
    }

    /// A project: `project.vibon` holds `document`, and each of `files` is a
    /// relative path and its content. A path ending in `/` is a directory.
    pub(crate) fn project(label: &str, document: &str, files: &[(&str, &str)]) -> Self {
        let directory = Self::new(label);
        directory.write("project.vibon", document);
        for (path, contents) in files {
            if let Some(path) = path.strip_suffix('/') {
                fs::create_dir_all(directory.path.join(path)).expect("directory");
            } else {
                directory.write(path, contents);
            }
        }
        directory
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn join(&self, relative: &str) -> PathBuf {
        self.path.join(relative)
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

/// A directory link at `link` to `target`: a symbolic link on Unix and a
/// junction, which needs no privilege, on Windows.
pub(crate) fn link_directory(target: &Path, link: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).expect("directory link");
    #[cfg(windows)]
    {
        // mklink does not take the verbatim \\?\ prefix.
        let plain = |path: &Path| {
            let text = path.display().to_string();
            text.strip_prefix(r"\\?\")
                .map_or(text.clone(), str::to_owned)
        };
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(plain(link))
            .arg(plain(target))
            .output()
            .expect("run mklink");
        assert!(status.status.success(), "mklink /J failed: {status:?}");
    }
}

/// A file link at `link` to `target`.
pub(crate) fn link_file(target: &Path, link: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).expect("file link");
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(target, link)
        .expect("a file symbolic link (Windows needs developer mode or elevation)");
}

/// The snapshot of the project at `root`, or its diagnostics.
pub(crate) fn capture(root: &Path) -> Result<SourceSnapshot, Vec<Diagnostic>> {
    SourceSnapshot::capture(discover_project_at(root)?)
}

/// The codes and source identities of a failure.
pub(crate) fn faults(
    diagnostics: &[Diagnostic],
) -> Vec<(DiagnosticCode, Option<String>)> {
    diagnostics
        .iter()
        .map(|diagnostic| {
            (diagnostic.code(), diagnostic.source_id().map(str::to_owned))
        })
        .collect()
}

/// The failure of `result`, which must have failed.
pub(crate) fn failure<T: std::fmt::Debug>(
    result: Result<T, Vec<Diagnostic>>,
) -> Vec<Diagnostic> {
    result.expect_err("the step fails")
}
