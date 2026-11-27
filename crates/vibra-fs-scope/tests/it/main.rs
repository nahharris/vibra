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

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
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
    pub fn inside_snapshot(&self) -> BTreeMap<PathBuf, Node> {
        snapshot(&self.inside())
    }

    /// Every entry under `outside`, with the bytes of each file.
    pub fn outside_snapshot(&self) -> BTreeMap<PathBuf, Node> {
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

/// One entry of a snapshot.
#[derive(Debug, PartialEq, Eq)]
pub enum Node {
    Directory,
    File(Vec<u8>),
    /// A symbolic link or junction, recorded and not followed.
    Link,
}

/// The recursive listing of `directory` and the bytes of every file, read
/// with `std::fs` and following no link.
pub fn snapshot(directory: &Path) -> BTreeMap<PathBuf, Node> {
    let mut found = BTreeMap::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in fs::read_dir(&next).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let kind = entry.file_type().unwrap();
            if kind.is_symlink() {
                found.insert(path, Node::Link);
            } else if kind.is_dir() {
                pending.push(path.clone());
                found.insert(path, Node::Directory);
            } else {
                found.insert(path.clone(), Node::File(fs::read(&path).unwrap()));
            }
        }
    }
    found
}

/// Prints the NOT RUN line of a case.
pub fn not_run(case: &str, reason: &str) {
    eprintln!("NOT RUN {case}: {reason}");
}

/// Creates a file symbolic link, or says why the machine cannot.
pub fn symlink_file(link: &Path, target: &Path) -> Result<(), String> {
    std::os::windows::fs::symlink_file(target, link)
        .map_err(|error| format!("cannot create a file symbolic link: {error}"))
}

/// Creates a directory symbolic link, or says why the machine cannot.
pub fn symlink_dir(link: &Path, target: &Path) -> Result<(), String> {
    std::os::windows::fs::symlink_dir(target, link)
        .map_err(|error| format!("cannot create a directory symbolic link: {error}"))
}

/// Creates a junction with `cmd /c mklink /J`, or says why it failed.
pub fn junction(link: &Path, target: &Path) -> Result<(), String> {
    let status = Command::new("cmd")
        .raw_arg(format!(
            "/c mklink /J \"{}\" \"{}\"",
            link.display(),
            target.display()
        ))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| format!("cannot run cmd: {error}"))?;
    if status.success() && link.exists() {
        Ok(())
    } else {
        Err(format!("mklink /J failed with {status}"))
    }
}

/// The 8.3 short name of `path`, or why it has none.
pub fn short_name(path: &Path) -> Result<String, String> {
    let output = Command::new("cmd")
        .raw_arg(format!(
            "/c for %I in (\"{}\") do @echo %~snxI",
            path.display()
        ))
        .output()
        .map_err(|error| format!("cannot run cmd: {error}"))?;
    let short = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let long = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if !output.status.success() || short.is_empty() {
        Err("cmd did not report a short name".to_owned())
    } else if short.eq_ignore_ascii_case(long) {
        Err(format!(
            "the volume generated no 8.3 short name for {long:?} (8.3 creation is disabled)"
        ))
    } else {
        Ok(short)
    }
}

/// Makes the empty directory `directory` case-sensitive, or says why the
/// machine cannot.
pub fn case_sensitive(directory: &Path) -> Result<(), String> {
    let status = Command::new("fsutil")
        .args(["file", "setCaseSensitiveInfo"])
        .arg(directory)
        .arg("enable")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| format!("cannot run fsutil: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "fsutil file setCaseSensitiveInfo failed with {status}"
        ))
    }
}

/// The file identity of `path` (its NTFS file ID), or why it cannot be read.
pub fn file_id(path: &Path) -> Result<String, String> {
    let output = Command::new("fsutil")
        .args(["file", "queryFileID"])
        .arg(path)
        .output()
        .map_err(|error| format!("cannot run fsutil: {error}"))?;
    let text = String::from_utf8_lossy(&output.stdout);
    match text
        .split_whitespace()
        .find(|token| token.starts_with("0x"))
    {
        Some(id) if output.status.success() => Ok(id.to_owned()),
        _ => Err(format!(
            "fsutil file queryFileID failed with {}",
            output.status
        )),
    }
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
