//! Typed views of `fn` and `proc`, named and anonymous.
//!
//! `docs/spec/01-source-language.md`, "Grammar" and "`fn` and `proc`". A
//! named declaration is a `@fnDecl` or `@procDecl` node, at the top level or
//! as a member; an anonymous one is an `@anonFn` or `@anonProc` node, the
//! primary of a `@postfix`. Each holds its name, its type parameters, its
//! `@signature` (the `takes`, `labelled`, and `returns` sections written
//! next to each other), its other sections as `@given`, `@requires`,
//! `@ensures`, `@performs`, `@visibility`, `@external`, `@symbol`,
//! `@native`, and `@doc` nodes, and `do` and its `@body`. A [`Callable`]
//! reads every kind; the shaping is in the source shaper.

use super::construct::is_trivia;
use super::declaration::{Ident, name_of};
use super::expr::{
    Expr, group_entries, is_mark, is_word, read_expr, read_label, significant,
};
use super::keyword::is_section;
use super::pattern::{Pattern, read_pattern};
use super::section::{Construct, Doc, Given, StringSection, TagSection};
use super::statement::Body;
use super::type_decl::declared_params;
use super::types::{Row, TypeExpr, read_type};
use crate::cst::{CstNode, SyntaxKind};

/// Whether a callable is a pure `fn` or a `proc`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallableKind {
    /// `fn … endfn`.
    Fn,
    /// `proc … endproc`.
    Proc,
}

/// A `fn` or `proc`: a named declaration or an anonymous expression.
#[derive(Clone, Copy, Debug)]
pub struct Callable<'a> {
    node: &'a CstNode,
    text: &'a str,
}

/// The children of a node between its opener and its closer.
fn contents(node: &CstNode) -> &[CstNode] {
    let children = node.children();
    children
        .get(1..children.len().saturating_sub(1))
        .unwrap_or_default()
}

/// The first direct child of `node` of `kind`.
fn child(node: &CstNode, kind: SyntaxKind) -> Option<&CstNode> {
    contents(node).iter().find(|child| child.kind() == kind)
}

impl<'a> Callable<'a> {
    /// The view of a `@fnDecl`, `@procDecl`, `@anonFn`, or `@anonProc` node
    /// of the document `text`.
    #[must_use]
    pub fn new(node: &'a CstNode, text: &'a str) -> Option<Self> {
        matches!(
            node.kind(),
            SyntaxKind::FnDecl
                | SyntaxKind::ProcDecl
                | SyntaxKind::AnonFn
                | SyntaxKind::AnonProc
        )
        .then_some(Self { node, text })
    }

    /// The node.
    #[must_use]
    pub const fn node(&self) -> &'a CstNode {
        self.node
    }

    /// The generic construct view.
    #[must_use]
    pub const fn construct(&self) -> Construct<'a> {
        Construct::of_declaration(self.node, self.text)
    }

    /// `fn` or `proc`.
    #[must_use]
    pub fn kind(&self) -> CallableKind {
        match self.node.kind() {
            SyntaxKind::ProcDecl | SyntaxKind::AnonProc => CallableKind::Proc,
            _ => CallableKind::Fn,
        }
    }

    /// Whether the callable is an anonymous expression.
    #[must_use]
    pub fn is_anonymous(&self) -> bool {
        matches!(self.node.kind(), SyntaxKind::AnonFn | SyntaxKind::AnonProc)
    }

    /// The opener keyword, `fn` or `proc`.
    #[must_use]
    pub fn keyword(&self) -> &'a str {
        self.construct().keyword()
    }

    /// The declared name; `None` for an anonymous callable.
    #[must_use]
    pub fn name(&self) -> Option<Ident<'a>> {
        if self.is_anonymous() {
            return None;
        }
        let first = contents(self.node).iter().find(|child| !is_trivia(child))?;
        let name = name_of(first, self.text)?;
        (!name.is_reserved() || !is_section(name.text())).then_some(name)
    }

    /// The type parameters, in written order.
    #[must_use]
    pub fn params(&self) -> Vec<Ident<'a>> {
        declared_params(self.node, self.text)
    }

    /// The `@signature` nodes, in written order: one, unless another
    /// section is written between two signature sections.
    #[must_use]
    pub fn signatures(&self) -> Vec<Signature<'a>> {
        contents(self.node)
            .iter()
            .filter(|child| child.kind() == SyntaxKind::Signature)
            .map(|node| Signature {
                node,
                text: self.text,
            })
            .collect()
    }

    /// The `takes` parameters, each a pattern and its type.
    #[must_use]
    pub fn takes(&self) -> Vec<(Pattern<'a>, TypeExpr<'a>)> {
        self.signatures()
            .iter()
            .flat_map(Signature::takes)
            .collect()
    }

    /// The `labelled` parameters, each a name, a type, and a default.
    #[must_use]
    pub fn labelled(&self) -> Vec<(Ident<'a>, TypeExpr<'a>, Expr<'a>)> {
        self.signatures()
            .iter()
            .flat_map(Signature::labelled)
            .collect()
    }

    /// The result name and type of `returns`.
    #[must_use]
    pub fn result(&self) -> Option<(Ident<'a>, TypeExpr<'a>)> {
        self.signatures().iter().find_map(Signature::result)
    }

    /// The `given` section.
    #[must_use]
    pub fn given(&self) -> Option<Given<'a>> {
        child(self.node, SyntaxKind::Given).and_then(|node| Given::new(node, self.text))
    }

    /// The `requires` guard.
    #[must_use]
    pub fn requires(&self) -> Option<Guard<'a>> {
        child(self.node, SyntaxKind::Requires)
            .and_then(|node| Guard::new(node, self.text))
    }

    /// The `ensures` guard.
    #[must_use]
    pub fn ensures(&self) -> Option<Guard<'a>> {
        child(self.node, SyntaxKind::Ensures)
            .and_then(|node| Guard::new(node, self.text))
    }

    /// The `performs` row of a `proc`.
    #[must_use]
    pub fn performs(&self) -> Option<Row<'a>> {
        performs_row(self.node, self.text)
    }

    /// The `visibility` section.
    #[must_use]
    pub fn visibility(&self) -> Option<TagSection<'a>> {
        child(self.node, SyntaxKind::Visibility)
            .and_then(|node| TagSection::new(node, self.text))
    }

    /// The `external` section.
    #[must_use]
    pub fn external(&self) -> Option<TagSection<'a>> {
        child(self.node, SyntaxKind::External)
            .and_then(|node| TagSection::new(node, self.text))
    }

    /// The `symbol` section.
    #[must_use]
    pub fn symbol(&self) -> Option<StringSection<'a>> {
        child(self.node, SyntaxKind::Symbol)
            .and_then(|node| StringSection::new(node, self.text))
    }

    /// The `native` section.
    #[must_use]
    pub fn native(&self) -> Option<StringSection<'a>> {
        child(self.node, SyntaxKind::Native)
            .and_then(|node| StringSection::new(node, self.text))
    }

    /// The `doc` section.
    #[must_use]
    pub fn doc(&self) -> Option<Doc<'a>> {
        child(self.node, SyntaxKind::Doc).and_then(|node| Doc::new(node, self.text))
    }

    /// The body after `do`; `None` when no `do` is written.
    #[must_use]
    pub fn body(&self) -> Option<Body<'a>> {
        child(self.node, SyntaxKind::Body).map(|node| Body::of(node, self.text))
    }
}

/// The row of the `@performs` child of `node`.
pub(crate) fn performs_row<'a>(node: &'a CstNode, text: &'a str) -> Option<Row<'a>> {
    child(node, SyntaxKind::Performs)?
        .children()
        .iter()
        .find_map(|child| Row::of(child, text))
}

/// The parameters of a `@params` node: a pattern and its type each.
pub(crate) fn params_of<'a>(
    params: &'a CstNode,
    text: &'a str,
) -> Vec<(Pattern<'a>, TypeExpr<'a>)> {
    let items = group_entries(params);
    let mut at = 0;
    let mut entries = Vec::new();
    while let Some(pattern) = read_pattern(&items, &mut at, text) {
        if !items.get(at).is_some_and(|colon| is_mark(colon, text, ":")) {
            break;
        }
        at += 1;
        let Some(ty) = read_type(&items, &mut at, text) else {
            break;
        };
        entries.push((pattern, ty));
    }
    entries
}

/// `[takes {…}] [labelled {…}] returns name: Type`, or the part of it
/// written in one run.
#[derive(Clone, Copy, Debug)]
pub struct Signature<'a> {
    node: &'a CstNode,
    text: &'a str,
}

impl<'a> Signature<'a> {
    /// The `@signature` node.
    #[must_use]
    pub const fn node(&self) -> &'a CstNode {
        self.node
    }

    fn part(&self, kind: SyntaxKind) -> Option<&'a CstNode> {
        self.node
            .children()
            .iter()
            .find(|child| child.kind() == kind)
    }

    /// The `takes` parameters, each a pattern and its type.
    #[must_use]
    pub fn takes(&self) -> Vec<(Pattern<'a>, TypeExpr<'a>)> {
        self.part(SyntaxKind::Params)
            .map(|params| params_of(params, self.text))
            .unwrap_or_default()
    }

    /// The `labelled` parameters, each a name, a type, and a default.
    #[must_use]
    pub fn labelled(&self) -> Vec<(Ident<'a>, TypeExpr<'a>, Expr<'a>)> {
        let Some(labelled) = self.part(SyntaxKind::Labelled) else {
            return Vec::new();
        };
        let text = self.text;
        let items = group_entries(labelled);
        let mut at = 0;
        let mut entries = Vec::new();
        while let Some(name) = read_label(&items, &mut at, text) {
            let Some(ty) = read_type(&items, &mut at, text) else {
                break;
            };
            if !items.get(at).is_some_and(|be| is_word(be, text, "be")) {
                break;
            }
            at += 1;
            let Some(default) = read_expr(&items, &mut at, text) else {
                break;
            };
            entries.push((name, ty, default));
        }
        entries
    }

    /// The result name and type after `returns`.
    #[must_use]
    pub fn result(&self) -> Option<(Ident<'a>, TypeExpr<'a>)> {
        let text = self.text;
        let items = significant(self.node);
        let returns = items
            .iter()
            .position(|item| is_word(item, text, "returns"))?;
        let name = name_of(items.get(returns + 1)?, text)?;
        if !items
            .get(returns + 2)
            .is_some_and(|colon| is_mark(colon, text, ":"))
        {
            return None;
        }
        let mut at = returns + 3;
        Some((name, read_type(&items, &mut at, text)?))
    }
}

/// `requires predicate [else consequence]` or `ensures predicate [else
/// consequence]`.
#[derive(Clone, Copy, Debug)]
pub struct Guard<'a> {
    node: &'a CstNode,
    text: &'a str,
}

impl<'a> Guard<'a> {
    /// The view of a `@requires` or `@ensures` node of the document `text`.
    #[must_use]
    pub fn new(node: &'a CstNode, text: &'a str) -> Option<Self> {
        matches!(node.kind(), SyntaxKind::Requires | SyntaxKind::Ensures)
            .then_some(Self { node, text })
    }

    /// The node.
    #[must_use]
    pub const fn node(&self) -> &'a CstNode {
        self.node
    }

    /// The section keyword, `requires` or `ensures`.
    #[must_use]
    pub fn keyword(&self) -> &'a str {
        self.node
            .children()
            .first()
            .map_or("", |keyword| keyword.text(self.text))
    }

    /// The predicate.
    #[must_use]
    pub fn predicate(&self) -> Option<Expr<'a>> {
        let items = significant(self.node);
        read_expr(&items, &mut 1, self.text)
    }

    /// The consequence after `else`, when an `else` is written.
    #[must_use]
    pub fn consequence(&self) -> Option<Expr<'a>> {
        let items = significant(self.node);
        let mut at = items
            .iter()
            .position(|item| is_word(item, self.text, "else"))?
            + 1;
        read_expr(&items, &mut at, self.text)
    }
}
