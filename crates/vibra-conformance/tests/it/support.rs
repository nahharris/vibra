//! Helpers shared by the test modules.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) use vibra_conformance::workspace_root;

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

/// A fresh, empty directory under the target's scratch directory, named after
/// `name` and unique within this run.
pub(crate) fn scratch_directory(name: &str) -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "{name}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    if directory.exists() {
        std::fs::remove_dir_all(&directory).expect("remove a stale scratch directory");
    }
    std::fs::create_dir_all(&directory).expect("create a scratch directory");
    directory
}

/// Writes `contents` at `relative` below `root`, creating directories.
pub(crate) fn write(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create a fixture directory");
    }
    std::fs::write(&path, contents).expect("write a fixture file");
}

/// The header of the rule index followed by one row per `(identity, status)`.
pub(crate) fn rules(rows: &[(&str, &str)]) -> String {
    let mut text = format!("{}\n", vibra_conformance::RULES_HEADER);
    for (identity, status) in rows {
        text.push_str(&format!(
            "{identity}\t{status}\t07-diagnostics-and-conformance.md#conformance-corpus\n"
        ));
    }
    text
}

/// A valid `reader` case manifest for `id` addressing `rule`, whose input is
/// `input.vibon`.
pub(crate) fn reader_case(id: &str, rule: &str) -> String {
    format!(
        "id = \"{id}\"\n\
         rule = \"{rule}\"\n\
         profile = \"readerV2\"\n\
         operation = \"reader\"\n\
         description = \"07-diagnostics-and-conformance.md#conformance-corpus\"\n\
         \n\
         [inputs]\n\
         data = [\"input.vibon\"]\n\
         \n\
         [expect]\n\
         verdict = \"accepted\"\n"
    )
}
