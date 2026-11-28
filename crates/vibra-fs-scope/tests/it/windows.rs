//! The Windows filesystem helpers the tests on the real filesystem share:
//! the snapshot of a directory, the NOT RUN line, and the objects a test
//! makes with the machine's own tools. The tests of `vibra-fs-scope` declare
//! this module; those of `vibra-host` include it with `#[path]`.

use std::collections::BTreeMap;
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// One entry of a snapshot.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Node {
    Directory,
    File(Vec<u8>),
    /// A symbolic link or junction, recorded and not followed.
    Link,
}

/// The recursive listing of `directory` and the bytes of every file, read
/// with `std::fs` and following no link.
pub(crate) fn snapshot(directory: &Path) -> BTreeMap<PathBuf, Node> {
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
pub(crate) fn not_run(case: &str, reason: &str) {
    eprintln!("NOT RUN {case}: {reason}");
}

/// Creates a file symbolic link, or says why the machine cannot.
pub(crate) fn symlink_file(link: &Path, target: &Path) -> Result<(), String> {
    std::os::windows::fs::symlink_file(target, link)
        .map_err(|error| format!("cannot create a file symbolic link: {error}"))
}

/// Creates a junction with `cmd /c mklink /J`, or says why it failed.
pub(crate) fn junction(link: &Path, target: &Path) -> Result<(), String> {
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
pub(crate) fn short_name(path: &Path) -> Result<String, String> {
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
pub(crate) fn case_sensitive(directory: &Path) -> Result<(), String> {
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
pub(crate) fn file_id(path: &Path) -> Result<String, String> {
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
