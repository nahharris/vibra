//! The Vibra reader: the lexer, the lossless syntax tree, and the VIBON data
//! grammar.
//!
//! `docs/spec/01-source-language.md`, "Lexical structure", fixes the tokens
//! shared by `.vib` and `.vibon` documents; `docs/spec/04-programs-and-packages.md`,
//! "VIBON data documents", fixes the data grammar; and
//! `docs/spec/07-diagnostics-and-conformance.md`, "Recovery" and "Persistent
//! observations", fix recovery and the `@tree.v2` observation.
//!
//! A document is parsed by the grammar its extension selects
//! ([`DocumentMode::from_path`]). [`parse_data`] is complete. [`parse_source`]
//! parses the source grammar ([`source`]): the module, the seven top-level
//! forms recognized by keyword and typed closer, generic sections, `use`,
//! and the type grammar (`type`, `interface`, `impl`, type expressions) in
//! full. The bodies of `fn`, `proc`, `let`, and `test` forms and of named
//! `fn` and `proc` members are reported `@tool.unavailable`, once per form.
//!
//! # Position in the architecture
//!
//! This crate depends only on `vibra-diagnostics`. It reads no file: callers
//! hand it bytes. Every tree walk here is iterative, so nesting depth is
//! bounded by memory, not by the stack.

mod cst;
mod data;
mod document;
mod lexer;
mod literal;
pub mod source;
mod tree;

pub use cst::{CstNode, SyntaxKind};
pub use data::{
    DataEntry, DataField, DataNode, DataValue, FloatLiteral, IntegerLiteral,
};
pub use document::{
    Document, DocumentMode, load, parse, parse_data, parse_source, parse_source_with,
};
pub use lexer::{
    Lexed, RESERVED_WORDS, Token, TokenKind, check_encoding, is_reserved_word, lex,
};
pub use literal::{
    FloatSuffix, IntegerSuffix, canonical_character, canonical_string,
    decode_character, decode_string,
};
pub use source::{
    Declaration, InterfaceDecl, Module, SourceOptions, TypeDecl, TypeExpr, Use, module,
};
pub use tree::{TREE_FORMAT, tree_document, tree_text};
