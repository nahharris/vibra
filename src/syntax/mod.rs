//! Dormant native S-expression syntax core.
//!
//! This module is intentionally independent from the current YAML frontend.
//! It supplies the lossless-enough reader tree, byte spans, diagnostics, and
//! canonical printer needed for the eventual source-language cutover.

mod lexer;
mod parser;
mod printer;
mod span;

pub use lexer::{lex, Token, TokenKind};
pub use parser::{parse, Atom, Document, Node, NodeKind, SyntaxError};
pub use printer::print;
pub use span::{LineIndex, Position, Span};
