//! Typed views of statements, bodies, and the two `let` forms.
//!
//! `docs/spec/01-source-language.md`, "Grammar", "Bodies and statements",
//! and "Module values". A body is a sequence of statements, each a local
//! `let` or an expression. The module value `let name: Type … be expr
//! endlet` and the local `let pattern [: Type] be expr [else body] endlet`
//! are two views over the one `let` construct, read in the grammar of where
//! it is written.

use super::declaration::{Ident, name_of};
use super::expr::{Expr, is_mark, is_word, read_expr, significant};
use super::pattern::{Pattern, read_pattern};
use super::section::{Construct, Doc, TagSection};
use super::types::{TypeExpr, type_entries};
use crate::cst::{CstNode, SyntaxKind};

/// One statement of a body.
#[derive(Clone, Debug)]
pub enum Statement<'a> {
    /// A local binding.
    Let(Let<'a>),
    /// An expression.
    Expr(Expr<'a>),
}

/// `body = { statement }`.
#[derive(Clone, Copy, Debug)]
pub struct Body<'a> {
    node: &'a CstNode,
    text: &'a str,
}

impl<'a> Body<'a> {
    pub(crate) const fn of(node: &'a CstNode, text: &'a str) -> Self {
        Self { node, text }
    }

    /// The `@body` node.
    #[must_use]
    pub const fn node(&self) -> &'a CstNode {
        self.node
    }

    /// The statements, in written order.
    #[must_use]
    pub fn statements(&self) -> Vec<Statement<'a>> {
        let items = significant(self.node);
        let mut statements = Vec::new();
        let mut at = 0;
        while let Some(&item) = items.get(at) {
            if item.kind() == SyntaxKind::Let {
                statements.push(Statement::Let(Let::of(item, self.text)));
                at += 1;
                continue;
            }
            match read_expr(&items, &mut at, self.text) {
                Some(expr) => statements.push(Statement::Expr(expr)),
                None => at += 1,
            }
        }
        statements
    }
}

/// The written type of a `let` node: the type expression between `:` and
/// the `be` keyword.
fn written_type<'a>(node: &'a CstNode, text: &'a str) -> Option<TypeExpr<'a>> {
    let children = node.children();
    let colon = children
        .iter()
        .position(|child| is_mark(child, text, ":"))?;
    let be = children
        .iter()
        .position(|child| is_word(child, text, "be"))?;
    let between = children.get(colon + 1..be)?;
    type_entries(between, text)
        .into_iter()
        .find_map(|(label, ty)| label.is_none().then_some(ty))
}

/// The expression after the `be` keyword of a `let` node.
fn value<'a>(node: &'a CstNode, text: &'a str) -> Option<Expr<'a>> {
    let items = significant(node);
    let mut at = items.iter().position(|item| is_word(item, text, "be"))? + 1;
    read_expr(&items, &mut at, text)
}

/// `let pattern [: Type] be expr [else body] endlet`.
#[derive(Clone, Copy, Debug)]
pub struct Let<'a> {
    node: &'a CstNode,
    text: &'a str,
}

impl<'a> Let<'a> {
    pub(crate) const fn of(node: &'a CstNode, text: &'a str) -> Self {
        Self { node, text }
    }

    /// The `@let` node.
    #[must_use]
    pub const fn node(&self) -> &'a CstNode {
        self.node
    }

    /// The pattern after `let`.
    #[must_use]
    pub fn pattern(&self) -> Option<Pattern<'a>> {
        let items = significant(self.node);
        read_pattern(&items, &mut 1, self.text)
    }

    /// The type written after `:`, when one is.
    #[must_use]
    pub fn written_type(&self) -> Option<TypeExpr<'a>> {
        written_type(self.node, self.text)
    }

    /// The initializer after `be`.
    #[must_use]
    pub fn value(&self) -> Option<Expr<'a>> {
        value(self.node, self.text)
    }

    /// The body after `else`, run when the pattern does not match.
    #[must_use]
    pub fn fallback(&self) -> Option<Body<'a>> {
        self.node
            .children()
            .iter()
            .find(|child| child.kind() == SyntaxKind::Body)
            .map(|body| Body::of(body, self.text))
    }
}

/// `let name: Type { visibility | doc } be expr endlet`, a module value.
#[derive(Clone, Copy, Debug)]
pub struct ModuleLet<'a> {
    node: &'a CstNode,
    text: &'a str,
}

impl<'a> ModuleLet<'a> {
    /// The view of a `@moduleLet` node of the document `text`.
    #[must_use]
    pub fn new(node: &'a CstNode, text: &'a str) -> Option<Self> {
        (node.kind() == SyntaxKind::ModuleLet).then_some(Self { node, text })
    }

    /// The generic view.
    #[must_use]
    pub const fn construct(&self) -> Construct<'a> {
        Construct::of_declaration(self.node, self.text)
    }

    /// The `@moduleLet` node.
    #[must_use]
    pub const fn node(&self) -> &'a CstNode {
        self.node
    }

    /// The declared name; a reserved word recovered as the name is one too.
    #[must_use]
    pub fn name(&self) -> Option<Ident<'a>> {
        significant(self.node)
            .get(1)
            .and_then(|name| name_of(name, self.text))
    }

    /// The declared type.
    #[must_use]
    pub fn written_type(&self) -> Option<TypeExpr<'a>> {
        written_type(self.node, self.text)
    }

    /// The `visibility` section.
    #[must_use]
    pub fn visibility(&self) -> Option<TagSection<'a>> {
        self.node
            .children()
            .iter()
            .find_map(|child| TagSection::new(child, self.text))
    }

    /// The `doc` section.
    #[must_use]
    pub fn doc(&self) -> Option<Doc<'a>> {
        self.node
            .children()
            .iter()
            .find_map(|child| Doc::new(child, self.text))
    }

    /// The value after `be`.
    #[must_use]
    pub fn value(&self) -> Option<Expr<'a>> {
        value(self.node, self.text)
    }
}
