//! The source grammar and its typed views.
//!
//! `docs/spec/01-source-language.md`, "Grammar" and "Declarations": a module
//! is a sequence of the seven top-level forms. The construct matcher builds
//! the generic structure every form shares, keyword-led constructs with
//! typed closers and sections; each form is then read in its own grammar,
//! with type expressions, cases, associated types, `impl` blocks, callables,
//! signatures, guards, expressions, statements, and patterns. Views
//! ([`Module`], [`Declaration`], [`TypeDecl`], [`InterfaceDecl`], [`Impl`],
//! [`Callable`], [`Test`], [`ModuleLet`], [`TypeExpr`], [`Expr`], [`Body`],
//! [`Pattern`], [`Construct`], [`Section`], [`Use`], …) are projections of
//! the lossless tree.

mod callable;
mod construct;
mod declaration;
mod expr;
mod impl_block;
mod interface_decl;
pub mod keyword;
mod module;
mod pattern;
mod section;
mod shape;
mod statement;
mod test_decl;
mod type_decl;
mod types;

pub use callable::{Callable, CallableKind, Guard, Signature};
pub use declaration::{Atom, Declaration, Ident, Use};
pub use expr::{
    Argument, DoBlock, Expr, ExprForm, If, Match, MatchArm, Postfix, Primary, Suffix,
};
pub use impl_block::{AssocChoice, Impl};
pub use interface_decl::{AssocType, InterfaceDecl};
pub use module::{Module, module};
pub use pattern::{Pattern, PatternForm};
pub use section::{
    Construct, Doc, Given, Section, StringSection, TagSection, Where, is_construct,
};
pub use statement::{Body, Let, ModuleLet, Statement};
pub use test_decl::Test;
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
