//! The Vibra formatter.
//!
//! `docs/spec/01-source-language.md`, "Canonical format", defines the layout:
//! UTF-8, LF line endings, one trailing newline, no trailing whitespace,
//! two-space indentation, 88 columns, spellings exactly as written, and
//! comments preserved. Formatting is idempotent, preserves meaning, and never
//! reorders anything. If the parser recovered from a syntax error anywhere in
//! a file, the formatter returns the file's bytes unchanged.
//!
//! VIBON data documents get the source-preserving formatter of
//! `docs/spec/04-programs-and-packages.md`. Source documents get the layout
//! of their top-level forms: one blank line between forms, comments kept,
//! sections in canonical order, and the expressions, statements, and
//! patterns of module values.
//!
//! # Position in the architecture
//!
//! This crate consumes the reader's tree; no language crate depends on it.

mod comments;
mod data;
mod expr;
mod layout;
mod source;

use vibra_syntax::{Document, DocumentMode};

/// The column limit of the canonical format.
pub const COLUMN_LIMIT: usize = layout::WIDTH;

/// Formats a document.
///
/// Returns `None` when the document has no text: it was never read, or it is
/// not valid UTF-8. A document with any diagnostic is returned byte for byte,
/// because recovery may have been ambiguous; it is the only document returned
/// unchanged. Every other document is laid out, comments included, with LF
/// line endings and one trailing line feed.
///
/// `None` for a document the reader accepts would be a defect of this
/// formatter: the reader's tree of every accepted document has a layout.
#[must_use]
pub fn format_document(document: &Document) -> Option<String> {
    let text = document.text()?;
    let root = document.root()?;
    if !document.diagnostics().is_empty() {
        return Some(text.to_owned());
    }
    match document.mode() {
        DocumentMode::Data => Some(layout::render(&data::document_items(root, text))),
        DocumentMode::Source => source::format(root, text),
    }
}
