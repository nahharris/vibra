//! The layout engine, and the canonical layout of expressions, statements,
//! bodies, patterns, and type expressions.
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
//! - a brace or bracket form, the arguments of an application, and a type
//!   expression's bracket forms on one line when they fit, otherwise one
//!   entry per line, one step in, with the closing token on its own line;
//! - the statements of a callable body one step in from `do`; a guard's
//!   `else` on the next line one step in from its guard when both do not
//!   fit on one line;
//! - arguments, fields, entries, and statements in written order, always.
//!
//! A `let` in a body breaks every construct around it. Every token is
//! written with its comments ([`crate::comments`]): a comment on a token's
//! line at the end of that line, any other comment on its own line before the
//! token that follows it, at that token's indentation. A comment before a
//! closing token is at the indentation of the contents just before it, the
//! last arm's body for `endmatch`; a comment after a closing token is
//! outside its group. A group that holds a comment is multi-line. The spaces
//! written before a comment at the end of a line are kept, as in the
//! chapter's examples: a comment starts no token.
//!
//! The layout of a whole top-level form, with its declarations, anonymous
//! callables, expressions, and types, is one iterative loop.

use vibra_syntax::{CstNode, SyntaxKind};

use crate::comments::Comments;
use crate::layout::Item;
use crate::source::declaration;

/// One step of the layout.
pub(crate) enum Work<'a> {
    Emit(Item),
    /// A token: the comments on their own lines before it, then this text.
    Text(&'a CstNode, String),
    /// The comments on their own lines before a node, alone.
    Before(&'a CstNode),
    /// The comments after a node.
    After(&'a CstNode),
    /// Holds the comments after a node, the last of a group, until
    /// [`Work::Release`].
    Hold(&'a CstNode),
    /// Writes the comments of the innermost [`Work::Hold`].
    Release,
    /// The tokens below some nodes on one line, with the spacing of the
    /// canonical format.
    Tokens(Vec<&'a CstNode>),
    /// An expression: its prefix words and its form node.
    Expr(Vec<&'a CstNode>),
    /// A form node in expression position, or a `let`.
    Form(&'a CstNode),
    /// A pattern: one node, or a path and the `(…)` applied to it.
    Pattern(Vec<&'a CstNode>),
    /// The statements of a `@body`, each after a break.
    Body(&'a CstNode),
    /// A written type: one entry of a type expression.
    Type(Vec<&'a CstNode>),
    /// One node of a type expression.
    TypeNode(&'a CstNode),
    /// A `requires` or `ensures` guard.
    Guard(&'a CstNode),
    /// The entries of a `labelled` section.
    Labelled(&'a CstNode),
    /// A declaration, a named member, or an anonymous callable.
    Declaration(&'a CstNode),
}

/// The grammar of a bracket group's entries.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Entries {
    Expressions,
    Patterns,
}

pub(crate) const fn emit<'a>(item: Item) -> Work<'a> {
    Work::Emit(item)
}

/// A token written as `written`, with its comments.
pub(crate) fn word<'a>(node: &'a CstNode, written: impl Into<String>) -> [Work<'a>; 2] {
    [Work::Text(node, written.into()), Work::After(node)]
}

/// A token written as itself, with its comments.
pub(crate) fn token<'a>(node: &'a CstNode, text: &str) -> [Work<'a>; 2] {
    word(node, node.text(text))
}

/// A closing token after the contents it closes: the comments before it one
/// step in, with the contents, unless `dedent` is false; then `separator`,
/// the token, the end of its group, and the comments after it.
pub(crate) fn close<'a>(
    node: &'a CstNode,
    text: &str,
    dedent: bool,
    separator: Item,
) -> Vec<Work<'a>> {
    let mut steps = vec![Work::Before(node)];
    if dedent {
        steps.push(emit(Item::Dedent));
    }
    steps.extend([
        emit(separator),
        Work::Text(node, node.text(text).to_owned()),
        emit(Item::End),
        Work::After(node),
    ]);
    steps
}

fn is_word(node: &CstNode, text: &str, word: &str) -> bool {
    node.kind() == SyntaxKind::ReservedWord && node.text(text) == word
}

pub(crate) fn is_colon(node: &CstNode, text: &str) -> bool {
    node.kind() == SyntaxKind::Punctuation && node.text(text) == ":"
}

fn is_prefix(node: &CstNode, text: &str) -> bool {
    ["return", "try", "await"]
        .iter()
        .any(|word| is_word(node, text, word))
}

/// The children of `node` that hold a token: not whitespace, not comments.
pub(crate) fn significant(node: &CstNode) -> Vec<&CstNode> {
    node.children()
        .iter()
        .filter(|child| {
            !matches!(child.kind(), SyntaxKind::Whitespace | SyntaxKind::Comment)
        })
        .collect()
}

/// Lays out `start` into `out`. `None` when the tree is not of a shape the
/// reader accepts.
pub(crate) fn run<'a>(
    start: Work<'a>,
    text: &str,
    comments: &mut Comments,
    out: &mut Vec<Item>,
) -> Option<()> {
    let mut work = vec![start];
    while let Some(step) = work.pop() {
        let mut next: Vec<Work<'a>> = Vec::new();
        match step {
            Work::Emit(item) => out.push(item),
            Work::Text(node, written) => {
                comments.before(node, out);
                out.push(Item::Text(written));
            }
            Work::Before(node) => comments.before(node, out),
            Work::After(node) => comments.after(node, out),
            Work::Hold(node) => comments.hold(node),
            Work::Release => comments.release(out),
            Work::Tokens(nodes) => tokens(&nodes, text, &mut next),
            Work::Expr(nodes) => {
                let (form, prefixes) = nodes.split_last()?;
                for prefix in prefixes {
                    if !is_prefix(prefix, text) {
                        return None;
                    }
                    next.extend(word(prefix, format!("{} ", prefix.text(text))));
                }
                next.push(Work::Form(form));
            }
            Work::Form(node) => form(node, text, &mut next)?,
            Work::Pattern(nodes) => pattern(&nodes, text, &mut next)?,
            Work::Body(node) => body(node, text, &mut next)?,
            Work::Type(nodes) => type_entry(&nodes, text, &mut next),
            Work::TypeNode(node) => type_node(node, text, &mut next)?,
            Work::Guard(node) => guard(node, text, &mut next)?,
            Work::Labelled(node) => labelled(node, text, &mut next)?,
            Work::Declaration(node) => declaration(node, text, comments, &mut next)?,
        }
        work.extend(next.into_iter().rev());
    }
    Some(())
}

/// The tokens below `nodes` on one line: one space between tokens, none
/// after an opening bracket or before a closing one, none before `(` or
/// `:`, and none around `.`.
fn tokens<'a>(nodes: &[&'a CstNode], text: &str, next: &mut Vec<Work<'a>>) {
    let mut previous: Option<&str> = None;
    for leaf in nodes.iter().flat_map(|node| node.leaves()) {
        if matches!(
            leaf.kind(),
            SyntaxKind::Whitespace | SyntaxKind::Missing | SyntaxKind::Comment
        ) {
            continue;
        }
        let token = leaf.text(text);
        let punctuation = leaf.kind() == SyntaxKind::Punctuation;
        let mut written = String::new();
        if let Some(previous) = previous {
            let glued_after = matches!(previous, "(" | "{" | "[" | ".");
            let glued_before =
                punctuation && matches!(token, ")" | "}" | "]" | "(" | ":" | ".");
            if !glued_after && !glued_before {
                written.push(' ');
            }
        }
        written.push_str(token);
        next.extend(word(leaf, written));
        previous = Some(if punctuation { token } else { "" });
    }
}

/// Schedules one form node.
fn form<'a>(node: &'a CstNode, text: &str, next: &mut Vec<Work<'a>>) -> Option<()> {
    match node.kind() {
        SyntaxKind::Postfix => {
            let items = significant(node);
            let (primary, suffixes) = items.split_first()?;
            next.push(Work::Form(primary));
            for suffix in suffixes {
                if suffix.kind() == SyntaxKind::Paren {
                    group(suffix, Entries::Expressions, text, next)?;
                } else {
                    next.push(Work::Tokens(vec![suffix]));
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
        SyntaxKind::AnonFn | SyntaxKind::AnonProc => next.push(Work::Declaration(node)),
        kind if kind.is_leaf() && kind != SyntaxKind::Comment => {
            next.extend(token(node, text));
        }
        _ => return None,
    }
    Some(())
}

/// `do body enddo`.
fn do_form<'a>(node: &'a CstNode, text: &str, next: &mut Vec<Work<'a>>) -> Option<()> {
    let items = significant(node);
    let [opener, body, closer] = items.as_slice() else {
        return None;
    };
    if body.kind() != SyntaxKind::Body {
        return None;
    }
    next.push(emit(Item::Begin));
    next.extend(word(opener, "do"));
    next.extend([emit(Item::Indent), Work::Body(body)]);
    next.extend(close(closer, text, true, Item::Space));
    Some(())
}

/// `if condition then body [else body] endif`.
fn if_form<'a>(node: &'a CstNode, text: &str, next: &mut Vec<Work<'a>>) -> Option<()> {
    let items = significant(node);
    let (opener, rest) = items.split_first()?;
    let (closer, rest) = rest.split_last()?;
    let then = rest.iter().position(|item| is_word(item, text, "then"))?;
    next.push(emit(Item::Begin));
    next.extend(word(opener, "if "));
    next.push(Work::Expr(rest.get(..then)?.to_vec()));
    let then_word = rest.get(then)?;
    next.extend(word(then_word, " then"));
    next.push(emit(Item::Indent));
    for item in rest.get(then + 1..)? {
        if is_word(item, text, "else") {
            next.extend([Work::Before(item), emit(Item::Dedent), emit(Item::Space)]);
            next.extend(word(item, "else"));
            next.push(emit(Item::Indent));
        } else if item.kind() == SyntaxKind::Body {
            next.push(Work::Body(item));
        } else {
            return None;
        }
    }
    next.extend(close(closer, text, true, Item::Space));
    Some(())
}

/// `match subject arm { arm } endmatch`; each `case` one step in, its body
/// one further step in.
fn match_form<'a>(
    node: &'a CstNode,
    text: &str,
    next: &mut Vec<Work<'a>>,
) -> Option<()> {
    let items = significant(node);
    let (opener, rest) = items.split_first()?;
    let (closer, rest) = rest.split_last()?;
    let first_arm = rest
        .iter()
        .position(|item| item.kind() == SyntaxKind::Arm)
        .unwrap_or(rest.len());
    next.push(emit(Item::Begin));
    next.extend(word(opener, "match "));
    next.extend([
        Work::Expr(rest.get(..first_arm)?.to_vec()),
        emit(Item::Indent),
    ]);
    let arms = rest.get(first_arm..)?;
    for (index, arm) in arms.iter().enumerate() {
        if arm.kind() != SyntaxKind::Arm {
            return None;
        }
        let parts = significant(arm);
        let (case, rest) = parts.split_first()?;
        let (body, pattern) = rest.split_last()?;
        if body.kind() != SyntaxKind::Body {
            return None;
        }
        next.push(emit(Item::Space));
        next.extend(word(case, "case "));
        next.extend([
            Work::Pattern(pattern.to_vec()),
            emit(Item::Indent),
            Work::Body(body),
        ]);
        // A comment before `endmatch` is in the last arm's body.
        if index + 1 == arms.len() {
            next.push(Work::Before(closer));
        }
        next.push(emit(Item::Dedent));
    }
    next.extend(close(closer, text, true, Item::Space));
    Some(())
}

/// `let pattern [: Type] be expr [else body] endlet`; a module value's name
/// takes the place of the pattern.
fn let_form<'a>(node: &'a CstNode, text: &str, next: &mut Vec<Work<'a>>) -> Option<()> {
    let items = significant(node);
    let be = items.iter().position(|item| is_word(item, text, "be"))?;
    let colon = items
        .get(..be)?
        .iter()
        .position(|item| is_colon(item, text));
    let closer = items.len().saturating_sub(1);
    let fallback = items
        .iter()
        .skip(be)
        .position(|item| is_word(item, text, "else"))
        .map(|offset| be + offset);
    next.push(emit(Item::Begin));
    next.extend(word(items.first()?, "let "));
    next.push(Work::Pattern(items.get(1..colon.unwrap_or(be))?.to_vec()));
    if let Some(colon) = colon {
        next.extend(word(items.get(colon)?, ": "));
        next.push(Work::Type(items.get(colon + 1..be)?.to_vec()));
    }
    next.extend(word(items.get(be)?, " be"));
    next.extend(value_group(items.get(be + 1..fallback.unwrap_or(closer))?));
    if let Some(fallback) = fallback {
        let body = items
            .get(fallback + 1)
            .filter(|body| body.kind() == SyntaxKind::Body)?;
        next.extend(word(items.get(fallback)?, " else"));
        next.extend([emit(Item::Indent), Work::Body(body)]);
    }
    next.extend(close(
        items.get(closer)?,
        text,
        fallback.is_some(),
        Item::Space,
    ));
    Some(())
}

/// The initializer after `be`: on the line of `be` when it fits, otherwise
/// on the next line one step in. A comment after its last token is written
/// after the group, so it does not move the initializer off that line.
pub(crate) fn value_group<'a>(value: &[&'a CstNode]) -> Vec<Work<'a>> {
    let mut steps = Vec::with_capacity(9);
    if let Some(last) = value.last() {
        steps.push(Work::Hold(last));
    }
    steps.extend([
        emit(Item::Begin),
        emit(Item::Indent),
        emit(Item::Space),
        Work::Expr(value.to_vec()),
        emit(Item::Dedent),
        emit(Item::End),
    ]);
    if !value.is_empty() {
        steps.push(Work::Release);
    }
    steps
}

/// Schedules one pattern.
fn pattern<'a>(
    nodes: &[&'a CstNode],
    text: &str,
    next: &mut Vec<Work<'a>>,
) -> Option<()> {
    let (first, rest) = nodes.split_first()?;
    match first.kind() {
        SyntaxKind::Path => next.push(Work::Tokens(vec![first])),
        SyntaxKind::Brace | SyntaxKind::Bracket => {
            group(first, Entries::Patterns, text, next)?;
        }
        kind if kind.is_leaf() && kind != SyntaxKind::Comment => {
            next.extend(token(first, text));
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

/// Schedules a bracket group: on one line when it fits and holds no
/// comment, otherwise one entry per line, one step in, with the closing token
/// on its own line.
fn group<'a>(
    node: &'a CstNode,
    entries: Entries,
    text: &str,
    next: &mut Vec<Work<'a>>,
) -> Option<()> {
    let items = significant(node);
    let (open, rest) = items.split_first()?;
    let (closer, inner) = rest.split_last()?;
    let empty_dict = entries == Entries::Expressions
        && matches!(inner, [colon] if is_colon(colon, text));
    if inner.is_empty() || empty_dict {
        for item in &items {
            next.extend(token(item, text));
        }
        return Some(());
    }
    next.push(emit(Item::Begin));
    next.extend(token(open, text));
    next.extend([emit(Item::Indent), emit(Item::Soft)]);
    let mut at = 0;
    while at < inner.len() {
        if at > 0 {
            next.push(emit(Item::Space));
        }
        let label = inner.get(at).filter(|label| {
            matches!(
                label.kind(),
                SyntaxKind::Identifier | SyntaxKind::ReservedWord
            )
        });
        let colon = inner.get(at + 1).filter(|colon| is_colon(colon, text));
        if let (Some(label), Some(colon)) = (label, colon) {
            next.extend(token(label, text));
            next.extend(word(colon, ": "));
            at += 2;
        }
        next.push(entry_value(inner, &mut at, entries, text)?);
        if entries == Entries::Expressions
            && let Some(colon) = inner.get(at).filter(|colon| is_colon(colon, text))
        {
            at += 1;
            next.extend(word(colon, ": "));
            next.push(entry_value(inner, &mut at, entries, text)?);
        }
    }
    next.extend(close(closer, text, true, Item::Soft));
    Some(())
}

/// Schedules the statements of a body, each after a break; a `let` breaks
/// every group around it.
fn body<'a>(node: &'a CstNode, text: &str, next: &mut Vec<Work<'a>>) -> Option<()> {
    let mut prefixes: Vec<&'a CstNode> = Vec::new();
    for child in significant(node) {
        if is_prefix(child, text) {
            prefixes.push(child);
            continue;
        }
        if child.kind() == SyntaxKind::Let {
            next.push(emit(Item::Force));
        }
        next.push(emit(Item::Space));
        let mut nodes = std::mem::take(&mut prefixes);
        nodes.push(child);
        next.push(Work::Expr(nodes));
    }
    prefixes.is_empty().then_some(())
}

/// `requires predicate [else consequence]`, or the same with `ensures`.
fn guard<'a>(node: &'a CstNode, text: &str, next: &mut Vec<Work<'a>>) -> Option<()> {
    let items = significant(node);
    let (keyword, operand) = items.split_first()?;
    let split = operand.iter().position(|item| is_word(item, text, "else"));
    let (predicate, consequence) = match split {
        Some(at) => (operand.get(..at)?, operand.get(at..)),
        None => (operand, None),
    };
    next.push(emit(Item::Begin));
    next.extend(word(keyword, format!("{} ", keyword.text(text))));
    next.push(Work::Expr(predicate.to_vec()));
    if let Some((else_word, consequence)) = consequence.and_then(<[_]>::split_first) {
        next.extend([emit(Item::Indent), emit(Item::Space)]);
        next.extend(word(else_word, "else "));
        next.extend([Work::Expr(consequence.to_vec()), emit(Item::Dedent)]);
    }
    next.push(emit(Item::End));
    Some(())
}

/// `{name: Type be default …}`: on one line when it fits, otherwise one
/// entry per line, one step in, with the closing brace on its own line.
fn labelled<'a>(node: &'a CstNode, text: &str, next: &mut Vec<Work<'a>>) -> Option<()> {
    let items = significant(node);
    let (open, rest) = items.split_first()?;
    let (closer, inner) = rest.split_last()?;
    if inner.is_empty() {
        next.extend(token(open, text));
        next.extend(token(closer, text));
        return Some(());
    }
    next.push(emit(Item::Begin));
    next.extend(token(open, text));
    next.extend([emit(Item::Indent), emit(Item::Soft)]);
    let mut at = 0;
    while at < inner.len() {
        if at > 0 {
            next.push(emit(Item::Space));
        }
        let label = *inner.get(at)?;
        let colon = inner.get(at + 1).filter(|colon| is_colon(colon, text))?;
        next.push(Work::Tokens(vec![label]));
        next.extend(word(colon, ": "));
        at += 2;
        let be = at
            + inner
                .get(at..)?
                .iter()
                .position(|item| is_word(item, text, "be"))?;
        next.push(Work::Type(inner.get(at..be)?.to_vec()));
        next.extend(word(inner.get(be)?, " be "));
        at = be + 1;
        next.push(entry_value(inner, &mut at, Entries::Expressions, text)?);
    }
    next.extend(close(closer, text, true, Item::Soft));
    Some(())
}

/// Whether a node is a bracket form of a type expression.
const fn is_type_group(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::Paren
            | SyntaxKind::Brace
            | SyntaxKind::RecordType
            | SyntaxKind::Inputs
            | SyntaxKind::Row
            | SyntaxKind::Params
    )
}

/// Splits the contents of a bracket form into entries: a new entry starts
/// at every node but a `:`, the node after a `:`, and a `(…)`.
fn entries<'a>(nodes: &[&'a CstNode], text: &str) -> Vec<Vec<&'a CstNode>> {
    let mut entries: Vec<Vec<&'a CstNode>> = Vec::new();
    let mut previous: Option<&CstNode> = None;
    for node in nodes {
        let joins = previous.is_some_and(|previous| is_colon(previous, text))
            || is_colon(node, text)
            || node.kind() == SyntaxKind::Paren;
        match entries.last_mut() {
            Some(entry) if joins => entry.push(node),
            _ => entries.push(vec![node]),
        }
        previous = Some(node);
    }
    entries
}

/// Schedules one entry of a type expression: a label and `:`, then one
/// space, then the type; a `(…)` is glued to the head it applies.
fn type_entry<'a>(nodes: &[&'a CstNode], text: &str, next: &mut Vec<Work<'a>>) {
    let nodes = nodes.iter().filter(|node| {
        !matches!(node.kind(), SyntaxKind::Whitespace | SyntaxKind::Comment)
    });
    for (index, node) in nodes.enumerate() {
        if index > 0 && !is_colon(node, text) && node.kind() != SyntaxKind::Paren {
            next.push(emit(Item::Text(" ".to_owned())));
        }
        next.push(Work::TypeNode(node));
    }
}

/// Schedules one node of a type expression: a bracket form on one line when
/// it fits and holds no comment, otherwise one entry per line.
fn type_node<'a>(
    node: &'a CstNode,
    text: &str,
    next: &mut Vec<Work<'a>>,
) -> Option<()> {
    match node.kind() {
        kind if is_type_group(kind) => {
            let items = significant(node);
            let (open, rest) = items.split_first()?;
            let (closer, inner) = rest.split_last()?;
            let parts = entries(inner, text);
            if parts.is_empty() {
                next.extend(token(open, text));
                next.extend(token(closer, text));
                return Some(());
            }
            next.push(emit(Item::Begin));
            next.extend(token(open, text));
            next.extend([emit(Item::Indent), emit(Item::Soft)]);
            for (index, entry) in parts.iter().enumerate() {
                if index > 0 {
                    next.push(emit(Item::Space));
                }
                type_entry(entry, text, next);
            }
            next.extend(close(closer, text, true, Item::Soft));
        }
        SyntaxKind::Path => next.push(Work::Tokens(vec![node])),
        kind if kind.is_leaf() && kind != SyntaxKind::Comment => {
            next.extend(token(node, text));
        }
        _ => return None,
    }
    Some(())
}
