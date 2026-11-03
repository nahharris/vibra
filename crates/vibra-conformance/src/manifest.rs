//! The neutral `case.toml` conformance manifest.
//!
//! The manifest is deliberately separate from VIBON. It is the oracle used to
//! test the VIBON decoder, so decoding expectations through the language's
//! data grammar would make the harness unable to catch decoder defects.

use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::Deserialize;
use vibra_diagnostics::{ByteSpan, DiagnosticCode, Level};

use crate::profile::ConformanceProfile;

/// The file name every corpus case uses.
pub const MANIFEST_FILE_NAME: &str = "case.toml";

/// The normative section prefixes accepted in a case identifier.
pub const NORMATIVE_SECTION_IDS: &[&str] = &[
    "V1-CHARTER",
    "V1-SRC-READER",
    "V1-SRC-CALLS",
    "V1-SRC-DECL",
    "V1-SRC-EXPR",
    "V1-SRC-FMT",
    "V1-TYPE-NOMINAL",
    "V1-TYPE-NAMES",
    "V1-TYPE-INFER",
    "V1-TYPE-GENERIC",
    "V1-TYPE-INTERFACE",
    "V1-TYPE-CONVERT",
    "V1-TYPE-CONTROL",
    "V1-EFFECT",
    "V1-PROJECT",
    "V1-TOOL",
    "V1-RUNTIME",
    "V1-DIAG",
];

/// A manifest decoding or validation failure.
#[derive(Debug)]
pub enum ManifestError {
    /// The TOML document could not be decoded.
    Toml(toml::de::Error),
    /// The decoded document violates the case contract.
    Invalid(String),
    /// A manifest file could not be read.
    Io {
        /// The path that was read.
        path: PathBuf,
        /// The underlying filesystem failure.
        source: std::io::Error,
    },
}

impl fmt::Display for ManifestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Toml(error) => write!(formatter, "invalid case.toml: {error}"),
            Self::Invalid(message) => formatter.write_str(message),
            Self::Io { path, source } => {
                write!(formatter, "cannot read {}: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for ManifestError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Toml(error) => Some(error),
            Self::Io { source, .. } => Some(source),
            Self::Invalid(_) => None,
        }
    }
}

impl From<toml::de::Error> for ManifestError {
    fn from(error: toml::de::Error) -> Self {
        Self::Toml(error)
    }
}

/// The input paths supplied to a conformance case.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CaseInputs {
    /// The source document, normally a `.vib` file.
    pub source: Option<String>,
    /// The project manifest, normally a `.vibon` file.
    pub project: Option<String>,
    /// Additional data documents, normally `.vibon` files.
    pub data: Vec<String>,
    /// Optional confined directory tree acquired by source-graph cases.
    pub tree: Option<String>,
    /// For `workspace-run` and `workspace-test`, the path of a
    /// `@host-world.v1` document that supplies the case's host world. It is
    /// copied fresh for each test and each backend; without it a case runs in
    /// the default world.
    pub host_world: Option<String>,
}

/// The closed operation selected by a conformance case.
///
/// Later slices add their operation to this enum before registering a handler;
/// input presence alone never selects a semantic implementation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConformanceOperation {
    /// Run the reader/data grammar handler.
    Reader,
    /// Decode one project VIBON document without resolution or I/O.
    ProjectDecode,
    /// Acquire a confined project tree and build its immutable source graph.
    SourceGraph,
    /// Resolve declarations/imports from a confined source graph.
    Resolve,
    /// Check one source document as the only module of a one-module workspace
    /// and execute it through the reference interpreter, observing the
    /// canonical encoding of the value its first function returns.
    Interpret,
    /// Render semantic facts from one immutable workspace snapshot.
    Query,
    /// Render the `@index.v1` document of one confined workspace snapshot.
    Index,
    /// Format one source document using bindings from its confined snapshot.
    Format,
    /// Check every declaration in one confined workspace snapshot.
    WorkspaceCheck,
    /// Check and interpret the unique binary target in a confined workspace.
    WorkspaceRun,
    /// Discover, check, and run all tests in a confined workspace.
    WorkspaceTest,
}

impl ConformanceOperation {
    /// The stable manifest spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Reader => "reader",
            Self::ProjectDecode => "project-decode",
            Self::SourceGraph => "source-graph",
            Self::Resolve => "resolve",
            Self::Interpret => "interpret",
            Self::Query => "query",
            Self::Index => "index",
            Self::Format => "format",
            Self::WorkspaceCheck => "workspace-check",
            Self::WorkspaceRun => "workspace-run",
            Self::WorkspaceTest => "workspace-test",
        }
    }

    /// Whether this operation executes a program, which is the operation half
    /// of an **executable case** (`CaseManifest::is_executable` adds the other).
    #[must_use]
    pub const fn executes_programs(self) -> bool {
        matches!(
            self,
            Self::Interpret | Self::WorkspaceRun | Self::WorkspaceTest
        )
    }
}

impl fmt::Display for ConformanceOperation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// One expected source span attached to a diagnostic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpectedRelatedSpan {
    /// The input path owning the related span, when known.
    pub source_id: Option<String>,
    /// The half-open UTF-8 byte span.
    pub span: ByteSpan,
    /// An optional expected explanation. Omitted explanations are not
    /// asserted because diagnostic prose is not a stable machine contract.
    pub message: Option<String>,
}

/// One expected diagnostic in corpus order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpectedDiagnostic {
    /// The closed-registry code.
    pub code: DiagnosticCode,
    /// The code's fixed registry level.
    pub level: Level,
    /// Optional human-facing message assertion.
    pub message: Option<String>,
    /// The input path owning the primary span, when known.
    pub source_id: Option<String>,
    /// The primary half-open UTF-8 byte span.
    pub primary_span: ByteSpan,
    /// Related spans in their expected order.
    pub related: Vec<ExpectedRelatedSpan>,
    /// Expected notes, when the case intentionally covers notes.
    pub notes: Vec<String>,
    /// Expected fixes, when the case intentionally covers fix behavior.
    pub fixes: Option<Vec<ExpectedFix>>,
}

/// A fix expectation attached to a diagnostic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpectedFix {
    /// Whether this fix is safe to apply automatically.
    pub safe: bool,
    /// Optional human-facing description assertion.
    pub description: Option<String>,
    /// Optional expected document revision.
    pub revision: Option<String>,
}

/// Snapshot references used by expected outputs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpectedExecution {
    /// A relative path to the expected result snapshot.
    pub result: Option<String>,
    /// A relative path to the expected ordered `.vibon` audit-trace snapshot.
    pub audit_trace: Option<String>,
}

/// One expected structural source-position query snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpectedQuery {
    /// The case-relative input document to query.
    pub input: String,
    /// The UTF-8 byte offset sent to the syntax query API.
    pub offset: usize,
    /// A case-relative JSON snapshot rendered by the schema adapter.
    pub snapshot: String,
    /// The expansions requested, as `--expand` values: `effect-witnesses`.
    pub expand: Vec<String>,
}

/// Expected execution-independent outputs of a case.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CaseExpectations {
    /// Whether the case is accepted after diagnostics are emitted.
    pub accepted: bool,
    /// Diagnostics in emission order.
    pub diagnostics: Vec<ExpectedDiagnostic>,
    /// A relative path to the canonical formatting snapshot.
    pub formatted: Option<String>,
    /// A relative path to the canonical source-graph snapshot.
    pub graph: Option<String>,
    /// A relative path to resolved-identity output.
    pub resolved: Option<String>,
    /// A relative path to the `@types.v1` observation of the accepted
    /// workspace, which `workspace-check` cases may declare.
    pub types: Option<String>,
    /// A relative path to effect output.
    pub effects: Option<String>,
    /// A relative path to the `@index.v1` document.
    pub index: Option<String>,
    /// Structural source-position query snapshots.
    pub queries: Vec<ExpectedQuery>,
    /// Expected reference-interpreter output.
    pub interpreter: Option<ExpectedExecution>,
    /// The host event an `interpret` or `workspace-run` case ends in, which
    /// replaces the result and audit-trace snapshots because such a run has
    /// neither.
    pub host_event: Option<String>,
    /// Expected deterministic artifact hashes, when the case covers an
    /// artifact-producing backend.
    pub artifact_hashes: Option<Vec<String>>,
}

/// The decoded, validated neutral case manifest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaseManifest {
    /// Stable case identifier, including its normative section prefix.
    pub id: String,
    /// Normative rule or section addressed by the case.
    pub rule_id: String,
    /// Minimum profile required to execute the case.
    pub profile: ConformanceProfile,
    /// Closed operation selected by the case inputs and operation field.
    pub operation: ConformanceOperation,
    /// Optional maintainer-facing description.
    pub description: Option<String>,
    /// Case inputs.
    pub inputs: CaseInputs,
    /// Expected observations.
    pub expectations: CaseExpectations,
}

impl CaseManifest {
    /// Whether this is an **executable case** of
    /// `docs/spec/07-diagnostics-and-conformance.md`, "Differential
    /// execution": its operation executes a program and its expectation is
    /// accepted, because a rejected program reaches no backend. Both backends
    /// run such a case against its one expectation.
    #[must_use]
    pub fn is_executable(&self) -> bool {
        self.operation.executes_programs() && self.expectations.accepted
    }

    /// Decodes and validates a TOML manifest.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(text: &str) -> Result<Self, ManifestError> {
        text.parse()
    }

    /// Reads, decodes, and validates a manifest file.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, ManifestError> {
        let path = path.as_ref();
        let text =
            std::fs::read_to_string(path).map_err(|source| ManifestError::Io {
                path: path.to_path_buf(),
                source,
            })?;
        Self::from_str(&text)
    }

    /// The stable case identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The normative rule identifier.
    #[must_use]
    pub fn rule_id(&self) -> &str {
        &self.rule_id
    }

    /// The normative section prefix addressed by this case.
    #[must_use]
    pub fn section(&self) -> &str {
        section_for(&self.rule_id).unwrap_or(&self.rule_id)
    }

    /// The required execution profile.
    #[must_use]
    pub const fn profile(&self) -> ConformanceProfile {
        self.profile
    }

    /// The closed operation selected for this case.
    #[must_use]
    pub const fn operation(&self) -> ConformanceOperation {
        self.operation
    }
}

impl FromStr for CaseManifest {
    type Err = ManifestError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let raw = toml::from_str::<RawCaseManifest>(text)?;
        Self::try_from(raw)
    }
}

impl TryFrom<RawCaseManifest> for CaseManifest {
    type Error = ManifestError;

    fn try_from(raw: RawCaseManifest) -> Result<Self, Self::Error> {
        validate_case_id(&raw.id)?;

        let rule_id = raw.rule;
        validate_rule_id(&rule_id)?;
        if section_for(&raw.id) != section_for(&rule_id) {
            return Err(ManifestError::Invalid(format!(
                "case `{}` and rule `{rule_id}` address different normative sections",
                raw.id
            )));
        }

        let profile = raw.profile.parse().map_err(|error| {
            ManifestError::Invalid(format!("{}: {error}", raw.profile))
        })?;

        let inputs = CaseInputs {
            source: raw.inputs.source,
            project: raw.inputs.project,
            data: raw.inputs.data,
            tree: raw.inputs.tree,
            host_world: raw.inputs.host_world,
        };
        let operation = decode_operation(raw.operation.as_deref(), profile, &inputs)?;

        let expectations = decode_expectations(raw.expect)?;
        if expectations.host_event.is_some() && !operation.executes_programs() {
            return Err(ManifestError::Invalid(format!(
                "host_event is valid only on interpret, workspace-run, and workspace-test cases, not {operation}"
            )));
        }
        if expectations.types.is_some()
            && (operation != ConformanceOperation::WorkspaceCheck
                || !expectations.accepted)
        {
            return Err(ManifestError::Invalid(format!(
                "a types snapshot is valid only on an accepted workspace-check case, not {operation}"
            )));
        }
        if operation == ConformanceOperation::Index && expectations.index.is_none() {
            return Err(ManifestError::Invalid(
                "index cases must declare the expected index document".to_owned(),
            ));
        }
        if operation == ConformanceOperation::Query && expectations.queries.is_empty() {
            return Err(ManifestError::Invalid(
                "query cases must declare at least one expected query".to_owned(),
            ));
        }
        if operation == ConformanceOperation::Format && expectations.formatted.is_none()
        {
            return Err(ManifestError::Invalid(
                "format cases must declare a formatted snapshot".to_owned(),
            ));
        }
        // A trap ends an accepted run with its runtime error diagnostic and no
        // result or trace: the run has none to record.
        let expects_trap = expectations.accepted
            && expectations.diagnostics.iter().any(|diagnostic| {
                diagnostic.level == Level::Error
                    && diagnostic.code.domain() == vibra_diagnostics::Domain::Runtime
            });
        if operation == ConformanceOperation::WorkspaceRun && expects_trap {
            if expectations.host_event.is_some() || expectations.interpreter.is_some() {
                return Err(ManifestError::Invalid(
                    "a workspace-run case that expects a trap declares neither a host event nor execution snapshots"
                        .to_owned(),
                ));
            }
        } else if operation == ConformanceOperation::WorkspaceRun
            && expectations.host_event.is_none()
        {
            if expectations.accepted
                && expectations.interpreter.as_ref().is_none_or(|execution| {
                    execution.result.is_none() || execution.audit_trace.is_none()
                })
            {
                return Err(ManifestError::Invalid(
                    "workspace-run cases require interpreter result and audit snapshots"
                        .to_owned(),
                ));
            }
            if expectations.interpreter.as_ref().is_some_and(|execution| {
                execution.result.is_none() || execution.audit_trace.is_none()
            }) {
                return Err(ManifestError::Invalid(
                    "workspace-run execution snapshots must include result and audit trace"
                        .to_owned(),
                ));
            }
        }
        if operation == ConformanceOperation::WorkspaceTest
            && expectations.host_event.is_none()
            && expectations.interpreter.as_ref().is_none_or(|execution| {
                execution.result.is_none() || execution.audit_trace.is_some()
            })
        {
            return Err(ManifestError::Invalid(
                "workspace-test cases require one result snapshot and no audit trace"
                    .to_owned(),
            ));
        }
        {
            for diagnostic in &expectations.diagnostics {
                if let Some(source_id) = &diagnostic.source_id
                    && !is_declared_input(&inputs, source_id)
                {
                    return Err(ManifestError::Invalid(format!(
                        "diagnostic source `{source_id}` is not declared by case `{}`",
                        raw.id
                    )));
                }
                for related in &diagnostic.related {
                    if let Some(source_id) = &related.source_id
                        && !is_declared_input(&inputs, source_id)
                    {
                        return Err(ManifestError::Invalid(format!(
                            "related diagnostic source `{source_id}` is not declared by case `{}`",
                            raw.id
                        )));
                    }
                }
            }
        }
        for query in &expectations.queries {
            if !is_declared_input(&inputs, &query.input) {
                return Err(ManifestError::Invalid(format!(
                    "query expectation input `{}` is not declared by case `{}`",
                    query.input, raw.id
                )));
            }
            if operation == ConformanceOperation::Query {
                let tree = inputs.tree.as_deref().unwrap_or_default();
                validate_query_input(tree, &query.input)?;
            }
            if let Some(unknown) = query
                .expand
                .iter()
                .find(|expansion| expansion.as_str() != "effect-witnesses")
            {
                return Err(ManifestError::Invalid(format!(
                    "query expectation expansion `{unknown}` is not `effect-witnesses` in case `{}`",
                    raw.id
                )));
            }
        }

        Ok(Self {
            id: raw.id,
            rule_id,
            profile,
            operation,
            description: raw.description,
            inputs,
            expectations,
        })
    }
}

fn decode_operation(
    raw: Option<&str>,
    profile: ConformanceProfile,
    inputs: &CaseInputs,
) -> Result<ConformanceOperation, ManifestError> {
    let input_kinds = usize::from(inputs.source.is_some())
        + usize::from(inputs.project.is_some())
        + usize::from(!inputs.data.is_empty());
    if raw.is_none() && profile != ConformanceProfile::ReaderV1 && input_kinds > 1 {
        return Err(ManifestError::Invalid(
            "an operation is required when a non-reader case has multiple input kinds"
                .to_owned(),
        ));
    }
    let operation = match raw {
        Some("reader") => ConformanceOperation::Reader,
        Some("project-decode") => ConformanceOperation::ProjectDecode,
        Some("source-graph") => ConformanceOperation::SourceGraph,
        Some("resolve") => ConformanceOperation::Resolve,
        Some("interpret") => ConformanceOperation::Interpret,
        Some("query") => ConformanceOperation::Query,
        Some("index") => ConformanceOperation::Index,
        Some("format") => ConformanceOperation::Format,
        Some("workspace-check") => ConformanceOperation::WorkspaceCheck,
        Some("workspace-run") => ConformanceOperation::WorkspaceRun,
        Some("workspace-test") => ConformanceOperation::WorkspaceTest,
        Some(value) => {
            return Err(ManifestError::Invalid(format!(
                "unknown conformance operation `{value}`"
            )));
        }
        None if profile != ConformanceProfile::ReaderV1
            && inputs.project.is_some()
            && inputs.tree.is_some() =>
        {
            ConformanceOperation::SourceGraph
        }
        None if profile != ConformanceProfile::ReaderV1 && inputs.project.is_some() => {
            ConformanceOperation::ProjectDecode
        }
        None => ConformanceOperation::Reader,
    };
    check_shape(operation, profile, inputs)?;
    if let Some(world) = inputs.host_world.as_deref() {
        validate_host_world_input(operation, world, inputs.tree.as_deref())?;
    }
    Ok(operation)
}

/// Whether an operation's case declares one kind of input.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Presence {
    Required,
    Forbidden,
    /// The operation does not look at it, and the case may declare it.
    Free,
}

/// The inputs an operation reads: its tree, project, source, and data. Where a
/// tree is required, the project beside it is exactly the tree's own
/// `project.vibon`.
const fn input_shape(
    operation: ConformanceOperation,
) -> Option<[(&'static str, Presence); 4]> {
    use Presence::{Forbidden, Free, Required};
    let [tree, project, source, data] = match operation {
        ConformanceOperation::Reader => return None,
        ConformanceOperation::ProjectDecode => {
            [Forbidden, Required, Forbidden, Forbidden]
        }
        ConformanceOperation::Interpret => {
            [Forbidden, Forbidden, Required, Forbidden]
        }
        ConformanceOperation::SourceGraph | ConformanceOperation::Resolve => {
            [Required, Required, Free, Free]
        }
        ConformanceOperation::Format => [Required, Required, Required, Forbidden],
        ConformanceOperation::Index
        | ConformanceOperation::Query
        | ConformanceOperation::WorkspaceCheck
        | ConformanceOperation::WorkspaceRun
        | ConformanceOperation::WorkspaceTest => {
            [Required, Required, Forbidden, Forbidden]
        }
    };
    Some([
        ("tree", tree),
        ("project", project),
        ("source", source),
        ("data", data),
    ])
}

/// The profile an operation runs under, for the operations that have one.
const fn required_profile(
    operation: ConformanceOperation,
) -> Option<ConformanceProfile> {
    match operation {
        ConformanceOperation::Format => Some(ConformanceProfile::ToolingV1),
        ConformanceOperation::WorkspaceCheck => Some(ConformanceProfile::StaticV1),
        ConformanceOperation::WorkspaceRun | ConformanceOperation::WorkspaceTest => {
            Some(ConformanceProfile::InterpreterV1)
        }
        _ => None,
    }
}

/// Holds a case's inputs to the shape of its operation.
fn check_shape(
    operation: ConformanceOperation,
    profile: ConformanceProfile,
    inputs: &CaseInputs,
) -> Result<(), ManifestError> {
    if let Some(required) = required_profile(operation)
        && profile != required
    {
        return Err(ManifestError::Invalid(format!(
            "{operation} cases require the {required} profile"
        )));
    }
    let Some(shape) = input_shape(operation) else {
        return Ok(());
    };
    let declared = [
        inputs.tree.is_some(),
        inputs.project.is_some(),
        inputs.source.is_some(),
        !inputs.data.is_empty(),
    ];
    for ((kind, presence), declared) in shape.into_iter().zip(declared) {
        match (presence, declared) {
            (Presence::Required, false) => {
                return Err(ManifestError::Invalid(format!(
                    "{operation} requires one {kind} input"
                )));
            }
            (Presence::Forbidden, true) => {
                return Err(ManifestError::Invalid(format!(
                    "{operation} cases may not declare a {kind} input"
                )));
            }
            _ => {}
        }
    }
    if let (Some(tree), Some(project)) = (inputs.tree.as_deref(), &inputs.project) {
        let expected_project = format!("{tree}/project.vibon");
        if *project != expected_project {
            return Err(ManifestError::Invalid(format!(
                "{operation} project input must be exactly `{expected_project}`"
            )));
        }
    }
    if operation == ConformanceOperation::Format
        && let (Some(tree), Some(source)) = (inputs.tree.as_deref(), &inputs.source)
    {
        validate_tree_source_input(tree, source, "format")?;
    }
    Ok(())
}

/// A `host-world` input belongs to `workspace-run` and `workspace-test`, which
/// run a program; `interpret` is pure and has no host world. It is a relative
/// `.vibon` file of the case directory, outside the confined tree, which is the
/// workspace and holds no world.
fn validate_host_world_input(
    operation: ConformanceOperation,
    world: &str,
    tree: Option<&str>,
) -> Result<(), ManifestError> {
    if !matches!(
        operation,
        ConformanceOperation::WorkspaceRun | ConformanceOperation::WorkspaceTest
    ) {
        return Err(ManifestError::Invalid(format!(
            "{operation} cases take no host-world input: only workspace-run and \
             workspace-test run in a host world"
        )));
    }
    let path = Path::new(world);
    if world.is_empty()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
        || path.extension().and_then(|extension| extension.to_str()) != Some("vibon")
    {
        return Err(ManifestError::Invalid(format!(
            "host-world input `{world}` must be a relative `.vibon` file of the case"
        )));
    }
    if tree.is_some_and(|tree| world.starts_with(&format!("{tree}/"))) {
        return Err(ManifestError::Invalid(format!(
            "host-world input `{world}` must not be inside the confined tree"
        )));
    }
    Ok(())
}

fn is_declared_input(inputs: &CaseInputs, source_id: &str) -> bool {
    inputs
        .source
        .as_deref()
        .is_some_and(|input| input == source_id)
        || inputs
            .project
            .as_deref()
            .is_some_and(|input| input == source_id)
        || inputs.data.iter().any(|input| input == source_id)
        || inputs
            .host_world
            .as_deref()
            .is_some_and(|input| input == source_id)
        || inputs.tree.as_deref().is_some_and(|tree| {
            source_id == tree
                || source_id
                    .strip_prefix(tree)
                    .is_some_and(|suffix| suffix.starts_with('/'))
                || (!source_id.is_empty()
                    && !source_id.starts_with('/')
                    && !source_id.contains(".."))
        })
}

fn validate_query_input(tree: &str, input: &str) -> Result<(), ManifestError> {
    validate_tree_source_input(tree, input, "query")
}

fn validate_tree_source_input(
    tree: &str,
    input: &str,
    operation: &str,
) -> Result<(), ManifestError> {
    let prefix = format!("{tree}/");
    let Some(source_id) = input.strip_prefix(&prefix) else {
        return Err(ManifestError::Invalid(format!(
            "{operation} input `{input}` must be beneath the declared tree `{tree}`"
        )));
    };
    let path = Path::new(source_id);
    if source_id.is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
        || path.extension().and_then(|extension| extension.to_str()) != Some("vib")
    {
        return Err(ManifestError::Invalid(format!(
            "{operation} input `{input}` must be a relative `.vib` file beneath `{tree}`"
        )));
    }
    Ok(())
}

fn validate_case_id(id: &str) -> Result<(), ManifestError> {
    if id.is_empty() || id.contains(['/', '\\']) || id.chars().any(char::is_whitespace)
    {
        return Err(ManifestError::Invalid(format!(
            "case id `{id}` must be a non-empty path-free identifier"
        )));
    }
    if section_for(id).is_none() {
        return Err(ManifestError::Invalid(format!(
            "case id `{id}` does not begin with a known normative section"
        )));
    }
    Ok(())
}

fn validate_rule_id(rule_id: &str) -> Result<(), ManifestError> {
    if rule_id.is_empty() || section_for(rule_id).is_none() {
        return Err(ManifestError::Invalid(format!(
            "rule `{rule_id}` does not begin with a known normative section"
        )));
    }
    Ok(())
}

/// Returns the longest matching section prefix, because several sections share
/// the `V1-SRC-` stem.
pub(crate) fn section_for(value: &str) -> Option<&str> {
    NORMATIVE_SECTION_IDS
        .iter()
        .copied()
        .filter(|section| {
            value == *section || value.starts_with(&format!("{section}-"))
        })
        .max_by_key(|section| section.len())
}

/// The closed host events a case may end in. A host event is an unlocated
/// error diagnostic that is never a trap (`docs/spec/06-runtime.md`,
/// "Activations and memory"); memory exhaustion is the only one.
const HOST_EVENTS: [&str; 1] = ["@runtime.memory-exhausted"];

fn decode_expectations(
    raw: RawExpectations,
) -> Result<CaseExpectations, ManifestError> {
    let diagnostics = raw
        .diagnostics
        .into_iter()
        .map(decode_diagnostic)
        .collect::<Result<Vec<_>, _>>()?;

    let accepted = raw.accepted;

    // A trap happens while an accepted program runs, so its diagnostic is
    // the one error an accepted case may expect.
    if accepted
        && diagnostics.iter().any(|diagnostic| {
            diagnostic.level == Level::Error
                && diagnostic.code.domain() != vibra_diagnostics::Domain::Runtime
        })
    {
        return Err(ManifestError::Invalid(
            "an accepted case cannot expect an error diagnostic".to_owned(),
        ));
    }
    if !accepted
        && !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.level == Level::Error)
    {
        return Err(ManifestError::Invalid(
            "a rejected case must expect at least one @error diagnostic".to_owned(),
        ));
    }

    let formatted = raw.formatted;
    if let Some(graph) = raw.graph.as_deref()
        && Path::new(graph)
            .extension()
            .and_then(|extension| extension.to_str())
            != Some("vibon")
    {
        return Err(ManifestError::Invalid(
            "source-graph snapshots must use the .vibon extension".to_owned(),
        ));
    }
    let interpreter = raw.interpreter.map(decode_execution).transpose()?;
    let host_event = raw.host_event;
    if let Some(event) = &host_event {
        if !HOST_EVENTS.contains(&event.as_str()) {
            return Err(ManifestError::Invalid(format!(
                "host_event `{event}` is not a registered host event; expected one of {HOST_EVENTS:?}"
            )));
        }
        if interpreter.is_some() {
            return Err(ManifestError::Invalid(
                "host_event replaces the result and audit-trace snapshots; a case cannot declare both"
                    .to_owned(),
            ));
        }
        if !accepted {
            return Err(ManifestError::Invalid(
                "host_event requires an accepted case: a program the checker rejects never runs"
                    .to_owned(),
            ));
        }
    }
    let artifact_hashes = raw.artifact.map(|artifact| artifact.hashes);

    Ok(CaseExpectations {
        accepted,
        diagnostics,
        formatted,
        graph: raw.graph,
        resolved: raw.resolved,
        types: raw.types,
        effects: raw.effects,
        index: raw.index,
        queries: raw
            .queries
            .into_iter()
            .map(|query| ExpectedQuery {
                input: query.input,
                offset: query.offset,
                snapshot: query.snapshot,
                expand: query.expand,
            })
            .collect(),
        interpreter,
        host_event,
        artifact_hashes,
    })
}

fn decode_execution(raw: RawExecution) -> Result<ExpectedExecution, ManifestError> {
    if raw.audit_trace.as_deref().is_some_and(|audit_trace| {
        Path::new(audit_trace)
            .extension()
            .and_then(|extension| extension.to_str())
            != Some("vibon")
    }) {
        return Err(ManifestError::Invalid(
            "audit-trace snapshots must use the .vibon extension".to_owned(),
        ));
    }

    Ok(ExpectedExecution {
        result: raw.result,
        audit_trace: raw.audit_trace,
    })
}

fn decode_diagnostic(
    raw: RawExpectedDiagnostic,
) -> Result<ExpectedDiagnostic, ManifestError> {
    let code = raw.code.parse::<DiagnosticCode>().map_err(|_| {
        ManifestError::Invalid(format!(
            "expected diagnostic `{}` is not in the closed v1 registry",
            raw.code
        ))
    })?;
    let level = parse_level(&raw.level)?;
    if code.level() != level {
        return Err(ManifestError::Invalid(format!(
            "expected diagnostic {} declares {}, but the registry fixes it at {}",
            code.as_atom(),
            level.as_atom(),
            code.level().as_atom()
        )));
    }
    if raw.fixes.is_some()
        && code.fix_capability() != vibra_diagnostics::FixCapability::Safe
    {
        return Err(ManifestError::Invalid(format!(
            "expected diagnostic {} declares fixes, but the registry marks it {}",
            code.as_atom(),
            code.fix_capability().as_atom()
        )));
    }

    let primary_span = decode_span(raw.span, code.as_atom())?;

    let related = raw
        .related
        .into_iter()
        .map(|related| {
            Ok(ExpectedRelatedSpan {
                source_id: related.source,
                span: decode_span(related.span, code.as_atom())?,
                message: related.message,
            })
        })
        .collect::<Result<Vec<_>, ManifestError>>()?;

    let fixes = raw.fixes.map(|fixes| {
        fixes
            .into_iter()
            .map(|fix| ExpectedFix {
                safe: fix.safe,
                description: fix.description,
                revision: fix.revision,
            })
            .collect()
    });

    Ok(ExpectedDiagnostic {
        code,
        level,
        message: raw.message,
        source_id: raw.source,
        primary_span,
        related,
        notes: raw.notes,
        fixes,
    })
}

fn decode_span(raw: RawSpan, code: &str) -> Result<ByteSpan, ManifestError> {
    if raw.end < raw.start {
        return Err(ManifestError::Invalid(format!(
            "expected diagnostic {code} has an inverted span {}..{}",
            raw.start, raw.end
        )));
    }
    Ok(ByteSpan::new(raw.start, raw.end))
}

fn parse_level(value: &str) -> Result<Level, ManifestError> {
    match value {
        "@error" => Ok(Level::Error),
        "@warning" => Ok(Level::Warning),
        _ => Err(ManifestError::Invalid(format!(
            "unknown diagnostic level `{value}`; use `@error` or `@warning`"
        ))),
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawCaseManifest {
    pub(crate) id: String,
    pub(crate) rule: String,
    pub(crate) profile: String,
    #[serde(default)]
    pub(crate) operation: Option<String>,
    #[serde(default)]
    pub(crate) description: Option<String>,
    #[serde(default)]
    pub(crate) inputs: RawInputs,
    pub(crate) expect: RawExpectations,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawInputs {
    #[serde(default)]
    pub(crate) source: Option<String>,
    #[serde(default)]
    pub(crate) project: Option<String>,
    #[serde(default)]
    pub(crate) data: Vec<String>,
    #[serde(default)]
    pub(crate) tree: Option<String>,
    #[serde(default, rename = "host-world")]
    pub(crate) host_world: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawExpectations {
    pub(crate) accepted: bool,
    #[serde(default)]
    pub(crate) diagnostics: Vec<RawExpectedDiagnostic>,
    #[serde(default)]
    pub(crate) formatted: Option<String>,
    #[serde(default)]
    pub(crate) graph: Option<String>,
    #[serde(default)]
    pub(crate) resolved: Option<String>,
    #[serde(default)]
    pub(crate) types: Option<String>,
    #[serde(default)]
    pub(crate) effects: Option<String>,
    #[serde(default)]
    pub(crate) index: Option<String>,
    #[serde(default)]
    pub(crate) queries: Vec<RawQuery>,
    #[serde(default)]
    pub(crate) interpreter: Option<RawExecution>,
    #[serde(default)]
    pub(crate) host_event: Option<String>,
    #[serde(default)]
    pub(crate) artifact: Option<RawArtifact>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawExpectedDiagnostic {
    pub(crate) code: String,
    pub(crate) level: String,
    #[serde(default)]
    pub(crate) message: Option<String>,
    #[serde(default, alias = "sourceId")]
    pub(crate) source: Option<String>,
    pub(crate) span: RawSpan,
    #[serde(default)]
    pub(crate) related: Vec<RawRelatedSpan>,
    #[serde(default)]
    pub(crate) notes: Vec<String>,
    #[serde(default)]
    pub(crate) fixes: Option<Vec<RawFix>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawRelatedSpan {
    #[serde(default, alias = "sourceId")]
    pub(crate) source: Option<String>,
    pub(crate) span: RawSpan,
    #[serde(default)]
    pub(crate) message: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawSpan {
    pub(crate) start: usize,
    pub(crate) end: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawFix {
    #[serde(default)]
    pub(crate) safe: bool,
    #[serde(default)]
    pub(crate) description: Option<String>,
    #[serde(default)]
    pub(crate) revision: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawExecution {
    #[serde(default)]
    pub(crate) result: Option<String>,
    #[serde(default)]
    pub(crate) audit_trace: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawArtifact {
    #[serde(default)]
    pub(crate) hashes: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawQuery {
    pub(crate) input: String,
    pub(crate) offset: usize,
    pub(crate) snapshot: String,
    #[serde(default)]
    pub(crate) expand: Vec<String>,
}
