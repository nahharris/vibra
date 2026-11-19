//! Documents: the grammar chosen by extension, and the two parsers.
//!
//! `docs/spec/04-programs-and-packages.md`, "VIBON data documents": the file
//! extension selects the grammar before parsing, and contents are never used
//! to guess. A document presented to the wrong loader is
//! `@data.invalidExtension` before its contents are read.

use std::path::Path;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};

use crate::cst::{CstNode, match_brackets};
use crate::data::{DataNode, DataShaper};
use crate::lexer::{check_encoding, lex};
use crate::source::SourceOptions;

/// The grammar of a document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DocumentMode {
    /// A `.vib` source document.
    Source,
    /// A `.vibon` data document.
    Data,
}

impl DocumentMode {
    /// The mode of a path by its exact extension: `vib` or `vibon`, case
    /// included. Any other path has no mode.
    #[must_use]
    pub fn from_path(path: impl AsRef<Path>) -> Option<Self> {
        match path.as_ref().extension()?.to_str()? {
            "vib" => Some(Self::Source),
            "vibon" => Some(Self::Data),
            _ => None,
        }
    }

    /// The extension of this mode, without the dot.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Source => "vib",
            Self::Data => "vibon",
        }
    }
}

/// One parsed document: its text, its lossless tree, and its diagnostics.
///
/// A document that is not valid UTF-8 has no text and no tree, and a document
/// refused by its extension was never read.
#[derive(Debug)]
pub struct Document {
    mode: DocumentMode,
    source_id: Option<String>,
    text: Option<String>,
    root: Option<CstNode>,
    diagnostics: Vec<Diagnostic>,
    value: Option<DataNode>,
}

impl Document {
    /// The grammar the document was parsed with.
    #[must_use]
    pub const fn mode(&self) -> DocumentMode {
        self.mode
    }

    /// The source identity the diagnostics carry, when one was given.
    #[must_use]
    pub fn source_id(&self) -> Option<&str> {
        self.source_id.as_deref()
    }

    /// The text, when the bytes were read and are valid UTF-8.
    #[must_use]
    pub fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }

    /// The document node of the lossless tree, when the document has text.
    #[must_use]
    pub const fn root(&self) -> Option<&CstNode> {
        self.root.as_ref()
    }

    /// The diagnostics, sorted by primary start and end and then registry
    /// order.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Whether any diagnostic is an error.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(Diagnostic::is_error)
    }

    /// The decoded value of a data document with no diagnostic.
    #[must_use]
    pub const fn value(&self) -> Option<&DataNode> {
        self.value.as_ref()
    }

    fn finish(
        mode: DocumentMode,
        source_id: Option<&str>,
        text: Option<String>,
        root: Option<CstNode>,
        mut diagnostics: Vec<Diagnostic>,
        value: Option<DataNode>,
    ) -> Self {
        diagnostics.sort_by_key(|diagnostic| {
            (
                diagnostic.primary_span().start(),
                diagnostic.primary_span().end(),
                diagnostic.code(),
            )
        });
        if let Some(source_id) = source_id {
            diagnostics = diagnostics
                .into_iter()
                .map(|diagnostic| diagnostic.with_source_id(source_id))
                .collect();
        }
        let value = if diagnostics.is_empty() { value } else { None };
        Self {
            mode,
            source_id: source_id.map(str::to_owned),
            text,
            root,
            diagnostics,
            value,
        }
    }
}

/// Parses a `.vibon` document.
///
/// Reports the encoding, literal, name, and separator faults, unmatched
/// delimiters, forms outside the data grammar, repeated record fields, and
/// repeated dict keys.
#[must_use]
pub fn parse_data(source_id: Option<&str>, bytes: &[u8]) -> Document {
    let (text, mut diagnostics) = check_encoding(bytes);
    let Some(text) = text else {
        return Document::finish(
            DocumentMode::Data,
            source_id,
            None,
            None,
            diagnostics,
            None,
        );
    };
    let lexed = lex(text);
    diagnostics.extend(lexed.diagnostics);
    let mut shaper = DataShaper::new(text);
    let items =
        match_brackets(&lexed.tokens, text.len(), &mut shaper, &mut diagnostics);
    let (root, value) = shaper.shape_document(items);
    diagnostics.extend(shaper.into_diagnostics());
    Document::finish(
        DocumentMode::Data,
        source_id,
        Some(text.to_owned()),
        Some(root),
        diagnostics,
        value,
    )
}

/// Parses a `.vib` document that is not a standard-library module.
///
/// Reports the encoding, literal, name, and separator faults, unmatched
/// delimiters and closers, top-level tokens that open no form, a late `use`,
/// malformed imports, repeated sections, `as` outside a `use`, and the
/// faults of the `type` and `interface` grammar. Each `fn`, `proc`, `let`,
/// and `test` form, and each named `fn` or `proc` member, is
/// `@tool.unavailable` over the whole form until its grammar exists.
#[must_use]
pub fn parse_source(source_id: Option<&str>, bytes: &[u8]) -> Document {
    parse_source_with(source_id, bytes, SourceOptions::default())
}

/// Parses a `.vib` document with the reader `options`: whether it is an
/// embedded standard-library module, where `role` and `Intrinsic` are
/// admitted.
#[must_use]
pub fn parse_source_with(
    source_id: Option<&str>,
    bytes: &[u8],
    options: SourceOptions,
) -> Document {
    let (text, mut diagnostics) = check_encoding(bytes);
    let Some(text) = text else {
        return Document::finish(
            DocumentMode::Source,
            source_id,
            None,
            None,
            diagnostics,
            None,
        );
    };
    let lexed = lex(text);
    diagnostics.extend(lexed.diagnostics);
    let root = crate::source::parse(text, &lexed.tokens, options, &mut diagnostics);
    Document::finish(
        DocumentMode::Source,
        source_id,
        Some(text.to_owned()),
        Some(root),
        diagnostics,
        None,
    )
}

/// Parses `bytes` with the parser of `mode`.
#[must_use]
pub fn parse(mode: DocumentMode, source_id: Option<&str>, bytes: &[u8]) -> Document {
    match mode {
        DocumentMode::Source => parse_source(source_id, bytes),
        DocumentMode::Data => parse_data(source_id, bytes),
    }
}

/// Loads the document `source_id` with the loader of `expected`.
///
/// When the extension of `source_id` does not select `expected`, the document
/// is refused with `@data.invalidExtension`, which has no location, and
/// `read` is never called. Otherwise `read` supplies the bytes.
pub fn load<E>(
    source_id: &str,
    expected: DocumentMode,
    read: impl FnOnce() -> Result<Vec<u8>, E>,
) -> Result<Document, E> {
    if DocumentMode::from_path(source_id) != Some(expected) {
        let diagnostic = Diagnostic::new(
            DiagnosticCode::DataInvalidExtension,
            ByteSpan::empty_at(0),
            format!(
                "a document read by this loader has the extension `.{}`",
                expected.extension()
            ),
        );
        return Ok(Document::finish(
            expected,
            Some(source_id),
            None,
            None,
            vec![diagnostic],
            None,
        ));
    }
    let bytes = read()?;
    Ok(parse(expected, Some(source_id), &bytes))
}
