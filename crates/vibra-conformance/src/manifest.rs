//! The neutral `case.toml` manifest.
//!
//! The manifest is deliberately not VIBON. It is the oracle used to test the
//! VIBON decoder, so decoding expectations through the language's data
//! grammar would make the harness unable to catch decoder defects.
//!
//! Every table of the manifest is closed: an unknown key anywhere is an
//! error, so a misspelt expectation can never be silently ignored.

use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;

use serde::Deserialize;
use vibra_diagnostics::{ByteSpan, DiagnosticCode, FixCapability, Level};

use crate::profile::{ConformanceOperation, ConformanceProfile};
use crate::rules::{is_suffix, parse_identity};

/// The file name every case uses.
pub const MANIFEST_FILE_NAME: &str = "case.toml";

/// The expected or observed verdict of a case.
///
/// Only [`Verdict::Accepted`] is a pass of the subject; the case itself passes
/// when the observed verdict and every other expectation match.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Verdict {
    /// The subject is admitted.
    Accepted,
    /// The subject is disproved or rejected.
    Refuted,
    /// The result is not established.
    Unknown,
    /// The subject is outside the supported profile.
    Unsupported,
}

impl Verdict {
    /// Every verdict.
    pub const ALL: &'static [Self] = &[
        Self::Accepted,
        Self::Refuted,
        Self::Unknown,
        Self::Unsupported,
    ];

    /// The spelling used by `case.toml`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Refuted => "refuted",
            Self::Unknown => "unknown",
            Self::Unsupported => "unsupported",
        }
    }
}

impl fmt::Display for Verdict {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A manifest decoding or validation failure.
#[derive(Debug)]
pub enum ManifestError {
    /// The TOML document could not be decoded, or it has an unknown key.
    Toml(toml::de::Error),
    /// The decoded document violates the case contract.
    Invalid(String),
    /// A manifest file could not be read.
    Io {
        /// The path that was read.
        path: PathBuf,
        /// The underlying failure.
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

/// The case-relative input paths of a case.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CaseInputs {
    /// A source document.
    pub source: Option<String>,
    /// A project document.
    pub project: Option<String>,
    /// Data documents.
    pub data: Vec<String>,
    /// A confined directory tree.
    pub tree: Option<String>,
    /// For `workspaceRun` and `workspaceTest`, a `@hostWorld.v2` document of
    /// the case directory, outside the tree.
    pub host_world: Option<String>,
}

/// One expected related span of a diagnostic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpectedRelatedSpan {
    /// The input path owning the span, when stated.
    pub source_id: Option<String>,
    /// The half-open UTF-8 byte span.
    pub span: ByteSpan,
}

/// One expected diagnostic, in emission order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpectedDiagnostic {
    /// The registry code.
    pub code: DiagnosticCode,
    /// The level, which the manifest states and the registry fixes.
    pub level: Level,
    /// The input path owning the primary span, when stated.
    pub source_id: Option<String>,
    /// The primary half-open UTF-8 byte span.
    pub primary_span: ByteSpan,
    /// Related spans, in order.
    pub related: Vec<ExpectedRelatedSpan>,
    /// Notes, asserted when the case states any.
    pub notes: Vec<String>,
    /// Fixes, in order, asserted when the case states the key. A code whose
    /// fix capability is `@none` offers no fix, so its list is empty.
    pub fixes: Option<Vec<ExpectedFix>>,
}

/// One expected fix of a diagnostic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpectedFix {
    /// Whether the fix may be applied automatically.
    pub safe: bool,
    /// The description, asserted when stated.
    pub description: Option<String>,
    /// The document revision the fix was computed against, asserted when
    /// stated.
    pub expected_revision: Option<String>,
}

/// The expected observations of a case. Every snapshot is a case-relative
/// path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaseExpectations {
    /// The expected verdict.
    pub verdict: Verdict,
    /// Diagnostics, in emission order.
    pub diagnostics: Vec<ExpectedDiagnostic>,
    /// The `@tree.v2` snapshot.
    pub tree: Option<String>,
    /// The formatted text.
    pub formatted: Option<String>,
    /// The `@sourceGraph.v1` snapshot.
    pub graph: Option<String>,
    /// The `@resolved.v2` snapshot.
    pub resolved: Option<String>,
    /// The `@types.v2` snapshot.
    pub types: Option<String>,
    /// The `@value.v2` snapshot of an executed result.
    pub result: Option<String>,
    /// The `@testRun.v2` snapshot.
    pub test_run: Option<String>,
    /// The atom of the host event the execution ends with, such as
    /// `@runtime.memoryExhausted`.
    pub host_event: Option<DiagnosticCode>,
}

impl CaseExpectations {
    /// Every snapshot the case declares, with its key.
    #[must_use]
    pub fn snapshots(&self) -> Vec<(&'static str, &str)> {
        [
            ("tree", self.tree.as_deref()),
            ("formatted", self.formatted.as_deref()),
            ("graph", self.graph.as_deref()),
            ("resolved", self.resolved.as_deref()),
            ("types", self.types.as_deref()),
            ("result", self.result.as_deref()),
            ("testRun", self.test_run.as_deref()),
        ]
        .into_iter()
        .filter_map(|(key, path)| path.map(|path| (key, path)))
        .collect()
    }
}

/// The decoded, validated manifest of one case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaseManifest {
    /// The case identity: the rule identity, or it followed by `-` and a
    /// further suffix.
    pub id: String,
    /// The rule identity the case addresses.
    pub rule: String,
    /// The profile the case requires.
    pub profile: ConformanceProfile,
    /// The operation the case selects.
    pub operation: ConformanceOperation,
    /// The description, citing the specification anchor.
    pub description: Option<String>,
    /// The inputs.
    pub inputs: CaseInputs,
    /// The expected observations.
    pub expectations: CaseExpectations,
}

impl CaseManifest {
    /// Decodes and validates a manifest.
    ///
    /// The rule index is not consulted here; [`crate::Corpus::discover`]
    /// checks that the rule is active.
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

    /// The section prefix of the case's identity, such as `SRC-READER`.
    #[must_use]
    pub fn prefix(&self) -> &str {
        parse_identity(&self.rule).map_or("", |parts| parts.prefix)
    }

    /// Whether this is an executable case: an accepted case whose operation
    /// executes a program ("Differential execution").
    #[must_use]
    pub fn is_executable(&self) -> bool {
        self.operation.executes_programs()
            && self.expectations.verdict == Verdict::Accepted
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
        parse_identity(&raw.rule)
            .map_err(|error| ManifestError::Invalid(format!("rule: {error}")))?;
        validate_case_id(&raw.id, &raw.rule)?;
        let profile = raw
            .profile
            .parse::<ConformanceProfile>()
            .map_err(|error| ManifestError::Invalid(error.to_string()))?;
        let operation = raw
            .operation
            .parse::<ConformanceOperation>()
            .map_err(|error| ManifestError::Invalid(error.to_string()))?;

        let inputs = CaseInputs {
            source: raw.inputs.source,
            project: raw.inputs.project,
            data: raw.inputs.data,
            tree: raw.inputs.tree,
            host_world: raw.inputs.host_world,
        };
        check_shape(operation, &inputs)?;
        if let Some(world) = inputs.host_world.as_deref() {
            validate_host_world(operation, world, inputs.tree.as_deref())?;
        }

        let expectations = decode_expectations(raw.expect)?;
        for diagnostic in &expectations.diagnostics {
            let sources = std::iter::once(diagnostic.source_id.as_deref()).chain(
                diagnostic
                    .related
                    .iter()
                    .map(|related| related.source_id.as_deref()),
            );
            for source_id in sources.flatten() {
                if !is_declared_input(&inputs, source_id) {
                    return Err(ManifestError::Invalid(format!(
                        "diagnostic source `{source_id}` is not an input of case `{}`",
                        raw.id
                    )));
                }
            }
        }

        Ok(Self {
            id: raw.id,
            rule: raw.rule,
            profile,
            operation,
            description: raw.description,
            inputs,
            expectations,
        })
    }
}

/// A case identity is its rule identity, or the rule identity followed by
/// `-` and a further suffix.
fn validate_case_id(id: &str, rule: &str) -> Result<(), ManifestError> {
    if id == rule {
        return Ok(());
    }
    let further = id
        .strip_prefix(rule)
        .and_then(|rest| rest.strip_prefix('-'));
    if further.is_some_and(is_suffix) {
        return Ok(());
    }
    Err(ManifestError::Invalid(format!(
        "case `{id}` is neither its rule `{rule}` nor the rule followed by `-` and a \
         suffix of lower-case letters, digits, and hyphens"
    )))
}

/// Whether an operation's case declares one kind of input.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Presence {
    Required,
    Forbidden,
    /// The operation does not look at it, and the case may declare it.
    Free,
}

/// The inputs an operation reads, per "Case operations and inputs": its tree,
/// project, source, and data. Where a tree is required, the project beside
/// it is exactly the tree's own `project.vibon`.
const fn input_shape(operation: ConformanceOperation) -> [(&'static str, Presence); 4] {
    use Presence::{Forbidden, Free, Required};
    let [tree, project, source, data] = match operation {
        // One document, of either grammar; checked below.
        ConformanceOperation::Reader => [Forbidden, Forbidden, Free, Free],
        ConformanceOperation::ProjectDecode => {
            [Forbidden, Required, Forbidden, Forbidden]
        }
        ConformanceOperation::Interpret => [Forbidden, Forbidden, Required, Forbidden],
        ConformanceOperation::SourceGraph | ConformanceOperation::Resolve => {
            [Required, Required, Free, Free]
        }
        ConformanceOperation::Format => [Required, Required, Required, Forbidden],
        ConformanceOperation::WorkspaceCheck
        | ConformanceOperation::Query
        | ConformanceOperation::WorkspaceRun
        | ConformanceOperation::WorkspaceTest => {
            [Required, Required, Forbidden, Forbidden]
        }
    };
    [
        ("tree", tree),
        ("project", project),
        ("source", source),
        ("data", data),
    ]
}

/// Holds a case's inputs to the shape of its operation.
fn check_shape(
    operation: ConformanceOperation,
    inputs: &CaseInputs,
) -> Result<(), ManifestError> {
    let declared = [
        inputs.tree.is_some(),
        inputs.project.is_some(),
        inputs.source.is_some(),
        !inputs.data.is_empty(),
    ];
    for ((kind, presence), declared) in input_shape(operation).into_iter().zip(declared)
    {
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
    if operation == ConformanceOperation::Reader
        && usize::from(inputs.source.is_some()) + inputs.data.len() != 1
    {
        return Err(ManifestError::Invalid(
            "reader cases declare exactly one document, as `source` or as one `data` entry"
                .to_owned(),
        ));
    }
    if let (Some(tree), Some(project)) = (inputs.tree.as_deref(), &inputs.project) {
        let expected = format!("{tree}/project.vibon");
        if *project != expected {
            return Err(ManifestError::Invalid(format!(
                "{operation} project input must be exactly `{expected}`"
            )));
        }
    }
    if operation == ConformanceOperation::Format
        && let (Some(tree), Some(source)) = (inputs.tree.as_deref(), &inputs.source)
        && !source
            .strip_prefix(tree)
            .is_some_and(|rest| rest.starts_with('/') && rest.len() > 1)
    {
        return Err(ManifestError::Invalid(format!(
            "format input `{source}` must be beneath the declared tree `{tree}`"
        )));
    }
    Ok(())
}

/// A host world belongs to `workspaceRun` and `workspaceTest`, which run in a
/// world; it is a relative file of the case directory, outside the tree.
fn validate_host_world(
    operation: ConformanceOperation,
    world: &str,
    tree: Option<&str>,
) -> Result<(), ManifestError> {
    if !matches!(
        operation,
        ConformanceOperation::WorkspaceRun | ConformanceOperation::WorkspaceTest
    ) {
        return Err(ManifestError::Invalid(format!(
            "{operation} cases take no hostWorld input: only workspaceRun and \
             workspaceTest run in a host world"
        )));
    }
    let path = Path::new(world);
    if world.is_empty()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(ManifestError::Invalid(format!(
            "hostWorld input `{world}` must be a relative file of the case"
        )));
    }
    if tree.is_some_and(|tree| world.starts_with(&format!("{tree}/"))) {
        return Err(ManifestError::Invalid(format!(
            "hostWorld input `{world}` must not be inside the confined tree"
        )));
    }
    Ok(())
}

fn is_declared_input(inputs: &CaseInputs, source_id: &str) -> bool {
    inputs.source.as_deref() == Some(source_id)
        || inputs.project.as_deref() == Some(source_id)
        || inputs.data.iter().any(|input| input == source_id)
        || inputs.host_world.as_deref() == Some(source_id)
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

fn decode_expectations(
    raw: RawExpectations,
) -> Result<CaseExpectations, ManifestError> {
    let verdict = Verdict::ALL
        .iter()
        .copied()
        .find(|verdict| verdict.as_str() == raw.verdict)
        .ok_or_else(|| {
            ManifestError::Invalid(format!(
                "unknown verdict `{}`; use `accepted`, `refuted`, `unknown`, or `unsupported`",
                raw.verdict
            ))
        })?;
    let diagnostics = raw
        .diagnostics
        .into_iter()
        .map(decode_diagnostic)
        .collect::<Result<Vec<_>, _>>()?;
    let host_event = raw
        .host_event
        .map(|atom| {
            atom.parse::<DiagnosticCode>().map_err(|_| {
                ManifestError::Invalid(format!(
                    "expected host event `{atom}` is not in the registry"
                ))
            })
        })
        .transpose()?;
    Ok(CaseExpectations {
        verdict,
        diagnostics,
        tree: raw.tree,
        formatted: raw.formatted,
        graph: raw.graph,
        resolved: raw.resolved,
        types: raw.types,
        result: raw.result,
        test_run: raw.test_run,
        host_event,
    })
}

fn decode_diagnostic(
    raw: RawExpectedDiagnostic,
) -> Result<ExpectedDiagnostic, ManifestError> {
    let code = raw.code.parse::<DiagnosticCode>().map_err(|_| {
        ManifestError::Invalid(format!(
            "expected diagnostic `{}` is not in the registry",
            raw.code
        ))
    })?;
    let level = match raw.level.as_str() {
        "@error" => Level::Error,
        "@warning" => Level::Warning,
        other => {
            return Err(ManifestError::Invalid(format!(
                "unknown diagnostic level `{other}`; use `@error` or `@warning`"
            )));
        }
    };
    if code.level() != level {
        return Err(ManifestError::Invalid(format!(
            "expected diagnostic {code} declares {level}, but the registry fixes it at {}",
            code.level()
        )));
    }
    if code.fix_capability() == FixCapability::None
        && raw.fixes.as_ref().is_some_and(|fixes| !fixes.is_empty())
    {
        return Err(ManifestError::Invalid(format!(
            "expected diagnostic {code} declares a fix, but the registry gives it fix \
             capability {}",
            code.fix_capability()
        )));
    }
    let fixes = raw.fixes.map(|fixes| {
        fixes
            .into_iter()
            .map(|fix| ExpectedFix {
                safe: fix.safe,
                description: fix.description,
                expected_revision: fix.expected_revision,
            })
            .collect()
    });
    let primary_span = decode_span(&raw.span, code)?;
    let related = raw
        .related
        .into_iter()
        .map(|related| {
            Ok(ExpectedRelatedSpan {
                source_id: related.source,
                span: decode_span(&related.span, code)?,
            })
        })
        .collect::<Result<Vec<_>, ManifestError>>()?;
    Ok(ExpectedDiagnostic {
        code,
        level,
        source_id: raw.source,
        primary_span,
        related,
        notes: raw.notes,
        fixes,
    })
}

fn decode_span(raw: &RawSpan, code: DiagnosticCode) -> Result<ByteSpan, ManifestError> {
    if raw.end < raw.start {
        return Err(ManifestError::Invalid(format!(
            "expected diagnostic {code} has an inverted span {}..{}",
            raw.start, raw.end
        )));
    }
    Ok(ByteSpan::new(raw.start, raw.end))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCaseManifest {
    id: String,
    rule: String,
    profile: String,
    operation: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    inputs: RawInputs,
    expect: RawExpectations,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RawInputs {
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    project: Option<String>,
    #[serde(default)]
    data: Vec<String>,
    #[serde(default)]
    tree: Option<String>,
    #[serde(default)]
    host_world: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RawExpectations {
    verdict: String,
    #[serde(default)]
    diagnostics: Vec<RawExpectedDiagnostic>,
    #[serde(default)]
    tree: Option<String>,
    #[serde(default)]
    formatted: Option<String>,
    #[serde(default)]
    graph: Option<String>,
    #[serde(default)]
    resolved: Option<String>,
    #[serde(default)]
    types: Option<String>,
    #[serde(default)]
    result: Option<String>,
    #[serde(default)]
    test_run: Option<String>,
    /// Spelled as `docs/spec/07-diagnostics-and-conformance.md` writes it.
    #[serde(default, rename = "host_event")]
    host_event: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawExpectedDiagnostic {
    code: String,
    level: String,
    #[serde(default)]
    source: Option<String>,
    span: RawSpan,
    #[serde(default)]
    related: Vec<RawRelatedSpan>,
    #[serde(default)]
    notes: Vec<String>,
    #[serde(default)]
    fixes: Option<Vec<RawFix>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RawFix {
    safe: bool,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    expected_revision: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRelatedSpan {
    #[serde(default)]
    source: Option<String>,
    span: RawSpan,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSpan {
    start: usize,
    end: usize,
}
