//! The canonical layout of expressions, statements, bodies, and patterns.
//!
//! `docs/spec/01-source-language.md`, "Canonical format":
//!
//! - a construct on one line when it holds no comment, no nested `let`, and
//!   fits within the limit;
//! - in a multi-line `if`, `then` on the condition's line and `else` alone on
//!   its line at the `if`'s indentation, each body one step in; in a
//!   multi-line `match`, each `case` one step in and its body one further
//!   step in; a multi-line `do` with its statements one step in and `enddo`
//!   on its own line;
//! - a multi-line `let` with `be` on the pattern's line, the initializer on
//!   that line when it fits and otherwise on the next line one step in,
//!   `else` at the end of the initializer's last line, the fallback one step
//!   in, and `endlet` on its own line at the `let`'s indentation; a
//!   single-line `let` ends with `endlet` on the same line;
//! - a brace or bracket form, and the arguments of an application, on one
//!   line when they fit, otherwise one entry per line, one step in, with the
//!   closing token on its own line;
//! - the statements of a callable body one step in from `do`; a guard's
//!   `else` on the next line one step in from its guard when both do not
//!   fit on one line;
//! - arguments, fields, entries, and statements in written order, always.
//!
//! A `let` in a body breaks every construct around it. A comment between two
//! statements of a body stays at the end of the statement's line or on its
//! own line; a comment anywhere else in an expression leaves the document
//! unchanged. The layout is iterative.

use vibra_syntax::{CstNode, SyntaxKind};

use crate::layout::Item;
use crate::source::{callable, flat, type_items};

/// One step of the layout.
enum Work<'a> {
    Emit(Item),
    /// An expression: its prefix words and its form node.
    Expr(Vec<&'a CstNode>),
    /// A form node in expression position, or a `let`.
    Form(&'a CstNode),
    /// A pattern: one node, or a path and the `(…)` applied to it.
    Pattern(Vec<&'a CstNode>),
    /// The statements of a `@body`, each after a break.
    Body(&'a CstNode),
    /// A written type.
    Type(Vec<&'a CstNode>),
    /// A `requires` or `ensures` guard.
    Guard(&'a CstNode),
    /// The entries of a `labelled` section.
    Labelled(&'a CstNode),
}

/// The grammar of a bracket group's entries.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Entries {
    Expressions,
    Patterns,
}

fn text_item(text: impl Into<String>) -> Work<'static> {
    Work::Emit(Item::Text(text.into()))
}

fn is_word(node: &CstNode, text: &str, word: &str) -> bool {
    node.kind() == SyntaxKind::ReservedWord && node.text(text) == word
}

fn is_mark(node: &CstNode, text: &str, mark: &str) -> bool {
    node.kind() == SyntaxKind::Punctuation && node.text(text) == mark
}

fn is_prefix(node: &CstNode, text: &str) -> bool {
    ["return", "try", "await"]
        .iter()
        .any(|word| is_word(node, text, word))
}

/// The children of `node` without whitespace; `None` when one is a comment
/// or a recovery.
fn significant(node: &CstNode) -> Option<Vec<&CstNode>> {
    let mut items = Vec::with_capacity(node.children().len());
    for child in node.children() {
        match child.kind() {
            SyntaxKind::Whitespace => {}
            SyntaxKind::Comment | SyntaxKind::Error | SyntaxKind::Missing => {
                return None;
            }
            _ => items.push(child),
        }
    }
    Some(items)
}

/// The layout items of one expression, written as `nodes`: its prefix words
/// and its form node.
pub(crate) fn expr_items(nodes: &[&CstNode], text: &str) -> Option<Vec<Item>> {
    run(Work::Expr(nodes.to_vec()), text)
}

/// The layout items of the statements of a `@body`, each after a break.
pub(crate) fn body_items(node: &CstNode, text: &str) -> Option<Vec<Item>> {
    run(Work::Body(node), text)
}

/// The layout items of a guard: its `else` on the next line, one step in,
/// when the guard does not fit on one line.
pub(crate) fn guard_items(node: &CstNode, text: &str) -> Option<Vec<Item>> {
    run(Work::Guard(node), text)
}

/// The layout items of the `@labelled` entries `name: Type be default`.
pub(crate) fn labelled_items(node: &CstNode, text: &str) -> Option<Vec<Item>> {
    run(Work::Labelled(node), text)
}

/// The layout items of a `let` node: a local `let`, or a module value with
/// no section but `be`.
pub(crate) fn let_items(node: &CstNode, text: &str) -> Option<Vec<Item>> {
    run(Work::Form(node), text)
}

fn run<'a>(start: Work<'a>, text: &str) -> Option<Vec<Item>> {
    let mut work = vec![start];
    let mut out = Vec::new();
    while let Some(step) = work.pop() {
        let mut next: Vec<Work<'a>> = Vec::new();
        match step {
            Work::Emit(item) => out.push(item),
            Work::Expr(nodes) => {
                let (form, prefixes) = nodes.split_last()?;
                for prefix in prefixes {
                    if !is_prefix(prefix, text) {
                        return None;
                    }
                    next.push(text_item(format!("{} ", prefix.text(text))));
                }
                next.push(Work::Form(form));
            }
            Work::Form(node) => form(node, text, &mut next, &mut out)?,
            Work::Pattern(nodes) => pattern(&nodes, text, &mut next)?,
            Work::Body(node) => body(node, text, &mut next)?,
            Work::Type(nodes) => out.extend(type_items(&nodes, text)?),
            Work::Guard(node) => guard(node, text, &mut next)?,
            Work::Labelled(node) => labelled(node, text, &mut next)?,
        }
        work.extend(next.into_iter().rev());
    }
    Some(out)
}

/// Schedules one form node.
fn form<'a>(
    node: &'a CstNode,
    text: &str,
    next: &mut Vec<Work<'a>>,
    out: &mut Vec<Item>,
) -> Option<()> {
    match node.kind() {
        SyntaxKind::Postfix => {
            let items = significant(node)?;
            let (primary, suffixes) = items.split_first()?;
            next.push(Work::Form(primary));
            for suffix in suffixes {
                if suffix.kind() == SyntaxKind::Paren {
                    group(suffix, Entries::Expressions, text, next)?;
                } else {
                    next.push(text_item(suffix.text(text)));
                }
            }
        }
        SyntaxKind::Brace | SyntaxKind::Bracket | SyntaxKind::Paren => {
            group(node, Entries::Expressions, text, next)?;
        }
        SyntaxKind::If => if_form(node, text, next)?,
        SyntaxKind::Match => match_form(node, text, next)?,
        SyntaxKind::DoBlock => do_form(node, text, next)?,
        SyntaxKind::Let | SyntaxKind::ModuleLet => let_form(node, text, next)?,
        SyntaxKind::AnonFn | SyntaxKind::AnonProc => callable(node, text, out)?,
        kind if kind.is_leaf() && kind != SyntaxKind::Comment => {
            next.push(text_item(node.text(text)));
        }
        _ => return None,
    }
    Some(())
}

/// The significant children of a construct, with its comments: the comments
/// written before the item at each index, as layout steps. A comment on the
/// line of the token before it stays at the end of that line; any other
/// comment goes on its own line.
struct Block<'a> {
    items: Vec<&'a CstNode>,
    comments: Vec<Vec<Work<'a>>>,
}

impl<'a> Block<'a> {
    fn of(node: &'a CstNode, text: &str) -> Option<Self> {
        let mut block = Self {
            items: Vec::new(),
            comments: vec![Vec::new()],
        };
        let mut newlines = 0;
        for child in node.children() {
            match child.kind() {
                SyntaxKind::Whitespace => {
                    newlines += child.text(text).matches('\n').count();
                    continue;
                }
                SyntaxKind::Comment => {
                    let comment = child.text(text).trim_end().to_owned();
                    let slot = block.comments.last_mut()?;
                    if newlines == 0 && !block.items.is_empty() {
                        slot.push(Work::Emit(Item::TrailingComment(comment)));
                    } else {
                        slot.extend([
                            Work::Emit(Item::Space),
                            Work::Emit(Item::Comment(comment)),
                        ]);
                    }
                }
                SyntaxKind::Error | SyntaxKind::Missing => return None,
                _ => {
                    block.items.push(child);
                    block.comments.push(Vec::new());
                }
            }
            newlines = 0;
        }
        Some(block)
    }

    fn item(&self, index: usize) -> Option<&'a CstNode> {
        self.items.get(index).copied()
    }

    /// Schedules the comments written before the item at `index`.
    fn comments_before(&mut self, index: usize, next: &mut Vec<Work<'a>>) {
        if let Some(comments) = self.comments.get_mut(index) {
            next.append(comments);
        }
    }

    /// Whether every comment was placed.
    fn placed(&self) -> Option<()> {
        self.comments.iter().all(Vec::is_empty).then_some(())
    }
}

/// `do body enddo`.
fn do_form<'a>(node: &'a CstNode, text: &str, next: &mut Vec<Work<'a>>) -> Option<()> {
    let mut block = Block::of(node, text)?;
    let body = block
        .item(1)
        .filter(|body| body.kind() == SyntaxKind::Body)?;
    next.extend([
        Work::Emit(Item::Begin),
        text_item("do"),
        Work::Emit(Item::Indent),
    ]);
    block.comments_before(1, next);
    next.push(Work::Body(body));
    block.comments_before(2, next);
    next.extend([
        Work::Emit(Item::Dedent),
        Work::Emit(Item::Space),
        text_item("enddo"),
        Work::Emit(Item::End),
    ]);
    block.placed()
}

/// `if condition then body [else body] endif`.
fn if_form<'a>(node: &'a CstNode, text: &str, next: &mut Vec<Work<'a>>) -> Option<()> {
    let mut block = Block::of(node, text)?;
    let then = block
        .items
        .iter()
        .position(|item| is_word(item, text, "then"))?;
    let condition = block.items.get(1..then)?.to_vec();
    let closer = block.items.len().saturating_sub(1);
    next.extend([
        Work::Emit(Item::Begin),
        text_item("if "),
        Work::Expr(condition),
        text_item(" then"),
        Work::Emit(Item::Indent),
    ]);
    let mut index = then + 1;
    while index < closer {
        let item = block.item(index)?;
        if is_word(item, text, "else") {
            block.comments_before(index, next);
            next.extend([
                Work::Emit(Item::Dedent),
                Work::Emit(Item::Space),
                text_item("else"),
                Work::Emit(Item::Indent),
            ]);
        } else if item.kind() == SyntaxKind::Body {
            block.comments_before(index, next);
            next.push(Work::Body(item));
        } else {
            return None;
        }
        index += 1;
    }
    block.comments_before(closer, next);
    next.extend([
        Work::Emit(Item::Dedent),
        Work::Emit(Item::Space),
        text_item("endif"),
        Work::Emit(Item::End),
    ]);
    block.placed()
}

/// `match subject arm { arm } endmatch`; each `case` one step in, its body
/// one further step in.
fn match_form<'a>(
    node: &'a CstNode,
    text: &str,
    next: &mut Vec<Work<'a>>,
) -> Option<()> {
    let mut block = Block::of(node, text)?;
    let closer = block.items.len().saturating_sub(1);
    let first_arm = block
        .items
        .iter()
        .position(|item| item.kind() == SyntaxKind::Arm)
        .unwrap_or(closer);
    let subject = block.items.get(1..first_arm)?.to_vec();
    next.extend([
        Work::Emit(Item::Begin),
        text_item("match "),
        Work::Expr(subject),
        Work::Emit(Item::Indent),
    ]);
    for index in first_arm..closer {
        let arm = block
            .item(index)
            .filter(|arm| arm.kind() == SyntaxKind::Arm)?;
        block.comments_before(index, next);
        let mut parts = Block::of(arm, text)?;
        let last = parts.items.len().saturating_sub(1);
        let body = parts
            .item(last)
            .filter(|body| body.kind() == SyntaxKind::Body)?;
        next.extend([
            Work::Emit(Item::Space),
            text_item("case "),
            Work::Pattern(parts.items.get(1..last)?.to_vec()),
            Work::Emit(Item::Indent),
        ]);
        parts.comments_before(last, next);
        next.extend([Work::Body(body), Work::Emit(Item::Dedent)]);
        parts.placed()?;
    }
    block.comments_before(closer, next);
    next.extend([
        Work::Emit(Item::Dedent),
        Work::Emit(Item::Space),
        text_item("endmatch"),
        Work::Emit(Item::End),
    ]);
    block.placed()
}

/// `let pattern [: Type] be expr [else body] endlet`; a module value's name
/// takes the place of the pattern.
fn let_form<'a>(node: &'a CstNode, text: &str, next: &mut Vec<Work<'a>>) -> Option<()> {
    let mut block = Block::of(node, text)?;
    let items = block.items.clone();
    let be = items.iter().position(|item| is_word(item, text, "be"))?;
    let colon = items
        .get(..be)?
        .iter()
        .position(|item| is_mark(item, text, ":"));
    let closer = items.len().saturating_sub(1);
    let fallback = items
        .iter()
        .skip(be)
        .position(|item| is_word(item, text, "else"))
        .map(|offset| be + offset);
    let pattern = items.get(1..colon.unwrap_or(be))?.to_vec();
    let value = items.get(be + 1..fallback.unwrap_or(closer))?.to_vec();
    next.extend([
        Work::Emit(Item::Begin),
        text_item("let "),
        Work::Pattern(pattern),
    ]);
    if let Some(colon) = colon {
        next.extend([
            text_item(": "),
            Work::Type(items.get(colon + 1..be)?.to_vec()),
        ]);
    }
    next.extend([
        text_item(" be"),
        Work::Emit(Item::Begin),
        Work::Emit(Item::Indent),
        Work::Emit(Item::Space),
        Work::Expr(value),
        Work::Emit(Item::Dedent),
        Work::Emit(Item::End),
    ]);
    if let Some(fallback) = fallback {
        let body = block
            .item(fallback + 1)
            .filter(|body| body.kind() == SyntaxKind::Body)?;
        next.extend([text_item(" else"), Work::Emit(Item::Indent)]);
        block.comments_before(fallback + 1, next);
        next.push(Work::Body(body));
        block.comments_before(closer, next);
        next.push(Work::Emit(Item::Dedent));
    }
    next.extend([
        Work::Emit(Item::Space),
        text_item("endlet"),
        Work::Emit(Item::End),
    ]);
    block.placed()
}
/// Schedules one pattern.
fn pattern<'a>(
    nodes: &[&'a CstNode],
    text: &str,
    next: &mut Vec<Work<'a>>,
) -> Option<()> {
    let (first, rest) = nodes.split_first()?;
    match first.kind() {
        SyntaxKind::Path => next.push(text_item(flat(&[first], text)?)),
        SyntaxKind::Brace | SyntaxKind::Bracket => {
            group(first, Entries::Patterns, text, next)?;
        }
        kind if kind.is_leaf() && kind != SyntaxKind::Comment => {
            next.push(text_item(first.text(text)));
        }
        _ => return None,
    }
    match rest {
        [] => {}
        [paren] if paren.kind() == SyntaxKind::Paren => {
            group(paren, Entries::Patterns, text, next)?;
        }
        _ => return None,
    }
    Some(())
}

/// Reads one expression or pattern of a group's entries from `items[*at]`.
fn entry_value<'a>(
    items: &[&'a CstNode],
    at: &mut usize,
    entries: Entries,
    text: &str,
) -> Option<Work<'a>> {
    let mut nodes = Vec::new();
    match entries {
        Entries::Expressions => {
            while let Some(&prefix) =
                items.get(*at).filter(|item| is_prefix(item, text))
            {
                nodes.push(prefix);
                *at += 1;
            }
            nodes.push(*items.get(*at)?);
            *at += 1;
            Some(Work::Expr(nodes))
        }
        Entries::Patterns => {
            let first = *items.get(*at)?;
            nodes.push(first);
            *at += 1;
            if first.kind() == SyntaxKind::Path
                && let Some(&paren) = items
                    .get(*at)
                    .filter(|item| item.kind() == SyntaxKind::Paren)
            {
                nodes.push(paren);
                *at += 1;
            }
            Some(Work::Pattern(nodes))
        }
    }
}

/// Schedules a bracket group: on one line when it fits, otherwise one entry
/// per line, one step in, with the closing token on its own line.
fn group<'a>(
    node: &'a CstNode,
    entries: Entries,
    text: &str,
    next: &mut Vec<Work<'a>>,
) -> Option<()> {
    let items = significant(node)?;
    let open = items.first()?.text(text);
    let close = items.last()?.text(text);
    let inner = items.get(1..items.len().saturating_sub(1))?;
    if inner.is_empty() {
        next.push(text_item(format!("{open}{close}")));
        return Some(());
    }
    if entries == Entries::Expressions
        && matches!(inner, [colon] if is_mark(colon, text, ":"))
    {
        next.push(text_item(format!("{open}:{close}")));
        return Some(());
    }
    next.extend([
        Work::Emit(Item::Begin),
        text_item(open),
        Work::Emit(Item::Indent),
        Work::Emit(Item::Soft),
    ]);
    let mut at = 0;
    while at < inner.len() {
        if at > 0 {
            next.push(Work::Emit(Item::Space));
        }
        let labelled = inner.get(at).is_some_and(|label| {
            matches!(
                label.kind(),
                SyntaxKind::Identifier | SyntaxKind::ReservedWord
            )
        }) && inner
            .get(at + 1)
            .is_some_and(|colon| is_mark(colon, text, ":"));
        if labelled {
            next.push(text_item(format!("{}: ", inner.get(at)?.text(text))));
            at += 2;
        }
        next.push(entry_value(inner, &mut at, entries, text)?);
        if entries == Entries::Expressions
            && inner.get(at).is_some_and(|colon| is_mark(colon, text, ":"))
        {
            at += 1;
            next.push(text_item(": "));
            next.push(entry_value(inner, &mut at, entries, text)?);
        }
    }
    next.extend([
        Work::Emit(Item::Dedent),
        Work::Emit(Item::Soft),
        text_item(close),
        Work::Emit(Item::End),
    ]);
    Some(())
}

/// Schedules the statements of a body, each after a break; a `let` breaks
/// every group around it. A comment on a statement's line stays at its end,
/// any other comment on its own line.
fn body<'a>(node: &'a CstNode, text: &str, next: &mut Vec<Work<'a>>) -> Option<()> {
    let mut newlines = 0;
    let mut written = false;
    let mut prefixes: Vec<&'a CstNode> = Vec::new();
    for child in node.children() {
        match child.kind() {
            SyntaxKind::Whitespace => {
                newlines += child.text(text).matches('\n').count();
                continue;
            }
            SyntaxKind::Comment => {
                if !prefixes.is_empty() {
                    return None;
                }
                let comment = child.text(text).trim_end().to_owned();
                if written && newlines == 0 {
                    next.push(Work::Emit(Item::TrailingComment(comment)));
                } else {
                    next.extend([
                        Work::Emit(Item::Space),
                        Work::Emit(Item::Comment(comment)),
                    ]);
                }
                written = true;
            }
            SyntaxKind::Error | SyntaxKind::Missing => return None,
            _ if is_prefix(child, text) => {
                prefixes.push(child);
                continue;
            }
            kind => {
                if kind == SyntaxKind::Let {
                    next.push(Work::Emit(Item::Force));
                }
                next.push(Work::Emit(Item::Space));
                let mut nodes = std::mem::take(&mut prefixes);
                nodes.push(child);
                next.push(Work::Expr(nodes));
                written = true;
            }
        }
        newlines = 0;
    }
    prefixes.is_empty().then_some(())
}

/// `requires predicate [else consequence]`, or the same with `ensures`.
fn guard<'a>(node: &'a CstNode, text: &str, next: &mut Vec<Work<'a>>) -> Option<()> {
    let items = significant(node)?;
    let (keyword, operand) = items.split_first()?;
    let split = operand.iter().position(|item| is_word(item, text, "else"));
    let (predicate, consequence) = match split {
        Some(at) => (operand.get(..at)?, operand.get(at + 1..)),
        None => (operand, None),
    };
    next.extend([
        Work::Emit(Item::Begin),
        text_item(format!("{} ", keyword.text(text))),
        Work::Expr(predicate.to_vec()),
    ]);
    if let Some(consequence) = consequence {
        next.extend([
            Work::Emit(Item::Indent),
            Work::Emit(Item::Space),
            text_item("else "),
            Work::Expr(consequence.to_vec()),
            Work::Emit(Item::Dedent),
        ]);
    }
    next.push(Work::Emit(Item::End));
    Some(())
}

/// `{name: Type be default …}`: on one line when it fits, otherwise one
/// entry per line, one step in, with the closing brace on its own line.
fn labelled<'a>(node: &'a CstNode, text: &str, next: &mut Vec<Work<'a>>) -> Option<()> {
    let items = significant(node)?;
    let open = items.first()?.text(text);
    let close = items.last()?.text(text);
    let inner = items.get(1..items.len().saturating_sub(1))?;
    if inner.is_empty() {
        next.push(text_item(format!("{open}{close}")));
        return Some(());
    }
    next.extend([
        Work::Emit(Item::Begin),
        text_item(open),
        Work::Emit(Item::Indent),
        Work::Emit(Item::Soft),
    ]);
    let mut at = 0;
    while at < inner.len() {
        if at > 0 {
            next.push(Work::Emit(Item::Space));
        }
        let label = *inner.get(at)?;
        if !inner
            .get(at + 1)
            .is_some_and(|colon| is_mark(colon, text, ":"))
        {
            return None;
        }
        next.push(text_item(format!("{}: ", flat(&[label], text)?)));
        at += 2;
        let be = at
            + inner
                .get(at..)?
                .iter()
                .position(|item| is_word(item, text, "be"))?;
        next.extend([Work::Type(inner.get(at..be)?.to_vec()), text_item(" be ")]);
        at = be + 1;
        next.push(entry_value(inner, &mut at, Entries::Expressions, text)?);
    }
    next.extend([
        Work::Emit(Item::Dedent),
        Work::Emit(Item::Soft),
        text_item(close),
        Work::Emit(Item::End),
    ]);
    Some(())
}
