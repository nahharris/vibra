//! The `run` payload and its records.
//!
//! `docs/spec/05-tooling.md`, "Command payloads": the `run` payload is
//! `target`, `programResult`, `applicationCode`, `stdout`, `stderr`,
//! `auditTrace` (the events of phase `@entry`), `drivers`, `policies`,
//! `startup`, and `trap`. The startup record holds the `@startup` events and
//! the `@host` events of phase `@startup`, so no event of a run is in both
//! lists, and every policy hash of a grant projection is a key of
//! `policies`. An unavailable fact is `null`.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::diagnostic::SpanDocument;
use crate::test_payload::{AuditEvent, TrapRecord};

/// `{provider, version}`: a driver bound in the run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DriverRecord {
    /// The provider atom the driver reports, such as `@host.io.stdout`.
    pub provider: String,
    /// The release of the toolchain that ships the driver.
    pub version: String,
}

/// The status of a startup record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StartupStatus {
    /// Startup did not begin.
    NotStarted,
    /// Every leaf was processed.
    Acquired,
    /// A leaf stopped startup, or a defect trap ended it.
    Failed,
}

/// The final status of one processed leaf.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BindingStatus {
    /// The issuer refused the leaf.
    Refused,
    /// The issuer failed.
    Failed,
    /// The leaf was acquired, and its grant was released: every acquired
    /// binding of a finished command is `released`.
    Released,
}

/// `StartupRecord.bindings[]`: one processed leaf.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BindingRecordDocument {
    /// The leaf's atom.
    pub name: String,
    /// `sha256:` and the SHA-256 of the leaf's recipe identity bytes.
    pub recipe_identity: String,
    /// The driver bound for the leaf's provider.
    pub provider_identity: DriverRecord,
    /// `refused`, `failed`, or `released`.
    pub status: BindingStatus,
    /// The JSON projection of the grant, exactly for an acquired leaf.
    pub grant_projection: Option<serde_json::Value>,
}

/// `StartupRecord.failure`: the leaf that stopped startup.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StartupFailureDocument {
    /// The leaf's atom.
    pub binding: String,
    /// `@grant.requiredUnavailable` or `@grant.acquisitionFailed`.
    pub code: String,
    /// The leaf's recipe in the project document.
    pub origin: SpanDocument,
}

/// `StartupRecord`: how startup went.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupRecordDocument {
    /// `notStarted`, `acquired`, or `failed`.
    pub status: StartupStatus,
    /// One per processed leaf, in processing order.
    pub bindings: Vec<BindingRecordDocument>,
    /// The `@startup` events and the `@host` events of phase `@startup`, in
    /// order.
    pub audit_trace: Vec<AuditEvent>,
    /// The leaf that stopped startup; `null` when none did, as when a
    /// defect trap ended it.
    pub failure: Option<StartupFailureDocument>,
}

/// The `run` payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunPayload {
    /// The binary target's atom.
    pub target: String,
    /// The canonical text of the entry's result; `null` when it did not
    /// return.
    pub program_result: Option<String>,
    /// The code of the `ExitCode` the entry returned; `null` when it did
    /// not return.
    pub application_code: Option<u8>,
    /// What the program wrote to its standard output, bytes that are not
    /// UTF-8 shown as U+FFFD.
    pub stdout: String,
    /// What the program wrote to its standard error, likewise.
    pub stderr: String,
    /// The events of phase `@entry`, in completion order.
    pub audit_trace: Vec<AuditEvent>,
    /// Every driver bound in the run, as the run's trace sorts them.
    pub drivers: Vec<DriverRecord>,
    /// Every policy a projection refers to: its hash, and the canonical text
    /// of the normalized `@scopePolicy.v1` document.
    pub policies: BTreeMap<String, String>,
    /// The startup record.
    pub startup: StartupRecordDocument,
    /// The defect trap that ended the run; `null` when none did.
    pub trap: Option<TrapRecord>,
}
