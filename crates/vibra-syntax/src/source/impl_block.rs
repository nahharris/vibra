//! `impl … endimpl` and its associated choices.
//!
//! `docs/spec/01-source-language.md`, "Grammar" and "`impl`":
//!
//! ```text
//! impl        = "impl", type-expr, { assoc-choice | fn-decl | proc-decl },
//!               "endimpl" ;
//! assoc-choice = "type", identifier, "of", type-expr, "endtype" ;
//! ```
//!
//! An `impl` is valid only as a direct child of a `type` or an `interface`;
//! anywhere else it is `@syntax.invalidForm` over the `impl`. Its operand is
//! one type expression, and it admits no section. It is shaped by the
//! declaration shaping of [`super::type_decl`].

use super::callable::Callable;
use super::construct::is_trivia;
use super::declaration::Ident;
use super::type_decl::{children_of_kind, declared_name, members, of_operand};
use super::types::{TypeExpr, type_entries};
use crate::cst::{CstNode, SyntaxKind};

/// `impl … endimpl`.
#[derive(Clone, Copy, Debug)]
pub struct Impl<'a> {
    node: &'a CstNode,
    text: &'a str,
}

impl<'a> Impl<'a> {
    /// The view of an `@impl` node of the document `text`.
    #[must_use]
    pub fn new(node: &'a CstNode, text: &'a str) -> Option<Self> {
        (node.kind() == SyntaxKind::Impl).then_some(Self { node, text })
    }

    /// The node.
    #[must_use]
    pub const fn node(&self) -> &'a CstNode {
        self.node
    }

    /// The operand: the interface a `type`'s `impl` implements, or the type
    /// an `interface`'s `impl` is for.
    #[must_use]
    pub fn target(&self) -> Option<TypeExpr<'a>> {
        let children = self.node.children();
        let contents = children
            .get(1..children.len().saturating_sub(1))
            .unwrap_or_default();
        let end = contents
            .iter()
            .position(|child| {
                !is_trivia(child)
                    && matches!(
                        child.kind(),
                        SyntaxKind::AssocChoice
                            | SyntaxKind::FnDecl
                            | SyntaxKind::ProcDecl
                            | SyntaxKind::Construct
                            | SyntaxKind::Error
                    )
            })
            .unwrap_or(contents.len());
        type_entries(contents.get(..end).unwrap_or_default(), self.text)
            .into_iter()
            .next()
            .map(|(_, target)| target)
    }

    /// The associated choices, in written order.
    #[must_use]
    pub fn choices(&self) -> Vec<AssocChoice<'a>> {
        children_of_kind(self.node, SyntaxKind::AssocChoice)
            .into_iter()
            .map(|node| AssocChoice {
                node,
                text: self.text,
            })
            .collect()
    }

    /// The named `fn` and `proc` members, in written order.
    #[must_use]
    pub fn members(&self) -> Vec<Callable<'a>> {
        members(self.node, self.text)
    }
}

/// `type Name of T endtype` in an `impl`: the choice of an associated type.
#[derive(Clone, Copy, Debug)]
pub struct AssocChoice<'a> {
    node: &'a CstNode,
    text: &'a str,
}

impl<'a> AssocChoice<'a> {
    /// The `@assocChoice` node.
    #[must_use]
    pub const fn node(&self) -> &'a CstNode {
        self.node
    }

    /// The name of the associated type it chooses.
    #[must_use]
    pub fn name(&self) -> Option<Ident<'a>> {
        declared_name(self.node, self.text)
    }

    /// The chosen type.
    #[must_use]
    pub fn chosen(&self) -> Option<TypeExpr<'a>> {
        of_operand(self.node, self.text)
    }
}
