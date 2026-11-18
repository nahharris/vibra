//! The Vibra formatter.
//!
//! `docs/spec/01-source-language.md`, "Canonical format", defines the layout:
//! UTF-8, LF line endings, one trailing newline, no trailing whitespace,
//! two-space indentation, 88 columns, spellings exactly as written, and
//! comments preserved. Formatting is idempotent, preserves meaning, and never
//! reorders anything. If the parser recovered from a syntax error anywhere in
//! a file, the formatter returns the file's bytes unchanged.
//!
//! This step formats VIBON data documents with the source-preserving
//! formatter of `docs/spec/04-programs-and-packages.md`. The source grammar
//! is not parsed yet, so a `.vib` document is returned unchanged.
//!
//! # Position in the architecture
//!
//! This crate consumes the reader's tree; no language crate depends on it.

mod data;
mod layout;

use vibra_syntax::{Document, DocumentMode};

/// The column limit of the canonical format.
pub const COLUMN_LIMIT: usize = layout::WIDTH;

/// Formats a document.
///
/// Returns `None` when the document has no text: it was never read, or it is
/// not valid UTF-8. A document with any diagnostic is returned byte for byte,
/// because recovery may have been ambiguous; so is a source document, whose
/// grammar is not parsed yet. Otherwise the data document is laid out and
/// ends with one line feed.
#[must_use]
pub fn format_document(document: &Document) -> Option<String> {
    let text = document.text()?;
    let root = document.root()?;
    if !document.diagnostics().is_empty() || document.mode() == DocumentMode::Source {
        return Some(text.to_owned());
    }
    Some(layout::render(&data::document_items(root, text)))
}
