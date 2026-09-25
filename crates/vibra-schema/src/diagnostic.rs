//! The JSON contract for diagnostics and registry entries.
//!
//! These types are the wire format, not the language model. They exist so a
//! tool can read a diagnostic without reimplementing the compiler, and they
//! are the only place a schema version appears.
//!
//! Every type rejects unknown fields when read. `docs/spec/05-tooling.md`
//! permits ignoring an unknown field only where an output schema explicitly
//! allows forward extension, and neither of these does.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode, LineIndex, Position};

/// A diagnostic could not be rendered because a related span names a source
/// document whose line index was not supplied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiagnosticRenderError {
    /// A cross-document related span has no index from which to derive its
    /// display positions.
    MissingSourceIndex(String),
}

impl std::fmt::Display for DiagnosticRenderError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingSourceIndex(source_id) => write!(
                formatter,
                "no line index was supplied for related source document {source_id:?}"
            ),
        }
    }
}

impl std::error::Error for DiagnosticRenderError {}

/// The major version of the diagnostic contracts in this module.
///
/// Schema identifiers and major versions are contracts. A reader must reject a
/// newer major version it cannot interpret rather than guessing.
pub const SCHEMA_VERSION: u32 = 1;

/// The published JSON Schema for [`DiagnosticDocument`].
pub const DIAGNOSTIC_SCHEMA: &str = include_str!("../schemas/v1/diagnostic.json");

/// The published JSON Schema for [`RegistryEntryDocument`].
pub const REGISTRY_ENTRY_SCHEMA: &str =
    include_str!("../schemas/v1/diagnostic-registry-entry.json");

/// A one-based display position.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PositionDocument {
    /// One-based line number.
    pub line: usize,
    /// One-based column, counted in Unicode scalar values.
    pub column: usize,
}

impl From<Position> for PositionDocument {
    fn from(position: Position) -> Self {
        Self {
            line: position.line,
            column: position.column,
        }
    }
}

/// A half-open byte range with its endpoints as display positions.
///
/// Both representations are carried because the specification stores byte
/// ranges and treats line and column as derived. A consumer that edits text
/// needs the bytes; a consumer that shows text to a person needs the
/// positions. Deriving one from the other requires the document, which a JSON
/// consumer may not have.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpanDocument {
    /// The case-relative or workspace-relative source identity owning the span.
    pub source_id: Option<String>,
    /// First byte offset.
    pub start: usize,
    /// One past the last byte offset.
    pub end: usize,
    /// Display position of [`Self::start`].
    pub start_position: PositionDocument,
    /// Display position of [`Self::end`].
    pub end_position: PositionDocument,
}

impl SpanDocument {
    /// Renders `span` against the document `index` was built from.
    #[must_use]
    pub fn render(span: ByteSpan, index: &LineIndex<'_>) -> Self {
        Self::render_with_source(span, index, None)
    }

    /// Renders `span` while retaining its owning source identity.
    #[must_use]
    pub fn render_with_source(
        span: ByteSpan,
        index: &LineIndex<'_>,
        source_id: Option<&str>,
    ) -> Self {
        Self {
            source_id: source_id.map(str::to_owned),
            start: span.start(),
            end: span.end(),
            start_position: index.position(span.start()).into(),
            end_position: index.position(span.end()).into(),
        }
    }
}

/// An additional span that explains a diagnostic.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RelatedSpanDocument {
    /// Where the related construct is.
    pub span: SpanDocument,
    /// What that construct contributes.
    pub message: String,
}

/// A fix offered for a diagnostic.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FixDocument {
    /// What the fix does.
    pub description: String,
    /// Whether it may be applied without review.
    pub safe: bool,
    /// The document revision it was computed against.
    pub expected_revision: String,
}

/// One diagnostic as sent to a tool.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiagnosticDocument {
    /// Major version of this contract.
    pub schema_version: u32,
    /// The exact atom spelling of the code.
    pub code: String,
    /// The exact atom spelling of the registered level.
    pub level: String,
    /// Human-facing text.
    pub message: String,
    /// Where the diagnostic is reported.
    pub primary_span: SpanDocument,
    /// Additional spans that explain it.
    pub related: Vec<RelatedSpanDocument>,
    /// Additional explanation carrying no span.
    pub notes: Vec<String>,
    /// Fixes offered for it.
    pub fixes: Vec<FixDocument>,
}

impl DiagnosticDocument {
    /// Renders `diagnostic` against the document `index` was built from.
    pub fn render(
        diagnostic: &Diagnostic,
        index: &LineIndex<'_>,
    ) -> Result<Self, DiagnosticRenderError> {
        Self::render_with_source_indexes(diagnostic, index, &BTreeMap::new())
    }

    /// Renders a diagnostic with indexes for related source documents.
    ///
    /// `primary_index` owns the primary span's document. When a related span
    /// carries a source identity present in `source_indexes`, its display
    /// positions are derived from that document instead of the primary one.
    /// A related span with the same source identity as the primary span uses
    /// `primary_index`. A related span naming a different source requires an
    /// entry in `source_indexes`; silently deriving its position from another
    /// document would publish a false location.
    pub fn render_with_source_indexes<'a>(
        diagnostic: &Diagnostic,
        primary_index: &LineIndex<'a>,
        source_indexes: &BTreeMap<String, LineIndex<'a>>,
    ) -> Result<Self, DiagnosticRenderError> {
        let related = diagnostic
            .related()
            .iter()
            .map(|related| {
                // A related span may point into another document. Use its own
                // line index whenever the caller supplied one. A related span
                // in the primary document is safe to render from the primary
                // index even when the caller did not populate the map.
                let index = match related.source_id.as_deref() {
                    None => primary_index,
                    Some(source_id) if diagnostic.source_id() == Some(source_id) => {
                        primary_index
                    }
                    Some(source_id) => {
                        source_indexes.get(source_id).ok_or_else(|| {
                            DiagnosticRenderError::MissingSourceIndex(
                                source_id.to_owned(),
                            )
                        })?
                    }
                };
                Ok(RelatedSpanDocument {
                    span: SpanDocument::render_with_source(
                        related.span,
                        index,
                        related.source_id.as_deref(),
                    ),
                    message: related.message.clone(),
                })
            })
            .collect::<Result<Vec<_>, DiagnosticRenderError>>()?;

        Ok(Self {
            schema_version: SCHEMA_VERSION,
            code: diagnostic.code().as_atom().to_owned(),
            level: diagnostic.level().as_atom().to_owned(),
            message: diagnostic.message().to_owned(),
            primary_span: SpanDocument::render_with_source(
                diagnostic.primary_span(),
                primary_index,
                diagnostic.source_id(),
            ),
            related,
            notes: diagnostic.notes().to_vec(),
            fixes: diagnostic
                .fixes()
                .iter()
                .map(|fix| FixDocument {
                    description: fix.description().to_owned(),
                    safe: fix.is_safe(),
                    expected_revision: fix.expected_revision().as_str().to_owned(),
                })
                .collect(),
        })
    }
}

/// One registry entry, as returned for a diagnostic-code query subject.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegistryEntryDocument {
    /// Major version of this contract.
    pub schema_version: u32,
    /// The exact atom spelling of the code.
    pub code: String,
    /// The exact atom spelling of its fixed level.
    pub level: String,
    /// The first component of the code's spelling.
    pub domain: String,
    /// A one-line description of the condition.
    pub summary: String,
    /// `@safe` or `@none`.
    pub fix_capability: String,
}

impl RegistryEntryDocument {
    /// Renders the registry entry for `code`.
    #[must_use]
    pub fn render(code: DiagnosticCode) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            code: code.as_atom().to_owned(),
            level: code.level().as_atom().to_owned(),
            domain: code.domain().as_str().to_owned(),
            summary: code.summary().to_owned(),
            fix_capability: code.fix_capability().as_atom().to_owned(),
        }
    }
}
