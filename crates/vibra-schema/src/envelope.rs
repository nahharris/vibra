//! The version 2 command result envelope.
//!
//! `docs/spec/05-tooling.md`, "Versioned envelope and results".

use std::fmt;

use serde::Serialize;

use crate::SCHEMA_VERSION;
use crate::capability::CommandCapability;
use crate::diagnostic::{DiagnosticDocument, SpanDocument};
use crate::validate::{SchemaViolation, validate_command_result};

/// The observation an envelope describes.
///
/// It describes the observation requested. Passing sampled tests is `passed`
/// for that run, not a universal claim about the program.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    /// The observation succeeded.
    Passed,
    /// The observation was disproved.
    Refuted,
    /// The observation was not established.
    Unknown,
    /// The tool does not support the observation.
    Unsupported,
}

/// The eight command results of the result table.
///
/// The exit code each one maps to belongs to the process interface, not to
/// the contract of the document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum CommandOutcome {
    /// The requested operation completed and succeeded.
    #[serde(rename = "@command.ok")]
    Ok,
    /// Static diagnostics prevent admission.
    #[serde(rename = "@command.diagnostics")]
    Diagnostics,
    /// At least one selected test failed.
    #[serde(rename = "@command.testFailed")]
    TestFailed,
    /// An invalid command, selector, or typed input.
    #[serde(rename = "@command.invalidInput")]
    InvalidInput,
    /// A tool, host, or startup failure or exhaustion.
    #[serde(rename = "@command.operationalFailure")]
    OperationalFailure,
    /// A known operation outside the supported profile.
    #[serde(rename = "@command.unavailable")]
    Unavailable,
    /// A registered toolchain or runtime defect.
    #[serde(rename = "@command.trap")]
    Trap,
    /// The program returned a non-zero `ExitCode`.
    #[serde(rename = "@command.programFailed")]
    ProgramFailed,
}

impl CommandOutcome {
    /// Every result, in the order of the result table.
    pub const ALL: [Self; 8] = [
        Self::Ok,
        Self::Diagnostics,
        Self::TestFailed,
        Self::InvalidInput,
        Self::OperationalFailure,
        Self::Unavailable,
        Self::Trap,
        Self::ProgramFailed,
    ];

    /// The exact atom spelling.
    #[must_use]
    pub const fn as_atom(self) -> &'static str {
        match self {
            Self::Ok => "@command.ok",
            Self::Diagnostics => "@command.diagnostics",
            Self::TestFailed => "@command.testFailed",
            Self::InvalidInput => "@command.invalidInput",
            Self::OperationalFailure => "@command.operationalFailure",
            Self::Unavailable => "@command.unavailable",
            Self::Trap => "@command.trap",
            Self::ProgramFailed => "@command.programFailed",
        }
    }
}

impl fmt::Display for CommandOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_atom())
    }
}

/// The limit profile and what an observation reached.
///
/// `reached` lists the limits reached, in order, and `complete` is `false`
/// after truncation or exhaustion.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Limits {
    /// The published limit profile identifier.
    pub profile: String,
    /// The limits reached, in order.
    pub reached: Vec<String>,
    /// Whether the observation is complete.
    pub complete: bool,
}

/// The `help` payload: the usage text and the support inventory.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HelpPayload {
    /// The command hierarchy, with unavailable commands marked.
    pub usage: String,
    /// One record per command line.
    pub capabilities: Vec<CommandCapability>,
}

/// The `fmt` payload.
///
/// `docs/spec/05-tooling.md`, "Formatting": a preview carries the formatted
/// text; a written file carries `text: null`; diagnostics that prevent
/// writing carry the planned text with `written: false`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FmtPayload {
    /// The workspace-relative path, written with `/`.
    pub path: String,
    /// Whether the input bytes differ from the formatted bytes.
    pub changed: bool,
    /// Whether the file was replaced.
    pub written: bool,
    /// The formatted or planned text; `null` after a write.
    pub text: Option<String>,
}

/// The kind of a bare guard obligation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ObligationKind {
    /// A `requires` guard.
    Requires,
    /// An `ensures` guard.
    Ensures,
    /// A `where` guard.
    Where,
}

/// One obligation of the `check` payload.
///
/// `docs/spec/05-tooling.md`, "Structural position and semantic envelope",
/// "Obligation record": `{kind, declaration, predicateIdentity, source,
/// status, ruleIds, dependencies, counterexample}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObligationRecord {
    /// `requires`, `ensures`, or `where`.
    pub kind: ObligationKind,
    /// The identity of the declaration that carries the guard.
    pub declaration: String,
    /// The identity of the predicate.
    pub predicate_identity: String,
    /// The predicate's span.
    pub source: SpanDocument,
    /// `passed`, `refuted`, `unknown`, or `unsupported`.
    pub status: Status,
    /// The discharge rules used, in order.
    pub rule_ids: Vec<String>,
    /// The checked inputs the obligation depends on.
    pub dependencies: Vec<String>,
    /// The canonical VIBON text of the refuting values, or `null`.
    pub counterexample: Option<String>,
}

/// The `check` payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckPayload {
    /// Whether the covered units are admitted: no error diagnostic.
    pub accepted: bool,
    /// Every bare guard obligation of the covered units.
    pub obligations: Vec<ObligationRecord>,
}

/// The payload, keyed by the command. This version publishes the `help`,
/// `fmt`, and `check` payloads and the empty payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum Payload {
    /// The closed empty object.
    Empty {},
    /// The payload of a successful `help`.
    Help(HelpPayload),
    /// The payload of `fmt` once its path was admitted.
    Fmt(FmtPayload),
    /// The payload of `check` once the workspace was checked.
    Check(CheckPayload),
}

/// The closed object every structured response is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Envelope {
    /// Major version of this contract.
    pub schema_version: u32,
    /// The toolchain version.
    pub tool_version: String,
    /// The recognized command line, or the empty string.
    pub command: String,
    /// The workspace revision, `null` when no snapshot was taken.
    pub workspace_revision: Option<String>,
    /// The observation requested.
    pub status: Status,
    /// The command result.
    pub result: CommandOutcome,
    /// The diagnostics, once each.
    pub diagnostics: Vec<DiagnosticDocument>,
    /// The limit profile and the limits reached.
    pub limits: Limits,
    /// The payload keyed by the command.
    pub payload: Payload,
}

impl Envelope {
    /// An envelope of this schema version, with no snapshot, no diagnostic,
    /// and the empty payload.
    #[must_use]
    pub fn new(
        tool_version: impl Into<String>,
        command: impl Into<String>,
        status: Status,
        result: CommandOutcome,
        limits: Limits,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            tool_version: tool_version.into(),
            command: command.into(),
            workspace_revision: None,
            status,
            result,
            diagnostics: Vec::new(),
            limits,
            payload: Payload::Empty {},
        }
    }

    /// The envelope as a JSON value, validated against its schema.
    ///
    /// # Errors
    ///
    /// Fails when the document does not validate, or cannot be built.
    pub fn to_validated_value(&self) -> Result<serde_json::Value, SchemaViolation> {
        let value = serde_json::to_value(self).map_err(|error| {
            SchemaViolation::new("command-result", vec![error.to_string()])
        })?;
        validate_command_result(&value)?;
        Ok(value)
    }

    /// The envelope as one line of compact JSON, validated against its
    /// schema before it is returned.
    ///
    /// # Errors
    ///
    /// Fails when the document does not validate, or cannot be built.
    pub fn to_validated_json(&self) -> Result<String, SchemaViolation> {
        self.to_validated_value()?;
        serde_json::to_string(self).map_err(|error| {
            SchemaViolation::new("command-result", vec![error.to_string()])
        })
    }
}
