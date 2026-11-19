//! Typed views of patterns.
//!
//! `docs/spec/01-source-language.md`, "Grammar" and "Patterns". One pattern
//! grammar serves `match` arms and local `let`. A pattern is no node of its
//! own: `_` is a punctuation leaf, a literal is its token, a name or a
//! constructor is a `@path` node with the `(…)` applied to it, and a brace or
//! bracket pattern is the bracket group. A single name is a binder or a
//! nullary constructor; resolution decides which.

use super::declaration::Ident;
use super::expr::{group_entries, is_mark, read_label};
use super::types::Path;
use crate::cst::{CstNode, SyntaxKind};

/// The pattern at `items[*at]`; moves past it.
pub(crate) fn read_pattern<'a>(
    items: &[&'a CstNode],
    at: &mut usize,
    text: &'a str,
) -> Option<Pattern<'a>> {
    let node = *items.get(*at)?;
    let fits = match node.kind() {
        SyntaxKind::Punctuation => is_mark(node, text, "_"),
        SyntaxKind::String
        | SyntaxKind::Character
        | SyntaxKind::Integer
        | SyntaxKind::Atom
        | SyntaxKind::Path
        | SyntaxKind::Brace
        | SyntaxKind::Bracket => true,
        _ => false,
    };
    if !fits {
        return None;
    }
    *at += 1;
    let applied = (node.kind() == SyntaxKind::Path)
        .then(|| items.get(*at).copied())
        .flatten()
        .filter(|paren| paren.kind() == SyntaxKind::Paren);
    if applied.is_some() {
        *at += 1;
    }
    Some(Pattern {
        node,
        applied,
        text,
    })
}

/// One pattern.
#[derive(Clone, Copy, Debug)]
pub struct Pattern<'a> {
    node: &'a CstNode,
    applied: Option<&'a CstNode>,
    text: &'a str,
}

/// One level of a pattern, as [`Pattern::form`] reads it.
#[derive(Clone, Debug)]
pub enum PatternForm<'a> {
    /// `_`.
    Discard,
    /// A string, character, integer, or atom literal.
    Literal(&'a CstNode),
    /// A dotted name: a binder or a nullary constructor.
    Path(Path<'a>),
    /// `path(…)`: a constructor and its positional or labelled patterns.
    Apply {
        /// The constructor.
        path: Path<'a>,
        /// The positional patterns, in written order.
        positional: Vec<Pattern<'a>>,
        /// The labelled patterns, in written order.
        labelled: Vec<(Ident<'a>, Pattern<'a>)>,
    },
    /// `{p q}`.
    Tuple(Vec<Pattern<'a>>),
    /// `{name: p …}`.
    Record(Vec<(Ident<'a>, Pattern<'a>)>),
    /// `[p q]`.
    List(Vec<Pattern<'a>>),
}

/// The positional and the labelled patterns of a group.
type Entries<'a> = (Vec<Pattern<'a>>, Vec<(Ident<'a>, Pattern<'a>)>);

fn entries<'a>(group: &'a CstNode, text: &'a str) -> Entries<'a> {
    let items = group_entries(group);
    let mut positional = Vec::new();
    let mut labelled = Vec::new();
    let mut at = 0;
    while at < items.len() {
        let label = read_label(&items, &mut at, text);
        let Some(pattern) = read_pattern(&items, &mut at, text) else {
            break;
        };
        match label {
            Some(label) => labelled.push((label, pattern)),
            None => positional.push(pattern),
        }
    }
    (positional, labelled)
}

impl<'a> Pattern<'a> {
    /// The node the pattern starts with: a leaf, a `@path`, or a group.
    #[must_use]
    pub const fn node(&self) -> &'a CstNode {
        self.node
    }

    /// The outermost level of the pattern.
    #[must_use]
    pub fn form(&self) -> PatternForm<'a> {
        let text = self.text;
        match self.node.kind() {
            SyntaxKind::Punctuation => PatternForm::Discard,
            SyntaxKind::Path => {
                let path = Path::of(self.node, text);
                match self.applied {
                    Some(paren) => {
                        let (positional, labelled) = entries(paren, text);
                        PatternForm::Apply {
                            path,
                            positional,
                            labelled,
                        }
                    }
                    None => PatternForm::Path(path),
                }
            }
            SyntaxKind::Brace => {
                let (positional, labelled) = entries(self.node, text);
                if labelled.is_empty() {
                    PatternForm::Tuple(positional)
                } else {
                    PatternForm::Record(labelled)
                }
            }
            SyntaxKind::Bracket => PatternForm::List(entries(self.node, text).0),
            _ => PatternForm::Literal(self.node),
        }
    }
}
