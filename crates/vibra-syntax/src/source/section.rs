//! Typed views of constructs and sections.
//!
//! A view is a projection of the lossless tree: it borrows the tree and the
//! document text and owns no bytes. A generic construct knows its opener
//! keyword, its head, its sections, its nested constructs, and its closer; a
//! generic section knows its keyword and its operand. The typed sections of
//! the declarations ([`Given`], [`Where`], [`TagSection`], [`Doc`]) read the
//! nodes of their productions.

use vibra_diagnostics::ByteSpan;

use super::construct::is_trivia;
use super::declaration::{Atom, Ident};
use super::types::{TypeExpr, type_entries};
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
            | SyntaxKind::Impl
            | SyntaxKind::AssocType
            | SyntaxKind::AssocChoice
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

    /// The view of a declaration node a typed view already checked.
    pub(crate) const fn of_declaration(node: &'a CstNode, text: &'a str) -> Self {
        Self { node, text }
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

/// The keyword and the operand of a typed section node: the keyword leaf
/// first, then the operand without trivia.
fn operand_of(node: &CstNode) -> Vec<&CstNode> {
    node.children()
        .iter()
        .skip(1)
        .filter(|child| !is_trivia(child))
        .collect()
}

/// `given { Parameter: Bound … }`.
#[derive(Clone, Copy, Debug)]
pub struct Given<'a> {
    node: &'a CstNode,
    text: &'a str,
}

impl<'a> Given<'a> {
    /// The view of a `@given` node of the document `text`.
    #[must_use]
    pub fn new(node: &'a CstNode, text: &'a str) -> Option<Self> {
        (node.kind() == SyntaxKind::Given).then_some(Self { node, text })
    }

    /// The node.
    #[must_use]
    pub const fn node(&self) -> &'a CstNode {
        self.node
    }

    /// The entries, each a type parameter and its bound, in written order.
    #[must_use]
    pub fn entries(&self) -> Vec<(Ident<'a>, TypeExpr<'a>)> {
        let Some(brace) = self
            .node
            .children()
            .iter()
            .find(|child| child.kind() == SyntaxKind::Brace)
        else {
            return Vec::new();
        };
        let children = brace.children();
        let inner = children
            .get(1..children.len().saturating_sub(1))
            .unwrap_or_default();
        type_entries(inner, self.text)
            .into_iter()
            .filter_map(|(label, bound)| Some((label?, bound)))
            .collect()
    }
}

/// `where predicate [else consequence]`. The operands are captured as
/// tokens here and typed by the expression grammar.
#[derive(Clone, Copy, Debug)]
pub struct Where<'a> {
    node: &'a CstNode,
    text: &'a str,
}

impl<'a> Where<'a> {
    /// The view of a `@where` node of the document `text`.
    #[must_use]
    pub fn new(node: &'a CstNode, text: &'a str) -> Option<Self> {
        (node.kind() == SyntaxKind::Where).then_some(Self { node, text })
    }

    /// The node.
    #[must_use]
    pub const fn node(&self) -> &'a CstNode {
        self.node
    }

    fn split(&self) -> (Vec<&'a CstNode>, Option<Vec<&'a CstNode>>) {
        let operand = operand_of(self.node);
        let at = operand.iter().position(|node| {
            node.kind() == SyntaxKind::ReservedWord && node.text(self.text) == "else"
        });
        match at {
            Some(at) => {
                let mut predicate = operand;
                let consequence = predicate.split_off(at).into_iter().skip(1).collect();
                (predicate, Some(consequence))
            }
            None => (operand, None),
        }
    }

    /// The predicate's tokens, groups, and constructs, without trivia.
    #[must_use]
    pub fn predicate_tokens(&self) -> Vec<&'a CstNode> {
        self.split().0
    }

    /// The consequence's tokens after `else`, when an `else` is written.
    #[must_use]
    pub fn else_tokens(&self) -> Option<Vec<&'a CstNode>> {
        self.split().1
    }
}

/// A section whose operand is one tag: `visibility`, `representation`, or
/// `role`.
#[derive(Clone, Copy, Debug)]
pub struct TagSection<'a> {
    node: &'a CstNode,
    text: &'a str,
}

impl<'a> TagSection<'a> {
    /// The view of a `@visibility`, `@representation`, or `@role` node of the
    /// document `text`.
    #[must_use]
    pub fn new(node: &'a CstNode, text: &'a str) -> Option<Self> {
        matches!(
            node.kind(),
            SyntaxKind::Visibility | SyntaxKind::Representation | SyntaxKind::Role
        )
        .then_some(Self { node, text })
    }

    /// The node.
    #[must_use]
    pub const fn node(&self) -> &'a CstNode {
        self.node
    }

    /// The section keyword.
    #[must_use]
    pub fn keyword(&self) -> &'a str {
        self.node
            .children()
            .first()
            .map_or("", |keyword| keyword.text(self.text))
    }

    /// The tag, when the operand is one atom.
    #[must_use]
    pub fn tag(&self) -> Option<Atom<'a>> {
        match operand_of(self.node).as_slice() {
            [tag] => Atom::of(tag, self.text),
            _ => None,
        }
    }
}

/// `doc """…"""`.
#[derive(Clone, Copy, Debug)]
pub struct Doc<'a> {
    node: &'a CstNode,
    text: &'a str,
}

impl<'a> Doc<'a> {
    /// The view of a `@doc` node of the document `text`.
    #[must_use]
    pub fn new(node: &'a CstNode, text: &'a str) -> Option<Self> {
        (node.kind() == SyntaxKind::Doc).then_some(Self { node, text })
    }

    /// The node.
    #[must_use]
    pub const fn node(&self) -> &'a CstNode {
        self.node
    }

    /// The doc string token as written, delimiters included.
    #[must_use]
    pub fn doc_string(&self) -> Option<&'a str> {
        match operand_of(self.node).as_slice() {
            [doc] if doc.kind() == SyntaxKind::DocString => Some(doc.text(self.text)),
            _ => None,
        }
    }
}
