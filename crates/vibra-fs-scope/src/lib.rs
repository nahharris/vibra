//! The Windows file-scope adapter.
//!
//! A [`ScopedDir`] holds one directory handle, acquired once. Every operation
//! walks from that handle one name at a time with `NtCreateFile` and a
//! `RootDirectory`, never by joining strings into an absolute path, and closes
//! every intermediate handle before it returns. No name is followed if it is a
//! symbolic link, junction, mount point, or other reparse point.
//!
//! The adapter decides no name test. Every open reports the names the
//! filesystem gives each opened component (`real_names`: the long name, in the
//! case stored on disk) and the entry's kind, so that the caller can check
//! them before anything is read, created, or changed:
//!
//! - [`ScopedDir::open_read`] and [`ScopedDir::open_for_replace`] return a
//!   [`ScopedFile`] with its real names, its size, and its link count; the
//!   read, or [`ScopedFile::replace_contents`], is the caller's later step. A
//!   replace open does not truncate.
//! - [`ScopedDir::list`] returns a [`ScopedListing`]; its entries are read by
//!   the later [`ScopedListing::entries`].
//! - [`ScopedDir::stat`] returns the entry's [`Metadata`].
//! - Creation is [`ScopedDir::open_parent`], which holds the parent directory
//!   and reports its real names, then [`ScopedParent::create_new`], which
//!   creates the leaf exclusively relative to that held directory. Nothing is
//!   created and then removed.
//!
//! An operation that walks and fails answers a [`WalkError`]: the failure,
//! and the real names of the directories it had opened by then.
//!
//! The crate has no remove, rename, append, or directory creation. Every name
//! of a request passes [`check_name`] first. On every platform other than
//! Windows, [`ScopedDir::open_root`] returns [`ScopeError::Unsupported`].
//!
//! [`is_regular_file`] tells a standard stream that is a regular file from a
//! pipe or a device, which on Windows needs the handle's file type, and
//! [`close`] closes a file reporting what the system answers.
//!
//! The adapter's feasibility record is
//! `docs/roadmap/v0.0.2/m0/file-scope-feasibility.md`.

use std::fmt;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};

mod name;

pub use name::check_name;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{ScopedDir, ScopedListing, ScopedParent, close, is_regular_file};

#[cfg(not(windows))]
mod other;
#[cfg(not(windows))]
pub use other::{ScopedDir, ScopedListing, ScopedParent, close, is_regular_file};

/// Why a scoped operation did not happen.
///
/// The enum is closed. Every case except [`ScopeError::Io`] means that nothing
/// outside the checks was attempted: no data was read, created, or changed.
#[derive(Debug)]
pub enum ScopeError {
    /// A component of the request, or the root itself, is a symbolic link,
    /// junction, mount point, or other reparse point.
    DeniedReparse,
    /// A requested name breaks the name profile ([`check_name`]), or the
    /// filesystem refused it as invalid.
    DeniedName,
    /// The entry, or a directory it needs, does not exist.
    NotFound,
    /// Create-new found an existing entry, under any alias.
    AlreadyExists,
    /// A directory or another non-regular entry where a file is required.
    NotAFile,
    /// A non-directory where a directory is required.
    NotADirectory,
    /// A name the filesystem reports is not Unicode.
    NotUnicode,
    /// The platform or the volume cannot give the guarantees of this adapter.
    Unsupported,
    /// Any other operating-system failure, `PermissionDenied` included.
    Io(io::Error),
}

impl fmt::Display for ScopeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DeniedReparse => formatter.write_str("deniedReparse"),
            Self::DeniedName => formatter.write_str("deniedName"),
            Self::NotFound => formatter.write_str("notFound"),
            Self::AlreadyExists => formatter.write_str("alreadyExists"),
            Self::NotAFile => formatter.write_str("notAFile"),
            Self::NotADirectory => formatter.write_str("notADirectory"),
            Self::NotUnicode => formatter.write_str("notUnicode"),
            Self::Unsupported => formatter.write_str("unsupported"),
            Self::Io(error) => write!(formatter, "io: {error}"),
        }
    }
}

impl std::error::Error for ScopeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

/// Why an operation that walks from the root did not happen, with the names
/// the filesystem reported for the directories it had opened by then, from
/// the root down: the caller can decide whether the answer may tell anything
/// about what lies below them.
#[derive(Debug)]
pub struct WalkError {
    /// Why the operation did not happen.
    pub error: ScopeError,
    /// The real names of the directories opened before the failure, a prefix
    /// of the request's components; empty when it failed at the root's first
    /// component or before any open.
    pub opened: Vec<String>,
}

impl From<ScopeError> for WalkError {
    fn from(error: ScopeError) -> Self {
        Self {
            error,
            opened: Vec::new(),
        }
    }
}

impl From<WalkError> for ScopeError {
    fn from(error: WalkError) -> Self {
        error.error
    }
}

impl fmt::Display for WalkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(formatter)
    }
}

impl std::error::Error for WalkError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

/// The kind of an opened entry, as the filesystem reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    /// A regular file.
    File,
    /// A directory.
    Directory,
    /// Any other kind of entry.
    Other,
}

/// What [`ScopedDir::stat`] reports of one entry, which it does not follow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Metadata {
    /// The name the filesystem reports for each component of the request.
    pub real_names: Vec<String>,
    /// The entry's kind.
    pub kind: EntryKind,
    /// The byte length of a regular file; zero for any other kind.
    pub size: u64,
}

/// A file opened through a [`ScopedDir`], whose checks passed.
///
/// The handle carries only the access its operation asked for: a read handle
/// cannot write, and a replace handle cannot read. Opening it read and changed
/// nothing; the caller inspects [`ScopedFile::real_names`],
/// [`ScopedFile::size`], and [`ScopedFile::links`] before the data step.
#[derive(Debug)]
pub struct ScopedFile {
    file: File,
    real_names: Vec<String>,
    size: u64,
    links: u32,
}

impl ScopedFile {
    #[cfg(windows)]
    fn new(file: File, real_names: Vec<String>, size: u64, links: u32) -> Self {
        Self {
            file,
            real_names,
            size,
            links,
        }
    }

    /// The name the filesystem reports for each component of the request,
    /// the leaf last: the long name, in the case stored on disk.
    pub fn real_names(&self) -> &[String] {
        &self.real_names
    }

    /// The entry's kind, which is always [`EntryKind::File`]: an open that
    /// finds another kind is [`ScopeError::NotAFile`].
    pub fn kind(&self) -> EntryKind {
        EntryKind::File
    }

    /// The byte length of the file when it was opened.
    pub fn size(&self) -> u64 {
        self.size
    }

    /// The link count of the opened file when it was opened: the number of
    /// names the file has, inside the root or not.
    pub fn links(&self) -> u32 {
        self.links
    }

    /// Reads at most `max` bytes from the start of the file. The buffer is
    /// reserved fallibly: an allocation the operating system refuses is
    /// [`ScopeError::Io`] of kind [`std::io::ErrorKind::OutOfMemory`].
    pub fn read_at_most(&mut self, max: u64) -> Result<Vec<u8>, ScopeError> {
        let mut bytes = Vec::new();
        let expected = usize::try_from(max.min(self.size)).unwrap_or(usize::MAX);
        bytes
            .try_reserve_exact(expected)
            .map_err(|_| ScopeError::Io(std::io::ErrorKind::OutOfMemory.into()))?;
        self.file
            .seek(SeekFrom::Start(0))
            .and_then(|_| (&mut self.file).take(max).read_to_end(&mut bytes))
            .map_err(ScopeError::Io)?;
        Ok(bytes)
    }

    /// Writes `bytes` at the current position. A new file starts empty.
    pub fn write_all(&mut self, bytes: &[u8]) -> Result<(), ScopeError> {
        self.file.write_all(bytes).map_err(ScopeError::Io)
    }

    /// The deliberate replacement step: truncates the opened file to zero
    /// bytes, then writes `bytes` from its start, on the handle that was
    /// opened and checked. The file keeps its identity and every name it has.
    /// It is not transactional: a failure can leave the file empty or holding
    /// a prefix of `bytes`.
    pub fn replace_contents(&mut self, bytes: &[u8]) -> Result<(), ScopeError> {
        self.file
            .set_len(0)
            .and_then(|()| self.file.seek(SeekFrom::Start(0)))
            .and_then(|_| self.file.write_all(bytes))
            .map_err(ScopeError::Io)
    }
}
