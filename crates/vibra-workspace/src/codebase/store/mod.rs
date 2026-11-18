//! SQLite persistence for immutable codebase revisions.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use rusqlite::{
    Connection, ErrorCode, OpenFlags, OptionalExtension, TransactionBehavior, params,
};
use sha2::{Digest, Sha256};
use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::{canonical_array, canonical_string};

use crate::codebase::graph::{EdgeTarget, EncodedGraph};
use crate::confined_fs::ConfinedDir;
use crate::index::IndexDocument;

const APPLICATION_ID: i32 = 0x5649_4252;
const SCHEMA_VERSION: i32 = 1;
const BUSY_TIMEOUT: Duration = Duration::from_millis(500);
const REQUIRED_TABLE_COLUMNS: &[(&str, &[&str])] = &[
    ("store_meta", &["key", "value_vibon"]),
    ("components", &["address", "digest", "payload_vibon"]),
    (
        "objects",
        &[
            "address",
            "component_address",
            "member_index",
            "kind",
            "key_vibon",
        ],
    ),
    (
        "source_attachments",
        &["address", "raw_hash", "payload_vibon"],
    ),
    ("revisions", &["address", "manifest_vibon"]),
    (
        "revision_entities",
        &[
            "revision_address",
            "entity_key",
            "object_address",
            "part_path_vibon",
        ],
    ),
    ("revision_objects", &["revision_address", "object_address"]),
    (
        "revision_sources",
        &[
            "revision_address",
            "source_id",
            "source_address",
            "source_hash",
            "origin",
        ],
    ),
    (
        "revision_edges",
        &[
            "revision_address",
            "edge_index",
            "owner_address",
            "part_path_vibon",
            "kind",
            "target_kind",
            "target_value",
            "target_registry",
        ],
    ),
    (
        "revision_origins",
        &[
            "revision_address",
            "origin_index",
            "object_address",
            "source_id",
            "start_byte",
            "end_byte",
            "part_path_vibon",
        ],
    ),
    ("index_documents", &["revision_address", "payload_vibon"]),
    ("heads", &["name", "revision_address"]),
];
static NEXT_TEMP_FILE: AtomicU64 = AtomicU64::new(0);

mod backup;
mod paths;
mod records;
mod verification;

pub(super) use paths::*;
pub(super) use records::*;
pub(super) use verification::*;

/// Access mode used when opening an initialized store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreAccess {
    /// Read without creating or modifying the store or its parent directory.
    ReadOnly,
    /// Read and write an already initialized store.
    ReadWrite,
}

/// A failure while opening or using a local codebase store.
#[derive(Debug)]
pub enum StoreError {
    /// The selected store does not exist.
    NotFound(PathBuf),
    /// A selected path escapes the workspace or traverses a link.
    UnsafePath(String),
    /// The database is corrupt or is not a Vibra codebase store.
    Corrupt(String),
    /// The store schema is not supported by this toolchain.
    UnsupportedVersion {
        /// Version found in the selected store.
        found: i32,
        /// Version implemented by this build.
        supported: i32,
    },
    /// The SQLite physical format is recognized but outside the M1 store contract.
    UnsupportedFormat(String),
    /// SQLite lock acquisition exceeded the bounded busy interval.
    Busy(String),
    /// A named head does not exist.
    UnknownHead(String),
    /// A requested revision is not in this store.
    UnknownRevision(String),
    /// An entity is not a member of the selected revision.
    UnknownEntity(String),
    /// An object is not a member of the selected revision.
    UnknownObject(String),
    /// The compare-and-swap head expectation did not match.
    StaleHead(String),
    /// An export plan or destination changed after preview.
    StalePlan(String),
    /// The command input is malformed.
    InvalidInput(String),
    /// A filesystem or non-busy SQLite operation failed.
    Io(String),
}

impl StoreError {
    /// The stable diagnostic classification for this failure.
    #[must_use]
    pub const fn code(&self) -> DiagnosticCode {
        match self {
            Self::NotFound(_) => DiagnosticCode::StoreNotFound,
            Self::UnsafePath(_) => DiagnosticCode::StoreUnsafePath,
            Self::Corrupt(_) => DiagnosticCode::StoreCorrupt,
            Self::UnsupportedVersion { .. } => DiagnosticCode::StoreUnsupportedVersion,
            Self::UnsupportedFormat(_) => DiagnosticCode::StoreUnsupportedVersion,
            Self::Busy(_) => DiagnosticCode::StoreBusy,
            Self::UnknownHead(_) => DiagnosticCode::CodebaseUnknownHead,
            Self::UnknownRevision(_) => DiagnosticCode::CodebaseUnknownRevision,
            Self::UnknownEntity(_) => DiagnosticCode::CodebaseUnknownEntity,
            Self::UnknownObject(_) => DiagnosticCode::CodebaseUnknownObject,
            Self::StaleHead(_) => DiagnosticCode::CodebaseStaleHead,
            Self::StalePlan(_) => DiagnosticCode::CodebaseStalePlan,
            Self::InvalidInput(_) => DiagnosticCode::CodebaseInvalidInput,
            Self::Io(_) => DiagnosticCode::StoreIoError,
        }
    }

    /// Converts this operational failure into the stable, unlocated diagnostic.
    #[must_use]
    pub fn diagnostic(&self) -> Diagnostic {
        Diagnostic::new(self.code(), ByteSpan::empty_at(0), self.to_string())
    }
}

impl fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(path) => {
                write!(
                    formatter,
                    "codebase store does not exist: {}",
                    path.display()
                )
            }
            Self::UnsafePath(message)
            | Self::Corrupt(message)
            | Self::UnsupportedFormat(message)
            | Self::Busy(message)
            | Self::UnknownHead(message)
            | Self::UnknownRevision(message)
            | Self::UnknownEntity(message)
            | Self::UnknownObject(message)
            | Self::StaleHead(message)
            | Self::StalePlan(message)
            | Self::InvalidInput(message)
            | Self::Io(message) => formatter.write_str(message),
            Self::UnsupportedVersion { found, supported } => write!(
                formatter,
                "codebase store schema version {found} is not supported (supported: {supported})"
            ),
        }
    }
}

/// Expected state of a named head during import publication.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HeadExpectation {
    /// The named head must not exist.
    Absent,
    /// The named head must point at this exact revision.
    Revision(String),
}

/// Result of a successful revision publication or same-revision no-op.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublishReceipt {
    /// The immutable candidate revision.
    pub revision: String,
    /// Whether this call advanced the head.
    pub published: bool,
    /// Number of checked object records in the candidate.
    pub object_count: u64,
    /// Number of exact source attachments in the candidate.
    pub source_count: u64,
}

/// Entity lookup result pinned to one immutable revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LookupResult {
    /// Selected immutable revision.
    pub revision: String,
    /// Canonical structured entity key.
    pub entity: String,
    /// Owning checked object address.
    pub object: String,
    /// Canonical owner-relative path.
    pub part: Vec<String>,
}

/// Closed shape of a direct reference returned by an addressed read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadTarget {
    /// Another component's checked object address.
    Object(String),
    /// A member ordinal within the selected component frame.
    InternalMember(u64),
    /// A registry-pinned compiler or host declaration.
    TrustedExternal {
        /// Canonical trusted symbol identity.
        identity: String,
        /// Registry fingerprint pinned by the selected revision.
        registry: String,
    },
}

/// One direct typed reference from a selected component member.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadReference {
    /// Owning member path in the checked definition.
    pub part: Vec<String>,
    /// Closed reference kind.
    pub kind: String,
    /// Resolved target.
    pub target: ReadTarget,
}

/// Addressed object response pinned to one revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadResult {
    /// Selected immutable revision.
    pub revision: String,
    /// Selected checked object address.
    pub object: String,
    /// Complete component address.
    pub component: String,
    /// Zero-based object ordinal within the component.
    pub member_index: u64,
    /// Exact canonical component frame.
    pub payload: String,
    /// Direct resolved references of the selected member.
    pub references: Vec<ReadReference>,
}

/// Integrity counts for the selected store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifyReport {
    /// Number of immutable revisions.
    pub revision_count: u64,
    /// Number of unique checked objects.
    pub object_count: u64,
    /// Number of unique source attachments.
    pub source_count: u64,
}

/// Counts produced by rebuilding revision-scoped derived indexes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReindexReport {
    /// Number of immutable revisions inspected.
    pub revision_count: u64,
    /// Number of rebuilt entity mappings.
    pub entity_count: u64,
    /// Number of rebuilt direct graph edges.
    pub edge_count: u64,
}

/// Exact source document attached to one revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceFile {
    /// Stable source ID.
    pub source_id: String,
    /// Export path pinned by the revision.
    pub origin: String,
    /// Exact UTF-8 source bytes.
    pub text: Vec<u8>,
    /// Raw SHA-256 without a domain prefix.
    pub raw_hash: String,
    /// Address of the canonical source wrapper.
    pub address: String,
}

/// Result of creating a standalone backup file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackupReport {
    /// Workspace-relative destination.
    pub destination: PathBuf,
    /// Number of immutable revisions included.
    pub revision_count: u64,
}

/// Result of restoring an independent store file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestoreReport {
    /// Workspace-relative restored store path.
    pub store: PathBuf,
    /// Number of immutable revisions restored.
    pub revision_count: u64,
    /// Number of unique objects restored.
    pub object_count: u64,
    /// Number of unique source attachments restored.
    pub source_count: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub(in crate::codebase) struct StoreGeneration {
    revisions: BTreeMap<String, Vec<u8>>,
    heads: BTreeMap<String, String>,
}

impl std::error::Error for StoreError {}

/// One opened local codebase store.
#[derive(Debug)]
pub struct CodebaseStore {
    pub(super) workspace_root: PathBuf,
    database_path: PathBuf,
    store_relative: PathBuf,
    _parent: ConfinedDir,
    _file_guard: File,
    access: StoreAccess,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PublicationCheckpoint {
    BeforeTransaction,
    ObjectsWritten,
    IndexesWritten,
    BeforeCommit,
    Committed,
}

impl CodebaseStore {
    /// Creates a new version-1 store at a workspace-relative file path.
    ///
    /// The path must not already exist. Parent directories are created through
    /// confined directory handles, and the new schema is installed in one
    /// SQLite transaction.
    pub fn init(
        workspace_root: impl AsRef<Path>,
        store_relative: impl AsRef<Path>,
    ) -> Result<Self, StoreError> {
        let (root, relative, parent_relative, file_name) =
            validate_store_path(workspace_root.as_ref(), store_relative.as_ref())?;
        let root_dir = ConfinedDir::open(&root).map_err(|error| {
            map_path_error(&root, error, "cannot open workspace safely")
        })?;
        let parent = if relative == Path::new(".vibra/codebase.sqlite") {
            create_confined_parent(&root_dir, &parent_relative)?
        } else {
            root_dir.open_dir(&parent_relative).map_err(|error| {
                map_path_error(
                    &relative,
                    error,
                    "explicit store parent must already exist",
                )
            })?
        };
        let temporary_name = temporary_file_name("init");
        parent
            .write_new_file(&temporary_name, &[], None)
            .map_err(|error| {
                map_path_error(&relative, error, "cannot stage codebase store")
            })?;
        let temporary_path =
            parent.path_for_child(&temporary_name).map_err(|error| {
                map_path_error(&relative, error, "cannot address staged store")
            })?;
        let staged_guard =
            parent
                .open_regular_file_write(&temporary_name)
                .map_err(|error| {
                    map_path_error(&relative, error, "cannot hold staged store safely")
                })?;
        let initialized = Self::initialize_database(&temporary_path);
        drop(staged_guard);
        if let Err(error) = initialized {
            cleanup_staged_file(&parent, &temporary_name);
            return Err(error);
        }
        let staged_file =
            parent
                .open_regular_file_write(&temporary_name)
                .map_err(|error| {
                    map_path_error(&relative, error, "cannot reopen staged store")
                })?;
        staged_file.sync_all().map_err(|error| {
            map_path_error(&relative, error, "cannot flush staged store")
        })?;
        drop(staged_file);
        sync_directory_if_supported(&parent).map_err(|error| {
            map_path_error(&relative, error, "cannot flush store directory")
        })?;
        if let Err(error) = parent.rename_to_new(&temporary_name, &parent, &file_name) {
            cleanup_staged_file(&parent, &temporary_name);
            if error.kind() == io::ErrorKind::AlreadyExists {
                return Err(StoreError::Io(format!(
                    "codebase store already exists: {}",
                    relative.display()
                )));
            }
            return Err(map_path_error(
                &relative,
                error,
                "cannot publish codebase store",
            ));
        }
        sync_directory_if_supported(&parent).map_err(|error| {
            published_sync_error(&relative, "codebase store", error)
        })?;
        Self::open(root, &relative, StoreAccess::ReadWrite)
    }

    /// Opens an initialized store without creating any filesystem entry.
    pub fn open(
        workspace_root: impl AsRef<Path>,
        store_relative: impl AsRef<Path>,
        access: StoreAccess,
    ) -> Result<Self, StoreError> {
        let (root, relative, parent_relative, file_name) =
            validate_store_path(workspace_root.as_ref(), store_relative.as_ref())?;
        let root_dir = ConfinedDir::open(&root).map_err(|error| {
            map_path_error(&root, error, "cannot open workspace safely")
        })?;
        let parent = root_dir.open_dir(&parent_relative).map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                StoreError::NotFound(relative.clone())
            } else {
                map_path_error(
                    &relative,
                    error,
                    "cannot open codebase directory safely",
                )
            }
        })?;
        parent.file_permissions(&file_name).map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                StoreError::NotFound(relative.clone())
            } else {
                map_path_error(&relative, error, "cannot open codebase file safely")
            }
        })?;
        let mut file_guard = parent.open_regular_file(&file_name).map_err(|error| {
            map_path_error(&relative, error, "cannot hold codebase store file safely")
        })?;
        validate_rollback_sqlite_header(&mut file_guard, &relative)?;
        let database_path = parent.path_for_child(&file_name).map_err(|error| {
            map_path_error(&relative, error, "cannot address codebase store")
        })?;
        let store = Self {
            workspace_root: root,
            database_path,
            store_relative: relative,
            _parent: parent,
            _file_guard: file_guard,
            access,
        };
        let mut connection = store.connect()?;
        let snapshot = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(map_sqlite_error)?;
        store.validate_schema(&snapshot)?;
        verify_authoritative(&snapshot, false)?;
        snapshot.commit().map_err(map_sqlite_error)?;
        Ok(store)
    }

    /// The workspace-relative SQLite database path.
    #[must_use]
    pub fn relative_path(&self) -> &Path {
        &self.store_relative
    }

    fn require_write_access(&self) -> Result<(), StoreError> {
        if self.access == StoreAccess::ReadWrite {
            Ok(())
        } else {
            Err(StoreError::Io(
                "codebase store is open read-only".to_owned(),
            ))
        }
    }

    /// Resolves one named head exactly once and returns its immutable revision.
    pub fn head(&self, name: Option<&str>) -> Result<String, StoreError> {
        let name = name.unwrap_or("main");
        validate_head_name(name)?;
        let mut connection = self.connect()?;
        let snapshot = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(map_sqlite_error)?;
        let revision = snapshot
            .query_row(
                "SELECT revision_address FROM heads WHERE name = ?1",
                [name],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(map_sqlite_error)?
            .ok_or_else(|| StoreError::UnknownHead(name.to_owned()))?;
        load_verified_revision(&snapshot, &revision, true)?;
        snapshot.commit().map_err(map_sqlite_error)?;
        Ok(revision)
    }

    /// Publishes a fully staged graph after an exact compare-and-swap check.
    ///
    /// The candidate is validated before SQLite is opened. The expectation is
    /// compared after acquiring an immediate writer transaction, and every
    /// immutable record, derived mapping, and the head update commit together.
    pub fn publish(
        &self,
        head: &str,
        expected: HeadExpectation,
        graph: &EncodedGraph,
        index: &IndexDocument,
    ) -> Result<PublishReceipt, StoreError> {
        self.publish_with_options(head, expected, graph, index, None, |_| {})
    }

    fn publish_with_options(
        &self,
        head: &str,
        expected: HeadExpectation,
        graph: &EncodedGraph,
        index: &IndexDocument,
        page_cache_size: Option<i64>,
        mut checkpoint: impl FnMut(PublicationCheckpoint),
    ) -> Result<PublishReceipt, StoreError> {
        self.require_write_access()?;
        validate_head_name(head)?;
        validate_graph(graph)?;
        let revision = graph.revision.address.as_str().to_owned();
        {
            let mut existing = self.connect()?;
            let snapshot = existing
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .map_err(map_sqlite_error)?;
            self.validate_schema(&snapshot)?;
            verify_authoritative(&snapshot, true)?;
            verify_rebuilt_indexes(&snapshot)?;
            snapshot.commit().map_err(map_sqlite_error)?;
        }
        let mut connection = self.connect()?;
        if let Some(page_cache_size) = page_cache_size {
            connection
                .pragma_update(None, "cache_size", page_cache_size)
                .map_err(map_sqlite_error)?;
        }
        checkpoint(PublicationCheckpoint::BeforeTransaction);
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite_error)?;
        self.validate_schema(&transaction)?;
        // Recheck the persisted authority after obtaining the writer lock so
        // a damaged store cannot accept a new revision on top of it.
        verify_authoritative(&transaction, false)?;
        let current = transaction
            .query_row(
                "SELECT revision_address FROM heads WHERE name = ?1",
                [head],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(map_sqlite_error)?;
        let matches = match (&expected, &current) {
            (HeadExpectation::Absent, None) => true,
            (HeadExpectation::Revision(expected), Some(current)) => expected == current,
            _ => false,
        };
        if !matches {
            return Err(StoreError::StaleHead(format!(
                "head {head:?} no longer matches the requested expectation"
            )));
        }
        let revision_already_stored = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM revisions WHERE address=?1)",
                [&revision],
                |row| row.get::<_, bool>(0),
            )
            .map_err(map_sqlite_error)?;
        if current.as_deref() == Some(revision.as_str()) {
            verify_immutable_revision(&transaction, graph)?;
            checkpoint(PublicationCheckpoint::BeforeCommit);
            transaction.commit().map_err(map_sqlite_error)?;
            checkpoint(PublicationCheckpoint::Committed);
            return Ok(PublishReceipt {
                revision,
                published: false,
                object_count: u64::try_from(graph.objects.len()).unwrap_or(u64::MAX),
                source_count: u64::try_from(graph.sources.len()).unwrap_or(u64::MAX),
            });
        }
        for component in &graph.components {
            let address = component.address.as_str();
            transaction
                .execute(
                    "INSERT OR IGNORE INTO components(address,digest,payload_vibon) VALUES (?1,?2,?3)",
                    params![address, component.digest.as_slice(), component.payload],
                )
                .map_err(map_sqlite_error)?;
            let stored: (Vec<u8>, Vec<u8>) = transaction
                .query_row(
                    "SELECT digest,payload_vibon FROM components WHERE address=?1",
                    [address],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map_err(map_sqlite_error)?;
            if stored.0 != component.digest || stored.1 != component.payload {
                return Err(StoreError::Corrupt(format!(
                    "component hash collision or mismatched stored frame at {address}"
                )));
            }
        }
        for object in &graph.objects {
            let address = object.address.as_str();
            let key = object.key.canonical();
            let member_index = i64::try_from(object.member_index).map_err(|_| {
                StoreError::Corrupt(
                    "object member index exceeds SQLite integer range".to_owned(),
                )
            })?;
            transaction
                .execute(
                    "INSERT OR IGNORE INTO objects(address,component_address,member_index,kind,key_vibon) VALUES (?1,?2,?3,?4,?5)",
                    params![address, object.component_address.as_str(), member_index, object.kind.as_str(), key.as_bytes()],
                )
                .map_err(map_sqlite_error)?;
            let stored: (String, i64, String, Vec<u8>) = transaction
                .query_row(
                    "SELECT component_address,member_index,kind,key_vibon FROM objects WHERE address=?1",
                    [address],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .map_err(map_sqlite_error)?;
            if stored
                != (
                    object.component_address.as_str().to_owned(),
                    member_index,
                    object.kind.as_str().to_owned(),
                    key.into_bytes(),
                )
            {
                return Err(StoreError::Corrupt(format!(
                    "object address collision or mismatched object at {address}"
                )));
            }
        }
        for source in &graph.sources {
            let address = source.address.as_str();
            let raw_hash = source.raw_hash.as_str();
            transaction
                .execute(
                    "INSERT OR IGNORE INTO source_attachments(address,raw_hash,payload_vibon) VALUES (?1,?2,?3)",
                    params![address, raw_hash, source.payload],
                )
                .map_err(map_sqlite_error)?;
            let stored: (String, Vec<u8>) = transaction
                .query_row(
                    "SELECT raw_hash,payload_vibon FROM source_attachments WHERE address=?1",
                    [address],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map_err(map_sqlite_error)?;
            if stored.0 != raw_hash || stored.1 != source.payload {
                return Err(StoreError::Corrupt(format!(
                    "source attachment address collision at {address}"
                )));
            }
        }
        transaction
            .execute(
                "INSERT OR IGNORE INTO revisions(address,manifest_vibon) VALUES (?1,?2)",
                params![revision, graph.revision.payload],
            )
            .map_err(map_sqlite_error)?;
        let stored_revision: Vec<u8> = transaction
            .query_row(
                "SELECT manifest_vibon FROM revisions WHERE address=?1",
                [&revision],
                |row| row.get(0),
            )
            .map_err(map_sqlite_error)?;
        if stored_revision != graph.revision.payload {
            return Err(StoreError::Corrupt(format!(
                "revision address collision at {revision}"
            )));
        }
        checkpoint(PublicationCheckpoint::ObjectsWritten);
        if revision_already_stored {
            // A revision is immutable and may be selected by more than one
            // named head. Reuse its checked rows instead of reinserting the
            // revision-scoped primary keys.
            checkpoint(PublicationCheckpoint::IndexesWritten);
            transaction
                .execute(
                    "INSERT INTO heads(name,revision_address) VALUES (?1,?2) ON CONFLICT(name) DO UPDATE SET revision_address=excluded.revision_address",
                    params![head, revision],
                )
                .map_err(map_sqlite_error)?;
            checkpoint(PublicationCheckpoint::BeforeCommit);
            transaction.commit().map_err(map_sqlite_error)?;
            checkpoint(PublicationCheckpoint::Committed);
            return Ok(PublishReceipt {
                revision,
                published: true,
                object_count: u64::try_from(graph.objects.len()).unwrap_or(u64::MAX),
                source_count: u64::try_from(graph.sources.len()).unwrap_or(u64::MAX),
            });
        }
        for object in &graph.objects {
            transaction
                .execute(
                    "INSERT OR IGNORE INTO revision_objects(revision_address,object_address) VALUES (?1,?2)",
                    params![revision, object.address.as_str()],
                )
                .map_err(map_sqlite_error)?;
        }
        for entity in &graph.entities {
            let key = entity.key.canonical();
            let part = canonical_path(&entity.part);
            transaction
                .execute(
                    "INSERT INTO revision_entities(revision_address,entity_key,object_address,part_path_vibon) VALUES (?1,?2,?3,?4)",
                    params![revision, key, entity.object.as_str(), part.as_bytes()],
                )
                .map_err(map_sqlite_error)?;
        }
        for (edge_index, edge) in graph.edges.iter().enumerate() {
            let edge_index = i64::try_from(edge_index).map_err(|_| {
                StoreError::Corrupt(
                    "edge count exceeds SQLite integer range".to_owned(),
                )
            })?;
            let (target_kind, target_value, target_registry) = match &edge.target {
                crate::codebase::graph::EdgeTarget::Object(address) => {
                    ("object", address.as_str(), None)
                }
                crate::codebase::graph::EdgeTarget::TrustedExternal {
                    identity,
                    registry,
                } => (
                    "trusted-external",
                    identity.as_str(),
                    Some(registry.as_str()),
                ),
            };
            transaction
                .execute(
                    "INSERT INTO revision_edges(revision_address,edge_index,owner_address,part_path_vibon,kind,target_kind,target_value,target_registry) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                    params![revision, edge_index, edge.owner.as_str(), canonical_path(&edge.part).as_bytes(), edge.kind.as_str(), target_kind, target_value, target_registry],
                )
                .map_err(map_sqlite_error)?;
        }
        for (origin_index, object) in graph
            .objects
            .iter()
            .flat_map(|object| {
                object.origins.iter().map(move |origin| (object, origin))
            })
            .enumerate()
        {
            let (object, origin) = object;
            let start = i64::try_from(origin.span.start()).map_err(|_| {
                StoreError::Corrupt(
                    "source origin start exceeds SQLite integer range".to_owned(),
                )
            })?;
            let end = i64::try_from(origin.span.end()).map_err(|_| {
                StoreError::Corrupt(
                    "source origin end exceeds SQLite integer range".to_owned(),
                )
            })?;
            transaction
                .execute(
                    "INSERT INTO revision_origins(revision_address,origin_index,object_address,source_id,start_byte,end_byte,part_path_vibon) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                    params![revision, i64::try_from(origin_index).unwrap_or(i64::MAX), object.address.as_str(), origin.source_id, start, end, canonical_path(&origin.part).as_bytes()],
                )
                .map_err(map_sqlite_error)?;
        }
        for source in &graph.sources {
            transaction
                .execute(
                    "INSERT INTO revision_sources(revision_address,source_id,source_address,source_hash,origin) VALUES (?1,?2,?3,?4,?5)",
                    params![revision, source.source_id, source.address.as_str(), source.raw_hash.as_str(), source.origin],
                )
                .map_err(map_sqlite_error)?;
        }
        let index_payload = index.canonical_vibon();
        transaction
            .execute(
                "INSERT OR REPLACE INTO index_documents(revision_address,payload_vibon) VALUES (?1,?2)",
                params![revision, index_payload.as_bytes()],
            )
            .map_err(map_sqlite_error)?;
        checkpoint(PublicationCheckpoint::IndexesWritten);
        transaction
            .execute(
                "INSERT INTO heads(name,revision_address) VALUES (?1,?2) ON CONFLICT(name) DO UPDATE SET revision_address=excluded.revision_address",
                params![head, revision],
            )
            .map_err(map_sqlite_error)?;
        checkpoint(PublicationCheckpoint::BeforeCommit);
        transaction.commit().map_err(map_sqlite_error)?;
        checkpoint(PublicationCheckpoint::Committed);
        Ok(PublishReceipt {
            revision,
            published: true,
            object_count: u64::try_from(graph.objects.len()).unwrap_or(u64::MAX),
            source_count: u64::try_from(graph.sources.len()).unwrap_or(u64::MAX),
        })
    }

    #[cfg(test)]
    fn publish_with_before_commit(
        &self,
        head: &str,
        expected: HeadExpectation,
        graph: &EncodedGraph,
        index: &IndexDocument,
        before_commit: impl FnOnce(),
    ) -> Result<PublishReceipt, StoreError> {
        let mut before_commit = Some(before_commit);
        self.publish_with_options(head, expected, graph, index, Some(1), |phase| {
            if phase == PublicationCheckpoint::BeforeCommit
                && let Some(before_commit) = before_commit.take()
            {
                before_commit();
            }
        })
    }

    /// Resolves a canonical structured entity key in one selected revision.
    pub fn lookup(
        &self,
        revision: &str,
        entity: &str,
    ) -> Result<LookupResult, StoreError> {
        validate_address_text(revision)?;
        if !crate::codebase::is_valid_entity_key(entity) {
            return Err(StoreError::InvalidInput(format!(
                "invalid canonical codebase entity key: {entity:?}"
            )));
        }
        let mut connection = self.connect()?;
        let snapshot = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(map_sqlite_error)?;
        load_verified_revision(&snapshot, revision, true)?;
        let result = snapshot
            .query_row(
                "SELECT e.object_address,e.part_path_vibon FROM revision_entities e WHERE e.revision_address=?1 AND e.entity_key=?2 AND EXISTS (SELECT 1 FROM revision_objects o WHERE o.revision_address=e.revision_address AND o.object_address=e.object_address)",
                params![revision, entity],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )
            .optional()
            .map_err(map_sqlite_error)?
            .ok_or_else(|| StoreError::UnknownEntity(entity.to_owned()))?;
        snapshot.commit().map_err(map_sqlite_error)?;
        Ok(LookupResult {
            revision: revision.to_owned(),
            entity: entity.to_owned(),
            object: result.0,
            part: parse_path(&result.1)?,
        })
    }

    /// Reads one checked object and its direct typed references in a pinned revision.
    pub fn read(&self, revision: &str, object: &str) -> Result<ReadResult, StoreError> {
        validate_address_text(revision)?;
        validate_address_text(object)?;
        let mut connection = self.connect()?;
        let snapshot = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(map_sqlite_error)?;
        let revision_frame = load_verified_revision(&snapshot, revision, true)?;
        let row = snapshot
            .query_row(
                "SELECT o.component_address,o.member_index,c.payload_vibon,c.digest FROM revision_objects r JOIN objects o ON o.address=r.object_address JOIN components c ON c.address=o.component_address WHERE r.revision_address=?1 AND r.object_address=?2",
                params![revision, object],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, Vec<u8>>(2)?, row.get::<_, Vec<u8>>(3)?)),
            )
            .optional()
            .map_err(map_sqlite_error)?
            .ok_or_else(|| StoreError::UnknownObject(object.to_owned()))?;
        let component_address = row.0.clone();
        let component_frame =
            crate::codebase::graph::validate_component_payload(&row.0, &row.3, &row.2)
                .map_err(|error| {
                    StoreError::Corrupt(format!("invalid component frame: {error:?}"))
                })?;
        let digest: [u8; 32] = row.3.as_slice().try_into().map_err(|_| {
            StoreError::Corrupt("component digest has invalid length".to_owned())
        })?;
        let member_index = u64::try_from(row.1)
            .map_err(|_| StoreError::Corrupt("negative member index".to_owned()))?;
        let revision_objects = manifest_object_addresses(&revision_frame)?;
        let references = read_member_references(
            &snapshot,
            &component_frame,
            &component_address,
            &digest,
            member_index,
            &revision_objects,
            &revision_frame,
        )?;
        snapshot.commit().map_err(map_sqlite_error)?;
        Ok(ReadResult {
            revision: revision.to_owned(),
            object: object.to_owned(),
            component: row.0,
            member_index: u64::try_from(row.1)
                .map_err(|_| StoreError::Corrupt("negative member index".to_owned()))?,
            payload: String::from_utf8(row.2)
                .map_err(|error| StoreError::Corrupt(error.to_string()))?,
            references,
        })
    }

    /// Verifies SQLite integrity and every authoritative record in the store.
    pub fn verify(&self) -> Result<VerifyReport, StoreError> {
        let mut connection = self.connect()?;
        let snapshot = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(map_sqlite_error)?;
        self.validate_schema(&snapshot)?;
        let report = verify_authoritative(&snapshot, true)?;
        verify_rebuilt_indexes(&snapshot)?;
        snapshot.commit().map_err(map_sqlite_error)?;
        Ok(report)
    }

    /// Rebuilds revision-scoped derived indexes from verified attachments.
    pub fn reindex(&self) -> Result<ReindexReport, StoreError> {
        self.reindex_with_before_publish(|| {})
    }

    fn reindex_with_before_publish(
        &self,
        before_publish: impl FnOnce(),
    ) -> Result<ReindexReport, StoreError> {
        self.require_write_access()?;
        let mut read_connection = self.connect()?;
        let snapshot = read_connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(map_sqlite_error)?;
        self.validate_schema(&snapshot)?;
        let authority = verify_authoritative(&snapshot, false)?;
        let generation = capture_generation(&snapshot)?;
        let mut revision_inputs = Vec::with_capacity(generation.revisions.len());
        for (revision, manifest) in &generation.revisions {
            let frame =
                crate::codebase::graph::validate_revision_payload(revision, manifest)
                    .map_err(|error| {
                    StoreError::Corrupt(format!(
                        "invalid revision {revision}: {error:?}"
                    ))
                })?;
            validate_revision_shape(&frame)?;
            let sources = source_files_from_manifest(&snapshot, &frame)?;
            revision_inputs.push((revision.clone(), manifest.clone(), sources));
        }
        snapshot.commit().map_err(map_sqlite_error)?;
        drop(read_connection);

        let mut rebuilt = Vec::with_capacity(revision_inputs.len());
        for (revision, manifest, sources) in revision_inputs {
            let workspace = ReindexWorkspace::materialize(&sources)?;
            let prepared = crate::codebase::import::prepare_import(workspace.path())
                .map_err(|error| {
                    StoreError::Corrupt(format!(
                        "stored revision {revision} no longer checks: {error}"
                    ))
                })?;
            if prepared.graph.revision.address.as_str() != revision
                || prepared.graph.revision.payload != manifest
            {
                return Err(StoreError::Corrupt(format!(
                    "rebuilding from exact source attachments produced a different revision than {revision}"
                )));
            }
            validate_graph(&prepared.graph)?;
            rebuilt.push((prepared.graph, prepared.index));
        }
        before_publish();
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite_error)?;
        self.validate_schema(&transaction)?;
        let current_generation = capture_generation(&transaction)?;
        if current_generation != generation {
            return Err(StoreError::StaleHead(
                "revision or head set changed while indexes were being rebuilt"
                    .to_owned(),
            ));
        }
        transaction
            .execute_batch(
                "DELETE FROM revision_edges;
                 DELETE FROM revision_origins;
                 DELETE FROM revision_entities;
                 DELETE FROM revision_sources;
                 DELETE FROM revision_objects;
                 DELETE FROM index_documents;",
            )
            .map_err(map_sqlite_error)?;
        let mut entity_count = 0_u64;
        let mut edge_count = 0_u64;
        for (graph, index) in &rebuilt {
            insert_derived_rows(&transaction, graph, index)?;
            entity_count = entity_count.saturating_add(
                u64::try_from(graph.entities.len()).unwrap_or(u64::MAX),
            );
            edge_count = edge_count
                .saturating_add(u64::try_from(graph.edges.len()).unwrap_or(u64::MAX));
        }
        transaction.commit().map_err(map_sqlite_error)?;
        Ok(ReindexReport {
            revision_count: authority.revision_count,
            entity_count,
            edge_count,
        })
    }

    /// Returns the exact source files attached to one immutable revision.
    pub fn revision_sources(
        &self,
        revision: &str,
    ) -> Result<Vec<SourceFile>, StoreError> {
        validate_address_text(revision)?;
        let mut connection = self.connect()?;
        let snapshot = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(map_sqlite_error)?;
        load_verified_revision(&snapshot, revision, true)?;
        let mut statement = snapshot
            .prepare(
                "SELECT rs.source_id,rs.origin,rs.source_hash,rs.source_address,sa.payload_vibon FROM revision_sources rs JOIN source_attachments sa ON sa.address=rs.source_address WHERE rs.revision_address=?1 ORDER BY CAST(rs.source_id AS BLOB)",
            )
            .map_err(map_sqlite_error)?;
        let rows = statement
            .query_map([revision], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Vec<u8>>(4)?,
                ))
            })
            .map_err(map_sqlite_error)?;
        let mut sources = Vec::new();
        for row in rows {
            let (source_id, origin, raw_hash, address, wrapper) =
                row.map_err(map_sqlite_error)?;
            let text = decode_source_wrapper(&wrapper)?;
            validate_source_record(&address, &raw_hash, &wrapper, text.as_bytes())?;
            sources.push(SourceFile {
                source_id,
                origin,
                text: text.into_bytes(),
                raw_hash,
                address,
            });
        }
        drop(statement);
        snapshot.commit().map_err(map_sqlite_error)?;
        Ok(sources)
    }

    /// Opens one SQLite connection in this store's access mode.
    pub(super) fn connect(&self) -> Result<Connection, StoreError> {
        let flags = match self.access {
            StoreAccess::ReadOnly => {
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW
            }
            StoreAccess::ReadWrite => {
                OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW
            }
        };
        let connection = Connection::open_with_flags(&self.database_path, flags)
            .map_err(map_sqlite_error)?;
        connection
            .busy_timeout(BUSY_TIMEOUT)
            .map_err(map_sqlite_error)?;
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(map_sqlite_error)?;
        if self.access == StoreAccess::ReadWrite {
            connection
                .pragma_update(None, "synchronous", "FULL")
                .map_err(map_sqlite_error)?;
        }
        Ok(connection)
    }

    fn initialize_database(database_path: &Path) -> Result<(), StoreError> {
        let mut connection = Connection::open_with_flags(
            database_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .map_err(map_sqlite_error)?;
        connection
            .busy_timeout(BUSY_TIMEOUT)
            .map_err(map_sqlite_error)?;
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(map_sqlite_error)?;
        connection
            .pragma_update(None, "journal_mode", "DELETE")
            .map_err(map_sqlite_error)?;
        connection
            .pragma_update(None, "synchronous", "FULL")
            .map_err(map_sqlite_error)?;
        let transaction = connection.transaction().map_err(map_sqlite_error)?;
        transaction
            .execute_batch(
                "CREATE TABLE store_meta (
                     key TEXT PRIMARY KEY NOT NULL,
                     value_vibon BLOB NOT NULL
                 ) STRICT, WITHOUT ROWID;
                 CREATE TABLE components (
                     address TEXT PRIMARY KEY NOT NULL,
                     digest BLOB NOT NULL,
                     payload_vibon BLOB NOT NULL
                 ) STRICT, WITHOUT ROWID;
                 CREATE TABLE objects (
                     address TEXT PRIMARY KEY NOT NULL,
                     component_address TEXT NOT NULL,
                     member_index INTEGER NOT NULL CHECK (member_index >= 0),
                     kind TEXT NOT NULL,
                     key_vibon BLOB NOT NULL,
                     FOREIGN KEY (component_address) REFERENCES components(address)
                 ) STRICT, WITHOUT ROWID;
                 CREATE TABLE source_attachments (
                     address TEXT PRIMARY KEY NOT NULL,
                     raw_hash TEXT NOT NULL,
                     payload_vibon BLOB NOT NULL
                 ) STRICT, WITHOUT ROWID;
                 CREATE TABLE revisions (
                     address TEXT PRIMARY KEY NOT NULL,
                     manifest_vibon BLOB NOT NULL
                 ) STRICT, WITHOUT ROWID;
                 CREATE TABLE revision_entities (
                     revision_address TEXT NOT NULL,
                     entity_key TEXT NOT NULL,
                     object_address TEXT NOT NULL,
                     part_path_vibon BLOB NOT NULL,
                     PRIMARY KEY (revision_address, entity_key),
                     FOREIGN KEY (revision_address) REFERENCES revisions(address),
                     FOREIGN KEY (object_address) REFERENCES objects(address)
                 ) STRICT, WITHOUT ROWID;
                 CREATE TABLE revision_objects (
                     revision_address TEXT NOT NULL,
                     object_address TEXT NOT NULL,
                     PRIMARY KEY (revision_address, object_address),
                     FOREIGN KEY (revision_address) REFERENCES revisions(address),
                     FOREIGN KEY (object_address) REFERENCES objects(address)
                 ) STRICT, WITHOUT ROWID;
                 CREATE TABLE revision_sources (
                     revision_address TEXT NOT NULL,
                     source_id TEXT NOT NULL,
                     source_address TEXT NOT NULL,
                     source_hash TEXT NOT NULL,
                     origin TEXT NOT NULL,
                     PRIMARY KEY (revision_address, source_id),
                     FOREIGN KEY (revision_address) REFERENCES revisions(address),
                     FOREIGN KEY (source_address) REFERENCES source_attachments(address)
                 ) STRICT, WITHOUT ROWID;
                 CREATE TABLE revision_edges (
                     revision_address TEXT NOT NULL,
                     edge_index INTEGER NOT NULL CHECK (edge_index >= 0),
                     owner_address TEXT NOT NULL,
                     part_path_vibon BLOB NOT NULL,
                     kind TEXT NOT NULL,
                     target_kind TEXT NOT NULL,
                     target_value TEXT NOT NULL,
                     target_registry TEXT,
                     PRIMARY KEY (revision_address, edge_index),
                     FOREIGN KEY (revision_address) REFERENCES revisions(address),
                     FOREIGN KEY (revision_address, owner_address)
                         REFERENCES revision_objects(revision_address, object_address)
                 ) STRICT, WITHOUT ROWID;
                 CREATE TABLE revision_origins (
                     revision_address TEXT NOT NULL,
                     origin_index INTEGER NOT NULL CHECK (origin_index >= 0),
                     object_address TEXT NOT NULL,
                     source_id TEXT NOT NULL,
                     start_byte INTEGER NOT NULL CHECK (start_byte >= 0),
                     end_byte INTEGER NOT NULL CHECK (end_byte >= start_byte),
                     part_path_vibon BLOB NOT NULL,
                     PRIMARY KEY (revision_address, origin_index),
                     FOREIGN KEY (revision_address, object_address)
                         REFERENCES revision_objects(revision_address, object_address)
                 ) STRICT, WITHOUT ROWID;
                 CREATE TABLE index_documents (
                     revision_address TEXT PRIMARY KEY NOT NULL,
                     payload_vibon BLOB NOT NULL,
                     FOREIGN KEY (revision_address) REFERENCES revisions(address)
                 ) STRICT, WITHOUT ROWID;
                 CREATE TABLE heads (
                     name TEXT PRIMARY KEY NOT NULL,
                     revision_address TEXT NOT NULL,
                     FOREIGN KEY (revision_address) REFERENCES revisions(address)
                 ) STRICT, WITHOUT ROWID;
                 CREATE INDEX revision_entities_by_object
                     ON revision_entities(revision_address, object_address);
                 CREATE INDEX revision_edges_by_owner
                     ON revision_edges(revision_address, owner_address);
                 CREATE INDEX revision_sources_by_address
                     ON revision_sources(source_address);",
            )
            .map_err(map_sqlite_error)?;
        transaction
            .execute(
                "INSERT INTO store_meta(key, value_vibon) VALUES (?1, ?2)",
                params![
                    "format",
                    b"(record format: @codebase-store.v1 schema: 1u64)\n".as_slice(),
                ],
            )
            .map_err(map_sqlite_error)?;
        transaction
            .pragma_update(None, "application_id", APPLICATION_ID)
            .map_err(map_sqlite_error)?;
        transaction
            .pragma_update(None, "user_version", SCHEMA_VERSION)
            .map_err(map_sqlite_error)?;
        transaction.commit().map_err(map_sqlite_error)
    }

    fn validate_schema(&self, connection: &Connection) -> Result<(), StoreError> {
        let application_id = connection
            .pragma_query_value(None, "application_id", |row| row.get::<_, i32>(0))
            .map_err(map_sqlite_error)?;
        if application_id != APPLICATION_ID {
            return Err(StoreError::Corrupt(format!(
                "{} is not a Vibra codebase store",
                self.store_relative.display()
            )));
        }
        let found = connection
            .pragma_query_value(None, "user_version", |row| row.get::<_, i32>(0))
            .map_err(map_sqlite_error)?;
        if found != SCHEMA_VERSION {
            return Err(StoreError::UnsupportedVersion {
                found,
                supported: SCHEMA_VERSION,
            });
        }
        validate_schema_layout(connection)?;
        let format = connection
            .query_row(
                "SELECT value_vibon FROM store_meta WHERE key = ?1",
                ["format"],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .map_err(map_schema_error)?;
        if format != b"(record format: @codebase-store.v1 schema: 1u64)\n" {
            return Err(StoreError::Corrupt(
                "codebase store format record is malformed".to_owned(),
            ));
        }
        Ok(())
    }
}

fn validate_rollback_sqlite_header(
    file: &mut File,
    relative: &Path,
) -> Result<(), StoreError> {
    let mut header = [0_u8; 20];
    file.read_exact(&mut header).map_err(|error| {
        if error.kind() == io::ErrorKind::UnexpectedEof {
            StoreError::Corrupt(format!(
                "codebase database has a truncated SQLite header: {}",
                relative.display()
            ))
        } else {
            map_path_error(relative, error, "cannot read codebase database header")
        }
    })?;
    const SQLITE_HEADER: &[u8; 16] = b"SQLite format 3\0";
    if header.get(..16) != Some(SQLITE_HEADER.as_slice()) {
        return Err(StoreError::Corrupt(format!(
            "codebase database has an invalid SQLite header: {}",
            relative.display()
        )));
    }
    if header.get(18..20) != Some([1_u8, 1].as_slice()) {
        return Err(StoreError::UnsupportedFormat(format!(
            "codebase SQLite journal format is unsupported: {} uses WAL or another non-rollback format; M1 requires the rollback-journal format",
            relative.display()
        )));
    }
    Ok(())
}

fn validate_schema_layout(connection: &Connection) -> Result<(), StoreError> {
    for (table, expected_columns) in REQUIRED_TABLE_COLUMNS {
        let sql = format!("PRAGMA table_info('{table}')");
        let mut statement = connection.prepare(&sql).map_err(map_schema_error)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(map_schema_error)?;
        let mut actual_columns = Vec::new();
        for row in rows {
            actual_columns.push(row.map_err(map_schema_error)?);
        }
        if actual_columns
            .iter()
            .map(String::as_str)
            .ne(expected_columns.iter().copied())
        {
            return Err(StoreError::Corrupt(format!(
                "codebase store table {table} has invalid columns: expected {expected_columns:?}, found {actual_columns:?}"
            )));
        }
    }
    Ok(())
}

fn validate_head_name(name: &str) -> Result<(), StoreError> {
    if !is_valid_head_name(name) {
        return Err(StoreError::InvalidInput(format!(
            "invalid codebase head name: {name:?}"
        )));
    }
    Ok(())
}

/// Returns whether a head label matches the canonical lowercase kebab-case grammar.
#[must_use]
pub fn is_valid_head_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    if !bytes.next().is_some_and(|byte| byte.is_ascii_lowercase()) {
        return false;
    }
    let mut previous_hyphen = false;
    for byte in bytes {
        if byte == b'-' {
            if previous_hyphen {
                return false;
            }
            previous_hyphen = true;
        } else if byte.is_ascii_lowercase() || byte.is_ascii_digit() {
            previous_hyphen = false;
        } else {
            return false;
        }
    }
    !previous_hyphen
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod publication_tests;
