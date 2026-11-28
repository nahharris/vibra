//! The host tests of the Windows file-scope adapter, on the real filesystem
//! under the temporary directory. Each case creates its objects in a new
//! directory. `r` is the scoped root (`Sandbox::inside`); `outside` is a
//! sibling directory the attacks aim at.
//!
//! A case that needs something the machine lacks (symbolic-link creation,
//! junctions, 8.3 short names, case-sensitive directories, `fsutil`) prints
//! `NOT RUN <case>: <reason>` to standard error and returns. Run with
//! `--nocapture` to see whether any case was not run; a NOT RUN case is never
//! counted as passed.

#![cfg(windows)]
#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    missing_docs
)]

mod attacks;
mod controls;
mod windows;

pub(crate) use windows::*;

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use vibra_fs_scope::{ScopeError, ScopedDir, ScopedFile};

/// The bytes of the file outside the root that every attack aims at.
pub const SECRET: &[u8] = b"outside secret";

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A fresh directory with an `inside` root and an `outside` target directory
/// holding `secret.txt`. It is removed on drop; removal does not follow links.
#[derive(Debug)]
pub struct Sandbox {
    base: PathBuf,
}

impl Sandbox {
    pub fn new(case: &str) -> Self {
        let base = std::env::temp_dir().join(format!(
            "vibra-fs-scope-{case}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("inside")).unwrap();
        fs::create_dir_all(base.join("outside")).unwrap();
        fs::write(base.join("outside").join("secret.txt"), SECRET).unwrap();
        Self { base }
    }

    pub fn base(&self) -> &Path {
        &self.base
    }

    /// The scoped root `r`.
    pub fn inside(&self) -> PathBuf {
        self.base.join("inside")
    }

    pub fn outside(&self) -> PathBuf {
        self.base.join("outside")
    }

    pub fn secret(&self) -> PathBuf {
        self.outside().join("secret.txt")
    }

    pub fn root(&self) -> ScopedDir {
        ScopedDir::open_root(&self.inside()).expect("the inside root opens")
    }

    /// Every entry under `r`, with the bytes of each file.
    pub(crate) fn inside_snapshot(&self) -> BTreeMap<PathBuf, Node> {
        snapshot(&self.inside())
    }

    /// Every entry under `outside`, with the bytes of each file.
    pub(crate) fn outside_snapshot(&self) -> BTreeMap<PathBuf, Node> {
        snapshot(&self.outside())
    }

    /// Holds `secret.txt` open with no sharing. While the lock lives, any
    /// open of the secret's data by anyone else fails with a sharing
    /// violation, so an attack that returns its typed denial cannot have
    /// opened the secret for reading or writing.
    pub fn lock_secret(&self) -> File {
        OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(self.secret())
            .expect("the secret can be locked")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

/// Creates a directory symbolic link, or says why the machine cannot.
pub fn symlink_dir(link: &Path, target: &Path) -> Result<(), String> {
    std::os::windows::fs::symlink_dir(target, link)
        .map_err(|error| format!("cannot create a directory symbolic link: {error}"))
}

/// Creates the file `names` as a driver does: holds the parent, then creates
/// the leaf relative to it. An empty request is `DeniedName`.
pub fn create(root: &ScopedDir, names: &[&str]) -> Result<ScopedFile, ScopeError> {
    let Some((leaf, parent)) = names.split_last() else {
        return Err(ScopeError::DeniedName);
    };
    root.open_parent(parent)?.create_new(leaf)
}

/// Reads the whole of an opened file, as much as its size at open.
pub fn read_opened(file: &mut ScopedFile) -> Vec<u8> {
    let size = file.size();
    file.read_at_most(size).unwrap()
}

/// Asserts the exact denial, with the case named in the message.
#[track_caller]
pub fn assert_denied<T: std::fmt::Debug, E: Into<ScopeError>>(
    result: Result<T, E>,
    expected: fn(&ScopeError) -> bool,
    case: &str,
) {
    match result.map_err(Into::into) {
        Err(error) if expected(&error) => {}
        other => panic!("{case}: expected the typed denial, got {other:?}"),
    }
}

pub fn is_denied_reparse(error: &ScopeError) -> bool {
    matches!(error, ScopeError::DeniedReparse)
}

pub fn is_denied_name(error: &ScopeError) -> bool {
    matches!(error, ScopeError::DeniedName)
}

pub fn is_already_exists(error: &ScopeError) -> bool {
    matches!(error, ScopeError::AlreadyExists)
}

pub fn is_not_found(error: &ScopeError) -> bool {
    matches!(error, ScopeError::NotFound)
}

pub fn is_not_a_file(error: &ScopeError) -> bool {
    matches!(error, ScopeError::NotAFile)
}

pub fn is_not_a_directory(error: &ScopeError) -> bool {
    matches!(error, ScopeError::NotADirectory)
}
