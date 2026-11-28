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
//! parses the whole source grammar ([`source`]): the module, the seven
//! top-level forms recognized by keyword and typed closer, generic sections,
//! `use`, the type grammar (`type`, `interface`, `impl`, type expressions),
//! the module value `let`, `fn` and `proc` declarations, members, and
//! anonymous expressions with their signatures and guards, `test`, and the
//! expression, statement, and pattern grammar.
//!
//! [`decode`] reads a data document under a closed record [`Schema`], in the
//! validation order of the programs chapter's "Schemas", and
//! [`TypedDocument::canonical_bytes`] writes its canonical bytes.
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
mod nesting;
mod schema;
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
    Lexed, RESERVED_WORDS, Token, TokenKind, check_encoding, is_identifier,
    is_reserved_word, lex,
};
pub use literal::{
    FloatSuffix, IntegerSuffix, canonical_character, canonical_string,
    decode_character, decode_string, span_record, split_number,
};
pub use nesting::{Nesting, nesting};
pub use schema::{
    Decoded, DefaultValue, FieldSchema, Presence, RecordSchema, Schema, Slot,
    SpellingRule, TypedDocument, TypedEntry, TypedField, TypedNode, TypedValue,
    VariantSchema, compare, decode,
};
pub use source::{
    Declaration, InterfaceDecl, Module, SourceOptions, TypeDecl, TypeExpr, Use, module,
};
pub use tree::{TREE_FORMAT, tree_document, tree_text};
