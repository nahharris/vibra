//! The nesting of a document: how many forms enclose each token.
//!
//! `docs/spec/05-tooling.md`, "Limits", defines the measure that the limit
//! profile's source nesting bounds. A form encloses the tokens written inside
//! it:
//!
//! - a bracket form `(…)`, `[…]`, or `{…}`, and an expression or statement
//!   construct (`do … enddo`, `if … endif`, `match … endmatch`, a `let …
//!   endlet` statement, an anonymous `fn … endfn` or `proc … endproc`), the
//!   tokens between its opener and its closer;
//! - `return`, `try`, and `await`, the tokens of the expression after them;
//! - each application `(…)` and field read `.name` of a `postfix`, the tokens
//!   of the form it applies to.
//!
//! A declaration encloses nothing. The nesting of a token is the number of
//! forms that enclose it; whitespace and comments are not measured.
//!
//! This is a measurement, not a rule of the reader: the reader reads every
//! depth, and a caller decides what a nesting means. The walk is iterative,
//! so it needs no host stack in proportion to the depth.

use vibra_diagnostics::ByteSpan;

use crate::cst::{CstNode, SyntaxKind};
use crate::document::Document;
use crate::source::keyword::closer_of;

/// The construct openers whose constructs are forms: those of expressions
/// and statements. `type`, `interface`, `impl`, `use`, and `test` only open
/// declarations.
const FORM_OPENERS: &[&str] = &["do", "if", "match", "let", "fn", "proc"];

/// The prefix words of `expr` and `unary`.
const PREFIXES: &[&str] = &["return", "try", "await"];

/// The node kinds of declarations, which enclose nothing even when their
/// opener also opens an expression form (`fn`, `proc`, `let`).
const DECLARATIONS: &[SyntaxKind] = &[
    SyntaxKind::Use,
    SyntaxKind::TypeDecl,
    SyntaxKind::InterfaceDecl,
    SyntaxKind::FnDecl,
    SyntaxKind::ProcDecl,
    SyntaxKind::ModuleLet,
    SyntaxKind::Test,
    SyntaxKind::Impl,
    SyntaxKind::AssocType,
    SyntaxKind::AssocChoice,
];

/// The greatest nesting of a token of `document`, and the first token in
/// source order whose nesting is greater than `limit`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Nesting {
    /// The greatest nesting of any measured token; 0 for a document with
    /// no tree or no token.
    pub deepest: usize,
    /// The first token, in source order, nested more than the limit.
    pub first_over: Option<ByteSpan>,
}

/// Measures `document` against `limit`: its deepest nesting, and the first
/// token nested more than `limit` levels. A document with no tree, because
/// it was never read or is not UTF-8, has nesting 0.
#[must_use]
pub fn nesting(document: &Document, limit: usize) -> Nesting {
    let mut found = Nesting {
        deepest: 0,
        first_over: None,
    };
    let (Some(root), Some(text)) = (document.root(), document.text()) else {
        return found;
    };
    let mut work: Vec<(&CstNode, usize)> = vec![(root, 0)];
    while let Some((node, depth)) = work.pop() {
        if node.kind().is_leaf() {
            if matches!(
                node.kind(),
                SyntaxKind::Whitespace | SyntaxKind::Comment | SyntaxKind::Missing
            ) {
                continue;
            }
            found.deepest = found.deepest.max(depth);
            if depth > limit && found.first_over.is_none() {
                found.first_over = Some(node.span());
            }
            continue;
        }
        let depths = child_depths(node, text, depth);
        work.extend(node.children().iter().zip(depths).rev());
    }
    found
}

/// The nesting of the tokens directly in each child of `node`, whose own
/// direct tokens are nested `depth` levels.
fn child_depths(node: &CstNode, text: &str, depth: usize) -> Vec<usize> {
    let children = node.children();
    let mut depths = vec![depth; children.len()];
    if let Some(closer) = frame_closer(node, text) {
        let last = children.len().saturating_sub(1);
        for (index, slot) in depths.iter_mut().enumerate().skip(1) {
            let is_closer = index == last
                && children.get(index).is_some_and(|child| {
                    child.kind().is_leaf() && child.text(text) == closer
                });
            if !is_closer {
                *slot = depth + 1;
            }
        }
    } else if node.kind() == SyntaxKind::Postfix {
        postfix_depths(children, text, depth, &mut depths);
    }
    // A run of prefix words: each encloses what follows it.
    let mut run = 0;
    for (child, slot) in children.iter().zip(depths.iter_mut()) {
        if is_trivia(child) {
            continue;
        }
        *slot += run;
        if child.kind() == SyntaxKind::ReservedWord
            && PREFIXES.contains(&child.text(text))
        {
            run += 1;
        } else {
            run = 0;
        }
    }
    depths
}

/// The closer of `node` when it is a form whose opener is its first child: a
/// bracket form, or an expression or statement construct.
fn frame_closer(node: &CstNode, text: &str) -> Option<&'static str> {
    let first = node.children().first()?;
    if !first.kind().is_leaf() {
        return None;
    }
    match (first.kind(), first.text(text)) {
        (SyntaxKind::Punctuation, "(") => Some(")"),
        (SyntaxKind::Punctuation, "[") => Some("]"),
        (SyntaxKind::Punctuation, "{") => Some("}"),
        (SyntaxKind::ReservedWord, word)
            if FORM_OPENERS.contains(&word) && !DECLARATIONS.contains(&node.kind()) =>
        {
            closer_of(word)
        }
        _ => None,
    }
}

/// The nesting of each child of a `@postfix` whose own tokens are nested
/// `depth` levels: with `k` applications and field reads, the primary is
/// nested `depth + k`, and the `j`-th suffix (from 1) `depth + k - j`.
fn postfix_depths(
    children: &[CstNode],
    text: &str,
    depth: usize,
    depths: &mut [usize],
) {
    let significant = children
        .iter()
        .enumerate()
        .filter(|(_, child)| {
            !is_trivia(child)
                && !matches!(child.kind(), SyntaxKind::Error | SyntaxKind::Missing)
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let Some((&primary, rest)) = significant.split_first() else {
        return;
    };
    // Each suffix with the children it is made of, as `Postfix::suffixes`
    // reads them.
    let mut suffixes: Vec<Vec<usize>> = Vec::new();
    let mut at = 0;
    while let Some(&index) = rest.get(at) {
        let Some(child) = children.get(index) else {
            break;
        };
        if child.kind() == SyntaxKind::Paren {
            suffixes.push(vec![index]);
            at += 1;
        } else if let Some(&name) = rest.get(at + 1)
            && child.kind() == SyntaxKind::Punctuation
            && child.text(text) == "."
            && children.get(name).is_some_and(|name| {
                matches!(
                    name.kind(),
                    SyntaxKind::Identifier | SyntaxKind::ReservedWord
                )
            })
        {
            suffixes.push(vec![index, name]);
            at += 2;
        } else {
            break;
        }
    }
    let count = suffixes.len();
    if let Some(slot) = depths.get_mut(primary) {
        *slot = depth + count;
    }
    for (position, parts) in suffixes.iter().enumerate() {
        for &index in parts {
            if let Some(slot) = depths.get_mut(index) {
                *slot = depth + (count - position - 1);
            }
        }
    }
}

fn is_trivia(node: &CstNode) -> bool {
    matches!(node.kind(), SyntaxKind::Whitespace | SyntaxKind::Comment)
}
