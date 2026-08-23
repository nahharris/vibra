//! Native S-expression syntax core.
//!
//! Supplies reader tree, byte spans, diagnostics, canonical printer for
//! Vibra source and editor tooling.

mod lexer;
mod parser;
mod printer;
mod span;

pub use lexer::{lex, Token, TokenKind};
pub use parser::{parse, Atom, Document, Node, NodeKind, SyntaxError};
pub use printer::print;
pub use span::{LineIndex, Position, Span};
