//! A project tree in a temporary directory of its own, for the crate's unit
//! tests.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::discovery::discover_project_at;
use crate::snapshot::SourceSnapshot;

/// A counter makes each tree's directory unique in the process.
static NEXT: AtomicU64 = AtomicU64::new(0);

/// A project tree, removed on drop.
pub(crate) struct Tree(PathBuf);

impl Tree {
    /// A tree of `files`, each a path under the root and its text.
    pub(crate) fn new(files: &[(&str, &str)]) -> Result<Self, String> {
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir()
            .join(format!("vibra-tree-{}-{serial}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let tree = Self(root);
        for (relative, text) in files {
            let path = tree.0.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            fs::write(path, text).map_err(|error| error.to_string())?;
        }
        Ok(tree)
    }

    /// The snapshot of the tree's project, found at exactly its root.
    pub(crate) fn snapshot(&self) -> Result<SourceSnapshot, String> {
        let project =
            discover_project_at(&self.0).map_err(|faults| format!("{faults:?}"))?;
        SourceSnapshot::capture(project).map_err(|faults| format!("{faults:?}"))
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
