//! Every platform other than Windows: each operation is unsupported. A request
//! with a name that is not plain is still denied as such.

use std::path::Path;

use crate::{ScopeError, ScopedFile, name};

/// A held root directory. On this platform none can be acquired.
#[derive(Debug)]
pub struct ScopedDir {
    _unconstructible: (),
}

impl ScopedDir {
    /// Returns [`ScopeError::Unsupported`].
    pub fn open_root(_path: &Path) -> Result<Self, ScopeError> {
        Err(ScopeError::Unsupported)
    }

    /// Returns [`ScopeError::DeniedName`] or [`ScopeError::Unsupported`].
    pub fn open_read(&self, names: &[&str]) -> Result<ScopedFile, ScopeError> {
        name::check_all(names)?;
        Err(ScopeError::Unsupported)
    }

    /// Returns [`ScopeError::DeniedName`] or [`ScopeError::Unsupported`].
    pub fn create_new(&self, names: &[&str]) -> Result<ScopedFile, ScopeError> {
        name::check_all(names)?;
        Err(ScopeError::Unsupported)
    }

    /// Returns [`ScopeError::DeniedName`] or [`ScopeError::Unsupported`].
    pub fn open_for_replace(&self, names: &[&str]) -> Result<ScopedFile, ScopeError> {
        name::check_all(names)?;
        Err(ScopeError::Unsupported)
    }
}
