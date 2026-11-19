//! The canonical layout of a module's top-level forms.
//!
//! `docs/spec/01-source-language.md`, "Canonical format":
//!
//! - one blank line between top-level forms, which keep their written order,
//!   so the formatter never repairs a late `use`;
//! - a comment on a form's line stays at the end of that line, and any other
//!   comment stays on its own line before the form that follows it; an
//!   own-line comment after a form starts the next block, after one blank
//!   line, and a blank line written between two comments of one block, or
//!   between a comment and its form, is kept as one blank line;
//! - a construct on one line when it holds no comment, no nested `let`, and
//!   fits within 88 columns; otherwise its opener line, each section on its
//!   own line one step in, and its closer on its own line at the opener's
//!   indentation;
//! - a `type`, `interface`, `impl`, `fn`, `proc`, or `test` construct always
//!   on several lines when it has a member or any section other than `of`,
//!   whatever its length; in a multi-line declaration `of` stays on the
//!   opener's line when that line fits;
//! - sections in the canonical order of [`vibra_syntax::source::keyword::SECTIONS`],
//!   with the `case` list in the place of `of`, then the associated types of
//!   an interface or the choices of an `impl`, then named `fn` and `proc`
//!   members, then `impl` blocks, then `do`; within each group the written
//!   order is kept, so cases, fields, and type arguments are never
//!   reordered, and an empty `takes {}`, `labelled {}`, `given {}`, or
//!   `performs {}` is removed;
//! - a type expression on one line when it fits, otherwise its brace or
//!   parenthesized forms one entry per line, one step in, with the closing
//!   token on its own line;
//! - a module value laid out as a `let` ([`crate::expr`]); with a
//!   `visibility` or `doc` section, as a declaration whose last section is
//!   `be` and its value.
//!
//! A `fn`, `proc`, or `test`, named or anonymous, is laid out with the
//! expressions ([`crate::expr`]): its signature split into its sections, a
//! guard's `else` on the next line one step in when both do not fit, and the
//! body statements one step in from `do`. A form whose comments cannot be
//! placed by these rules (a comment between the tokens of one operand) leaves
//! the whole document unchanged.

use vibra_syntax::source::Construct;
use vibra_syntax::source::keyword::{SECTIONS, section_rank};
use vibra_syntax::{CstNode, SyntaxKind};

use crate::expr::{body_items, expr_items, guard_items, labelled_items, let_items};
use crate::layout::{Item, render};

/// The rank of the groups after the sections.
const TYPE_MEMBERS: usize = SECTIONS.len();
const CALLABLE_MEMBERS: usize = SECTIONS.len() + 1;
const IMPL_BLOCKS: usize = SECTIONS.len() + 2;
const BODY: usize = SECTIONS.len() + 3;

/// The number of line breaks in a whitespace text; CRLF counts once.
fn breaks(text: &str) -> usize {
    text.replace("\r\n", "\n").matches(['\n', '\r']).count()
}

fn comment(node: &CstNode, text: &str) -> String {
    node.text(text).trim_end().to_owned()
}

/// What was written last at the top level.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Last {
    Nothing,
    Form,
    Comment,
}

/// The canonical text of a module with no diagnostic, or `None` when it
/// holds a comment the layout cannot place.
pub(crate) fn format(root: &CstNode, text: &str) -> Option<String> {
    let mut items = Vec::new();
    let mut last = Last::Nothing;
    let mut newlines = 0;
    for child in root.children() {
        match child.kind() {
            SyntaxKind::Whitespace => newlines += breaks(child.text(text)),
            SyntaxKind::Comment if last == Last::Form && newlines == 0 => {
                items.push(Item::TrailingComment(comment(child, text)));
                last = Last::Comment;
            }
            SyntaxKind::Comment => {
                separate(&mut items, last, newlines);
                items.push(Item::Comment(comment(child, text)));
                last = Last::Comment;
                newlines = 0;
            }
            _ => {
                separate(&mut items, last, newlines);
                declaration(child, text, &mut items)?;
                last = Last::Form;
                newlines = 0;
            }
        }
    }
    if last != Last::Nothing {
        items.push(Item::Hard);
    }
    Some(render(&items))
}

/// The line breaks before the next top-level line: one blank line after a
/// form, and after a comment a blank line only where one was written.
fn separate(items: &mut Vec<Item>, last: Last, newlines: usize) {
    match last {
        Last::Nothing => {}
        Last::Form => items.extend([Item::Hard, Item::Hard]),
        Last::Comment => {
            items.push(Item::Hard);
            if newlines >= 2 {
                items.push(Item::Hard);
            }
        }
    }
}

/// One reorderable part of a declaration: a section with what belongs to
/// it, a case, a member, or the body.
struct Unit<'a> {
    rank: usize,
    leading: Vec<String>,
    nodes: Vec<&'a CstNode>,
    trailing: Option<String>,
}

/// A declaration split into its head and its units.
struct Parts<'a> {
    head: Vec<&'a CstNode>,
    head_trailing: Option<String>,
    units: Vec<Unit<'a>>,
    /// Comments after the last unit, before the closer.
    closing: Vec<String>,
}

/// The rank of a child of a typed declaration that starts a unit, and
/// whether more nodes may follow it in the same unit (`of` and its type).
fn typed_rank(node: &CstNode, text: &str) -> Option<(usize, bool)> {
    let keyword = || {
        node.children()
            .first()
            .map_or("", |keyword| keyword.text(text))
    };
    match node.kind() {
        SyntaxKind::ReservedWord if node.text(text) == "of" => {
            Some((section_rank("of")?, true))
        }
        SyntaxKind::Case => Some((section_rank("of")?, false)),
        SyntaxKind::Given
        | SyntaxKind::Where
        | SyntaxKind::Visibility
        | SyntaxKind::Representation
        | SyntaxKind::Role
        | SyntaxKind::Doc => Some((section_rank(keyword())?, false)),
        SyntaxKind::AssocType | SyntaxKind::AssocChoice => Some((TYPE_MEMBERS, false)),
        SyntaxKind::FnDecl | SyntaxKind::ProcDecl => Some((CALLABLE_MEMBERS, false)),
        SyntaxKind::Impl => Some((IMPL_BLOCKS, false)),
        _ => None,
    }
}

/// Whether a `let` construct is nested anywhere in `node`.
fn has_nested_let(node: &CstNode, text: &str) -> bool {
    let mut stack: Vec<&CstNode> = node.children().iter().collect();
    while let Some(current) = stack.pop() {
        if Construct::new(current, text).is_some_and(|nested| nested.keyword() == "let")
        {
            return true;
        }
        stack.extend(current.children());
    }
    false
}

/// Splits the contents of a declaration into its head and its units.
/// `start` says whether a child starts a unit, with its rank and whether
/// the nodes after it, up to the next unit, belong to it.
fn split<'a>(
    contents: impl IntoIterator<Item = &'a CstNode>,
    text: &str,
    start: impl Fn(&CstNode) -> Option<(usize, bool)>,
) -> Option<Parts<'a>> {
    let mut parts = Parts {
        head: Vec::new(),
        head_trailing: None,
        units: Vec::new(),
        closing: Vec::new(),
    };
    // Whether the last unit takes the nodes that follow it.
    let mut open = false;
    let mut newlines = 0;
    for child in contents {
        match child.kind() {
            SyntaxKind::Whitespace => newlines += breaks(child.text(text)),
            SyntaxKind::Comment => {
                let slot = match parts.units.last_mut() {
                    Some(unit) => &mut unit.trailing,
                    None => &mut parts.head_trailing,
                };
                if newlines == 0 && parts.closing.is_empty() && slot.is_none() {
                    *slot = Some(comment(child, text));
                } else {
                    parts.closing.push(comment(child, text));
                }
            }
            _ => {
                if let Some((rank, takes_more)) = start(child) {
                    parts.units.push(Unit {
                        rank,
                        leading: std::mem::take(&mut parts.closing),
                        nodes: vec![child],
                        trailing: None,
                    });
                    open = takes_more;
                } else {
                    if !parts.closing.is_empty() {
                        return None;
                    }
                    match parts.units.last_mut() {
                        Some(unit) if open && unit.trailing.is_none() => {
                            unit.nodes.push(child)
                        }
                        None if parts.head_trailing.is_none() => parts.head.push(child),
                        _ => return None,
                    }
                }
                newlines = 0;
            }
        }
    }
    parts.units.sort_by_key(|unit| unit.rank);
    Some(parts)
}

/// The layout items of one top-level form.
fn declaration(node: &CstNode, text: &str, out: &mut Vec<Item>) -> Option<()> {
    match node.kind() {
        SyntaxKind::TypeDecl
        | SyntaxKind::InterfaceDecl
        | SyntaxKind::Impl
        | SyntaxKind::AssocType
        | SyntaxKind::AssocChoice => typed(node, text, out),
        SyntaxKind::ModuleLet => module_let(node, text, out),
        SyntaxKind::FnDecl | SyntaxKind::ProcDecl | SyntaxKind::Test => {
            callable(node, text, out)
        }
        _ => generic(node, text, out),
    }
}

/// A module value: the layout of a `let` when it has only `be`, otherwise
/// the declaration layout with `be` and its value as the last section.
fn module_let(node: &CstNode, text: &str, out: &mut Vec<Item>) -> Option<()> {
    let children = node.children();
    if children
        .iter()
        .any(|child| child.kind() == SyntaxKind::Comment)
    {
        return None;
    }
    let sections: Vec<&CstNode> = children
        .iter()
        .filter(|child| {
            matches!(child.kind(), SyntaxKind::Visibility | SyntaxKind::Doc)
        })
        .collect();
    if sections.is_empty() {
        out.extend(let_items(node, text)?);
        return Some(());
    }
    let opener = children.first()?;
    let closer = children
        .last()
        .filter(|last| last.kind() != SyntaxKind::Missing)?;
    let contents = children.get(1..children.len().saturating_sub(1))?;
    let be = contents.iter().position(|child| {
        child.kind() == SyntaxKind::ReservedWord && child.text(text) == "be"
    })?;
    let head: Vec<&CstNode> = contents
        .get(..be)?
        .iter()
        .filter(|child| {
            child.kind() != SyntaxKind::Whitespace
                && !matches!(child.kind(), SyntaxKind::Visibility | SyntaxKind::Doc)
        })
        .collect();
    let value: Vec<&CstNode> = contents
        .get(be + 1..)?
        .iter()
        .filter(|child| {
            child.kind() != SyntaxKind::Whitespace
                && !matches!(child.kind(), SyntaxKind::Visibility | SyntaxKind::Doc)
        })
        .collect();
    let mut opener_items = vec![Item::Text(format!("{} ", opener.text(text)))];
    let (name, written) = head.split_at(head.len().min(2));
    opener_items.push(Item::Text(flat(name, text)?));
    opener_items.push(Item::Text(" ".to_owned()));
    opener_items.extend(type_items(written, text)?);
    let mut units: Vec<Unit<'_>> = sections
        .iter()
        .map(|section| {
            let keyword = section
                .children()
                .first()
                .map_or("", |word| word.text(text));
            Some(Unit {
                rank: section_rank(keyword)?,
                leading: Vec::new(),
                nodes: vec![*section],
                trailing: None,
            })
        })
        .collect::<Option<_>>()?;
    units.sort_by_key(|unit| unit.rank);
    let mut laid: Vec<(&Unit<'_>, Vec<Item>)> = Vec::with_capacity(units.len() + 1);
    for unit in &units {
        laid.push((unit, vec![Item::Text(flat(&unit.nodes, text)?)]));
    }
    let be_unit = Unit {
        rank: BODY,
        leading: Vec::new(),
        nodes: Vec::new(),
        trailing: None,
    };
    // The value on the line of `be` when it fits, otherwise on the next line
    // one step in, as in a `let`.
    let mut be_items = vec![
        Item::Text("be".to_owned()),
        Item::Begin,
        Item::Indent,
        Item::Space,
    ];
    be_items.extend(expr_items(&value, text)?);
    be_items.extend([Item::Dedent, Item::End]);
    laid.push((&be_unit, be_items));
    let parts = Parts {
        head: Vec::new(),
        head_trailing: None,
        units: Vec::new(),
        closing: Vec::new(),
    };
    lay_out(
        opener_items,
        &parts,
        laid,
        closer.text(text),
        false,
        false,
        out,
    );
    Some(())
}

/// A `use`: its sections by keyword, each operand written flat.
fn generic(node: &CstNode, text: &str, out: &mut Vec<Item>) -> Option<()> {
    let construct = Construct::new(node, text)?;
    let closer = construct.closer()?;
    let parts = split(construct.contents(), text, |child| {
        if child.kind() != SyntaxKind::Section {
            return None;
        }
        let keyword = child
            .children()
            .first()
            .map_or("", |keyword| keyword.text(text));
        Some((section_rank(keyword)?, true))
    })?;
    let mut opener = construct.keyword().to_owned();
    if !parts.head.is_empty() {
        opener.push(' ');
        opener.push_str(&flat(&parts.head, text)?);
    }
    let mut units = Vec::with_capacity(parts.units.len());
    for unit in &parts.units {
        units.push((unit, vec![Item::Text(flat(&unit.nodes, text)?)]));
    }
    lay_out(
        vec![Item::Text(opener)],
        &parts,
        units,
        closer.text(text),
        has_nested_let(node, text),
        false,
        out,
    );
    Some(())
}

/// The rank of a child of a callable that starts a unit, and whether the
/// nodes after it belong to it: a signature keyword takes its operand and
/// `do` its body; every other section is one node.
fn callable_rank(node: &CstNode, text: &str) -> Option<(usize, bool)> {
    match node.kind() {
        SyntaxKind::ReservedWord => match node.text(text) {
            "do" => Some((BODY, true)),
            word @ ("takes" | "labelled" | "returns") => {
                Some((section_rank(word)?, true))
            }
            _ => None,
        },
        SyntaxKind::Given
        | SyntaxKind::Requires
        | SyntaxKind::Ensures
        | SyntaxKind::Performs
        | SyntaxKind::Visibility
        | SyntaxKind::External
        | SyntaxKind::Symbol
        | SyntaxKind::Native
        | SyntaxKind::Doc => {
            let keyword = node.children().first()?.text(text);
            Some((section_rank(keyword)?, false))
        }
        _ => None,
    }
}

/// A `fn`, `proc`, or `test`, named or anonymous: its signature split into
/// its sections, every section in the canonical order and on its own line,
/// and the body statements one step in from `do`. With no section it is one
/// line.
pub(crate) fn callable(node: &CstNode, text: &str, out: &mut Vec<Item>) -> Option<()> {
    let children = node.children();
    let opener = children.first()?;
    let closer = children
        .last()
        .filter(|last| last.kind() != SyntaxKind::Missing)?;
    let contents = children.get(1..children.len().saturating_sub(1))?;
    let flattened = contents.iter().flat_map(|child| {
        if child.kind() == SyntaxKind::Signature {
            child.children()
        } else {
            std::slice::from_ref(child)
        }
    });
    let mut parts = split(flattened, text, |child| callable_rank(child, text))?;
    drop_empty(&mut parts, text);
    let mut head = opener.text(text).to_owned();
    if !parts.head.is_empty() {
        head.push(' ');
        head.push_str(&flat(&parts.head, text)?);
    }
    let mut units = Vec::with_capacity(parts.units.len());
    for unit in &parts.units {
        units.push((unit, callable_unit(unit, text)?));
    }
    let broken = !parts.units.is_empty();
    lay_out(
        vec![Item::Text(head)],
        &parts,
        units,
        closer.text(text),
        broken,
        false,
        out,
    );
    Some(())
}

/// A callable unit's items.
fn callable_unit(unit: &Unit<'_>, text: &str) -> Option<Vec<Item>> {
    let first = *unit.nodes.first()?;
    let rest = unit.nodes.get(1..)?;
    let mut out = Vec::new();
    match first.kind() {
        SyntaxKind::ReservedWord => match (first.text(text), rest) {
            ("do", []) => out.push(Item::Text("do".to_owned())),
            ("do", [body]) => {
                out.push(Item::Text("do".to_owned()));
                let statements = body_items(body, text)?;
                if !statements.is_empty() {
                    out.push(Item::Indent);
                    out.extend(statements);
                    out.push(Item::Dedent);
                }
            }
            ("takes", [params]) => {
                out.push(Item::Text("takes ".to_owned()));
                out.extend(type_items(&[params], text)?);
            }
            ("labelled", [labelled]) => {
                out.push(Item::Text("labelled ".to_owned()));
                out.extend(labelled_items(labelled, text)?);
            }
            ("returns", [name, colon, written @ ..]) if is_colon(colon, text) => {
                out.push(Item::Text(format!("returns {}: ", flat(&[name], text)?)));
                out.extend(type_items(written, text)?);
            }
            _ => return None,
        },
        SyntaxKind::Requires | SyntaxKind::Ensures => {
            out.extend(guard_items(first, text)?)
        }
        SyntaxKind::Given | SyntaxKind::Performs => {
            let keyword = first.children().first()?.text(text);
            let operand: Vec<&CstNode> = first.children().iter().skip(1).collect();
            out.push(Item::Text(format!("{keyword} ")));
            out.extend(type_items(&operand, text)?);
        }
        _ => out.push(Item::Text(flat(&unit.nodes, text)?)),
    }
    Some(out)
}
/// Whether a unit is an empty `takes {}`, `labelled {}`, `given {}`, or
/// `performs {}`, which the formatter removes.
fn is_empty_section(unit: &Unit<'_>, text: &str) -> bool {
    let Some(first) = unit.nodes.first() else {
        return false;
    };
    let keyword = if first.kind() == SyntaxKind::ReservedWord {
        first.text(text)
    } else {
        first
            .children()
            .first()
            .map_or("", |keyword| keyword.text(text))
    };
    matches!(keyword, "takes" | "labelled" | "given" | "performs")
        && unit
            .nodes
            .iter()
            .copied()
            .chain(first.children())
            .any(is_empty_bracket)
}

/// Whether a node is a brace form with nothing between its brackets.
fn is_empty_bracket(node: &CstNode) -> bool {
    matches!(
        node.kind(),
        SyntaxKind::Brace | SyntaxKind::Row | SyntaxKind::Params | SyntaxKind::Labelled
    ) && node.children().iter().all(|child| {
        matches!(
            child.kind(),
            SyntaxKind::Whitespace | SyntaxKind::Punctuation
        )
    })
}
/// A typed declaration: `type`, `interface`, `impl`, an associated type,
/// or an associated choice.
fn typed(node: &CstNode, text: &str, out: &mut Vec<Item>) -> Option<()> {
    let children = node.children();
    let opener = children.first()?;
    let closer = children
        .last()
        .filter(|last| last.kind() != SyntaxKind::Missing)?;
    let contents = children.get(1..children.len().saturating_sub(1))?;
    let mut parts = split(contents, text, |child| typed_rank(child, text))?;
    drop_empty(&mut parts, text);
    let mut opener_items = vec![Item::Text(opener.text(text).to_owned())];
    if !parts.head.is_empty() {
        opener_items.push(Item::Text(" ".to_owned()));
        if node.kind() == SyntaxKind::Impl {
            opener_items.extend(type_items(&parts.head, text)?);
        } else {
            opener_items.push(Item::Text(flat(&parts.head, text)?));
        }
    }
    let mut units = Vec::with_capacity(parts.units.len());
    for unit in &parts.units {
        let mut items = Vec::new();
        typed_unit(unit, text, &mut items)?;
        units.push((unit, items));
    }
    // Every unit but `of` is a case, a section other than `of`, or a member.
    let always_broken = parts.units.iter().any(|unit| {
        unit.nodes
            .first()
            .is_none_or(|node| node.kind() != SyntaxKind::ReservedWord)
    });
    let of_first = parts.units.first().is_some_and(|unit| {
        unit.leading.is_empty()
            && unit
                .nodes
                .first()
                .is_some_and(|node| node.kind() == SyntaxKind::ReservedWord)
    });
    lay_out(
        opener_items,
        &parts,
        units,
        closer.text(text),
        always_broken || has_nested_let(node, text),
        of_first,
        out,
    );
    Some(())
}

/// Removes every empty section; its comments move to the unit after it, or
/// before the closer.
fn drop_empty(parts: &mut Parts<'_>, text: &str) {
    let mut carried: Vec<String> = Vec::new();
    let mut kept = Vec::with_capacity(parts.units.len());
    for mut unit in std::mem::take(&mut parts.units) {
        if is_empty_section(&unit, text) {
            carried.append(&mut unit.leading);
            carried.extend(unit.trailing.take());
            continue;
        }
        if !carried.is_empty() {
            carried.append(&mut unit.leading);
            unit.leading = std::mem::take(&mut carried);
        }
        kept.push(unit);
    }
    carried.append(&mut parts.closing);
    parts.closing = carried;
    parts.units = kept;
}

/// A typed unit's items.
fn typed_unit(unit: &Unit<'_>, text: &str, out: &mut Vec<Item>) -> Option<()> {
    let first = *unit.nodes.first()?;
    match first.kind() {
        SyntaxKind::ReservedWord => {
            out.push(Item::Text("of ".to_owned()));
            out.extend(type_items(unit.nodes.get(1..)?, text)?);
        }
        SyntaxKind::Case => {
            let parts: Vec<&CstNode> = first
                .children()
                .iter()
                .filter(|child| child.kind() != SyntaxKind::Whitespace)
                .collect();
            let (record, name): (Vec<&CstNode>, Vec<&CstNode>) = parts
                .into_iter()
                .partition(|child| child.kind() == SyntaxKind::RecordType);
            out.push(Item::Text(flat(&name, text)?));
            if !record.is_empty() {
                out.push(Item::Text(" ".to_owned()));
                out.extend(type_items(&record, text)?);
            }
        }
        SyntaxKind::Given => {
            let brace: Vec<&CstNode> = first
                .children()
                .iter()
                .filter(|child| child.kind() == SyntaxKind::Brace)
                .collect();
            if first
                .children()
                .iter()
                .any(|child| child.kind() == SyntaxKind::Comment)
            {
                return None;
            }
            out.push(Item::Text("given ".to_owned()));
            out.extend(type_items(&brace, text)?);
        }
        SyntaxKind::Where => {
            let operand: Vec<&CstNode> = first
                .children()
                .iter()
                .skip(1)
                .filter(|child| child.kind() != SyntaxKind::Whitespace)
                .collect();
            let split = operand.iter().position(|node| {
                node.kind() == SyntaxKind::ReservedWord && node.text(text) == "else"
            });
            let (predicate, consequence) = match split {
                Some(at) => (operand.get(..at)?, operand.get(at + 1..)),
                None => (operand.as_slice(), None),
            };
            out.push(Item::Text("where ".to_owned()));
            out.extend(expr_items(predicate, text)?);
            if let Some(consequence) = consequence {
                out.push(Item::Text(" else ".to_owned()));
                out.extend(expr_items(consequence, text)?);
            }
        }
        SyntaxKind::AssocType
        | SyntaxKind::AssocChoice
        | SyntaxKind::Impl
        | SyntaxKind::FnDecl
        | SyntaxKind::ProcDecl => declaration(first, text, out)?,
        _ => out.push(Item::Text(flat(&unit.nodes, text)?)),
    }
    Some(())
}

/// Lays out a declaration: on one line when it fits, holds no comment, and
/// is not `broken`; otherwise its opener line, each unit on its own line one
/// step in, and the closer on its own line. With `of_first`, the first unit
/// is `of`, which stays on the opener's line when that line fits.
fn lay_out(
    opener: Vec<Item>,
    parts: &Parts<'_>,
    units: Vec<(&Unit<'_>, Vec<Item>)>,
    closer: &str,
    broken: bool,
    of_first: bool,
    out: &mut Vec<Item>,
) {
    out.push(Item::Begin);
    if broken {
        out.push(Item::Force);
    }
    out.extend(opener);
    if let Some(trailing) = &parts.head_trailing {
        out.push(Item::TrailingComment(trailing.clone()));
    }
    out.push(Item::Indent);
    let glue = of_first && parts.head_trailing.is_none();
    for (index, (unit, items)) in units.into_iter().enumerate() {
        for leading in &unit.leading {
            out.push(Item::Space);
            out.push(Item::Comment(leading.clone()));
        }
        if index == 0 && glue {
            out.extend([Item::Begin, Item::Space]);
            out.extend(items);
            out.push(Item::End);
        } else {
            out.push(Item::Space);
            out.extend(items);
        }
        if let Some(trailing) = &unit.trailing {
            out.push(Item::TrailingComment(trailing.clone()));
        }
    }
    for closing in &parts.closing {
        out.push(Item::Space);
        out.push(Item::Comment(closing.clone()));
    }
    out.push(Item::Dedent);
    out.push(Item::Space);
    out.push(Item::Text(closer.to_owned()));
    out.push(Item::End);
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

fn is_colon(node: &CstNode, text: &str) -> bool {
    node.kind() == SyntaxKind::Punctuation && node.text(text) == ":"
}

/// One step of the type-expression layout.
enum Step<'a> {
    Node(&'a CstNode),
    Emit(Item),
}

/// Pushes the steps of one entry: a label and `:`, then one space, then the
/// type; a `(…)` is glued to the head it applies.
fn entry_steps<'a>(nodes: &[&'a CstNode], text: &str, steps: &mut Vec<Step<'a>>) {
    for (index, node) in nodes.iter().enumerate() {
        if index > 0 && !is_colon(node, text) && node.kind() != SyntaxKind::Paren {
            steps.push(Step::Emit(Item::Text(" ".to_owned())));
        }
        steps.push(Step::Node(node));
    }
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

/// The layout items of a type expression: on one line when it fits,
/// otherwise each bracket form one entry per line. `None` when a comment is
/// among its tokens. Iterative.
pub(crate) fn type_items(nodes: &[&CstNode], text: &str) -> Option<Vec<Item>> {
    let significant: Vec<&CstNode> = nodes
        .iter()
        .copied()
        .filter(|node| node.kind() != SyntaxKind::Whitespace)
        .collect();
    let mut steps: Vec<Step<'_>> = Vec::new();
    entry_steps(&significant, text, &mut steps);
    steps.reverse();
    let mut out = Vec::new();
    while let Some(step) = steps.pop() {
        let node = match step {
            Step::Emit(item) => {
                out.push(item);
                continue;
            }
            Step::Node(node) => node,
        };
        match node.kind() {
            SyntaxKind::Comment => return None,
            kind if is_type_group(kind) => {
                let children = node.children();
                let open = children.first()?.text(text);
                let close = children.last()?.text(text);
                let inner: Vec<&CstNode> = children
                    .get(1..children.len().saturating_sub(1))?
                    .iter()
                    .filter(|child| child.kind() != SyntaxKind::Whitespace)
                    .collect();
                let mut group: Vec<Step<'_>> = Vec::new();
                let parts = entries(&inner, text);
                if parts.is_empty() {
                    group.push(Step::Emit(Item::Text(format!("{open}{close}"))));
                } else {
                    group.extend([
                        Step::Emit(Item::Begin),
                        Step::Emit(Item::Text(open.to_owned())),
                        Step::Emit(Item::Indent),
                        Step::Emit(Item::Soft),
                    ]);
                    for (index, entry) in parts.iter().enumerate() {
                        if index > 0 {
                            group.push(Step::Emit(Item::Space));
                        }
                        entry_steps(entry, text, &mut group);
                    }
                    group.extend([
                        Step::Emit(Item::Dedent),
                        Step::Emit(Item::Soft),
                        Step::Emit(Item::Text(close.to_owned())),
                        Step::Emit(Item::End),
                    ]);
                }
                steps.extend(group.into_iter().rev());
            }
            SyntaxKind::Path => out.push(Item::Text(flat(&[node], text)?)),
            _ if node.kind().is_leaf() => {
                out.push(Item::Text(node.text(text).to_owned()))
            }
            _ => return None,
        }
    }
    Some(out)
}

/// The tokens below `nodes` on one line: one space between tokens, none
/// after an opening bracket or before a closing one, none before `(` or
/// `:`, and none around `.`. `None` when a comment is among them.
pub(crate) fn flat(nodes: &[&CstNode], text: &str) -> Option<String> {
    let mut out = String::new();
    let mut previous: Option<&str> = None;
    for leaf in nodes.iter().flat_map(|node| node.leaves()) {
        match leaf.kind() {
            SyntaxKind::Whitespace | SyntaxKind::Missing => continue,
            SyntaxKind::Comment => return None,
            _ => {}
        }
        let token = leaf.text(text);
        let punctuation = leaf.kind() == SyntaxKind::Punctuation;
        if let Some(previous) = previous {
            let glued_after = matches!(previous, "(" | "{" | "[" | ".");
            let glued_before =
                punctuation && matches!(token, ")" | "}" | "]" | "(" | ":" | ".");
            if !glued_after && !glued_before {
                out.push(' ');
            }
        }
        out.push_str(token);
        previous = Some(if punctuation { token } else { "" });
    }
    Some(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use vibra_syntax::parse_source;

    use super::format;

    /// The layout of a document, whatever its diagnostics, so the layout of a
    /// form the reader rejects is reachable here.
    fn layout(source: &str) -> String {
        let document = parse_source(None, source.as_bytes());
        format(document.root().unwrap(), document.text().unwrap()).unwrap()
    }

    #[test]
    fn sections_are_written_in_the_canonical_order() {
        assert_eq!(
            layout("fn f returns r: I32 takes {x: I32} do return x endfn"),
            "fn f\n  takes {x: I32}\n  returns r: I32\n  do\n    return x\nendfn\n"
        );
        assert_eq!(
            layout("type A visibility @public of U8 endtype"),
            "type A of U8\n  visibility @public\nendtype\n"
        );
    }

    #[test]
    fn a_declaration_that_does_not_fit_has_one_section_per_line() {
        let long = "x".repeat(70);
        assert_eq!(
            layout(&format!(
                "fn {long} visibility @public returns r: I32 do return 1i32 endfn"
            )),
            format!(
                "fn {long}\n  returns r: I32\n  visibility @public\n  do\n    return 1i32\nendfn\n"
            )
        );
    }

    #[test]
    fn a_comment_breaks_the_declaration_and_moves_with_its_section() {
        assert_eq!(
            layout(
                "fn f\n  // the result\n  returns r: I32 takes {x: I32} // one\nendfn"
            ),
            "fn f\n  takes {x: I32} // one\n  // the result\n  returns r: I32\nendfn\n"
        );
        assert_eq!(
            layout(
                "type A // head\n  // vis\n  visibility @public of U8 // body\nendtype"
            ),
            "type A // head\n  of U8 // body\n  // vis\n  visibility @public\nendtype\n"
        );
    }

    #[test]
    fn a_nested_let_breaks_the_declaration() {
        assert_eq!(
            layout("fn f returns r: I32 do let x: I32 be 1i32 endlet return x endfn"),
            "fn f\n  returns r: I32\n  do\n    let x: I32 be 1i32 endlet\n    return x\nendfn\n"
        );
    }

    #[test]
    fn members_follow_the_sections_and_do_comes_last() {
        assert_eq!(
            layout(
                "interface A impl B endimpl fn g returns r: I32 endfn visibility @public type C endtype endinterface"
            ),
            "interface A\n  visibility @public\n  type C endtype\n  fn g\n    returns r: I32\n  endfn\n  impl B endimpl\nendinterface\n"
        );
        assert_eq!(
            layout(
                "type A of U8 fn g takes {a: Self} returns r: I32 do return 1i32 endfn visibility @public endtype"
            ),
            "type A of U8\n  visibility @public\n  fn g\n    takes {a: Self}\n    returns r: I32\n    do\n      return 1i32\n  endfn\nendtype\n"
        );
    }

    #[test]
    fn an_operand_keeps_its_token_order_and_its_anonymous_constructs() {
        assert_eq!(
            layout(
                "let f: Fn({} I32) be fn returns r: I32 do return 1i32 endfn visibility @public endlet"
            ),
            "let f: Fn({} I32)\n  visibility @public\n  be\n    fn\n      returns r: I32\n      do\n        return 1i32\n    endfn\nendlet\n"
        );
    }

    #[test]
    fn a_declaration_with_only_of_stays_on_one_line() {
        assert_eq!(
            layout("type Celsius of F64 endtype"),
            "type Celsius of F64 endtype\n"
        );
        assert_eq!(layout("fn f endfn"), "fn f endfn\n");
        assert_eq!(
            layout("type A of U8 impl B endimpl endtype"),
            "type A of U8\n  impl B endimpl\nendtype\n"
        );
    }

    #[test]
    fn a_case_list_always_breaks() {
        assert_eq!(
            layout("type Color case red case green endtype"),
            "type Color\n  case red\n  case green\nendtype\n"
        );
    }

    #[test]
    fn a_callable_body_is_two_steps_in_and_empty_sections_are_removed() {
        assert_eq!(
            layout(
                "proc p takes {} labelled {} performs {} returns r: Void do let x: I32 be 1i32 endlet return void endproc"
            ),
            "proc p\n  returns r: Void\n  do\n    let x: I32 be 1i32 endlet\n    return void\nendproc\n"
        );
        assert_eq!(
            layout("test \"t\" takes {w: World} do return void endtest"),
            "test \"t\"\n  takes {w: World}\n  do\n    return void\nendtest\n"
        );
    }

    #[test]
    fn a_guard_else_breaks_one_step_in_only_when_the_guard_does_not_fit() {
        assert_eq!(
            layout("fn f returns r: I32 requires ready else 0i32 do return 1i32 endfn"),
            "fn f\n  returns r: I32\n  requires ready else 0i32\n  do\n    return 1i32\nendfn\n"
        );
        let long = "x".repeat(60);
        assert_eq!(
            layout(&format!(
                "fn f returns r: I32 requires {long} else Result.err(error: E.bad()) do return 1i32 endfn"
            )),
            format!(
                "fn f\n  returns r: I32\n  requires {long}\n    else Result.err(error: E.bad())\n  do\n    return 1i32\nendfn\n"
            )
        );
        assert_eq!(
            layout(
                "fn f labelled {up: Bool be false} returns r: I32 do return 1i32 endfn"
            ),
            "fn f\n  labelled {up: Bool be false}\n  returns r: I32\n  do\n    return 1i32\nendfn\n"
        );
    }

    #[test]
    fn a_short_type_with_visibility_breaks() {
        assert_eq!(
            layout("type A of U8 visibility @public endtype"),
            "type A of U8\n  visibility @public\nendtype\n"
        );
        assert_eq!(
            layout("type A of U8 doc \"\"\"An A.\"\"\" endtype"),
            "type A of U8\n  doc \"\"\"An A.\"\"\"\nendtype\n"
        );
    }

    #[test]
    fn of_leaves_the_opener_line_only_when_it_does_not_fit() {
        let fields = "first: Str second: Str third: U64 fourth: Bool fifth: Bool";
        assert_eq!(
            layout(&format!(
                "type Wide of {{{fields}}} visibility @public endtype"
            )),
            format!("type Wide of {{{fields}}}\n  visibility @public\nendtype\n")
        );
        let fields = format!("{fields} sixth: Bool seven: U8");
        assert_eq!(
            layout(&format!(
                "type Wide of {{{fields}}} visibility @public endtype"
            )),
            format!("type Wide\n  of {{{fields}}}\n  visibility @public\nendtype\n")
        );
    }

    #[test]
    fn an_interface_member_with_takes_and_returns_breaks() {
        assert_eq!(
            layout(
                "interface Printable fn render takes {value: Self} returns text: Str endfn endinterface"
            ),
            "interface Printable\n  fn render\n    takes {value: Self}\n    returns text: Str\n  endfn\nendinterface\n"
        );
        assert_eq!(
            layout("interface Iter type Item endtype endinterface"),
            "interface Iter\n  type Item endtype\nendinterface\n"
        );
    }

    #[test]
    fn an_anonymous_callable_breaks_like_a_declaration() {
        assert_eq!(
            layout(
                "let f: Fn({} I32) be fn returns r: I32 do return 1i32 endfn endlet"
            ),
            "let f: Fn({} I32) be\n  fn\n    returns r: I32\n    do\n      return 1i32\n  endfn\nendlet\n"
        );
        assert_eq!(
            layout("let f: Fn({} Void) be fn endfn endlet"),
            "let f: Fn({} Void) be fn endfn endlet\n"
        );
    }

    #[test]
    fn a_comment_inside_an_operand_leaves_the_document_unchanged() {
        let document = parse_source(None, b"fn f returns r: // c\n I32 endfn");
        assert!(format(document.root().unwrap(), document.text().unwrap()).is_none());
        let document = parse_source(None, b"type A of {x: // c\n U8} endtype");
        assert!(format(document.root().unwrap(), document.text().unwrap()).is_none());
    }
}
