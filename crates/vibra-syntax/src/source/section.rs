//! Typed views of constructs and sections.
//!
//! A view is a projection of the lossless tree: it borrows the tree and the
//! document text and owns no bytes. Constructs and sections are generic in
//! this step: a construct knows its opener keyword, its head, its sections,
//! its nested constructs, and its closer; a section knows its keyword and its
//! operand.

use vibra_diagnostics::ByteSpan;

use super::construct::is_trivia;
use crate::cst::{CstNode, SyntaxKind};

/// Whether `kind` is a keyword-led construct: a top-level form or a generic
/// nested construct.
#[must_use]
pub const fn is_construct(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::Use
            | SyntaxKind::TypeDecl
            | SyntaxKind::InterfaceDecl
            | SyntaxKind::FnDecl
            | SyntaxKind::ProcDecl
            | SyntaxKind::ModuleLet
            | SyntaxKind::Test
            | SyntaxKind::Construct
    )
}

/// A keyword-led construct, from its opener keyword to its typed closer.
#[derive(Clone, Copy, Debug)]
pub struct Construct<'a> {
    node: &'a CstNode,
    text: &'a str,
}

impl<'a> Construct<'a> {
    /// The view of `node`, when it is a construct of the document `text`.
    #[must_use]
    pub fn new(node: &'a CstNode, text: &'a str) -> Option<Self> {
        is_construct(node.kind()).then_some(Self { node, text })
    }

    /// The node.
    #[must_use]
    pub const fn node(&self) -> &'a CstNode {
        self.node
    }

    /// The bytes from the opener to the closer, or to the last written token
    /// when the closer is missing.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.node.span()
    }

    /// The opener keyword.
    #[must_use]
    pub fn keyword(&self) -> &'a str {
        self.node
            .children()
            .first()
            .map_or("", |keyword| keyword.text(self.text))
    }

    /// The span of the opener keyword.
    #[must_use]
    pub fn keyword_span(&self) -> ByteSpan {
        self.node
            .children()
            .first()
            .map_or(self.node.span(), CstNode::span)
    }

    /// The closer leaf, or `None` when the closer is missing.
    #[must_use]
    pub fn closer(&self) -> Option<&'a CstNode> {
        self.node
            .children()
            .last()
            .filter(|last| last.kind() != SyntaxKind::Missing)
    }

    /// The children between the opener keyword and the closer, trivia
    /// included.
    #[must_use]
    pub fn contents(&self) -> &'a [CstNode] {
        let children = self.node.children();
        children
            .get(1..children.len().saturating_sub(1))
            .unwrap_or_default()
    }

    /// The tokens and groups written before the first section or nested
    /// construct, without trivia: the name and type parameters of a
    /// declaration, the locator of a `use`.
    #[must_use]
    pub fn head(&self) -> Vec<&'a CstNode> {
        self.contents()
            .iter()
            .take_while(|child| {
                child.kind() != SyntaxKind::Section && !is_construct(child.kind())
            })
            .filter(|child| !is_trivia(child))
            .collect()
    }

    /// The sections, in written order.
    #[must_use]
    pub fn sections(&self) -> Vec<Section<'a>> {
        self.contents()
            .iter()
            .filter_map(|child| Section::new(child, self.text))
            .collect()
    }

    /// The constructs nested directly in this one, outside its sections.
    #[must_use]
    pub fn nested(&self) -> Vec<Self> {
        self.contents()
            .iter()
            .filter_map(|child| Self::new(child, self.text))
            .collect()
    }
}

/// A section of a construct: its keyword and its operand.
#[derive(Clone, Copy, Debug)]
pub struct Section<'a> {
    node: &'a CstNode,
    text: &'a str,
}

impl<'a> Section<'a> {
    /// The view of `node`, when it is a section of the document `text`.
    #[must_use]
    pub fn new(node: &'a CstNode, text: &'a str) -> Option<Self> {
        (node.kind() == SyntaxKind::Section).then_some(Self { node, text })
    }

    /// The node.
    #[must_use]
    pub const fn node(&self) -> &'a CstNode {
        self.node
    }

    /// The bytes from the keyword to the last operand token.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.node.span()
    }

    /// The section keyword.
    #[must_use]
    pub fn keyword(&self) -> &'a str {
        self.node
            .children()
            .first()
            .map_or("", |keyword| keyword.text(self.text))
    }

    /// The span of the section keyword.
    #[must_use]
    pub fn keyword_span(&self) -> ByteSpan {
        self.node
            .children()
            .first()
            .map_or(self.node.span(), CstNode::span)
    }

    /// The operand: the tokens, groups, and nested constructs after the
    /// keyword, without trivia, in written order.
    #[must_use]
    pub fn operand(&self) -> Vec<&'a CstNode> {
        self.node
            .children()
            .iter()
            .skip(1)
            .filter(|child| !is_trivia(child))
            .collect()
    }
}
