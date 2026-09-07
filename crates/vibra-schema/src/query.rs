//! The JSON contract for structural source-position facts.

use serde::{Deserialize, Serialize};
use vibra_diagnostics::LineIndex;
use vibra_syntax::{FactStatus, GrammarCategory, StructuralQuery, SyntaxKind};

use crate::{SCHEMA_VERSION, SpanDocument};

/// The published JSON Schema for [`SourcePositionQueryDocument`].
pub const SOURCE_POSITION_QUERY_SCHEMA: &str =
    include_str!("../schemas/v1/source-position-query.json");

/// One structural source-position result rendered for a tooling consumer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourcePositionQueryDocument {
    /// Major version of this contract.
    pub schema_version: u32,
    /// The queried UTF-8 byte offset.
    pub offset: usize,
    /// The source identity owning the query span, when known.
    pub source_id: Option<String>,
    /// The selected half-open syntax span and its display endpoints.
    pub span: SpanDocument,
    /// The extension-selected document mode.
    pub mode: String,
    /// The selected lossless CST kind.
    pub syntax_kind: String,
    /// The grammar category owning the selected node.
    pub category: String,
    /// Whether the structural facts are exact, recovered, or unavailable.
    pub status: String,
    /// Permitted child forms, or null when unavailable.
    pub permitted_forms: Option<Vec<String>>,
    /// Permitted labels, or null when unavailable.
    pub permitted_labels: Option<Vec<String>>,
}

impl SourcePositionQueryDocument {
    /// Renders a syntax-owned query against the source's line index.
    #[must_use]
    pub fn render(query: &StructuralQuery, index: &LineIndex<'_>) -> Self {
        Self::render_with_source(query, index, None)
    }

    /// Renders a query while retaining its owning source identity.
    #[must_use]
    pub fn render_with_source(
        query: &StructuralQuery,
        index: &LineIndex<'_>,
        source_id: Option<&str>,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            offset: query.offset(),
            source_id: source_id.map(str::to_owned),
            span: SpanDocument::render_with_source(query.span(), index, source_id),
            mode: query.mode().as_str().to_owned(),
            syntax_kind: syntax_kind_name(query.syntax_kind()).to_owned(),
            category: query.category().as_str().to_owned(),
            status: query.status().as_str().to_owned(),
            permitted_forms: query.permitted_forms().map(|values| values.to_vec()),
            permitted_labels: query.permitted_labels().map(|values| values.to_vec()),
        }
    }
}

fn syntax_kind_name(kind: SyntaxKind) -> &'static str {
    match kind {
        SyntaxKind::Root => "root",
        SyntaxKind::List => "list",
        SyntaxKind::Atom => "atom",
        SyntaxKind::Whitespace => "whitespace",
        SyntaxKind::LineComment => "line-comment",
        SyntaxKind::OpenParen => "open-paren",
        SyntaxKind::CloseParen => "close-paren",
        SyntaxKind::Error => "error",
    }
}

/// Converts a syntax status to its closed wire vocabulary.
#[must_use]
pub const fn status_name(status: FactStatus) -> &'static str {
    status.as_str()
}

/// Converts a syntax category to its closed wire vocabulary.
#[must_use]
pub const fn category_name(category: GrammarCategory) -> &'static str {
    category.as_str()
}
