//! Vibra's reader: lexer, lossless recovery CST, document modes, and the
//! generic VIBON data decoder.
//!
//! One lexer serves both document grammars. The `.vib` source grammar and the
//! `.vibon` data grammar are selected by extension before parsing and are
//! never inferred from contents. The VIBON section of
//! `docs/spec/04-programs-and-packages.md` is the normative definition of the
//! data grammar.
//!
//! # Position in the architecture
//!
//! It may depend on [`vibra_diagnostics_v1`] and nothing else in the workspace.
//! It must never depend on the formatter, the schemas, or any tool surface.
mod data;
mod literal;
mod name;
mod reader;

pub use data::{DataDecode, DataField, DataNode, DataValue, decode_data_root};
pub use literal::{
    BooleanLiteral, CharacterLiteral, FloatLiteral, FloatSuffix, IntegerLiteral,
    IntegerSuffix, InvalidLiteralKind, Literal, LiteralClassification, LiteralKind,
    StringLiteral, VoidLiteral, canonical_character_spelling, classify,
};
pub use name::{Name, NameClassification, NameKind, classify_name};
pub use reader::{
    CstNode, Document, DocumentMode, DocumentModeError, Lexed, SyntaxKind, Token,
    TokenKind, lex, parse_data, parse_with_mode,
};
