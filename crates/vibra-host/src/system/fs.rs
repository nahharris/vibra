//! The directory driver: `@host.fs.directory` over the file-scope adapter of
//! `vibra-fs-scope`.
//!
//! `docs/spec/03-effects.md`, "Filesystem policies", and
//! `docs/spec/06-runtime.md`, "Closed operation semantics", fix what it does.
//! The adapter opens every request from the held root, one name at a time,
//! following no link, and reports the names the filesystem gives each opened
//! component. This driver decides nothing itself: it shows the gate what the
//! adapter opened, and acts on that same object only after the gate admitted
//! it.
//!
//! **The order of a call.** Open through the adapter with the request's
//! names, the prefix of every `sub` included; build [`Found`] from the opened
//! object (its real names, the action found, its size, its link count); call
//! [`Gate::admit`]; and only then read, replace, create, list, or return
//! metadata. A refusal of the gate answers `Answer::Refused` and nothing
//! more happens.
//!
//! - A write of an existing file opens it without truncation, shows the gate
//!   `Replace` with its link count, and then replaces its contents on that
//!   handle.
//! - A write that finds no entry is a create: the parent is opened and held,
//!   the gate is shown `Create` with the parent's real names followed by the
//!   proposed leaf, and then the leaf is created exclusively relative to the
//!   held parent. A create that finds an entry by then is a completed
//!   `denied`; it never falls back to a replace.
//! - A walk that fails below directories it opened, a missing entry
//!   included, shows the gate their real names followed by the rest of the
//!   request's names ([`Gate::admit_error`]) before it answers the error. A
//!   refusal answers `Answer::Refused`: whether an entry exists below a
//!   directory the policy does not permit is not told.
//!
//! The driver keeps no handle but the root between calls, and has no remove,
//! rename, append, or directory creation. On every platform other than
//! Windows the adapter acquires no root, and `SUPPORTED` is `false`.

use std::io;
use std::path::Path;

use vibra_fs_scope::{EntryKind, ScopeError, ScopedDir, ScopedFile, WalkError};
use vibra_ir::authority::{HostFsKind, HostOk};
use vibra_ir::external::host::HostOp;

use super::{Answer, length, moved};
use crate::driver::{
    Acquire, Action, Found, FoundAction, Gate, Measured, Operand, Progress,
};

/// Whether this platform has a directory driver.
pub(super) const SUPPORTED: bool = cfg!(windows);

/// The progress of a write that failed after it began, whose count the
/// system does not report.
const WRITE_UNCERTAIN: Progress = Progress {
    input: Measured::Known(0),
    output: Measured::Uncertain,
};

/// The progress of a read that failed after it began.
const READ_UNCERTAIN: Progress = Progress {
    input: Measured::Uncertain,
    output: Measured::Known(0),
};

/// The state of the directory driver besides its roots, which the system
/// driver holds as resources.
#[derive(Debug, Default)]
pub(super) struct Directories {
    /// Set by a test hook: every adapter call that reports the names the
    /// filesystem gives answers as a volume that cannot, before it opens
    /// anything.
    pub(super) names_unsupported: bool,
    /// The number of times a file's contents were read.
    pub(super) reads: u64,
}

impl Directories {
    /// Calls the adapter through `open`, unless a test hook made the name
    /// query fail.
    fn adapter<T, E: From<ScopeError>>(
        &self,
        open: impl FnOnce() -> Result<T, E>,
    ) -> Result<T, E> {
        if self.names_unsupported {
            Err(ScopeError::Unsupported.into())
        } else {
            open()
        }
    }

    /// Acquires the directory `path`, once, at startup. A relative path is
    /// resolved against the process's working directory as it is then.
    ///
    /// 04 "Startup order": a path that names nothing, names something that
    /// is not a directory, that the operating system denies the process
    /// permission to open, or that is not Unicode is refused; any other
    /// error, a root that is itself a reparse point, a remote volume, and a
    /// volume on which the adapter cannot report normalized names or link
    /// counts included, is a failure.
    pub(super) fn open_root(&self, path: &str) -> Result<ScopedDir, Acquire> {
        self.adapter(|| ScopedDir::open_root(Path::new(path)))
            .map_err(|error| match error {
                ScopeError::NotFound
                | ScopeError::NotADirectory
                | ScopeError::NotAFile
                | ScopeError::NotUnicode => Acquire::Refused,
                ScopeError::Io(error)
                    if error.kind() == io::ErrorKind::PermissionDenied =>
                {
                    Acquire::Refused
                }
                ScopeError::Unsupported => Acquire::Failed(
                    "the volume is remote or does not report normalized names or \
                     link counts, so the leaf's policy cannot be enforced"
                        .to_owned(),
                ),
                other => {
                    Acquire::Failed(format!("the directory cannot be held: {other}"))
                }
            })
    }

    /// One operation on the held directory `root`.
    pub(super) fn call(
        &mut self,
        root: &ScopedDir,
        action: &Action<'_>,
        gate: &mut dyn Gate,
    ) -> Answer {
        let names = action.names.iter().map(String::as_str).collect::<Vec<_>>();
        match action.op {
            HostOp::FsReadFile => self.read(root, &names, false, gate),
            HostOp::FsReadText => self.read(root, &names, true, gate),
            HostOp::FsReadEntries => self.list(root, &names, gate),
            HostOp::FsMetadataStat => self.stat(root, &names, gate),
            HostOp::FsWriteFile | HostOp::FsWriteText => {
                match written(action.operand) {
                    Some(contents) => self.write(root, &names, contents, gate),
                    None => Answer::NotSubmitted("unsupported"),
                }
            }
            _ => Answer::NotSubmitted("unsupported"),
        }
    }

    /// A read of a whole file: its size goes to the gate, and the read takes
    /// at most what the gate reserved.
    fn read(
        &mut self,
        root: &ScopedDir,
        names: &[&str],
        text: bool,
        gate: &mut dyn Gate,
    ) -> Answer {
        let mut file = match self.adapter(|| root.open_read(names)) {
            Ok(file) => file,
            Err(failure) => return walk_failed(failure, names, gate),
        };
        let size = file.size();
        let found = Found {
            real_names: file.real_names(),
            action: FoundAction::Read,
            size: Some(size),
            links: Some(file.links()),
        };
        let Ok(reserved) = gate.admit(&found) else {
            return Answer::Refused;
        };
        if reserved.input < size {
            return Answer::NotSubmitted("unsupported");
        }
        self.reads = self.reads.saturating_add(1);
        match file.read_at_most(reserved.input) {
            Ok(bytes) => {
                let taken = moved(length(bytes.len()), 0);
                if !text {
                    return Answer::Ok(HostOk::Bytes(bytes), taken);
                }
                match String::from_utf8(bytes) {
                    Ok(value) => Answer::Ok(HostOk::Text(value), taken),
                    Err(_) => Answer::Err("invalidText", taken),
                }
            }
            Err(error) => map_error(error, READ_UNCERTAIN),
        }
    }

    /// The entries of a directory, read only after the gate admitted the
    /// listing.
    fn list(&self, root: &ScopedDir, names: &[&str], gate: &mut dyn Gate) -> Answer {
        let listing = match self.adapter(|| root.list(names)) {
            Ok(listing) => listing,
            Err(failure) => return walk_failed(failure, names, gate),
        };
        let found = Found {
            real_names: listing.real_names(),
            action: FoundAction::List,
            size: None,
            links: None,
        };
        if gate.admit(&found).is_err() {
            return Answer::Refused;
        }
        match listing.entries() {
            Ok(entries) => Answer::Ok(HostOk::Names(entries), Progress::NONE),
            Err(error) => map_error(error, Progress::NONE),
        }
    }

    /// The metadata of an entry, which the adapter does not follow; returned
    /// only after the gate admitted the stat of its kind.
    fn stat(&self, root: &ScopedDir, names: &[&str], gate: &mut dyn Gate) -> Answer {
        let metadata = match self.adapter(|| root.stat(names)) {
            Ok(metadata) => metadata,
            Err(failure) => return walk_failed(failure, names, gate),
        };
        let (action, kind) = match metadata.kind {
            EntryKind::Directory => (FoundAction::StatDirectory, HostFsKind::Directory),
            EntryKind::File => (FoundAction::StatFile, HostFsKind::File),
            EntryKind::Other => (FoundAction::StatFile, HostFsKind::Other),
        };
        let found = Found {
            real_names: &metadata.real_names,
            action,
            size: None,
            links: None,
        };
        if gate.admit(&found).is_err() {
            return Answer::Refused;
        }
        Answer::Ok(
            HostOk::Metadata {
                kind,
                size: metadata.size,
            },
            Progress::NONE,
        )
    }

    /// A write: a replace of the existing file, or a create when the adapter
    /// finds no entry.
    fn write(
        &self,
        root: &ScopedDir,
        names: &[&str],
        contents: &[u8],
        gate: &mut dyn Gate,
    ) -> Answer {
        match self.adapter(|| root.open_for_replace(names)) {
            Ok(file) => replace(file, contents, gate),
            Err(WalkError {
                error: ScopeError::NotFound,
                ..
            }) => self.create(root, names, contents, gate),
            Err(failure) => walk_failed(failure, names, gate),
        }
    }

    /// A create of the leaf of `names`: the parent is opened and held, the
    /// gate decides on its real names and the proposed leaf, and only then is
    /// the leaf created, exclusively, relative to the held parent.
    fn create(
        &self,
        root: &ScopedDir,
        names: &[&str],
        contents: &[u8],
        gate: &mut dyn Gate,
    ) -> Answer {
        // The empty path names the root, which a replace open reports as no
        // file; a create of it has no leaf.
        let Some((leaf, parent)) = names.split_last() else {
            return map_error(ScopeError::NotAFile, Progress::NONE);
        };
        let held = match self.adapter(|| root.open_parent(parent)) {
            Ok(held) => held,
            Err(failure) => return walk_failed(failure, names, gate),
        };
        let mut real_names = held.real_names().to_vec();
        real_names.push((*leaf).to_owned());
        let found = Found {
            real_names: &real_names,
            action: FoundAction::Create,
            size: None,
            links: None,
        };
        if gate.admit(&found).is_err() {
            return Answer::Refused;
        }
        match held.create_new(leaf) {
            Ok(mut file) => match file.write_all(contents) {
                Ok(()) => Answer::Ok(HostOk::Void, moved(0, length(contents.len()))),
                Err(error) => map_error(error, WRITE_UNCERTAIN),
            },
            Err(error) => map_error(error, Progress::NONE),
        }
    }
}

/// The replace of an opened file: the gate decides on its real names and its
/// link count, and only then are its contents replaced on that handle.
fn replace(mut file: ScopedFile, contents: &[u8], gate: &mut dyn Gate) -> Answer {
    let found = Found {
        real_names: file.real_names(),
        action: FoundAction::Replace,
        size: None,
        links: Some(file.links()),
    };
    if gate.admit(&found).is_err() {
        return Answer::Refused;
    }
    match file.replace_contents(contents) {
        Ok(()) => Answer::Ok(HostOk::Void, moved(0, length(contents.len()))),
        Err(error) => map_error(error, WRITE_UNCERTAIN),
    }
}

/// The answer to a walk of `names` that failed. When the failure was decided
/// below directories the walk opened, the gate is asked first whether the
/// policy permits the request on their real names followed by the rest of
/// `names`; a refusal answers `Answer::Refused`, so the answer tells nothing
/// of what lies below a directory the policy does not permit. A failure
/// before any directory opened was decided on the request's own names, which
/// step 3 permitted.
fn walk_failed(failure: WalkError, names: &[&str], gate: &mut dyn Gate) -> Answer {
    let WalkError { error, mut opened } = failure;
    if !opened.is_empty() {
        let rest = names
            .iter()
            .skip(opened.len())
            .map(|name| (*name).to_owned());
        opened.extend(rest);
        if gate.admit_error(&opened).is_err() {
            return Answer::Refused;
        }
    }
    map_error(error, Progress::NONE)
}

/// The contents a write carries.
fn written(operand: Operand<'_>) -> Option<&[u8]> {
    match operand {
        Operand::Text(text) => Some(text.as_bytes()),
        Operand::Bytes(bytes) => Some(bytes),
        Operand::None | Operand::Count(_) => None,
    }
}

/// The answer to an error of the adapter, with the progress the call made.
///
/// A link or other reparse point, and an entry a create finds under any
/// alias, are a completed `denied`; an entry the adapter cannot report the
/// guarantees for is `unsupported`, known not submitted, before acting. A
/// name the filesystem cannot interpret is `invalidPath`, a name that is not
/// Unicode `invalidText`, and an operating-system refusal `permissionDenied`;
/// an allocation the operating system refuses is memory exhaustion, never an
/// error case (06 "Memory"); every other failure is `other`.
pub(super) fn map_error(error: ScopeError, progress: Progress) -> Answer {
    let case = match error {
        ScopeError::Unsupported => return Answer::NotSubmitted("unsupported"),
        ScopeError::Io(error) if error.kind() == io::ErrorKind::OutOfMemory => {
            return Answer::MemoryExhausted(progress);
        }
        ScopeError::DeniedReparse | ScopeError::AlreadyExists => "denied",
        ScopeError::DeniedName => "invalidPath",
        ScopeError::NotFound => "notFound",
        ScopeError::NotAFile => "notAFile",
        ScopeError::NotADirectory => "notADirectory",
        ScopeError::NotUnicode => "invalidText",
        ScopeError::Io(error) if error.kind() == io::ErrorKind::PermissionDenied => {
            "permissionDenied"
        }
        ScopeError::Io(_) => "other",
    };
    Answer::Err(case, progress)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case(answer: &Answer) -> Option<&'static str> {
        match answer {
            Answer::Err(case, _) => Some(case),
            _ => None,
        }
    }

    #[test]
    fn each_adapter_error_has_its_case() {
        let mapped = |error| case(&map_error(error, Progress::NONE));
        assert_eq!(mapped(ScopeError::DeniedReparse), Some("denied"));
        assert_eq!(mapped(ScopeError::AlreadyExists), Some("denied"));
        assert_eq!(mapped(ScopeError::DeniedName), Some("invalidPath"));
        assert_eq!(mapped(ScopeError::NotFound), Some("notFound"));
        assert_eq!(mapped(ScopeError::NotAFile), Some("notAFile"));
        assert_eq!(mapped(ScopeError::NotADirectory), Some("notADirectory"));
        assert_eq!(mapped(ScopeError::NotUnicode), Some("invalidText"));
        assert_eq!(
            mapped(ScopeError::Io(io::Error::from(
                io::ErrorKind::PermissionDenied
            ))),
            Some("permissionDenied")
        );
        assert_eq!(mapped(ScopeError::Io(io::Error::other("x"))), Some("other"));
        assert!(matches!(
            map_error(ScopeError::Unsupported, Progress::NONE),
            Answer::NotSubmitted("unsupported")
        ));
        // F6 of the drivers review: a refused allocation is memory
        // exhaustion, with the progress the call made.
        assert!(matches!(
            map_error(
                ScopeError::Io(io::Error::from(io::ErrorKind::OutOfMemory)),
                WRITE_UNCERTAIN
            ),
            Answer::MemoryExhausted(WRITE_UNCERTAIN)
        ));
    }

    #[test]
    fn a_failure_keeps_its_progress() {
        let answer = map_error(ScopeError::Io(io::Error::other("x")), WRITE_UNCERTAIN);
        assert!(matches!(answer, Answer::Err("other", WRITE_UNCERTAIN)));
    }
}
