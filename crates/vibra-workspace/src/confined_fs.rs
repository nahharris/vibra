//! Directory-handle operations for confined workspace mutations.

use std::ffi::OsStr;
#[cfg(not(unix))]
use std::fs::{self, OpenOptions};
use std::fs::{File, Permissions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

/// An opened directory whose child operations do not follow reparse points.
///
/// Unix uses directory descriptors and `*at` calls. Windows keeps each
/// traversed directory handle open without delete sharing, and opens each new
/// component with `FILE_FLAG_OPEN_REPARSE_POINT` before accepting it.
#[derive(Debug)]
pub struct ConfinedDir {
    path: PathBuf,
    #[cfg(unix)]
    handle: rustix::fd::OwnedFd,
    #[cfg(windows)]
    handles: Vec<File>,
}

impl ConfinedDir {
    /// Opens an absolute directory by traversing each component without
    /// following symbolic links, junctions, or other reparse points.
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref();
        if !path.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "confined directory path must be absolute",
            ));
        }
        #[cfg(unix)]
        {
            use rustix::fs::{Mode, OFlags, open, openat};

            let mut current = Self {
                path: PathBuf::from("/"),
                handle: open(
                    "/",
                    OFlags::RDONLY
                        | OFlags::DIRECTORY
                        | OFlags::CLOEXEC
                        | OFlags::NOFOLLOW,
                    Mode::empty(),
                )
                .map_err(to_io_error)?,
            };
            for component in path.components() {
                match component {
                    Component::RootDir => {}
                    Component::Normal(name) => {
                        let handle = openat(
                            &current.handle,
                            name,
                            OFlags::RDONLY
                                | OFlags::DIRECTORY
                                | OFlags::CLOEXEC
                                | OFlags::NOFOLLOW,
                            Mode::empty(),
                        )
                        .map_err(to_io_error)?;
                        current = Self {
                            path: current.path.join(name),
                            handle,
                        };
                    }
                    Component::CurDir => {}
                    Component::ParentDir | Component::Prefix(_) => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "confined directory path is not normalized",
                        ));
                    }
                }
            }
            Ok(current)
        }
        #[cfg(windows)]
        {
            let canonical = fs::canonicalize(path)?;
            if !same_windows_path(&canonical, path) {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "confined directory path is not canonical",
                ));
            }
            let handle = open_windows_directory(path)?;
            Ok(Self {
                path: PathBuf::from(path),
                handles: vec![handle],
            })
        }
        #[cfg(not(any(unix, windows)))]
        {
            let canonical = fs::canonicalize(path)?;
            if canonical != path {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "platform does not support handle-safe confined paths",
                ));
            }
            Ok(Self { path: canonical })
        }
    }

    /// Returns the lexical path represented by this directory handle.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Opens a child directory path, rejecting non-normal components.
    pub fn open_dir(&self, relative: impl AsRef<Path>) -> io::Result<Self> {
        let mut current = self.clone_handle()?;
        let relative = relative.as_ref();
        if relative.as_os_str().is_empty() {
            return Ok(current);
        }
        for component in relative.components() {
            match component {
                Component::Normal(name) => current = current.open_child(name)?,
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "confined path may contain only normal components",
                    ));
                }
            }
        }
        Ok(current)
    }

    /// Creates a single child directory.
    pub fn create_dir(&self, name: &OsStr) -> io::Result<()> {
        validate_name(name)?;
        #[cfg(unix)]
        {
            use rustix::fs::{Mode, mkdirat};
            mkdirat(&self.handle, name, Mode::from_raw_mode(0o777)).map_err(to_io_error)
        }
        #[cfg(windows)]
        {
            fs::create_dir(self.path.join(name))
        }
        #[cfg(not(any(unix, windows)))]
        {
            fs::create_dir(self.path.join(name))
        }
    }

    /// Reads a regular child file without following a symbolic link or
    /// reparse point.
    pub fn read_file(&self, name: &OsStr) -> io::Result<Vec<u8>> {
        let mut file = self.open_file(name)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    /// Returns permissions for a regular child file.
    pub fn file_permissions(&self, name: &OsStr) -> io::Result<Permissions> {
        Ok(self.open_file(name)?.metadata()?.permissions())
    }

    /// Creates a new regular child file exclusively and writes all bytes.
    pub fn write_new_file(
        &self,
        name: &OsStr,
        bytes: &[u8],
        permissions: Option<&Permissions>,
    ) -> io::Result<()> {
        self.write_new_file_with(name, |file| {
            if let Some(permissions) = permissions {
                file.set_permissions(permissions.clone())?;
            }
            file.write_all(bytes)?;
            file.sync_all()
        })
    }

    /// Atomically renames one direct child entry to a direct child name in
    /// another opened directory.
    pub fn rename_to(
        &self,
        from: &OsStr,
        destination: &Self,
        to: &OsStr,
    ) -> io::Result<()> {
        validate_name(from)?;
        validate_name(to)?;
        #[cfg(unix)]
        {
            rustix::fs::renameat(&self.handle, from, &destination.handle, to)
                .map_err(to_io_error)
        }
        #[cfg(windows)]
        {
            fs::rename(self.path.join(from), destination.path.join(to))
        }
        #[cfg(not(any(unix, windows)))]
        {
            fs::rename(self.path.join(from), destination.path.join(to))
        }
    }

    /// Renames a direct child only if the destination does not already exist.
    pub fn rename_noreplace_to(
        &self,
        from: &OsStr,
        destination: &Self,
        to: &OsStr,
    ) -> io::Result<()> {
        validate_name(from)?;
        validate_name(to)?;
        #[cfg(all(
            unix,
            any(target_os = "linux", target_os = "macos", target_os = "redox")
        ))]
        {
            rustix::fs::renameat_with(
                &self.handle,
                from,
                &destination.handle,
                to,
                rustix::fs::RenameFlags::NOREPLACE,
            )
            .map_err(to_io_error)
        }
        #[cfg(all(
            unix,
            not(any(target_os = "linux", target_os = "macos", target_os = "redox"))
        ))]
        {
            if destination.open_dir(to).is_ok() || destination.open_file(to).is_ok() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "destination already exists",
                ));
            }
            rustix::fs::renameat(&self.handle, from, &destination.handle, to)
                .map_err(to_io_error)
        }
        #[cfg(windows)]
        {
            fs::rename(self.path.join(from), destination.path.join(to))
        }
        #[cfg(not(any(unix, windows)))]
        {
            if destination.path.join(to).exists() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "destination already exists",
                ));
            }
            fs::rename(self.path.join(from), destination.path.join(to))
        }
    }

    /// Checks whether a direct child entry names the same directory as an
    /// already opened handle.
    pub fn is_same_directory(&self, name: &OsStr, other: &Self) -> io::Result<bool> {
        let candidate = self.open_child(name)?;
        #[cfg(unix)]
        {
            let candidate =
                rustix::fs::fstat(&candidate.handle).map_err(to_io_error)?;
            let opened = rustix::fs::fstat(&other.handle).map_err(to_io_error)?;
            Ok(candidate.st_dev == opened.st_dev && candidate.st_ino == opened.st_ino)
        }
        #[cfg(windows)]
        {
            Ok(candidate.path == other.path)
        }
        #[cfg(not(any(unix, windows)))]
        {
            Ok(candidate.path == other.path)
        }
    }

    /// Checks whether two opened directory handles identify the same object.
    pub fn same_as(&self, other: &Self) -> io::Result<bool> {
        #[cfg(unix)]
        {
            let left = rustix::fs::fstat(&self.handle).map_err(to_io_error)?;
            let right = rustix::fs::fstat(&other.handle).map_err(to_io_error)?;
            Ok(left.st_dev == right.st_dev && left.st_ino == right.st_ino)
        }
        #[cfg(windows)]
        {
            Ok(self.path == other.path)
        }
        #[cfg(not(any(unix, windows)))]
        {
            Ok(self.path == other.path)
        }
    }

    /// Removes a direct child file without following links.
    pub fn remove_file(&self, name: &OsStr) -> io::Result<()> {
        validate_name(name)?;
        #[cfg(unix)]
        {
            rustix::fs::unlinkat(&self.handle, name, rustix::fs::AtFlags::empty())
                .map_err(to_io_error)
        }
        #[cfg(windows)]
        {
            fs::remove_file(self.path.join(name))
        }
        #[cfg(not(any(unix, windows)))]
        {
            fs::remove_file(self.path.join(name))
        }
    }

    /// Removes a direct child directory only when it is empty.
    pub fn remove_dir(&self, name: &OsStr) -> io::Result<()> {
        validate_name(name)?;
        #[cfg(unix)]
        {
            rustix::fs::unlinkat(&self.handle, name, rustix::fs::AtFlags::REMOVEDIR)
                .map_err(to_io_error)
        }
        #[cfg(windows)]
        {
            fs::remove_dir(self.path.join(name))
        }
        #[cfg(not(any(unix, windows)))]
        {
            fs::remove_dir(self.path.join(name))
        }
    }

    /// Reports whether the directory represented by this handle is empty.
    pub fn is_empty(&self) -> io::Result<bool> {
        self.is_empty_except_name(None)
    }

    /// Reports whether the directory is empty apart from one named child.
    pub fn is_empty_except(&self, name: &OsStr) -> io::Result<bool> {
        validate_name(name)?;
        self.is_empty_except_name(Some(name))
    }

    fn is_empty_except_name(&self, allowed: Option<&OsStr>) -> io::Result<bool> {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;

            let entries =
                rustix::fs::Dir::read_from(&self.handle).map_err(to_io_error)?;
            entries_are_empty(entries.map(|entry| {
                entry.map(|entry| {
                    let name = entry.file_name().to_bytes();
                    name == b"."
                        || name == b".."
                        || allowed.is_some_and(|allowed| name == allowed.as_bytes())
                })
            }))
        }
        #[cfg(windows)]
        {
            entries_are_empty(fs::read_dir(&self.path)?.map(|entry| {
                entry.map(|entry| {
                    allowed.is_some_and(|allowed| entry.file_name() == allowed)
                })
            }))
        }
        #[cfg(not(any(unix, windows)))]
        {
            entries_are_empty(fs::read_dir(&self.path)?.map(|entry| {
                entry.map(|entry| {
                    allowed.is_some_and(|allowed| entry.file_name() == allowed)
                })
            }))
        }
    }

    fn open_child(&self, name: &OsStr) -> io::Result<Self> {
        validate_name(name)?;
        #[cfg(unix)]
        {
            use rustix::fs::{Mode, OFlags, openat};
            let handle = openat(
                &self.handle,
                name,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
                Mode::empty(),
            )
            .map_err(to_io_error)?;
            Ok(Self {
                path: self.path.join(name),
                handle,
            })
        }
        #[cfg(windows)]
        {
            let child_path = self.path.join(name);
            let child = open_windows_directory(&child_path)?;
            let mut handles = self
                .handles
                .iter()
                .map(File::try_clone)
                .collect::<io::Result<Vec<_>>>()?;
            handles.push(child);
            Ok(Self {
                path: child_path,
                handles,
            })
        }
        #[cfg(not(any(unix, windows)))]
        {
            let child_path = self.path.join(name);
            let canonical = fs::canonicalize(&child_path)?;
            if canonical.parent() != Some(self.path.as_path()) {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "child directory escaped its opened parent",
                ));
            }
            Ok(Self { path: canonical })
        }
    }

    fn open_file(&self, name: &OsStr) -> io::Result<File> {
        validate_name(name)?;
        #[cfg(unix)]
        {
            use rustix::fs::{Mode, OFlags, openat};
            let handle = openat(
                &self.handle,
                name,
                OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
                Mode::empty(),
            )
            .map_err(to_io_error)?;
            let file = File::from(handle);
            if !file.metadata()?.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "confined path is not a regular file",
                ));
            }
            Ok(file)
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
            const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
            const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
            let file = OpenOptions::new()
                .read(true)
                .share_mode(0x0000_0001 | 0x0000_0002)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .open(self.path.join(name))?;
            let metadata = file.metadata()?;
            if !metadata.is_file()
                || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "confined path is not a regular non-reparse file",
                ));
            }
            Ok(file)
        }
        #[cfg(not(any(unix, windows)))]
        {
            let path = self.path.join(name);
            let metadata = fs::symlink_metadata(&path)?;
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "confined path is not a regular file",
                ));
            }
            File::open(path)
        }
    }

    fn create_file(&self, name: &OsStr) -> io::Result<File> {
        validate_name(name)?;
        #[cfg(unix)]
        {
            use rustix::fs::{Mode, OFlags, openat};
            let handle = openat(
                &self.handle,
                name,
                OFlags::WRONLY
                    | OFlags::CREATE
                    | OFlags::EXCL
                    | OFlags::CLOEXEC
                    | OFlags::NOFOLLOW,
                Mode::from_raw_mode(0o666),
            )
            .map_err(to_io_error)?;
            Ok(File::from(handle))
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .share_mode(0x0000_0001 | 0x0000_0002)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .open(self.path.join(name))
        }
        #[cfg(not(any(unix, windows)))]
        {
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(self.path.join(name))
        }
    }

    fn write_new_file_with(
        &self,
        name: &OsStr,
        write_contents: impl FnOnce(&mut File) -> io::Result<()>,
    ) -> io::Result<()> {
        validate_name(name)?;
        let mut file = self.create_file(name)?;
        let result = write_contents(&mut file);
        if result.is_err() {
            drop(file);
            let _ = self.remove_file(name);
        }
        result
    }

    fn clone_handle(&self) -> io::Result<Self> {
        #[cfg(unix)]
        {
            Ok(Self {
                path: self.path.clone(),
                handle: rustix::io::dup(&self.handle).map_err(to_io_error)?,
            })
        }
        #[cfg(windows)]
        {
            Ok(Self {
                path: self.path.clone(),
                handles: self
                    .handles
                    .iter()
                    .map(File::try_clone)
                    .collect::<io::Result<Vec<_>>>()?,
            })
        }
        #[cfg(not(any(unix, windows)))]
        {
            Ok(Self {
                path: self.path.clone(),
            })
        }
    }
}

fn validate_name(name: &OsStr) -> io::Result<()> {
    let mut components = Path::new(name).components();
    if matches!(components.next(), Some(Component::Normal(_)))
        && components.next().is_none()
    {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "confined file operation requires one normal path component",
        ))
    }
}

fn entries_are_empty<I, E>(entries: I) -> io::Result<bool>
where
    I: IntoIterator<Item = Result<bool, E>>,
    E: std::fmt::Display,
{
    for entry in entries {
        if !entry.map_err(|error| io::Error::other(error.to_string()))? {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(unix)]
fn to_io_error(error: rustix::io::Errno) -> io::Error {
    io::Error::from_raw_os_error(error.raw_os_error())
}

#[cfg(windows)]
fn open_windows_directory(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    const FILE_READ_ATTRIBUTES: u32 = 0x0000_0080;
    let file = OpenOptions::new()
        .read(true)
        .share_mode(0x0000_0001 | 0x0000_0002)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .access_mode(FILE_READ_ATTRIBUTES)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_dir()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} is not a regular directory", path.display()),
        ));
    }
    Ok(file)
}

#[cfg(windows)]
fn same_windows_path(left: &Path, right: &Path) -> bool {
    let left = left.to_string_lossy().replace(r"\\?\", "");
    let right = right.to_string_lossy().replace(r"\\?\", "");
    left.eq_ignore_ascii_case(&right)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used
    )]

    use super::{ConfinedDir, entries_are_empty};
    use std::ffi::OsStr;
    use std::fs;
    use std::io::{self, Write};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn empty_directory_check_propagates_entry_read_failures() {
        let entries = [Ok(true), Err("injected directory read error")];

        let error =
            entries_are_empty(entries).expect_err("read errors must not look empty");

        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert!(error.to_string().contains("injected directory read error"));
    }

    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new() -> Self {
            let nonce = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("vibra-confined-fs-{}-{nonce}", std::process::id()));
            fs::create_dir_all(&path).expect("create confined directory test root");
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn failed_new_file_write_removes_the_partial_entry() {
        let root = TempDir::new();
        let canonical = root.0.canonicalize().expect("canonical test root");
        let directory = ConfinedDir::open(canonical).expect("open confined test root");
        let error = directory
            .write_new_file_with(OsStr::new("partial.txt"), |file| {
                file.write_all(b"partial")?;
                Err(io::Error::other("injected write failure"))
            })
            .expect_err("injected write failure must be returned");

        assert_eq!(error.to_string(), "injected write failure");
        assert!(directory.is_empty().expect("inspect test root"));
    }
}
