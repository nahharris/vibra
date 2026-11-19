//! The source grammar and its typed views.
//!
//! `docs/spec/01-source-language.md`, "Grammar" and "Declarations": a module
//! is a sequence of the seven top-level forms. The construct matcher builds
//! the generic structure every form shares, keyword-led constructs with
//! typed closers and sections; `use`, `type`, and `interface` are then read
//! in their own grammar, with type expressions, cases, associated types, and
//! `impl` blocks. The forms whose grammar does not exist yet are reported
//! `@tool.unavailable`. Views ([`Module`], [`Declaration`], [`TypeDecl`],
//! [`InterfaceDecl`], [`Impl`], [`TypeExpr`], [`Construct`], [`Section`],
//! [`Use`], …) are projections of the lossless tree.

mod construct;
mod declaration;
mod impl_block;
mod interface_decl;
pub mod keyword;
mod module;
mod section;
mod type_decl;
mod types;

pub use declaration::{Atom, Declaration, Ident, Use};
pub use impl_block::{AssocChoice, Impl};
pub use interface_decl::{AssocType, InterfaceDecl};
pub use module::{Module, module};
pub use section::{Construct, Doc, Given, Section, TagSection, Where, is_construct};
pub use type_decl::{Case, SourceOptions, TypeBody, TypeDecl};
pub use types::{Inputs, Path, Row, TypeExpr, TypeForm};

use vibra_diagnostics::Diagnostic;

use crate::cst::CstNode;
use crate::lexer::Token;

/// Parses the tokens of a source text into its `@source` document node.
pub(crate) fn parse(
    text: &str,
    tokens: &[Token],
    options: SourceOptions,
    diagnostics: &mut Vec<Diagnostic>,
) -> CstNode {
    let items = construct::match_constructs(text, tokens, diagnostics);
    module::shape_module(items, text, options, diagnostics)
}
