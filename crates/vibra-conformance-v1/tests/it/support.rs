//! Helpers shared by the test modules.
//!
//! Each helper lives here once. Where several modules once carried their own
//! copy, this is the one form that serves every caller: a module that needs a
//! variation builds it from these, and a module that needs something no other
//! module needs keeps it beside its tests.

use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

use vibra_conformance_v1::{
    Case, CaseObservation, ConformanceOperation, Corpus, HandlerError, ProfileHandler,
};
use vibra_ir_v1::{CheckedProgram, Expr};
use vibra_types_v1::check_source;

pub(crate) use vibra_conformance_v1::{corpus_root, workspace_root};

// -- the filesystem -----------------------------------------------------------

/// The one process-wide serial that keeps every temporary directory of this
/// test binary apart, whichever module made it.
static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

/// A fresh, empty directory under the system temporary directory, removed when
/// it drops. It dereferences to its path.
#[derive(Debug)]
pub(crate) struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// A directory named for `label`, this process, and a serial.
    pub(crate) fn new(label: &str) -> Self {
        Self::under(&std::env::temp_dir(), label)
    }

    /// Like [`TempDir::new`], under the canonical form of the temporary
    /// directory, for the checks that confine a tree to its canonical root.
    pub(crate) fn canonical(label: &str) -> Self {
        let parent = std::fs::canonicalize(std::env::temp_dir())
            .expect("a canonical temporary directory");
        Self::under(&parent, label)
    }

    fn under(parent: &Path, label: &str) -> Self {
        let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(
            "vibra-conformance-{label}-{}-{serial}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("a temporary directory");
        Self { path }
    }

    /// The directory.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Deref for TempDir {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for TempDir {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Copies the tree `from` to `to`, creating `to`.
pub(crate) fn copy_directory(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("directory");
    for entry in std::fs::read_dir(from).expect("a directory") {
        let entry = entry.expect("an entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("a type").is_dir() {
            copy_directory(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).expect("copy");
        }
    }
}

/// Every `.rs` file below `directory`, appended to `found`.
pub(crate) fn rust_sources(directory: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("readable source directory") {
        let path = entry.expect("readable directory entry").path();
        if path.is_dir() {
            rust_sources(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

/// The production code of a Rust source: everything before its test module,
/// without line comments.
pub(crate) fn production(path: &Path) -> String {
    let text = std::fs::read_to_string(path).expect("readable source");
    let end = text.find("\n#[cfg(test)]").unwrap_or(text.len());
    text[..end]
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `(workspace-relative path, production code)` of every source of the crate
/// `name`.
pub(crate) fn crate_sources(name: &str) -> Vec<(String, String)> {
    let mut paths = Vec::new();
    rust_sources(
        &workspace_root().join("crates").join(name).join("src"),
        &mut paths,
    );
    paths
        .into_iter()
        .map(|path| {
            let relative = path
                .strip_prefix(workspace_root())
                .expect("inside the workspace")
                .to_string_lossy()
                .replace('\\', "/");
            (relative, production(&path))
        })
        .collect()
}

// -- the corpus ---------------------------------------------------------------

/// The repository corpus, loaded once for the whole test binary.
pub(crate) fn corpus() -> &'static Corpus {
    static CORPUS: OnceLock<Corpus> = OnceLock::new();
    CORPUS.get_or_init(|| Corpus::discover(corpus_root()).expect("the corpus loads"))
}

/// The one case of the corpus directory `id`.
pub(crate) fn corpus_case(id: &str) -> Case {
    Corpus::discover(corpus_root().join(id))
        .expect("a corpus case directory")
        .cases()
        .first()
        .cloned()
        .expect("a corpus case")
}

/// The text of the one source document of a case: the source of an `interpret`
/// case, or the only module of the one-module workspace of a `workspace-check`
/// case, which is `src/app/main.vib` of its tree. `None` for any other case,
/// and for a workspace of several modules.
pub(crate) fn single_source(case: &Case) -> Option<String> {
    match case.manifest().operation() {
        ConformanceOperation::Interpret => {
            let source_id = case.manifest().inputs.source.as_deref()?;
            Some(case.read_file(source_id).expect("the source reads"))
        }
        ConformanceOperation::WorkspaceCheck => {
            let files = case.tree_files().expect("the tree reads");
            let mut modules =
                files.iter().filter(|file| file.source_id.ends_with(".vib"));
            let module = modules.next()?;
            if modules.next().is_some() || module.source_id != "tree/src/app/main.vib" {
                return None;
            }
            Some(String::from_utf8(module.bytes.clone()).expect("a UTF-8 source"))
        }
        _ => None,
    }
}

/// The source of the one-source corpus case `id`, so that a host test and its
/// case cannot drift.
pub(crate) fn corpus_source(id: &str) -> String {
    single_source(&corpus_case(id)).unwrap_or_else(|| panic!("{id} has no one source"))
}

/// A source program of the corpus and the canonical form of what the
/// checker made of it.
#[derive(Debug)]
pub(crate) struct CheckedSource {
    pub(crate) case_id: String,
    pub(crate) source: String,
    /// `None` when the checker rejected the source.
    pub(crate) canonical: Option<String>,
}

/// Checks the source document of every `interpret` case and the module of every
/// one-module `workspace-check` case, in corpus order. This is the whole-corpus check; most callers want
/// [`checked_sources`], which does it once.
pub(crate) fn check_every_source() -> Vec<CheckedSource> {
    let mut forms = Vec::new();
    for case in corpus().cases() {
        let Some(source) = single_source(case) else {
            continue;
        };
        let result = check_source("input.vib", &source);
        forms.push(CheckedSource {
            case_id: case.manifest().id.clone(),
            canonical: result.program().map(CheckedProgram::canonical_vibon),
            source,
        });
    }
    forms
}

/// [`check_every_source`], computed once for the whole test binary.
pub(crate) fn checked_sources() -> &'static [CheckedSource] {
    static CHECKED: OnceLock<Vec<CheckedSource>> = OnceLock::new();
    CHECKED.get_or_init(check_every_source)
}

/// A handler that reports one fixed observation for every case.
pub(crate) struct FixedHandler {
    pub(crate) observation: CaseObservation,
}

impl FixedHandler {
    pub(crate) fn new(observation: CaseObservation) -> Self {
        Self { observation }
    }
}

impl ProfileHandler for FixedHandler {
    fn run(&self, _case: &Case) -> Result<CaseObservation, HandlerError> {
        Ok(self.observation.clone())
    }
}

// -- running the test binary again --------------------------------------------

/// Runs the `#[ignore]`d test `name` of the module `module_path` in a child
/// process of this test binary, with its output captured.
///
/// `module_path` is the caller's `module_path!()`. A test is selected by its
/// path from the crate root, so the crate's own name is dropped from it. That is
/// what `cargo test` selects by, and nextest runs the same binary through the
/// same harness, so one name serves both.
pub(crate) fn run_ignored_child(module_path: &str, name: &str) -> Output {
    let in_crate = module_path.split_once("::").map_or("", |(_, path)| path);
    Command::new(std::env::current_exe().expect("this test binary"))
        .args([
            "--ignored",
            "--exact",
            &format!("{in_crate}::{name}"),
            "--nocapture",
            "--test-threads=1",
        ])
        .output()
        .expect("the child process runs")
}

// -- checked programs ------------------------------------------------------------

/// The checked program of `source`.
pub(crate) fn checked(source: &str) -> CheckedProgram {
    let result = check_source("input.vib", source);
    assert!(result.accepted(), "{:?}\n{source}", result.diagnostics());
    result.program().expect("an accepted program").clone()
}

/// How many calls in tail position the functions and module values of the
/// source `source_id` hold, not counting those of the standard library the
/// program also carries.
pub(crate) fn tail_calls(program: &CheckedProgram, source_id: &str) -> usize {
    let count = |body: &Expr| format!("{body:?}").matches("tail: true").count();
    let functions = program
        .functions()
        .iter()
        .filter(|function| function.origin().source_id() == source_id)
        .map(|function| count(function.body()));
    let globals = program
        .globals()
        .iter()
        .filter(|global| global.origin().source_id() == source_id)
        .map(|global| count(global.initializer()));
    functions.chain(globals).sum()
}
