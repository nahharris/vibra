//! The workspace revision.
//!
//! `docs/spec/05-tooling.md`, "Snapshot, claim and witness identity":
//! `workspaceRevision` is `sha256:` and 64 lower-case hexadecimal digits of
//! the SHA-256 of the ASCII text `vibraWorkspaceRevisionV2`, one zero byte,
//! the number of records as a big-endian 64-bit integer, and the records
//! sorted by the UTF-8 bytes of `(kind, sourceId)`. A record is three byte
//! strings, each preceded by its length as a big-endian 64-bit integer: the
//! kind, the source identifier, and the exact input bytes. It hashes captured
//! input bytes, never canonical bytes, and every revision holds exactly one
//! `toolchain` record ([`toolchain_record`]): the canonical bytes of the
//! `@toolchain.v2` document of this release, whose `stdlib` is the digest of
//! the canonical bytes of the embedded library's manifest.

use std::fmt;
use std::fmt::Write as _;
use std::sync::OnceLock;

use sha2::{Digest, Sha256};
use vibra_diagnostics::Diagnostic;
use vibra_syntax::{canonical_string, decode};
use vibra_types::{StdlibInputs, manifest_schema};

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

/// Why records give no revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RevisionError {
    /// Two records of one kind with one identifier, which the framing
    /// forbids.
    Duplicate {
        /// The kind of both records.
        kind: RevisionKind,
        /// Their identifier.
        source_id: String,
    },
    /// The records hold no `toolchain` record, or more than one: every
    /// revision is computed with exactly one.
    Toolchain {
        /// How many the records hold.
        count: usize,
    },
}

impl fmt::Display for RevisionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Duplicate { kind, source_id } => write!(
                formatter,
                "two `{}` records are identified `{source_id}`",
                kind.as_str()
            ),
            Self::Toolchain { count } => write!(
                formatter,
                "a revision is computed with exactly one `toolchain` record, not {count}"
            ),
        }
    }
}

impl std::error::Error for RevisionError {}

/// The identifier of the one `toolchain` record.
pub const TOOLCHAIN_SOURCE_ID: &str = "@toolchain";

/// The canonical bytes of the `@toolchain.v2` document of this release,
/// whose `stdlib` is `stdlib`: every other field is a name the release
/// fixes.
#[must_use]
pub fn toolchain_document(stdlib: &str) -> String {
    format!(
        "{{format: @toolchain.v2 version: \"0.0.2\" grammar: \"sourceV2\" checker: \
         \"staticV2\" stdlib: {} drivers: \"systemV2\" policy: \"scopePolicyV1\" \
         registry: {{major: 2u32 minor: 0u32}}}}",
        canonical_string(stdlib)
    )
}

/// The `toolchain` record of this release with the library whose manifest
/// is `manifest`: its `stdlib` is `sha256:` and the SHA-256 of the
/// manifest's canonical bytes.
///
/// # Errors
///
/// The faults of a `manifest` that is not a `@stdlibManifest.v2` document.
pub fn toolchain_record(manifest: &[u8]) -> Result<RevisionRecord, Vec<Diagnostic>> {
    let (document, diagnostics) =
        decode(&manifest_schema(), Some("stdlib/manifest.vibon"), manifest)
            .into_parts();
    let document = document.ok_or(diagnostics)?;
    let stdlib = digest(document.canonical_bytes().as_bytes());
    Ok(RevisionRecord::new(
        RevisionKind::Toolchain,
        TOOLCHAIN_SOURCE_ID,
        toolchain_document(&stdlib),
    ))
}

/// The `toolchain` record of this toolchain, over its embedded library;
/// `None` only when the embedded manifest does not decode, which the
/// library's own tests rule out.
#[must_use]
pub fn embedded_toolchain_record() -> Option<RevisionRecord> {
    static RECORD: OnceLock<Option<RevisionRecord>> = OnceLock::new();
    RECORD
        .get_or_init(|| toolchain_record(StdlibInputs::embedded().manifest).ok())
        .clone()
}

/// `sha256:` and the lower-case hexadecimal SHA-256 of `bytes`.
fn digest(bytes: &[u8]) -> String {
    let mut text = String::from("sha256:");
    for byte in Sha256::digest(bytes) {
        let _ = write!(text, "{byte:02x}");
    }
    text
}
fn length(hasher: &mut Sha256, length: usize) {
    hasher.update(u64::try_from(length).unwrap_or(u64::MAX).to_be_bytes());
}

/// The `workspaceRevision` of `records`.
///
/// # Errors
///
/// Two records of one kind with one identifier, or not exactly one
/// `toolchain` record.
pub fn workspace_revision(records: &[RevisionRecord]) -> Result<String, RevisionError> {
    let count = records
        .iter()
        .filter(|record| record.kind == RevisionKind::Toolchain)
        .count();
    if count != 1 {
        return Err(RevisionError::Toolchain { count });
    }
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
            return Err(RevisionError::Duplicate {
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
