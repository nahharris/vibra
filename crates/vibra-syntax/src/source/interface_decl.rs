//! `interface … endinterface` and its associated types.
//!
//! `docs/spec/01-source-language.md`, "Grammar" and "`interface`":
//!
//! ```text
//! interface-decl = "interface", identifier, [ type-params ],
//!                  { given | role | visibility | doc | assoc-type | fn-decl
//!                  | proc-decl | impl },
//!                  "endinterface" ;
//! assoc-type  = "type", identifier, "endtype" ;
//! ```
//!
//! An interface is shaped by the declaration shaping of
//! [`super::type_decl`]. A nested `type` is an associated type, a member of
//! the interface: it has a name and nothing else, and a body or an item in
//! it is `@syntax.invalidForm` over the nested `type`.

use super::declaration::Ident;
use super::impl_block::Impl;
use super::section::{Construct, Doc, Given, TagSection};
use super::type_decl::{
    child_of_kind, children_of_kind, declared_name, declared_params, members,
};
use crate::cst::{CstNode, SyntaxKind};

/// `interface … endinterface`.
#[derive(Clone, Copy, Debug)]
pub struct InterfaceDecl<'a> {
    node: &'a CstNode,
    text: &'a str,
}

impl<'a> InterfaceDecl<'a> {
    /// The view of an `@interfaceDecl` node of the document `text`.
    #[must_use]
    pub fn new(node: &'a CstNode, text: &'a str) -> Option<Self> {
        (node.kind() == SyntaxKind::InterfaceDecl).then_some(Self { node, text })
    }

    /// The node.
    #[must_use]
    pub const fn node(&self) -> &'a CstNode {
        self.node
    }

    /// The generic construct view.
    #[must_use]
    pub fn construct(&self) -> Construct<'a> {
        Construct::of_declaration(self.node, self.text)
    }

    /// The name.
    #[must_use]
    pub fn name(&self) -> Option<Ident<'a>> {
        declared_name(self.node, self.text)
    }

    /// The type parameters, in written order.
    #[must_use]
    pub fn params(&self) -> Vec<Ident<'a>> {
        declared_params(self.node, self.text)
    }

    /// The associated types, in written order.
    #[must_use]
    pub fn assoc_types(&self) -> Vec<AssocType<'a>> {
        children_of_kind(self.node, SyntaxKind::AssocType)
            .into_iter()
            .map(|node| AssocType {
                node,
                text: self.text,
            })
            .collect()
    }

    /// The `given` section.
    #[must_use]
    pub fn given(&self) -> Option<Given<'a>> {
        child_of_kind(self.node, SyntaxKind::Given)
            .and_then(|node| Given::new(node, self.text))
    }

    /// The `role` section.
    #[must_use]
    pub fn role(&self) -> Option<TagSection<'a>> {
        child_of_kind(self.node, SyntaxKind::Role)
            .and_then(|node| TagSection::new(node, self.text))
    }

    /// The `visibility` section.
    #[must_use]
    pub fn visibility(&self) -> Option<TagSection<'a>> {
        child_of_kind(self.node, SyntaxKind::Visibility)
            .and_then(|node| TagSection::new(node, self.text))
    }

    /// The `doc` section.
    #[must_use]
    pub fn doc(&self) -> Option<Doc<'a>> {
        child_of_kind(self.node, SyntaxKind::Doc)
            .and_then(|node| Doc::new(node, self.text))
    }

    /// The named `fn` and `proc` members, in written order.
    #[must_use]
    pub fn members(&self) -> Vec<Construct<'a>> {
        members(self.node, self.text)
    }

    /// The `impl` blocks, in written order.
    #[must_use]
    pub fn impls(&self) -> Vec<Impl<'a>> {
        children_of_kind(self.node, SyntaxKind::Impl)
            .into_iter()
            .filter_map(|node| Impl::new(node, self.text))
            .collect()
    }
}

/// `type Name endtype` in an interface: an associated type.
#[derive(Clone, Copy, Debug)]
pub struct AssocType<'a> {
    node: &'a CstNode,
    text: &'a str,
}

impl<'a> AssocType<'a> {
    /// The `@assocType` node.
    #[must_use]
    pub const fn node(&self) -> &'a CstNode {
        self.node
    }

    /// The associated type's name.
    #[must_use]
    pub fn name(&self) -> Option<Ident<'a>> {
        declared_name(self.node, self.text)
    }
}
