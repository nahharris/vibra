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
}

/// One expected source span attached to a diagnostic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpectedRelatedSpan {
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
    /// A relative path to the expected ordered audit-trace snapshot.
    pub audit_trace: Option<String>,
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
    /// A relative path to resolved-identity output.
    pub resolved: Option<String>,
    /// A relative path to type output.
    pub types: Option<String>,
    /// A relative path to effect output.
    pub effects: Option<String>,
    /// Expected reference-interpreter output.
    pub interpreter: Option<ExpectedExecution>,
    /// Expected Wasm output.
    pub wasm: Option<ExpectedExecution>,
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
    /// Optional maintainer-facing description.
    pub description: Option<String>,
    /// Case inputs.
    pub inputs: CaseInputs,
    /// Expected observations.
    pub expectations: CaseExpectations,
}

impl CaseManifest {
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
        };

        let expectations = decode_expectations(raw.expect)?;

        Ok(Self {
            id: raw.id,
            rule_id,
            profile,
            description: raw.description,
            inputs,
            expectations,
        })
    }
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

fn decode_expectations(
    raw: RawExpectations,
) -> Result<CaseExpectations, ManifestError> {
    let diagnostics = raw
        .diagnostics
        .into_iter()
        .map(decode_diagnostic)
        .collect::<Result<Vec<_>, _>>()?;

    let accepted = raw.accepted;

    if accepted
        && diagnostics
            .iter()
            .any(|diagnostic| diagnostic.level == Level::Error)
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
    let interpreter = raw.interpreter.map(decode_execution);
    let wasm = raw.wasm.map(decode_execution);
    let artifact_hashes = raw.artifact.map(|artifact| artifact.hashes);

    Ok(CaseExpectations {
        accepted,
        diagnostics,
        formatted,
        resolved: raw.resolved,
        types: raw.types,
        effects: raw.effects,
        interpreter,
        wasm,
        artifact_hashes,
    })
}

fn decode_execution(raw: RawExecution) -> ExpectedExecution {
    ExpectedExecution {
        result: raw.result,
        audit_trace: raw.audit_trace,
    }
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
    pub(crate) resolved: Option<String>,
    #[serde(default)]
    pub(crate) types: Option<String>,
    #[serde(default)]
    pub(crate) effects: Option<String>,
    #[serde(default)]
    pub(crate) interpreter: Option<RawExecution>,
    #[serde(default)]
    pub(crate) wasm: Option<RawExecution>,
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
