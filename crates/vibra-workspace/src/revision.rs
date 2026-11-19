//! The workspace revision.
//!
//! `docs/spec/05-tooling.md`, "Snapshot, claim and witness identity":
//! `workspaceRevision` is `sha256:` and 64 lower-case hexadecimal digits of
//! the SHA-256 of the ASCII text `vibraWorkspaceRevisionV2`, one zero byte,
//! the number of records as a big-endian 64-bit integer, and the records
//! sorted by the UTF-8 bytes of `(kind, sourceId)`. A record is three byte
//! strings, each preceded by its length as a big-endian 64-bit integer: the
//! kind, the source identifier, and the exact input bytes. It hashes captured
//! input bytes, never canonical bytes.

use std::fmt;
use std::fmt::Write as _;

use sha2::{Digest, Sha256};

/// The kind of one revision record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RevisionKind {
    /// The project file.
    Project,
    /// The lock file.
    Lock,
    /// A policy document.
    Policy,
    /// An approval.
    Approval,
    /// A source module.
    Source,
    /// A dependency input.
    Dependency,
    /// The single toolchain record.
    Toolchain,
}

impl RevisionKind {
    /// The kind as it is hashed.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Lock => "lock",
            Self::Policy => "policy",
            Self::Approval => "approval",
            Self::Source => "source",
            Self::Dependency => "dependency",
            Self::Toolchain => "toolchain",
        }
    }
}

/// One admitted input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RevisionRecord {
    kind: RevisionKind,
    source_id: String,
    bytes: Vec<u8>,
}

impl RevisionRecord {
    /// A record of `kind`, identified by `source_id`, over exact `bytes`.
    #[must_use]
    pub fn new(
        kind: RevisionKind,
        source_id: impl Into<String>,
        bytes: impl Into<Vec<u8>>,
    ) -> Self {
        Self {
            kind,
            source_id: source_id.into(),
            bytes: bytes.into(),
        }
    }

    /// The kind.
    #[must_use]
    pub const fn kind(&self) -> RevisionKind {
        self.kind
    }

    /// The project-relative source identifier.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// The exact input bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Two records of one kind with one identifier, which the framing forbids.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DuplicateRecord {
    kind: RevisionKind,
    source_id: String,
}

impl fmt::Display for DuplicateRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "two `{}` records are identified `{}`",
            self.kind.as_str(),
            self.source_id
        )
    }
}

impl std::error::Error for DuplicateRecord {}

fn length(hasher: &mut Sha256, length: usize) {
    hasher.update(u64::try_from(length).unwrap_or(u64::MAX).to_be_bytes());
}

/// The `workspaceRevision` of `records`.
pub fn workspace_revision(
    records: &[RevisionRecord],
) -> Result<String, DuplicateRecord> {
    let mut sorted = records.iter().collect::<Vec<_>>();
    sorted.sort_by(|left, right| {
        (left.kind.as_str().as_bytes(), left.source_id.as_bytes())
            .cmp(&(right.kind.as_str().as_bytes(), right.source_id.as_bytes()))
    });
    for pair in sorted.windows(2) {
        if let [left, right] = pair
            && left.kind == right.kind
            && left.source_id == right.source_id
        {
            return Err(DuplicateRecord {
                kind: left.kind,
                source_id: left.source_id.clone(),
            });
        }
    }
    let mut hasher = Sha256::new();
    hasher.update(b"vibraWorkspaceRevisionV2");
    hasher.update([0]);
    length(&mut hasher, sorted.len());
    for record in sorted {
        for part in [
            record.kind.as_str().as_bytes(),
            record.source_id.as_bytes(),
            record.bytes.as_slice(),
        ] {
            length(&mut hasher, part.len());
            hasher.update(part);
        }
    }
    let mut revision = String::from("sha256:");
    for byte in hasher.finalize() {
        let _ = write!(revision, "{byte:02x}");
    }
    Ok(revision)
}
