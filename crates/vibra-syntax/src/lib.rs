//! Vibra's reader: lexer, lossless recovery CST, document modes, and AST.
//!
//! One lexer serves both document grammars. The `.vib` source grammar and the
//! `.vibon` data grammar are selected by extension before parsing and are
//! never inferred from contents. `docs/spec/01-source-language.md` and the
//! VIBON section of `docs/spec/04-programs-and-packages.md` are the normative
//! definitions.
//!
//! # Position in the architecture
//!
//! This is the first node of the roadmap's dependency chain. It may depend on
//! [`vibra_diagnostics`] and nothing else in the workspace. It must never
//! depend on the formatter, the schemas, or any tool surface.
//!
//! # Status
//!
//! Milestone 1 step 4 supplies the reader spine: a shared UTF-8 lexer, a
//! hand-rolled lossless recovery CST, extension-selected document modes, and
//! the opaque leaf boundary used by later literal/name/AST steps. See
//! `docs/roadmap/milestone-1/README.md`.

mod reader;

pub use reader::{
    CstNode, Document, DocumentMode, DocumentModeError, Lexed, Lexer, SyntaxKind,
    Token, TokenKind, lex, lex_bytes, parse, parse_data, parse_document, parse_source,
    parse_with_mode,
};
