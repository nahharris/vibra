//! The lossless syntax tree, bracket matching, and recovery.
//!
//! Every byte of a document is in exactly one leaf of its tree. The bracket
//! forms `{…}`, `[…]`, and `(…)` are matched here with the recovery rule of
//! the diagnostics chapter (`docs/spec/07-diagnostics-and-conformance.md`,
//! "Recovery"):
//!
//! - a closer that matches the innermost open form closes it;
//! - a closer that does not, while a form of its kind is open, closes every
//!   form inside that one with a `@missing` leaf and then closes it, with one
//!   `@syntax.unmatchedDelimiter` at the closer related to the innermost
//!   opener;
//! - a closer with no form of its kind open closes nothing: it is kept as its
//!   `@punctuation` leaf inside an `@error` node, with the same code at it;
//! - a form still open at the end of the file gets a `@missing` leaf and
//!   `@syntax.unmatchedDelimiter` at its opener.
//!
//! Every recovery step consumes a token or returns. The matcher is iterative,
//! and so is dropping a tree, so nesting depth is bounded by memory only.

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};

use crate::lexer::{Token, TokenKind};

/// The kind of a node of the syntax tree.
///
/// Interior kinds are the nodes of the `@tree.v2` observation, named after
/// the grammar productions; leaf kinds are its leaf kinds. The three bracket
/// kinds are generic groups for a grammar this crate does not parse yet; the
/// tree observation writes their children in their place.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SyntaxKind {
    /// The document node of a `.vibon` document.
    Data,
    /// The document node of a `.vib` document.
    Source,
    /// The `tuple` production.
    Tuple,
    /// The `record` production.
    Record,
    /// The `field` production.
    Field,
    /// The `list` production.
    List,
    /// The `dict` production.
    Dict,
    /// The `entry` production.
    Entry,
    /// Tokens a recovery step consumed without building a form.
    Error,
    /// A generic `{…}` group.
    Brace,
    /// A generic `[…]` group.
    Bracket,
    /// A generic `(…)` group.
    Paren,
    /// An identifier leaf.
    Identifier,
    /// A reserved word leaf.
    ReservedWord,
    /// An integer leaf.
    Integer,
    /// A float leaf.
    Float,
    /// An atom leaf.
    Atom,
    /// A string leaf.
    String,
    /// A doc string leaf.
    DocString,
    /// A character leaf.
    Character,
    /// Punctuation, the discard, and a character that starts no token.
    Punctuation,
    /// A whitespace leaf.
    Whitespace,
    /// A comment leaf.
    Comment,
    /// An empty leaf where a required element was not written.
    Missing,
}

impl SyntaxKind {
    /// The `kind` atom of this node in the `@tree.v2` observation, or `None`
    /// for a generic group, which is never a node there.
    #[must_use]
    pub const fn tree_kind(self) -> Option<&'static str> {
        Some(match self {
            Self::Data => "@data",
            Self::Source => "@source",
            Self::Tuple => "@tuple",
            Self::Record => "@record",
            Self::Field => "@field",
            Self::List => "@list",
            Self::Dict => "@dict",
            Self::Entry => "@entry",
            Self::Error => "@error",
            Self::Brace | Self::Bracket | Self::Paren => return None,
            Self::Identifier => "@identifier",
            Self::ReservedWord => "@reservedWord",
            Self::Integer => "@integer",
            Self::Float => "@float",
            Self::Atom => "@atom",
            Self::String => "@string",
            Self::DocString => "@docString",
            Self::Character => "@character",
            Self::Punctuation => "@punctuation",
            Self::Whitespace => "@whitespace",
            Self::Comment => "@comment",
            Self::Missing => "@missing",
        })
    }

    /// Whether nodes of this kind are leaves.
    #[must_use]
    pub const fn is_leaf(self) -> bool {
        matches!(
            self,
            Self::Identifier
                | Self::ReservedWord
                | Self::Integer
                | Self::Float
                | Self::Atom
                | Self::String
                | Self::DocString
                | Self::Character
                | Self::Punctuation
                | Self::Whitespace
                | Self::Comment
                | Self::Missing
        )
    }

    /// The leaf kind of a token.
    #[must_use]
    pub const fn of_token(kind: TokenKind) -> Self {
        match kind {
            TokenKind::Whitespace => Self::Whitespace,
            TokenKind::Comment => Self::Comment,
            TokenKind::Identifier => Self::Identifier,
            TokenKind::ReservedWord => Self::ReservedWord,
            TokenKind::Atom => Self::Atom,
            TokenKind::Integer => Self::Integer,
            TokenKind::Float => Self::Float,
            TokenKind::String => Self::String,
            TokenKind::DocString => Self::DocString,
            TokenKind::Character => Self::Character,
            TokenKind::Discard
            | TokenKind::OpenParen
            | TokenKind::CloseParen
            | TokenKind::OpenBrace
            | TokenKind::CloseBrace
            | TokenKind::OpenBracket
            | TokenKind::CloseBracket
            | TokenKind::Colon
            | TokenKind::Dot
            | TokenKind::Stray
            | TokenKind::Undecodable => Self::Punctuation,
        }
    }
}

/// One node of the lossless syntax tree.
///
/// A leaf holds the span of its exact text; an interior node holds its
/// children in source order, and its span runs from its first child to its
/// last. Leaves own no bytes: [`CstNode::text`] slices the document text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CstNode {
    kind: SyntaxKind,
    span: ByteSpan,
    children: Vec<CstNode>,
}

impl CstNode {
    /// A leaf of `kind` over `span`.
    #[must_use]
    pub const fn leaf(kind: SyntaxKind, span: ByteSpan) -> Self {
        Self {
            kind,
            span,
            children: Vec::new(),
        }
    }

    /// An interior node of `kind`. Its span runs from its first child to its
    /// last; a node with no child is empty at `at`.
    #[must_use]
    pub fn interior(kind: SyntaxKind, children: Vec<Self>, at: usize) -> Self {
        let span = match (children.first(), children.last()) {
            (Some(first), Some(last)) => {
                ByteSpan::new(first.span.start(), last.span.end())
            }
            _ => ByteSpan::empty_at(at),
        };
        Self {
            kind,
            span,
            children,
        }
    }

    /// The node's kind.
    #[must_use]
    pub const fn kind(&self) -> SyntaxKind {
        self.kind
    }

    /// The bytes the node covers.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }

    /// The children, in source order; empty for a leaf.
    #[must_use]
    pub fn children(&self) -> &[Self] {
        &self.children
    }

    /// Takes the children out of this node, leaving it with none.
    pub fn take_children(&mut self) -> Vec<Self> {
        std::mem::take(&mut self.children)
    }

    /// The text of the node in `source`, the document it was parsed from.
    #[must_use]
    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        self.span.text(source).unwrap_or_default()
    }

    /// Every leaf below this node, in source order.
    #[must_use]
    pub fn leaves(&self) -> Vec<&Self> {
        let mut leaves = Vec::new();
        let mut stack = vec![self];
        while let Some(node) = stack.pop() {
            if node.kind.is_leaf() {
                leaves.push(node);
            } else {
                stack.extend(node.children.iter().rev());
            }
        }
        leaves
    }
}

impl Drop for CstNode {
    fn drop(&mut self) {
        let mut stack = std::mem::take(&mut self.children);
        while let Some(mut node) = stack.pop() {
            stack.append(&mut node.children);
        }
    }
}

/// The kind of a bracket form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GroupKind {
    Brace,
    Bracket,
    Paren,
}

impl GroupKind {
    const fn opened_by(kind: TokenKind) -> Option<Self> {
        match kind {
            TokenKind::OpenBrace => Some(Self::Brace),
            TokenKind::OpenBracket => Some(Self::Bracket),
            TokenKind::OpenParen => Some(Self::Paren),
            _ => None,
        }
    }

    const fn closed_by(kind: TokenKind) -> Option<Self> {
        match kind {
            TokenKind::CloseBrace => Some(Self::Brace),
            TokenKind::CloseBracket => Some(Self::Bracket),
            TokenKind::CloseParen => Some(Self::Paren),
            _ => None,
        }
    }
}

/// Builds the items of a tree from the bracket structure of a token stream.
///
/// The matcher calls [`Shaper::group`] when a form closes, with every item of
/// the form: its opener first, then its contents, then its closer or a
/// `@missing` leaf. A grammar decides there what node the form is.
pub(crate) trait Shaper {
    /// What the matcher collects.
    type Item;
    /// A form of `kind` opens.
    fn open(&mut self, kind: GroupKind);
    /// One token outside any recovery.
    fn token(&mut self, token: Token) -> Self::Item;
    /// A `@missing` leaf at `at`.
    fn missing(&mut self, at: usize) -> Self::Item;
    /// A closer that closes nothing, kept inside an `@error` node.
    fn unmatched(&mut self, closer: Token) -> Self::Item;
    /// A form of `kind` with its items, opener and closer included.
    fn group(&mut self, kind: GroupKind, items: Vec<Self::Item>) -> Self::Item;
}

struct Frame<T> {
    kind: GroupKind,
    opener: ByteSpan,
    items: Vec<T>,
}

/// Matches the brackets of `tokens`, a document of `end` bytes, and returns
/// the items of the document's top level.
pub(crate) fn match_brackets<S: Shaper>(
    tokens: &[Token],
    end: usize,
    shaper: &mut S,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<S::Item> {
    let mut top = Vec::new();
    let mut stack: Vec<Frame<S::Item>> = Vec::new();
    for token in tokens {
        if let Some(kind) = GroupKind::opened_by(token.kind) {
            shaper.open(kind);
            let opener = shaper.token(*token);
            stack.push(Frame {
                kind,
                opener: token.span,
                items: vec![opener],
            });
            continue;
        }
        let Some(kind) = GroupKind::closed_by(token.kind) else {
            let item = shaper.token(*token);
            current(&mut stack, &mut top).push(item);
            continue;
        };
        let Some(position) = stack.iter().rposition(|frame| frame.kind == kind) else {
            diagnostics.push(Diagnostic::new(
                DiagnosticCode::SyntaxUnmatchedDelimiter,
                token.span,
                "this closer closes nothing",
            ));
            let item = shaper.unmatched(*token);
            current(&mut stack, &mut top).push(item);
            continue;
        };
        if position + 1 != stack.len()
            && let Some(innermost) = stack.last()
        {
            diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::SyntaxUnmatchedDelimiter,
                    token.span,
                    "this closer does not match the innermost open form",
                )
                .with_related(innermost.opener, "the innermost open form"),
            );
        }
        while stack.len() > position + 1 {
            let missing = shaper.missing(token.span.start());
            close(&mut stack, &mut top, shaper, missing);
        }
        let closer = shaper.token(*token);
        close(&mut stack, &mut top, shaper, closer);
    }
    while let Some(frame) = stack.last() {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::SyntaxUnmatchedDelimiter,
            frame.opener,
            "this form is never closed",
        ));
        let missing = shaper.missing(end);
        close(&mut stack, &mut top, shaper, missing);
    }
    top
}

fn current<'a, T>(stack: &'a mut [Frame<T>], top: &'a mut Vec<T>) -> &'a mut Vec<T> {
    match stack.last_mut() {
        Some(frame) => &mut frame.items,
        None => top,
    }
}

/// Closes the innermost form with `last`, its closer or a `@missing` leaf.
fn close<S: Shaper>(
    stack: &mut Vec<Frame<S::Item>>,
    top: &mut Vec<S::Item>,
    shaper: &mut S,
    last: S::Item,
) {
    let Some(mut frame) = stack.pop() else {
        return;
    };
    frame.items.push(last);
    let item = shaper.group(frame.kind, frame.items);
    current(stack, top).push(item);
}

/// The shaper of a grammar this crate does not parse: every form is a
/// generic group.
#[derive(Debug, Default)]
pub(crate) struct GenericShaper;

impl Shaper for GenericShaper {
    type Item = CstNode;

    fn open(&mut self, _kind: GroupKind) {}

    fn token(&mut self, token: Token) -> CstNode {
        CstNode::leaf(SyntaxKind::of_token(token.kind), token.span)
    }

    fn missing(&mut self, at: usize) -> CstNode {
        CstNode::leaf(SyntaxKind::Missing, ByteSpan::empty_at(at))
    }

    fn unmatched(&mut self, closer: Token) -> CstNode {
        error_around(closer)
    }

    fn group(&mut self, kind: GroupKind, items: Vec<CstNode>) -> CstNode {
        let at = items.first().map_or(0, |item| item.span().start());
        let kind = match kind {
            GroupKind::Brace => SyntaxKind::Brace,
            GroupKind::Bracket => SyntaxKind::Bracket,
            GroupKind::Paren => SyntaxKind::Paren,
        };
        CstNode::interior(kind, items, at)
    }
}

/// An `@error` node holding the `@punctuation` leaf of an unmatched closer.
pub(crate) fn error_around(closer: Token) -> CstNode {
    CstNode::interior(
        SyntaxKind::Error,
        vec![CstNode::leaf(SyntaxKind::Punctuation, closer.span)],
        closer.span.start(),
    )
}
