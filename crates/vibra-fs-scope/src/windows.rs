//! The Windows adapter: `NtCreateFile` relative to a held directory handle.
//!
//! Each component of a request is opened by itself, with the previous
//! directory as `RootDirectory` and with `FILE_OPEN_REPARSE_POINT`, so a link
//! is opened as the link and never as its target. `OBJ_DONT_REPARSE` also
//! makes the I/O manager refuse any reparse it would otherwise follow. Each
//! opened entry is then checked on its own handle: its attributes must carry
//! no reparse point and it must have the kind the operation needs. The last
//! component of its normalized name is reported as its real name; the data
//! access is performed on that same handle, by the caller's later step.
//! Intermediate handles live only for one operation; the next one walks from
//! the root again.

use std::fs::{File, OpenOptions};
use std::io;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::{
    AsHandle, AsRawHandle, BorrowedHandle, FromRawHandle, IntoRawHandle, OwnedHandle,
};
use std::path::Path;
use std::ptr;

use windows_sys::Wdk::Foundation::OBJECT_ATTRIBUTES;
use windows_sys::Wdk::Storage::FileSystem::{
    FILE_CREATE, FILE_OPEN, FILE_OPEN_REPARSE_POINT, FILE_STANDARD_INFORMATION,
    FILE_SYNCHRONOUS_IO_NONALERT, FileAttributeTagInformation, FileFsDeviceInformation,
    FileNamesInformation, FileNormalizedNameInformation, FileStandardInformation,
    NtCreateFile, NtQueryDirectoryFile, NtQueryInformationFile,
    NtQueryVolumeInformationFile,
};
use windows_sys::Wdk::System::SystemServices::{
    FILE_FS_DEVICE_INFORMATION, FILE_REMOTE_DEVICE,
};
use windows_sys::Win32::Foundation::{
    CloseHandle, HANDLE, NTSTATUS, OBJ_DONT_REPARSE, STATUS_ACCESS_DENIED,
    STATUS_FILE_IS_A_DIRECTORY, STATUS_INVALID_INFO_CLASS, STATUS_NO_MORE_FILES,
    STATUS_NO_SUCH_FILE, STATUS_NOT_A_DIRECTORY, STATUS_NOT_IMPLEMENTED,
    STATUS_NOT_SUPPORTED, STATUS_OBJECT_NAME_COLLISION, STATUS_OBJECT_NAME_INVALID,
    STATUS_OBJECT_NAME_NOT_FOUND, STATUS_OBJECT_PATH_NOT_FOUND,
    STATUS_REPARSE_POINT_ENCOUNTERED, STATUS_STOPPED_ON_SYMLINK, UNICODE_STRING,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DEVICE, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_TAG_INFO, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES,
    FILE_READ_DATA, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    FILE_TRAVERSE, FILE_TYPE_DISK, FILE_WRITE_DATA, GetFileType, SYNCHRONIZE,
};
use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;

use crate::{EntryKind, Metadata, ScopeError, ScopedFile, WalkError, name};

/// A held root directory. Every operation walks from this handle.
///
/// The handle shares delete access, so the directory can be renamed while it
/// is held; the root keeps referring to the renamed directory, and a new
/// directory at the old path is not the root.
#[derive(Debug)]
pub struct ScopedDir {
    root: File,
}

/// A directory opened for one creation, whose checks passed.
///
/// It holds the directory's handle, so the entry [`ScopedParent::create_new`]
/// makes is created in this directory object, whatever its path names by
/// then. No entry exists yet when it is returned.
#[derive(Debug)]
pub struct ScopedParent<'r> {
    root: BorrowedHandle<'r>,
    held: Option<OwnedHandle>,
    real_names: Vec<String>,
}

/// A directory opened for listing, whose checks passed. No entry has been
/// read when it is returned; [`ScopedListing::entries`] reads them.
#[derive(Debug)]
pub struct ScopedListing {
    handle: OwnedHandle,
    real_names: Vec<String>,
}

/// What one open asks of `NtCreateFile`. `FILE_OPEN_REPARSE_POINT` and
/// `FILE_SYNCHRONOUS_IO_NONALERT` are added to every open.
struct Open {
    access: u32,
    share: u32,
    disposition: u32,
}

const SHARE_ALL: u32 = FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE;

/// A directory to walk through, or to hold for a creation: enough to check it
/// and to open below it.
const TRAVERSE: Open = Open {
    access: FILE_TRAVERSE | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
    share: SHARE_ALL,
    disposition: FILE_OPEN,
};

/// A directory whose entries will be read.
const LIST: Open = Open {
    access: FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
    share: SHARE_ALL,
    disposition: FILE_OPEN,
};

/// An entry whose metadata is reported.
const STAT: Open = Open {
    access: FILE_READ_ATTRIBUTES | SYNCHRONIZE,
    share: SHARE_ALL,
    disposition: FILE_OPEN,
};

/// An existing file, for reading.
const READ: Open = Open {
    access: FILE_READ_DATA | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
    share: SHARE_ALL,
    disposition: FILE_OPEN,
};

/// An existing file, for a later replacement. `FILE_OPEN` never truncates;
/// `FILE_OVERWRITE` and `FILE_SUPERSEDE` are never used.
const REPLACE: Open = Open {
    access: FILE_WRITE_DATA | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
    share: FILE_SHARE_READ,
    disposition: FILE_OPEN,
};

/// A new file. `FILE_CREATE` fails on any existing entry the filesystem
/// matches, whatever alias matched it and whatever its kind. Without
/// `FILE_DIRECTORY_FILE` it creates a file; `FILE_NON_DIRECTORY_FILE` is left
/// out because it would report an existing directory as a wrong kind instead
/// of a collision. The access has no `DELETE`: a created entry is never
/// removed.
const CREATE: Open = Open {
    access: FILE_WRITE_DATA | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
    share: FILE_SHARE_READ,
    disposition: FILE_CREATE,
};

const TAG_INFO_SIZE: u32 = size_of::<FILE_ATTRIBUTE_TAG_INFO>() as u32;
const DEVICE_INFO_SIZE: u32 = size_of::<FILE_FS_DEVICE_INFORMATION>() as u32;
const STANDARD_INFO_SIZE: u32 = size_of::<FILE_STANDARD_INFORMATION>() as u32;
const OBJECT_ATTRIBUTES_SIZE: u32 = size_of::<OBJECT_ATTRIBUTES>() as u32;

/// The name buffer, in 32-bit words: the byte length, then room for a name of
/// 32,768 UTF-16 units, the longest path Windows accepts.
const NAME_BUFFER_WORDS: usize = 1 + 16_384;
const NAME_BUFFER_BYTES: u32 = (NAME_BUFFER_WORDS * 4) as u32;

/// The listing buffer, in 64-bit words: 64 KiB of `FILE_NAMES_INFORMATION`
/// records per call.
const LIST_BUFFER_WORDS: usize = 8_192;
const LIST_BUFFER_BYTES: u32 = (LIST_BUFFER_WORDS * 8) as u32;

/// The kind an operation needs of the entry it opens.
#[derive(Clone, Copy)]
enum Need {
    Directory,
    File,
    Any,
}

/// One opened and checked entry.
struct Opened {
    handle: OwnedHandle,
    kind: EntryKind,
}

impl ScopedDir {
    /// Acquires `path` as a root. The path itself is opened through the
    /// ordinary Win32 path rules, once; the directory it names must not be a
    /// reparse point. A remote volume, and a volume that does not report the
    /// root's normalized name or its link count, is
    /// [`ScopeError::Unsupported`]: an SMB server resolves its own links,
    /// which the client does not see as reparse points, and without the
    /// names no entry below the root could be checked.
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
        if kind_of(info) != EntryKind::Directory {
            return Err(ScopeError::NotADirectory);
        }
        root_guarantees(
            device_characteristics(root.as_handle()),
            real_name(root.as_handle()),
        )?;
        standard(root.as_handle())?;
        Ok(Self { root })
    }

    /// Closes the root handle, reporting what the system answers.
    ///
    /// # Errors
    ///
    /// The system reported the close as failed; the handle is closed all the
    /// same.
    pub fn close(self) -> io::Result<()> {
        close(self.root)
    }

    /// Opens an existing file below the root for reading. Nothing is read.
    /// The empty path names the root, so it is [`ScopeError::NotAFile`].
    pub fn open_read(&self, names: &[&str]) -> Result<ScopedFile, WalkError> {
        self.open_file(names, &READ)
    }

    /// Opens an existing file below the root for a later replacement. The
    /// open does not change the file; [`ScopedFile::replace_contents`] does.
    /// The empty path names the root, so it is [`ScopeError::NotAFile`].
    pub fn open_for_replace(&self, names: &[&str]) -> Result<ScopedFile, WalkError> {
        self.open_file(names, &REPLACE)
    }

    /// Opens the directory `names` for listing; no names is the root. No
    /// entry is read until [`ScopedListing::entries`].
    pub fn list(&self, names: &[&str]) -> Result<ScopedListing, WalkError> {
        name::check_each(names)?;
        let (opened, real_names) = self.walk_to(names, &LIST, Need::Directory)?;
        Ok(ScopedListing {
            handle: opened.handle,
            real_names,
        })
    }

    /// Reports the kind and size of the entry `names`, without following it
    /// if it is a link; no names is the root.
    pub fn stat(&self, names: &[&str]) -> Result<Metadata, WalkError> {
        name::check_each(names)?;
        let (opened, real_names) = self.walk_to(names, &STAT, Need::Any)?;
        let size = match opened.kind {
            EntryKind::File => {
                standard(opened.handle.as_handle())
                    .map_err(|error| below(error, &real_names))?
                    .size
            }
            EntryKind::Directory | EntryKind::Other => 0,
        };
        Ok(Metadata {
            real_names,
            kind: opened.kind,
            size,
        })
    }

    /// Opens the directory `names` and holds it for one creation; no names
    /// is the root. Every component is refused if it is a reparse point.
    /// Nothing is created.
    pub fn open_parent(&self, names: &[&str]) -> Result<ScopedParent<'_>, WalkError> {
        name::check_each(names)?;
        let (held, real_names) = self.walk(names)?;
        Ok(ScopedParent {
            root: self.root.as_handle(),
            held,
            real_names,
        })
    }

    /// Opens the file `names` with `open`. The empty path names the root, a
    /// directory, so it is [`ScopeError::NotAFile`] as any directory is; the
    /// root is reopened only to read its attributes, with no data access.
    fn open_file(&self, names: &[&str], open: &Open) -> Result<ScopedFile, WalkError> {
        name::check_each(names)?;
        let open = if names.is_empty() { &STAT } else { open };
        let (opened, real_names) = self.walk_to(names, open, Need::File)?;
        let info = standard(opened.handle.as_handle())
            .map_err(|error| below(error, &real_names))?;
        Ok(ScopedFile::new(
            File::from(opened.handle),
            real_names,
            info.size,
            info.links,
        ))
    }

    /// Walks every name as a directory. Returns the last directory's handle,
    /// `None` for the root, and the real names; every other handle is closed.
    /// A failure carries the real names of the directories opened before it.
    fn walk(
        &self,
        names: &[&str],
    ) -> Result<(Option<OwnedHandle>, Vec<String>), WalkError> {
        let mut held: Option<OwnedHandle> = None;
        let mut real_names = Vec::with_capacity(names.len());
        for component in names {
            let base = held
                .as_ref()
                .map_or(self.root.as_handle(), AsHandle::as_handle);
            let opened = open_checked(base, component, &TRAVERSE, Need::Directory)
                .map_err(|error| below(error, &real_names))?;
            let name = real_name(opened.handle.as_handle())
                .map_err(|error| below(error, &real_names))?;
            real_names.push(name);
            // The previous directory's handle is closed here.
            held = Some(opened.handle);
        }
        Ok((held, real_names))
    }

    /// Walks to the last name and opens it with `open`; no names reopens the
    /// root itself. A failure carries the real names of the directories
    /// opened before it, the leaf's excluded.
    fn walk_to(
        &self,
        names: &[&str],
        open: &Open,
        need: Need,
    ) -> Result<(Opened, Vec<String>), WalkError> {
        let Some((leaf, directories)) = names.split_last() else {
            let opened = open_checked(self.root.as_handle(), "", open, need)?;
            return Ok((opened, Vec::new()));
        };
        let (parent, mut real_names) = self.walk(directories)?;
        let base = parent
            .as_ref()
            .map_or(self.root.as_handle(), AsHandle::as_handle);
        let opened = open_checked(base, leaf, open, need)
            .map_err(|error| below(error, &real_names))?;
        let name = real_name(opened.handle.as_handle())
            .map_err(|error| below(error, &real_names))?;
        real_names.push(name);
        Ok((opened, real_names))
    }
}

/// The failure `error`, decided below the directories whose real names begin
/// `real_names`: a failure after the leaf opened carries the leaf's name
/// too, since it is decided on the opened object.
fn below(error: ScopeError, real_names: &[String]) -> WalkError {
    WalkError {
        error,
        opened: real_names.to_vec(),
    }
}

impl ScopedParent<'_> {
    /// The name the filesystem reports for each component of the directory,
    /// empty for the root.
    pub fn real_names(&self) -> &[String] {
        &self.real_names
    }

    /// Creates the file `leaf` in the held directory, exclusively, in one
    /// system call relative to the held handle. An existing entry of that
    /// name under any alias, a link included, is
    /// [`ScopeError::AlreadyExists`], and nothing changes. The new file is
    /// empty and its real names are the directory's followed by `leaf`. An
    /// empty `leaf` names nothing to create: [`ScopeError::DeniedName`].
    pub fn create_new(self, leaf: &str) -> Result<ScopedFile, ScopeError> {
        name::check_name(leaf)?;
        let base = self.held.as_ref().map_or(self.root, AsHandle::as_handle);
        let handle = nt_open(base, leaf, &CREATE)?;
        let mut real_names = self.real_names;
        real_names.push(leaf.to_owned());
        Ok(ScopedFile::new(File::from(handle), real_names, 0, 1))
    }
}

impl ScopedListing {
    /// The name the filesystem reports for each component of the directory,
    /// empty for the root.
    pub fn real_names(&self) -> &[String] {
        &self.real_names
    }

    /// The names of the directory's entries, without `.` and `..`, sorted by
    /// Unicode scalar. A name that is not Unicode is
    /// [`ScopeError::NotUnicode`].
    pub fn entries(self) -> Result<Vec<String>, ScopeError> {
        let mut names = Vec::new();
        let mut buffer = vec![0_u64; LIST_BUFFER_WORDS];
        let mut restart = true;
        loop {
            let mut status_block = IO_STATUS_BLOCK::default();
            // SAFETY: `self.handle` is an open directory handle owned by this
            // value, opened for synchronous I/O with FILE_LIST_DIRECTORY.
            // `buffer` is a writable allocation of exactly
            // `LIST_BUFFER_BYTES` bytes, aligned to eight bytes, more than
            // FILE_NAMES_INFORMATION needs. No event, APC routine, context,
            // or file-name filter is passed, which the call permits.
            // `status_block` is a writable local.
            let status = unsafe {
                NtQueryDirectoryFile(
                    self.handle.as_raw_handle(),
                    ptr::null_mut(),
                    None,
                    ptr::null(),
                    &mut status_block,
                    buffer.as_mut_ptr().cast(),
                    LIST_BUFFER_BYTES,
                    FileNamesInformation,
                    false,
                    ptr::null(),
                    restart,
                )
            };
            restart = false;
            if status == STATUS_NO_MORE_FILES {
                break;
            }
            if status < 0 {
                return Err(status_error(status));
            }
            let written = status_block.Information.min(LIST_BUFFER_BYTES as usize);
            let bytes = buffer
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .take(written)
                .collect::<Vec<u8>>();
            read_names(&bytes, &mut names)?;
        }
        names.sort_unstable();
        Ok(names)
    }
}

/// Appends the names of a buffer of `FILE_NAMES_INFORMATION` records, without
/// `.` and `..`. Each record is a 32-bit offset to the next record (zero for
/// the last), a 32-bit file index, a 32-bit name length in bytes, and the
/// UTF-16 name.
fn read_names(bytes: &[u8], names: &mut Vec<String>) -> Result<(), ScopeError> {
    let malformed = || ScopeError::Io(io::Error::other("malformed directory record"));
    let mut offset = 0_usize;
    while offset < bytes.len() {
        let record = bytes.get(offset..).ok_or_else(malformed)?;
        let next = word(record, 0).ok_or_else(malformed)?;
        let length = usize::try_from(word(record, 8).ok_or_else(malformed)?)
            .map_err(|_| malformed())?;
        let name = record
            .get(12..12_usize.checked_add(length).ok_or_else(malformed)?)
            .ok_or_else(malformed)?;
        let units = name
            .chunks_exact(2)
            .map(|pair| <[u8; 2]>::try_from(pair).map(u16::from_le_bytes))
            .collect::<Result<Vec<u16>, _>>()
            .map_err(|_| malformed())?;
        let text = String::from_utf16(&units).map_err(|_| ScopeError::NotUnicode)?;
        if text != "." && text != ".." {
            names.push(text);
        }
        if next == 0 {
            break;
        }
        offset = offset
            .checked_add(usize::try_from(next).map_err(|_| malformed())?)
            .ok_or_else(malformed)?;
    }
    Ok(())
}

/// The little-endian 32-bit word at `at` in `bytes`.
fn word(bytes: &[u8], at: usize) -> Option<u32> {
    let slice = bytes.get(at..at.checked_add(4)?)?;
    <[u8; 4]>::try_from(slice).ok().map(u32::from_le_bytes)
}

/// Opens one name relative to `parent` and checks it: no reparse point, and
/// the kind `need` asks for. An empty name reopens `parent` itself.
fn open_checked(
    parent: BorrowedHandle<'_>,
    component: &str,
    open: &Open,
    need: Need,
) -> Result<Opened, ScopeError> {
    let handle = nt_open(parent, component, open)?;
    let info = attributes(handle.as_handle())?;
    if info.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(ScopeError::DeniedReparse);
    }
    let kind = kind_of(info);
    match (need, kind) {
        (Need::Directory, EntryKind::File | EntryKind::Other) => {
            Err(ScopeError::NotADirectory)
        }
        (Need::File, EntryKind::Directory | EntryKind::Other) => {
            Err(ScopeError::NotAFile)
        }
        _ => Ok(Opened { handle, kind }),
    }
}

fn kind_of(info: FILE_ATTRIBUTE_TAG_INFO) -> EntryKind {
    if info.FileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
        EntryKind::Directory
    } else if info.FileAttributes & FILE_ATTRIBUTE_DEVICE != 0 {
        EntryKind::Other
    } else {
        EntryKind::File
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
    // `length` bytes and outlives the call; a zero length names `parent`
    // itself. `parent` is a borrowed handle, so it stays open for the call.
    // A null allocation size and a null extended attribute buffer of length
    // zero are documented as permitted. The buffer is never written by the
    // callee.
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
            FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
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

/// The size and link count of an opened object.
struct Standard {
    size: u64,
    links: u32,
}

/// The byte length and link count of the opened object itself.
fn standard(handle: BorrowedHandle<'_>) -> Result<Standard, ScopeError> {
    let mut info = FILE_STANDARD_INFORMATION::default();
    let mut status_block = IO_STATUS_BLOCK::default();
    // SAFETY: `handle` is borrowed, so it is open for the call. `info` is a
    // writable FILE_STANDARD_INFORMATION and `STANDARD_INFO_SIZE` is its
    // size, the structure this class writes. `status_block` is a writable
    // local.
    let status = unsafe {
        NtQueryInformationFile(
            handle.as_raw_handle(),
            &mut status_block,
            (&raw mut info).cast(),
            STANDARD_INFO_SIZE,
            FileStandardInformation,
        )
    };
    if status < 0 {
        return Err(status_error(status));
    }
    let size = u64::try_from(info.EndOfFile)
        .map_err(|_| ScopeError::Io(io::Error::other("negative end of file")))?;
    Ok(Standard {
        size,
        links: info.NumberOfLinks,
    })
}

/// The last component of the opened object's normalized name: its long name,
/// in the case stored on disk. A volume that does not answer the query is
/// [`ScopeError::Unsupported`].
fn real_name(handle: BorrowedHandle<'_>) -> Result<String, ScopeError> {
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
    String::from_utf16(full.get(start..).unwrap_or_default())
        .map_err(|_| ScopeError::NotUnicode)
}

/// Whether the adapter can give its guarantees on a root whose volume has
/// the device characteristics `device` and whose normalized name query
/// answered `name`: a local volume that reports the root's name. A remote
/// volume, a volume whose characteristics cannot be read, and any failure of
/// the name query are [`ScopeError::Unsupported`].
fn root_guarantees(
    device: Result<u32, ScopeError>,
    name: Result<String, ScopeError>,
) -> Result<(), ScopeError> {
    match device {
        Ok(characteristics) if characteristics & FILE_REMOTE_DEVICE == 0 => {}
        _ => return Err(ScopeError::Unsupported),
    }
    name.map(|_| ()).map_err(|_| ScopeError::Unsupported)
}

/// The device characteristics of the volume the opened object is on.
fn device_characteristics(handle: BorrowedHandle<'_>) -> Result<u32, ScopeError> {
    let mut info = FILE_FS_DEVICE_INFORMATION::default();
    let mut status_block = IO_STATUS_BLOCK::default();
    // SAFETY: `handle` is borrowed, so it is open for the call. `info` is a
    // writable FILE_FS_DEVICE_INFORMATION and `DEVICE_INFO_SIZE` is its size,
    // the structure this class writes. `status_block` is a writable local.
    let status = unsafe {
        NtQueryVolumeInformationFile(
            handle.as_raw_handle(),
            &mut status_block,
            (&raw mut info).cast(),
            DEVICE_INFO_SIZE,
            FileFsDeviceInformation,
        )
    };
    if status < 0 {
        return Err(status_error(status));
    }
    Ok(info.Characteristics)
}

/// Closes `file`, reporting what `CloseHandle` answers, which dropping a
/// file does not.
///
/// # Errors
///
/// The system reported the close as failed; the handle is not valid after
/// the call either way.
pub fn close(file: File) -> io::Result<()> {
    let handle = file.into_raw_handle();
    // SAFETY: `handle` was owned by `file`, which `into_raw_handle` consumed,
    // so it is open and nothing else closes it.
    if unsafe { CloseHandle(handle) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Whether the open `file` is a regular file of a disk volume: its handle's
/// type is `FILE_TYPE_DISK` and its metadata says it is a file. The metadata
/// alone does not tell: the standard library reports an anonymous pipe as a
/// file whose length is the bytes the pipe holds at that moment.
pub fn is_regular_file(file: &File) -> bool {
    // SAFETY: the handle is borrowed from `file`, so it is open for the
    // call, and GetFileType takes no pointer.
    let kind = unsafe { GetFileType(file.as_raw_handle()) };
    kind == FILE_TYPE_DISK && file.metadata().is_ok_and(|metadata| metadata.is_file())
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
        STATUS_NOT_A_DIRECTORY => ScopeError::NotADirectory,
        STATUS_FILE_IS_A_DIRECTORY => ScopeError::NotAFile,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn named() -> Result<String, ScopeError> {
        Ok("inside".to_owned())
    }

    /// F2 of the drivers review: the classification of a root's volume,
    /// apart from a machine that can reach a share.
    #[test]
    fn a_root_is_held_only_on_a_local_volume_that_reports_its_name() {
        assert!(root_guarantees(Ok(0), named()).is_ok());
        assert!(matches!(
            root_guarantees(Ok(FILE_REMOTE_DEVICE), named()),
            Err(ScopeError::Unsupported)
        ));
        assert!(matches!(
            root_guarantees(Err(ScopeError::Io(io::Error::other("x"))), named()),
            Err(ScopeError::Unsupported)
        ));
        // Over SMB the name query fails with STATUS_INVALID_PARAMETER for a
        // buffer of 65,540 bytes (probe 2): any failure is unsupported.
        for failure in [
            ScopeError::Unsupported,
            ScopeError::Io(io::Error::other("NTSTATUS 0xc000000d")),
            ScopeError::NotUnicode,
        ] {
            assert!(matches!(
                root_guarantees(Ok(0), Err(failure)),
                Err(ScopeError::Unsupported)
            ));
        }
    }
}
