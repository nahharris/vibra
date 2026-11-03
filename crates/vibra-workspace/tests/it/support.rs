//! Helpers shared by the `vibra-workspace` integration tests: one temporary
//! directory with one process-wide counter, and the project documents the
//! tests write into it.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// A counter, not the clock, makes a name unique within the process: the clock
/// of some platforms is too coarse to tell two parallel tests apart.
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

fn fresh_directory(label: &str, canonical: bool) -> PathBuf {
    let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let parent = if canonical {
        fs::canonicalize(std::env::temp_dir()).expect("canonical temp directory")
    } else {
        std::env::temp_dir()
    };
    let root = parent.join(format!(
        "vibra-workspace-{label}-{}-{serial}",
        std::process::id()
    ));
    fs::create_dir_all(&root).expect("create temporary directory");
    root
}

/// A fresh directory under the canonical system temporary directory.
pub(crate) fn temporary_directory(label: &str) -> PathBuf {
    fresh_directory(label, true)
}

/// A fresh directory under the system temporary directory as the platform
/// spells it, for tests that hand the path to a tool such as `mklink`.
pub(crate) fn plain_directory(label: &str) -> PathBuf {
    fresh_directory(label, false)
}

/// Writes `contents` at `path`, creating its parents.
pub(crate) fn write(path: &Path, contents: impl AsRef<[u8]>) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("parent directory");
    }
    fs::write(path, contents).expect("fixture file");
}

pub(crate) fn cleanup(path: &Path) {
    let _ = fs::remove_dir_all(path);
}

/// A temporary directory removed on drop, along with any
/// [`TempDir::sibling_dir`] made beside it.
pub(crate) struct TempDir {
    path: PathBuf,
    siblings: Vec<PathBuf>,
}

impl TempDir {
    pub(crate) fn new(label: &str) -> Self {
        Self {
            path: temporary_directory(label),
            siblings: Vec::new(),
        }
    }

    /// A directory spelled as the platform spells the temporary directory.
    pub(crate) fn plain(label: &str) -> Self {
        Self {
            path: plain_directory(label),
            siblings: Vec::new(),
        }
    }

    /// A project directory: `project.vibon` holds `document`, and each of
    /// `sources` is a project-relative path and its content.
    pub(crate) fn project(
        label: &str,
        document: &str,
        sources: &[(&str, &str)],
    ) -> Self {
        let directory = Self::new(label);
        write(&directory.path.join("project.vibon"), document);
        for (relative, source) in sources {
            write(&directory.path.join(relative), source);
        }
        directory
    }

    /// A project directory holding [`demo_project`].
    pub(crate) fn demo(label: &str, sources: &[(&str, &str)]) -> Self {
        Self::project(label, &demo_project(), sources)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// Writes `content` at the directory-relative `relative`.
    pub(crate) fn write(&self, relative: &str, content: &str) {
        write(&self.path.join(relative), content);
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
        cleanup(&self.path);
        for path in &self.siblings {
            cleanup(path);
        }
    }
}

/// A `project.vibon` document with the package `package`, the given `targets`
/// and `dependencies` (each already spelled as vibon), and a trailing newline.
pub(crate) fn manifest(package: &str, targets: &str, dependencies: &str) -> String {
    format!(
        "(record format: @project.v1 package: (record name: \"{package}\" version: \"0.1.0\") targets: (array {targets}) dependencies: {dependencies})\n"
    )
}

/// The project most tests need: package `demo` with one binary target `app`
/// rooted at `src/app` whose entry is `@app.main.execute`.
pub(crate) fn demo_project() -> String {
    manifest(
        "demo",
        "(record name: @app kind: @bin root: \"src/app\" entry: @app.main.execute effects: (array))",
        "(dict)",
    )
}

/// A library target named `name` whose sources are under `root`.
pub(crate) fn lib_target(name: &str, root: &str) -> String {
    format!("(record name: @{name} kind: @lib root: \"{root}\")")
}

/// A `project.vibon` document for package `hello` with the given `targets`
/// and `dependencies` (each already spelled as vibon), and no trailing newline.
pub(crate) fn lib_project(targets: &str, dependencies: &str) -> String {
    format!(
        "(record format: @project.v1 package: (record name: \"hello\" version: \"0.1.0\") targets: (array {targets}) dependencies: (dict {dependencies}))"
    )
}
