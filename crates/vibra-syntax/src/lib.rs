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
//! lexes and matches brackets and reports the source grammar as
//! `@tool.unavailable`: the source grammar arrives in a later step.
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
mod tree;

pub use cst::{CstNode, SyntaxKind};
pub use data::{
    DataEntry, DataField, DataNode, DataValue, FloatLiteral, IntegerLiteral,
};
pub use document::{Document, DocumentMode, load, parse, parse_data, parse_source};
pub use lexer::{
    Lexed, RESERVED_WORDS, Token, TokenKind, check_encoding, is_reserved_word, lex,
};
pub use literal::{
    FloatSuffix, IntegerSuffix, canonical_character, canonical_string,
    decode_character, decode_string,
};
pub use tree::{TREE_FORMAT, tree_document, tree_text};
