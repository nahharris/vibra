//! `DiagnosticDocument` and `SpanDocument`, version 2.
//!
//! These types are the wire format, not the language model. They exist so a
//! tool can read a diagnostic without reimplementing the compiler.
//! `docs/spec/07-diagnostics-and-conformance.md`, "Diagnostic documents and
//! spans", defines them.

use std::collections::BTreeMap;

use serde::Serialize;
use vibra_diagnostics::{ByteSpan, Diagnostic, LineIndex, Position};

use crate::SCHEMA_VERSION;

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

/// A one-based display position.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
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
/// Edits use the offsets; positions are for display. A failure with no
/// location has a `null` source, offsets 0 and 0, and position 1:1.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpanDocument {
    /// The source identity owning the span.
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
    /// Renders `span` against the document `index` was built from, naming
    /// `source_id` as its owner.
    #[must_use]
    pub fn render(
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelatedSpanDocument {
    /// Where the related construct is.
    pub span: SpanDocument,
    /// What that construct contributes.
    pub message: String,
}

/// A fix a diagnostic describes. The diagnostic never applies it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FixDocument {
    /// What the fix does.
    pub description: String,
    /// Whether it may be applied without review.
    pub safe: bool,
    /// The document revision it was computed against.
    pub expected_revision: String,
}

/// One diagnostic as sent to a tool.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticDocument {
    /// Major version of this contract.
    pub schema_version: u32,
    /// The exact atom spelling of the code.
    pub code: String,
    /// The exact atom spelling of the registered level.
    pub level: String,
    /// Text for a person.
    pub message: String,
    /// Where the diagnostic is reported.
    pub primary_span: SpanDocument,
    /// Additional spans that explain it.
    pub related: Vec<RelatedSpanDocument>,
    /// Additional explanation carrying no span.
    pub notes: Vec<String>,
    /// Fixes it describes.
    pub fixes: Vec<FixDocument>,
}

impl DiagnosticDocument {
    /// Renders `diagnostic` against the document `index` was built from.
    ///
    /// # Errors
    ///
    /// Fails when a related span names another source.
    pub fn render(
        diagnostic: &Diagnostic,
        index: &LineIndex<'_>,
    ) -> Result<Self, DiagnosticRenderError> {
        Self::render_with_source_indexes(diagnostic, index, &BTreeMap::new())
    }

    /// Renders a diagnostic with indexes for related source documents.
    ///
    /// `primary_index` owns the primary span's document. A related span
    /// naming another source needs an entry in `source_indexes`; deriving its
    /// position from the wrong document would publish a false location.
    ///
    /// # Errors
    ///
    /// Fails when a related span names a source with no entry.
    pub fn render_with_source_indexes<'a>(
        diagnostic: &Diagnostic,
        primary_index: &LineIndex<'a>,
        source_indexes: &BTreeMap<String, LineIndex<'a>>,
    ) -> Result<Self, DiagnosticRenderError> {
        let related = diagnostic
            .related()
            .iter()
            .map(|related| {
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
                    span: SpanDocument::render(
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
            primary_span: SpanDocument::render(
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
