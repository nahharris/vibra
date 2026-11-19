//! The Windows file-scope adapter.
//!
//! A [`ScopedDir`] holds one directory handle, acquired once. Every later open
//! walks from that handle one name at a time with `NtCreateFile` and a
//! `RootDirectory`, never by joining strings into an absolute path. No name is
//! followed if it is a symbolic link, junction, mount point, or other reparse
//! point, and the name the filesystem reports for each opened entry must equal
//! the requested name exactly. Opening a file for replacement does not
//! truncate it; [`ScopedFile::replace_contents`] is the later, deliberate step.
//! Creation is create-new only.
//!
//! The adapter is the feasibility check of v0.0.2 M0. Its record is
//! `docs/roadmap/v0.0.2/m0/file-scope-feasibility.md`. On every platform other
//! than Windows each operation returns [`ScopeError::Unsupported`].

use std::fmt;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};

mod name;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::ScopedDir;

#[cfg(not(windows))]
mod other;
#[cfg(not(windows))]
pub use other::ScopedDir;

/// Why a scoped operation did not happen.
///
/// The enum is closed. Every case except [`ScopeError::Io`] means that nothing
/// outside the checks was attempted: no data was read, created, or changed.
#[derive(Debug)]
pub enum ScopeError {
    /// A component of the request, or the root itself, is a symbolic link,
    /// junction, mount point, or other reparse point.
    DeniedReparse,
    /// A requested name breaks the lexical rules of `RelativePath`, or the
    /// entry the filesystem opened has a different real name: another case,
    /// an 8.3 short name, or any other alias.
    DeniedName,
    /// The entry, or a directory it needs, does not exist.
    NotFound,
    /// Create-new found an existing entry, under any alias.
    AlreadyExists,
    /// The platform or the volume cannot give the guarantees of this adapter.
    Unsupported,
    /// Any other operating-system failure, including a wrong kind of entry
    /// (`NotADirectory`, `IsADirectory`) and `PermissionDenied`.
    Io(io::Error),
}

impl fmt::Display for ScopeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DeniedReparse => formatter.write_str("deniedReparse"),
            Self::DeniedName => formatter.write_str("deniedName"),
            Self::NotFound => formatter.write_str("notFound"),
            Self::AlreadyExists => formatter.write_str("alreadyExists"),
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

/// A file opened through a [`ScopedDir`], after every check passed.
///
/// The handle carries only the access its operation asked for: a read handle
/// cannot write, and a replace handle cannot read.
#[derive(Debug)]
pub struct ScopedFile {
    file: File,
}

impl ScopedFile {
    #[cfg(windows)]
    fn new(file: File) -> Self {
        Self { file }
    }

    /// Reads the whole file from its start.
    pub fn read_all(&mut self) -> Result<Vec<u8>, ScopeError> {
        let mut bytes = Vec::new();
        self.file
            .seek(SeekFrom::Start(0))
            .and_then(|_| self.file.read_to_end(&mut bytes))
            .map_err(ScopeError::Io)?;
        Ok(bytes)
    }

    /// Writes `bytes` at the current position. A new file starts empty.
    pub fn write_all(&mut self, bytes: &[u8]) -> Result<(), ScopeError> {
        self.file.write_all(bytes).map_err(ScopeError::Io)
    }

    /// The deliberate replacement step: truncates the opened file to zero
    /// bytes, then writes `bytes` from its start, on the handle that was
    /// opened and checked. It is not transactional: a failure can leave the
    /// file empty or holding a prefix of `bytes`.
    pub fn replace_contents(&mut self, bytes: &[u8]) -> Result<(), ScopeError> {
        self.file
            .set_len(0)
            .and_then(|()| self.file.seek(SeekFrom::Start(0)))
            .and_then(|_| self.file.write_all(bytes))
            .map_err(ScopeError::Io)
    }
}
