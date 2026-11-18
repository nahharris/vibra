//! The Windows adapter: `NtCreateFile` relative to a held directory handle.
//!
//! Each component of a request is opened by itself, with the previous
//! directory as `RootDirectory` and with `FILE_OPEN_REPARSE_POINT`, so a link
//! is opened as the link and never as its target. `OBJ_DONT_REPARSE` also
//! makes the I/O manager refuse any reparse it would otherwise follow. Each
//! opened entry is then checked on its own handle: its attributes must carry
//! no reparse point and the right kind, and the last component of its
//! normalized name must equal the requested name exactly. The data access is
//! performed on that same handle. Intermediate handles live only for one
//! operation; the next one walks from the root again.

use std::fs::{File, OpenOptions};
use std::io;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::{
    AsHandle, AsRawHandle, BorrowedHandle, FromRawHandle, OwnedHandle,
};
use std::path::Path;
use std::ptr;

use windows_sys::Wdk::Foundation::OBJECT_ATTRIBUTES;
use windows_sys::Wdk::Storage::FileSystem::{
    FILE_CREATE, FILE_DISPOSITION_INFORMATION, FILE_OPEN, FILE_OPEN_REPARSE_POINT,
    FILE_SYNCHRONOUS_IO_NONALERT, FileAttributeTagInformation,
    FileDispositionInformation, FileNormalizedNameInformation, NtCreateFile,
    NtQueryInformationFile, NtSetInformationFile,
};
use windows_sys::Win32::Foundation::{
    HANDLE, NTSTATUS, OBJ_DONT_REPARSE, STATUS_ACCESS_DENIED,
    STATUS_FILE_IS_A_DIRECTORY, STATUS_INVALID_INFO_CLASS, STATUS_NO_SUCH_FILE,
    STATUS_NOT_A_DIRECTORY, STATUS_NOT_IMPLEMENTED, STATUS_NOT_SUPPORTED,
    STATUS_OBJECT_NAME_COLLISION, STATUS_OBJECT_NAME_INVALID,
    STATUS_OBJECT_NAME_NOT_FOUND, STATUS_OBJECT_PATH_NOT_FOUND,
    STATUS_REPARSE_POINT_ENCOUNTERED, STATUS_STOPPED_ON_SYMLINK, UNICODE_STRING,
};
use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_TAG_INFO, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES,
    FILE_READ_DATA, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    FILE_TRAVERSE, FILE_WRITE_DATA, SYNCHRONIZE,
};
use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;

use crate::{ScopeError, ScopedFile, name};

/// A held root directory. Every operation walks from this handle.
///
/// The handle shares delete access, so the directory can be renamed while it
/// is held; the root keeps referring to the renamed directory, and a new
/// directory at the old path is not the root.
#[derive(Debug)]
pub struct ScopedDir {
    root: File,
}

/// What one open asks of `NtCreateFile`. `FILE_OPEN_REPARSE_POINT` and
/// `FILE_SYNCHRONOUS_IO_NONALERT` are added to every open.
struct Open {
    access: u32,
    share: u32,
    disposition: u32,
    options: u32,
}

const SHARE_ALL: u32 = FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE;

/// An intermediate directory: enough to check it and to open below it.
const TRAVERSE: Open = Open {
    access: FILE_TRAVERSE | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
    share: SHARE_ALL,
    disposition: FILE_OPEN,
    options: 0,
};

/// An existing file, for reading.
const READ: Open = Open {
    access: FILE_READ_DATA | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
    share: SHARE_ALL,
    disposition: FILE_OPEN,
    options: 0,
};

/// An existing file, for a later replacement. `FILE_OPEN` never truncates;
/// `FILE_OVERWRITE` and `FILE_SUPERSEDE` are never used.
const REPLACE: Open = Open {
    access: FILE_WRITE_DATA | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
    share: FILE_SHARE_READ,
    disposition: FILE_OPEN,
    options: 0,
};

/// A new file. `FILE_CREATE` fails on any existing entry the filesystem
/// matches, whatever alias matched it and whatever its kind. Without
/// `FILE_DIRECTORY_FILE` it creates a file; `FILE_NON_DIRECTORY_FILE` is left
/// out because it would report an existing directory as a wrong kind instead
/// of a collision. `DELETE` lets a created entry that fails its check be
/// removed again.
const CREATE: Open = Open {
    access: FILE_WRITE_DATA | FILE_READ_ATTRIBUTES | DELETE | SYNCHRONIZE,
    share: FILE_SHARE_READ,
    disposition: FILE_CREATE,
    options: 0,
};

const TAG_INFO_SIZE: u32 = size_of::<FILE_ATTRIBUTE_TAG_INFO>() as u32;
const DISPOSITION_SIZE: u32 = size_of::<FILE_DISPOSITION_INFORMATION>() as u32;
const OBJECT_ATTRIBUTES_SIZE: u32 = size_of::<OBJECT_ATTRIBUTES>() as u32;

/// The name buffer, in 32-bit words: the byte length, then room for a name of
/// 32,768 UTF-16 units, the longest path Windows accepts.
const NAME_BUFFER_WORDS: usize = 1 + 16_384;
const NAME_BUFFER_BYTES: u32 = (NAME_BUFFER_WORDS * 4) as u32;

/// The kind an opened entry must have.
#[derive(Clone, Copy)]
enum Kind {
    Directory,
    File,
}

impl ScopedDir {
    /// Acquires `path` as a root. The path itself is opened through the
    /// ordinary Win32 path rules, once; the directory it names must not be a
    /// reparse point.
    pub fn open_root(path: &Path) -> Result<Self, ScopeError> {
        let root = OpenOptions::new()
            .access_mode(
                FILE_LIST_DIRECTORY
                    | FILE_TRAVERSE
                    | FILE_READ_ATTRIBUTES
                    | SYNCHRONIZE,
            )
            .share_mode(SHARE_ALL)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .map_err(|error| match error.kind() {
                io::ErrorKind::NotFound => ScopeError::NotFound,
                _ => ScopeError::Io(error),
            })?;
        let info = attributes(root.as_handle())?;
        if info.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(ScopeError::DeniedReparse);
        }
        check_kind(info, Kind::Directory)?;
        Ok(Self { root })
    }

    /// Opens an existing file below the root for reading.
    pub fn open_read(&self, names: &[&str]) -> Result<ScopedFile, ScopeError> {
        self.open_leaf(names, &READ)
    }

    /// Creates a new file below the root. An existing entry under any alias,
    /// a link included, is [`ScopeError::AlreadyExists`].
    pub fn create_new(&self, names: &[&str]) -> Result<ScopedFile, ScopeError> {
        self.open_leaf(names, &CREATE)
    }

    /// Opens an existing file below the root for a later replacement. The
    /// open does not change the file; [`ScopedFile::replace_contents`] does.
    pub fn open_for_replace(&self, names: &[&str]) -> Result<ScopedFile, ScopeError> {
        self.open_leaf(names, &REPLACE)
    }

    fn open_leaf(&self, names: &[&str], open: &Open) -> Result<ScopedFile, ScopeError> {
        name::check_all(names)?;
        let Some((leaf, directories)) = names.split_last() else {
            return Err(ScopeError::DeniedName);
        };
        let mut parent: Option<OwnedHandle> = None;
        for component in directories {
            let base = parent
                .as_ref()
                .map_or(self.root.as_handle(), AsHandle::as_handle);
            let handle = nt_open(base, component, &TRAVERSE)?;
            check_entry(handle.as_handle(), component, Kind::Directory)?;
            parent = Some(handle);
        }
        let base = parent
            .as_ref()
            .map_or(self.root.as_handle(), AsHandle::as_handle);
        let handle = nt_open(base, leaf, open)?;
        if let Err(error) = check_entry(handle.as_handle(), leaf, Kind::File) {
            if open.disposition == FILE_CREATE {
                delete_on_close(handle.as_handle())?;
            }
            return Err(error);
        }
        Ok(ScopedFile::new(File::from(handle)))
    }
}

/// Opens one name relative to `parent`, without following a reparse point.
fn nt_open(
    parent: BorrowedHandle<'_>,
    component: &str,
    open: &Open,
) -> Result<OwnedHandle, ScopeError> {
    let wide = component.encode_utf16().collect::<Vec<u16>>();
    let length = wide
        .len()
        .checked_mul(2)
        .and_then(|bytes| u16::try_from(bytes).ok())
        .ok_or(ScopeError::DeniedName)?;
    let object_name = UNICODE_STRING {
        Length: length,
        MaximumLength: length,
        Buffer: wide.as_ptr().cast_mut(),
    };
    let attributes = OBJECT_ATTRIBUTES {
        Length: OBJECT_ATTRIBUTES_SIZE,
        RootDirectory: parent.as_raw_handle(),
        ObjectName: &object_name,
        Attributes: OBJ_DONT_REPARSE,
        SecurityDescriptor: ptr::null(),
        SecurityQualityOfService: ptr::null(),
    };
    let mut handle: HANDLE = ptr::null_mut();
    let mut status_block = IO_STATUS_BLOCK::default();
    // SAFETY: `handle` and `status_block` are writable locals. `attributes`
    // is a fully initialized OBJECT_ATTRIBUTES of the stated length whose
    // `ObjectName` points at `object_name`, whose buffer `wide` holds exactly
    // `length` bytes and outlives the call. `parent` is a borrowed handle, so
    // it stays open for the call. A null allocation size and a null extended
    // attribute buffer of length zero are documented as permitted. The
    // buffer is never written by the callee.
    let status = unsafe {
        NtCreateFile(
            &mut handle,
            open.access,
            &attributes,
            &mut status_block,
            ptr::null(),
            FILE_ATTRIBUTE_NORMAL,
            open.share,
            open.disposition,
            open.options | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
            ptr::null(),
            0,
        )
    };
    if status < 0 {
        return Err(status_error(status));
    }
    // SAFETY: NtCreateFile succeeded, so `handle` is a new open handle that
    // nothing else owns; the OwnedHandle closes it exactly once.
    Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
}

/// Checks an opened entry: no reparse point, the right kind, and a real name
/// equal to `requested`.
fn check_entry(
    handle: BorrowedHandle<'_>,
    requested: &str,
    kind: Kind,
) -> Result<(), ScopeError> {
    let info = attributes(handle)?;
    if info.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(ScopeError::DeniedReparse);
    }
    check_kind(info, kind)?;
    let real = final_name(handle)?;
    if real.iter().copied().eq(requested.encode_utf16()) {
        Ok(())
    } else {
        Err(ScopeError::DeniedName)
    }
}

fn check_kind(info: FILE_ATTRIBUTE_TAG_INFO, kind: Kind) -> Result<(), ScopeError> {
    let is_directory = info.FileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0;
    match (kind, is_directory) {
        (Kind::Directory, false) => {
            Err(ScopeError::Io(io::ErrorKind::NotADirectory.into()))
        }
        (Kind::File, true) => Err(ScopeError::Io(io::ErrorKind::IsADirectory.into())),
        _ => Ok(()),
    }
}

/// The attributes and reparse tag of the opened object itself.
fn attributes(
    handle: BorrowedHandle<'_>,
) -> Result<FILE_ATTRIBUTE_TAG_INFO, ScopeError> {
    let mut info = FILE_ATTRIBUTE_TAG_INFO::default();
    let mut status_block = IO_STATUS_BLOCK::default();
    // SAFETY: `handle` is borrowed, so it is open for the call. `info` is a
    // writable FILE_ATTRIBUTE_TAG_INFO and `TAG_INFO_SIZE` is its size; it
    // has the layout of FILE_ATTRIBUTE_TAG_INFORMATION, which this class
    // writes. `status_block` is a writable local.
    let status = unsafe {
        NtQueryInformationFile(
            handle.as_raw_handle(),
            &mut status_block,
            (&raw mut info).cast(),
            TAG_INFO_SIZE,
            FileAttributeTagInformation,
        )
    };
    if status < 0 {
        return Err(status_error(status));
    }
    Ok(info)
}

/// The last component of the opened object's normalized name: its long name,
/// in the case stored on disk.
fn final_name(handle: BorrowedHandle<'_>) -> Result<Vec<u16>, ScopeError> {
    let mut buffer = vec![0_u32; NAME_BUFFER_WORDS];
    let mut status_block = IO_STATUS_BLOCK::default();
    // SAFETY: `handle` is borrowed, so it is open for the call. `buffer` is a
    // writable allocation of exactly `NAME_BUFFER_BYTES` bytes, aligned to
    // four bytes, the alignment of FILE_NAME_INFORMATION, which this class
    // writes. `status_block` is a writable local.
    let status = unsafe {
        NtQueryInformationFile(
            handle.as_raw_handle(),
            &mut status_block,
            buffer.as_mut_ptr().cast(),
            NAME_BUFFER_BYTES,
            FileNormalizedNameInformation,
        )
    };
    match status {
        STATUS_INVALID_INFO_CLASS | STATUS_NOT_IMPLEMENTED | STATUS_NOT_SUPPORTED => {
            return Err(ScopeError::Unsupported);
        }
        failure if failure < 0 => return Err(status_error(failure)),
        _ => {}
    }
    // FILE_NAME_INFORMATION: a 32-bit byte length, then the UTF-16 name.
    // Windows is little-endian, so the low half of each word comes first.
    let Some((&byte_length, words)) = buffer.split_first() else {
        return Err(ScopeError::Unsupported);
    };
    let units =
        usize::try_from(byte_length / 2).map_err(|_| ScopeError::Unsupported)?;
    let full = words
        .iter()
        .flat_map(|&word| [word as u16, (word >> 16) as u16])
        .take(units)
        .collect::<Vec<u16>>();
    let start = full
        .iter()
        .rposition(|&unit| unit == u16::from(b'\\'))
        .map_or(0, |separator| separator + 1);
    Ok(full.get(start..).unwrap_or_default().to_vec())
}

/// Marks a just-created file for deletion when its handle closes.
fn delete_on_close(handle: BorrowedHandle<'_>) -> Result<(), ScopeError> {
    let info = FILE_DISPOSITION_INFORMATION { DeleteFile: true };
    let mut status_block = IO_STATUS_BLOCK::default();
    // SAFETY: `handle` is borrowed, so it is open for the call, and it was
    // opened with DELETE access. `info` is a readable
    // FILE_DISPOSITION_INFORMATION and `DISPOSITION_SIZE` is its size.
    // `status_block` is a writable local.
    let status = unsafe {
        NtSetInformationFile(
            handle.as_raw_handle(),
            &mut status_block,
            (&raw const info).cast(),
            DISPOSITION_SIZE,
            FileDispositionInformation,
        )
    };
    if status < 0 {
        return Err(status_error(status));
    }
    Ok(())
}

/// Maps a failed NTSTATUS to the closed error enum.
fn status_error(status: NTSTATUS) -> ScopeError {
    match status {
        STATUS_OBJECT_NAME_NOT_FOUND
        | STATUS_OBJECT_PATH_NOT_FOUND
        | STATUS_NO_SUCH_FILE => ScopeError::NotFound,
        STATUS_OBJECT_NAME_COLLISION => ScopeError::AlreadyExists,
        STATUS_OBJECT_NAME_INVALID => ScopeError::DeniedName,
        STATUS_REPARSE_POINT_ENCOUNTERED | STATUS_STOPPED_ON_SYMLINK => {
            ScopeError::DeniedReparse
        }
        STATUS_NOT_A_DIRECTORY => ScopeError::Io(io::ErrorKind::NotADirectory.into()),
        STATUS_FILE_IS_A_DIRECTORY => {
            ScopeError::Io(io::ErrorKind::IsADirectory.into())
        }
        STATUS_ACCESS_DENIED => ScopeError::Io(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "NTSTATUS 0xc0000022",
        )),
        other => ScopeError::Io(io::Error::other(format!(
            "NTSTATUS {:#010x}",
            other.cast_unsigned()
        ))),
    }
}
