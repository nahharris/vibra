//! Every platform other than Windows: no root can be acquired, so
//! [`ScopedDir::open_root`] returns [`ScopeError::Unsupported`] and no other
//! operation can be reached. The signatures are those of the Windows adapter.

use std::marker::PhantomData;
use std::path::Path;

use crate::{Metadata, ScopeError, ScopedFile, WalkError, name};

/// A held root directory. On this platform none can be acquired.
#[derive(Debug)]
pub struct ScopedDir {
    _unconstructible: (),
}

/// A directory held for one creation. On this platform none exists.
#[derive(Debug)]
pub struct ScopedParent<'r> {
    _unconstructible: PhantomData<&'r ScopedDir>,
}

/// A directory opened for listing. On this platform none exists.
#[derive(Debug)]
pub struct ScopedListing {
    _unconstructible: (),
}

impl ScopedDir {
    /// Returns [`ScopeError::Unsupported`].
    pub fn open_root(_path: &Path) -> Result<Self, ScopeError> {
        Err(ScopeError::Unsupported)
    }

    /// Closes the root; none exists on this platform.
    ///
    /// # Errors
    ///
    /// None on this platform.
    pub fn close(self) -> std::io::Result<()> {
        Ok(())
    }

    /// Returns [`ScopeError::DeniedName`] or [`ScopeError::Unsupported`].
    pub fn open_read(&self, names: &[&str]) -> Result<ScopedFile, WalkError> {
        name::check_each(names)?;
        Err(ScopeError::Unsupported.into())
    }

    /// Returns [`ScopeError::DeniedName`] or [`ScopeError::Unsupported`].
    pub fn open_for_replace(&self, names: &[&str]) -> Result<ScopedFile, WalkError> {
        name::check_each(names)?;
        Err(ScopeError::Unsupported.into())
    }

    /// Returns [`ScopeError::DeniedName`] or [`ScopeError::Unsupported`].
    pub fn list(&self, names: &[&str]) -> Result<ScopedListing, WalkError> {
        name::check_each(names)?;
        Err(ScopeError::Unsupported.into())
    }

    /// Returns [`ScopeError::DeniedName`] or [`ScopeError::Unsupported`].
    pub fn stat(&self, names: &[&str]) -> Result<Metadata, WalkError> {
        name::check_each(names)?;
        Err(ScopeError::Unsupported.into())
    }

    /// Returns [`ScopeError::DeniedName`] or [`ScopeError::Unsupported`].
    pub fn open_parent(&self, names: &[&str]) -> Result<ScopedParent<'_>, WalkError> {
        name::check_each(names)?;
        Err(ScopeError::Unsupported.into())
    }
}

impl ScopedParent<'_> {
    /// The directory's real names; none exist on this platform.
    pub fn real_names(&self) -> &[String] {
        &[]
    }

    /// Returns [`ScopeError::DeniedName`] or [`ScopeError::Unsupported`].
    pub fn create_new(self, leaf: &str) -> Result<ScopedFile, ScopeError> {
        name::check_name(leaf)?;
        Err(ScopeError::Unsupported)
    }
}

impl ScopedListing {
    /// The directory's real names; none exist on this platform.
    pub fn real_names(&self) -> &[String] {
        &[]
    }

    /// Returns [`ScopeError::Unsupported`].
    pub fn entries(self) -> Result<Vec<String>, ScopeError> {
        Err(ScopeError::Unsupported)
    }
}

/// Closes `file`. Without a system call the standard library does not offer,
/// the close is the file's drop, whose failure is not reported.
///
/// # Errors
///
/// None on this platform.
pub fn close(file: std::fs::File) -> std::io::Result<()> {
    drop(file);
    Ok(())
}

/// Whether the open `file` is a regular file: its metadata says so. A pipe,
/// a terminal, and a character device are not.
pub fn is_regular_file(file: &std::fs::File) -> bool {
    file.metadata().is_ok_and(|metadata| metadata.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_root_is_unsupported() {
        let here = std::env::temp_dir();
        assert!(matches!(
            ScopedDir::open_root(&here),
            Err(ScopeError::Unsupported)
        ));
    }
}
