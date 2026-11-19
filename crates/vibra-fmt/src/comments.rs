//! The comments inside a source document's forms, each attached to a token.
//!
//! `docs/spec/01-source-language.md`, "Canonical format": a comment on a
//! construct's line stays at the end of that line, and any other comment
//! stays on its own line before the form that follows it. So a comment with
//! a token before it on its line is attached after that token, and any other
//! comment before the token that follows it. Whatever lays out a token writes
//! its comments with it, so a comment moves with its token and no comment is
//! lost or written twice.
//!
//! The comments between top-level forms are the top level's own; this table
//! holds only the comments inside a form.

use std::collections::HashMap;

use vibra_syntax::{CstNode, SyntaxKind};

use crate::layout::{Item, trailing_comment};

/// A comment written after a token.
struct Note {
    text: String,
    /// The comment at the end of the token's line, with the spaces written
    /// before it; `None` for a comment on a line of its own after the last
    /// token of a form.
    trailing: Option<Item>,
}

/// The comments of a document, by the start of the token they belong to.
#[derive(Default)]
pub(crate) struct Comments {
    /// The comments on their own lines before a token.
    before: HashMap<usize, Vec<String>>,
    /// The comment at the end of a token's line.
    after: HashMap<usize, Vec<Note>>,
    /// The comments after the last token of a group, held until the group
    /// ends, innermost last.
    held: Vec<Vec<Note>>,
}

/// Whether a node holds no token.
const fn is_trivia(kind: SyntaxKind) -> bool {
    matches!(kind, SyntaxKind::Whitespace | SyntaxKind::Comment)
}

/// The first token of `node`.
fn first_token(node: &CstNode) -> &CstNode {
    let mut current = node;
    while !current.kind().is_leaf() {
        match current
            .children()
            .iter()
            .find(|child| !is_trivia(child.kind()))
        {
            Some(child) => current = child,
            None => break,
        }
    }
    current
}

/// The last token of `node`.
fn last_token(node: &CstNode) -> &CstNode {
    let mut current = node;
    while !current.kind().is_leaf() {
        match current
            .children()
            .iter()
            .rev()
            .find(|child| !is_trivia(child.kind()))
        {
            Some(child) => current = child,
            None => break,
        }
    }
    current
}

/// Whether a token is written before `start` on its line.
fn follows_a_token(text: &str, start: usize) -> bool {
    text.get(..start)
        .unwrap_or_default()
        .chars()
        .rev()
        .find(|character| !matches!(character, ' ' | '\t'))
        .is_some_and(|character| !matches!(character, '\n' | '\r'))
}

fn write_note(note: Note, out: &mut Vec<Item>) {
    match note.trailing {
        Some(trailing) => out.extend([trailing, Item::Line]),
        None => out.extend([Item::Line, Item::Comment(note.text), Item::Line]),
    }
}

impl Comments {
    /// The comments inside the forms of `root`.
    pub(crate) fn of(root: &CstNode, text: &str) -> Self {
        let mut comments = Self::default();
        let mut previous: Option<usize> = None;
        let mut waiting: Vec<String> = Vec::new();
        for form in root.children() {
            if is_trivia(form.kind()) {
                continue;
            }
            for leaf in form.leaves() {
                let start = leaf.span().start();
                match leaf.kind() {
                    SyntaxKind::Whitespace => {}
                    SyntaxKind::Comment => {
                        let comment = leaf.text(text).trim_end().to_owned();
                        match previous {
                            Some(token) if follows_a_token(text, start) => {
                                comments.after.entry(token).or_default().push(Note {
                                    text: comment,
                                    trailing: Some(trailing_comment(text, start)),
                                });
                            }
                            _ => waiting.push(comment),
                        }
                    }
                    _ => {
                        if !waiting.is_empty() {
                            comments
                                .before
                                .entry(start)
                                .or_default()
                                .append(&mut waiting);
                        }
                        previous = Some(start);
                    }
                }
            }
            if let Some(token) = previous {
                for comment in waiting.drain(..) {
                    comments.after.entry(token).or_default().push(Note {
                        text: comment,
                        trailing: None,
                    });
                }
            }
            previous = None;
        }
        comments
    }

    /// Writes the comments on their own lines before the first token of
    /// `node`.
    pub(crate) fn before(&mut self, node: &CstNode, out: &mut Vec<Item>) {
        let start = first_token(node).span().start();
        for comment in self.before.remove(&start).unwrap_or_default() {
            out.extend([Item::Line, Item::Comment(comment), Item::Line]);
        }
    }

    /// Writes the comments after the last token of `node`.
    pub(crate) fn after(&mut self, node: &CstNode, out: &mut Vec<Item>) {
        let start = last_token(node).span().start();
        for note in self.after.remove(&start).unwrap_or_default() {
            write_note(note, out);
        }
    }

    /// Holds the comments after the last token of `node` until
    /// [`Self::release`], so they are written after the group that ends with
    /// it and do not break that group.
    pub(crate) fn hold(&mut self, node: &CstNode) {
        let start = last_token(node).span().start();
        self.held
            .push(self.after.remove(&start).unwrap_or_default());
    }

    /// Writes the comments of the innermost [`Self::hold`].
    pub(crate) fn release(&mut self, out: &mut Vec<Item>) {
        for note in self.held.pop().unwrap_or_default() {
            write_note(note, out);
        }
    }

    /// Moves every comment of the tokens of `from`, which the layout removes,
    /// to the lines before the first token of `to`.
    pub(crate) fn carry(&mut self, from: &[&CstNode], to: &CstNode) {
        let mut carried = Vec::new();
        for leaf in from.iter().flat_map(|node| node.leaves()) {
            let start = leaf.span().start();
            carried.extend(self.before.remove(&start).unwrap_or_default());
            carried.extend(
                self.after
                    .remove(&start)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|note| note.text),
            );
        }
        if carried.is_empty() {
            return;
        }
        let target = self
            .before
            .entry(first_token(to).span().start())
            .or_default();
        carried.append(target);
        *target = carried;
    }
}
