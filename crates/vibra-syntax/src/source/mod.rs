//! The source grammar's skeleton and its typed views.
//!
//! `docs/spec/01-source-language.md`, "Grammar" and "Declarations": a module
//! is a sequence of the seven top-level forms. This step parses the generic
//! structure every form shares, keyword-led constructs with typed closers
//! and sections, completes `use`, and reports the bodies of the other six
//! forms `@tool.unavailable`. Views ([`Module`], [`Declaration`],
//! [`Construct`], [`Section`], [`Use`]) are projections of the lossless tree.

mod construct;
mod declaration;
pub mod keyword;
mod module;
mod section;

pub use declaration::{Atom, Declaration, Ident, Use};
pub use module::{Module, module};
pub use section::{Construct, Section, is_construct};

use vibra_diagnostics::Diagnostic;

use crate::cst::CstNode;
use crate::lexer::Token;

/// Parses the tokens of a source text into its `@source` document node.
pub(crate) fn parse(
    text: &str,
    tokens: &[Token],
    diagnostics: &mut Vec<Diagnostic>,
) -> CstNode {
    let items = construct::match_constructs(text, tokens, diagnostics);
    module::shape_module(items, text, diagnostics)
}
