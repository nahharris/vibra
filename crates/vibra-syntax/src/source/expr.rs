//! Typed views of expressions.
//!
//! `docs/spec/01-source-language.md`, "Grammar", "Application", "Anonymous
//! products and collections", and "Bodies and statements". An expression is
//! no node of its own: it is the `return`, `try`, and `await` tokens written
//! before one `@postfix` node, in the node that holds it. A `@postfix` holds
//! its primary, then each `(…)` application and `.name` field read in written
//! order. The views read what shaping built; a part that shaping kept in an
//! `@error` node is absent from them.

use super::callable::Callable;
use super::construct::is_trivia;
use super::declaration::{Ident, name_of};
use super::pattern::{Pattern, read_pattern};
use super::statement::{Body, Let};
use crate::cst::{CstNode, SyntaxKind};

/// The significant children of `node`: no trivia, no `@error` node, and no
/// `@missing` leaf.
pub(crate) fn significant(node: &CstNode) -> Vec<&CstNode> {
    node.children()
        .iter()
        .filter(|child| {
            !is_trivia(child)
                && !matches!(child.kind(), SyntaxKind::Error | SyntaxKind::Missing)
        })
        .collect()
}

/// The significant children of a bracket group, without its brackets.
pub(crate) fn group_entries(node: &CstNode) -> Vec<&CstNode> {
    let mut items = significant(node);
    if node
        .children()
        .last()
        .is_some_and(|last| last.kind() != SyntaxKind::Missing)
    {
        items.pop();
    }
    if !items.is_empty() {
        items.remove(0);
    }
    items
}

/// Whether `node` is the punctuation `mark` of the document `text`.
pub(crate) fn is_mark(node: &CstNode, text: &str, mark: &str) -> bool {
    node.kind() == SyntaxKind::Punctuation && node.text(text) == mark
}

/// Whether `node` is the reserved word `word` of the document `text`.
pub(crate) fn is_word(node: &CstNode, text: &str, word: &str) -> bool {
    node.kind() == SyntaxKind::ReservedWord && node.text(text) == word
}

/// The label at `items[*at]`, a name followed by `:`; moves past both.
pub(crate) fn read_label<'a>(
    items: &[&'a CstNode],
    at: &mut usize,
    text: &'a str,
) -> Option<Ident<'a>> {
    let name = items.get(*at)?;
    if !name.kind().is_leaf()
        || !items
            .get(*at + 1)
            .is_some_and(|colon| is_mark(colon, text, ":"))
    {
        return None;
    }
    let label = name_of(name, text)?;
    *at += 2;
    Some(label)
}

/// The expression at `items[*at]`: its prefix tokens and its form node;
/// moves past it.
pub(crate) fn read_expr<'a>(
    items: &[&'a CstNode],
    at: &mut usize,
    text: &'a str,
) -> Option<Expr<'a>> {
    let mut prefixes = Vec::new();
    while let Some(&word) = items.get(*at) {
        if !["return", "try", "await"]
            .iter()
            .any(|prefix| is_word(word, text, prefix))
        {
            break;
        }
        prefixes.push(word);
        *at += 1;
    }
    let form = *items.get(*at)?;
    if !matches!(form.kind(), SyntaxKind::Postfix | SyntaxKind::Let) {
        return None;
    }
    *at += 1;
    Some(Expr {
        prefixes,
        form,
        text,
    })
}

/// One expression: `return expr`, `try unary`, `await unary`, or a postfix
/// form. A `let` written where an expression is expected is the statement it
/// spells, reported `@syntax.misplacedBinding`.
#[derive(Clone, Debug)]
pub struct Expr<'a> {
    prefixes: Vec<&'a CstNode>,
    form: &'a CstNode,
    text: &'a str,
}

/// One level of an expression, as [`Expr::form`] reads it.
#[derive(Clone, Debug)]
pub enum ExprForm<'a> {
    /// `return expr`.
    Return(Expr<'a>),
    /// `try unary`.
    Try(Expr<'a>),
    /// `await unary`.
    Await(Expr<'a>),
    /// A primary and its applications and field reads.
    Postfix(Postfix<'a>),
    /// A misplaced `let`.
    Let(Let<'a>),
}

impl<'a> Expr<'a> {
    /// The prefix tokens, `return`, `try`, and `await`, outermost first.
    #[must_use]
    pub fn prefixes(&self) -> &[&'a CstNode] {
        &self.prefixes
    }

    /// The `@postfix` or `@let` node the prefixes apply to.
    #[must_use]
    pub const fn node(&self) -> &'a CstNode {
        self.form
    }

    /// The outermost level of the expression.
    #[must_use]
    pub fn form(&self) -> ExprForm<'a> {
        if let Some((first, rest)) = self.prefixes.split_first() {
            let operand = Self {
                prefixes: rest.to_vec(),
                form: self.form,
                text: self.text,
            };
            return match first.text(self.text) {
                "return" => ExprForm::Return(operand),
                "try" => ExprForm::Try(operand),
                _ => ExprForm::Await(operand),
            };
        }
        if self.form.kind() == SyntaxKind::Let {
            return ExprForm::Let(Let::of(self.form, self.text));
        }
        ExprForm::Postfix(Postfix {
            node: self.form,
            text: self.text,
        })
    }
}

/// `primary { "(" { argument } ")" | "." identifier }`.
#[derive(Clone, Copy, Debug)]
pub struct Postfix<'a> {
    node: &'a CstNode,
    text: &'a str,
}

/// One application or field read of a postfix form.
#[derive(Clone, Debug)]
pub enum Suffix<'a> {
    /// `(arguments…)`, in written order.
    Call(Vec<Argument<'a>>),
    /// `.name`.
    Field(Ident<'a>),
}

impl<'a> Postfix<'a> {
    /// The `@postfix` node.
    #[must_use]
    pub const fn node(&self) -> &'a CstNode {
        self.node
    }

    /// The primary the suffixes apply to.
    #[must_use]
    pub fn primary(&self) -> Option<Primary<'a>> {
        significant(self.node)
            .first()
            .and_then(|node| Primary::of(node, self.text))
    }

    /// The applications and field reads, in written order.
    #[must_use]
    pub fn suffixes(&self) -> Vec<Suffix<'a>> {
        let items = significant(self.node);
        let mut suffixes = Vec::new();
        let mut at = 1;
        while let Some(&item) = items.get(at) {
            if item.kind() == SyntaxKind::Paren {
                suffixes.push(Suffix::Call(arguments(item, self.text)));
                at += 1;
            } else if let Some(name) =
                items.get(at + 1).and_then(|name| name_of(name, self.text))
            {
                suffixes.push(Suffix::Field(name));
                at += 2;
            } else {
                break;
            }
        }
        suffixes
    }

    /// The dotted name the form spells, when it is a name and field reads
    /// only: `Str.length`, `user.name`. Resolution decides whether it is a
    /// path or a field read.
    #[must_use]
    pub fn dotted(&self) -> Option<String> {
        let Some(Primary::Name(first)) = self.primary() else {
            return None;
        };
        let mut parts = vec![first.text()];
        for suffix in self.suffixes() {
            match suffix {
                Suffix::Field(name) => parts.push(name.text()),
                Suffix::Call(_) => return None,
            }
        }
        Some(parts.join("."))
    }
}

/// `expr` or `label: expr`.
#[derive(Clone, Debug)]
pub struct Argument<'a> {
    /// The label, when one is written.
    pub label: Option<Ident<'a>>,
    /// The value.
    pub value: Expr<'a>,
}

/// The arguments of a `(…)` application.
fn arguments<'a>(paren: &'a CstNode, text: &'a str) -> Vec<Argument<'a>> {
    let items = group_entries(paren);
    let mut out = Vec::new();
    let mut at = 0;
    while at < items.len() {
        let label = read_label(&items, &mut at, text);
        let Some(value) = read_expr(&items, &mut at, text) else {
            break;
        };
        out.push(Argument { label, value });
    }
    out
}

/// The form a postfix form starts with.
#[derive(Clone, Debug)]
pub enum Primary<'a> {
    /// A name.
    Name(Ident<'a>),
    /// A string, character, integer, float, or atom literal, or `void`.
    Literal(&'a CstNode),
    /// `{a b}`.
    Tuple(Vec<Expr<'a>>),
    /// `{name: a …}`.
    Record(Vec<(Ident<'a>, Expr<'a>)>),
    /// `[a b]`.
    List(Vec<Expr<'a>>),
    /// `[key: value …]`, `[:]` when empty.
    Dict(Vec<(Expr<'a>, Expr<'a>)>),
    /// `if … endif`.
    If(If<'a>),
    /// `match … endmatch`.
    Match(Match<'a>),
    /// `do … enddo`.
    Do(DoBlock<'a>),
    /// An anonymous `fn` or `proc`.
    Callable(Callable<'a>),
}

impl<'a> Primary<'a> {
    fn of(node: &'a CstNode, text: &'a str) -> Option<Self> {
        Some(match node.kind() {
            SyntaxKind::Identifier => Self::Name(name_of(node, text)?),
            SyntaxKind::String
            | SyntaxKind::Character
            | SyntaxKind::Integer
            | SyntaxKind::Float
            | SyntaxKind::Atom
            | SyntaxKind::ReservedWord => Self::Literal(node),
            SyntaxKind::Brace => product(node, text),
            SyntaxKind::Bracket => collection(node, text),
            SyntaxKind::If => Self::If(If { node, text }),
            SyntaxKind::Match => Self::Match(Match { node, text }),
            SyntaxKind::DoBlock => Self::Do(DoBlock { node, text }),
            _ => Self::Callable(Callable::new(node, text)?),
        })
    }
}

fn product<'a>(brace: &'a CstNode, text: &'a str) -> Primary<'a> {
    let items = group_entries(brace);
    let mut at = 0;
    let mut fields = Vec::new();
    let mut components = Vec::new();
    while at < items.len() {
        let label = read_label(&items, &mut at, text);
        let Some(value) = read_expr(&items, &mut at, text) else {
            break;
        };
        match label {
            Some(label) => fields.push((label, value)),
            None => components.push(value),
        }
    }
    if fields.is_empty() {
        Primary::Tuple(components)
    } else {
        Primary::Record(fields)
    }
}

fn collection<'a>(bracket: &'a CstNode, text: &'a str) -> Primary<'a> {
    let items = group_entries(bracket);
    if matches!(items.as_slice(), [colon] if is_mark(colon, text, ":")) {
        return Primary::Dict(Vec::new());
    }
    let mut at = 0;
    let mut elements = Vec::new();
    let mut entries = Vec::new();
    while let Some(key) = read_expr(&items, &mut at, text) {
        if items.get(at).is_some_and(|colon| is_mark(colon, text, ":")) {
            at += 1;
            let Some(value) = read_expr(&items, &mut at, text) else {
                break;
            };
            entries.push((key, value));
        } else {
            elements.push(key);
        }
    }
    if entries.is_empty() {
        Primary::List(elements)
    } else {
        Primary::Dict(entries)
    }
}

/// The `@body` child of `node` after `skip` others.
fn body_child<'a>(node: &'a CstNode, skip: usize, text: &'a str) -> Option<Body<'a>> {
    node.children()
        .iter()
        .filter(|child| child.kind() == SyntaxKind::Body)
        .nth(skip)
        .map(|body| Body::of(body, text))
}

/// `if condition then body [else body] endif`.
#[derive(Clone, Copy, Debug)]
pub struct If<'a> {
    node: &'a CstNode,
    text: &'a str,
}

impl<'a> If<'a> {
    /// The `@if` node.
    #[must_use]
    pub const fn node(&self) -> &'a CstNode {
        self.node
    }

    /// The condition.
    #[must_use]
    pub fn condition(&self) -> Option<Expr<'a>> {
        let items = significant(self.node);
        read_expr(&items, &mut 1, self.text)
    }

    /// The body after `then`.
    #[must_use]
    pub fn then_body(&self) -> Option<Body<'a>> {
        body_child(self.node, 0, self.text)
    }

    /// The body after `else`, when one is written.
    #[must_use]
    pub fn else_body(&self) -> Option<Body<'a>> {
        body_child(self.node, 1, self.text)
    }
}

/// `match subject arm { arm } endmatch`.
#[derive(Clone, Copy, Debug)]
pub struct Match<'a> {
    node: &'a CstNode,
    text: &'a str,
}

impl<'a> Match<'a> {
    /// The `@match` node.
    #[must_use]
    pub const fn node(&self) -> &'a CstNode {
        self.node
    }

    /// The subject.
    #[must_use]
    pub fn subject(&self) -> Option<Expr<'a>> {
        let items = significant(self.node);
        read_expr(&items, &mut 1, self.text)
    }

    /// The arms, in written order.
    #[must_use]
    pub fn arms(&self) -> Vec<MatchArm<'a>> {
        self.node
            .children()
            .iter()
            .filter(|child| child.kind() == SyntaxKind::Arm)
            .map(|node| MatchArm {
                node,
                text: self.text,
            })
            .collect()
    }
}

/// `case pattern body`.
#[derive(Clone, Copy, Debug)]
pub struct MatchArm<'a> {
    node: &'a CstNode,
    text: &'a str,
}

impl<'a> MatchArm<'a> {
    /// The `@arm` node.
    #[must_use]
    pub const fn node(&self) -> &'a CstNode {
        self.node
    }

    /// The pattern after `case`.
    #[must_use]
    pub fn pattern(&self) -> Option<Pattern<'a>> {
        let items = significant(self.node);
        read_pattern(&items, &mut 1, self.text)
    }

    /// The body, which ends at the next `case` or at `endmatch`.
    #[must_use]
    pub fn body(&self) -> Option<Body<'a>> {
        body_child(self.node, 0, self.text)
    }
}

/// `do body enddo`.
#[derive(Clone, Copy, Debug)]
pub struct DoBlock<'a> {
    node: &'a CstNode,
    text: &'a str,
}

impl<'a> DoBlock<'a> {
    /// The `@doBlock` node.
    #[must_use]
    pub const fn node(&self) -> &'a CstNode {
        self.node
    }

    /// The statements of the block.
    #[must_use]
    pub fn body(&self) -> Option<Body<'a>> {
        body_child(self.node, 0, self.text)
    }
}
