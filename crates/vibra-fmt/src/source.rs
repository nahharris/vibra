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
//! - sections in the canonical order of [`vibra_syntax::source::keyword::SECTIONS`],
//!   with the `case` list in the place of `of`, then the associated types of
//!   an interface or the choices of an `impl`, then named `fn` and `proc`
//!   members, then `impl` blocks, then `do`; within each group the written
//!   order is kept, so cases, fields, and type arguments are never
//!   reordered, and an empty `given {}` is removed;
//! - a type expression on one line when it fits, otherwise its brace or
//!   parenthesized forms one entry per line, one step in, with the closing
//!   token on its own line.
//!
//! The grammar of `fn`, `proc`, `let`, and `test` arrives in later steps, and
//! their bodies are written flat here, one space between tokens. A form whose
//! comments cannot be placed by these rules (a comment between the tokens of
//! one operand) leaves the whole document unchanged.

use vibra_syntax::source::keyword::{SECTIONS, section_rank};
use vibra_syntax::source::{Construct, is_construct};
use vibra_syntax::{CstNode, SyntaxKind};

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

/// Whether a construct nested directly in a generic declaration is a
/// member: a named `type`, `fn`, or `proc`, or an `impl`. An anonymous `fn`
/// or `proc` and every other construct belong to the operand written before
/// them.
fn member_rank(node: &CstNode, text: &str) -> Option<usize> {
    let construct = Construct::new(node, text)?;
    let named = construct
        .head()
        .first()
        .is_some_and(|first| first.kind() == SyntaxKind::Identifier);
    match construct.keyword() {
        "type" if named => Some(TYPE_MEMBERS),
        "fn" | "proc" if named => Some(CALLABLE_MEMBERS),
        "impl" => Some(IMPL_BLOCKS),
        _ => None,
    }
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
        SyntaxKind::Construct => Some((CALLABLE_MEMBERS, false)),
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
    contents: &'a [CstNode],
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
        _ => generic(node, text, out),
    }
}

/// A declaration whose grammar is not typed yet: sections and members by
/// keyword, each operand written flat.
fn generic(node: &CstNode, text: &str, out: &mut Vec<Item>) -> Option<()> {
    let construct = Construct::new(node, text)?;
    let closer = construct.closer()?;
    let parts = split(construct.contents(), text, |child| match child.kind() {
        SyntaxKind::Section => {
            let keyword = child
                .children()
                .first()
                .map_or("", |keyword| keyword.text(text));
            let rank = if keyword == "do" {
                BODY
            } else {
                section_rank(keyword)?
            };
            Some((rank, true))
        }
        kind if is_construct(kind) => {
            member_rank(child, text).map(|rank| (rank, false))
        }
        _ => None,
    })?;
    // In a generic declaration, an anonymous construct written after a
    // section's operand stays with it.
    let mut opener = construct.keyword().to_owned();
    if !parts.head.is_empty() {
        opener.push(' ');
        opener.push_str(&flat(&parts.head, text)?);
    }
    let mut units = Vec::with_capacity(parts.units.len());
    for unit in &parts.units {
        let mut items = Vec::new();
        generic_unit(unit, text, &mut items)?;
        units.push((unit, items));
    }
    lay_out(
        vec![Item::Text(opener)],
        &parts,
        units,
        closer.text(text),
        has_nested_let(node, text),
        out,
    );
    Some(())
}

/// A generic unit's items; the body of `do` goes on the next line, one step
/// further in, when the declaration does not fit on one line.
fn generic_unit(unit: &Unit<'_>, text: &str, out: &mut Vec<Item>) -> Option<()> {
    if unit.rank != BODY {
        out.push(Item::Text(flat(&unit.nodes, text)?));
        return Some(());
    }
    out.push(Item::Text("do".to_owned()));
    let body: Vec<&CstNode> = unit
        .nodes
        .iter()
        .flat_map(|node| node.children().iter().skip(1))
        .collect();
    let body = flat(&body, text)?;
    if !body.is_empty() {
        out.extend([Item::Indent, Item::Space, Item::Text(body), Item::Dedent]);
    }
    Some(())
}

/// Whether a `given` section is empty, which the formatter removes.
fn is_empty_given(node: &CstNode) -> bool {
    node.kind() == SyntaxKind::Given
        && node
            .children()
            .iter()
            .find(|child| child.kind() == SyntaxKind::Brace)
            .is_some_and(|brace| {
                brace.children().iter().all(|child| {
                    child.kind() != SyntaxKind::Brace && is_bracket_or_trivia(child)
                })
            })
}

fn is_bracket_or_trivia(node: &CstNode) -> bool {
    matches!(
        node.kind(),
        SyntaxKind::Whitespace | SyntaxKind::Punctuation
    )
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
    drop_empty_givens(&mut parts);
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
    lay_out(
        opener_items,
        &parts,
        units,
        closer.text(text),
        has_nested_let(node, text),
        out,
    );
    Some(())
}

/// Removes every empty `given {}`; its comments move to the unit after it,
/// or before the closer.
fn drop_empty_givens(parts: &mut Parts<'_>) {
    let mut carried: Vec<String> = Vec::new();
    let mut kept = Vec::with_capacity(parts.units.len());
    for mut unit in std::mem::take(&mut parts.units) {
        if unit.nodes.first().is_some_and(|node| is_empty_given(node)) {
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
        SyntaxKind::AssocType
        | SyntaxKind::AssocChoice
        | SyntaxKind::Impl
        | SyntaxKind::Construct => declaration(first, text, out)?,
        _ => out.push(Item::Text(flat(&unit.nodes, text)?)),
    }
    Some(())
}

/// Lays out a declaration: on one line when it fits, holds no comment, and
/// holds no nested `let`; otherwise its opener line, each unit on its own
/// line one step in, and the closer on its own line.
fn lay_out(
    opener: Vec<Item>,
    parts: &Parts<'_>,
    units: Vec<(&Unit<'_>, Vec<Item>)>,
    closer: &str,
    nested_let: bool,
    out: &mut Vec<Item>,
) {
    out.push(Item::Begin);
    out.extend(opener);
    if let Some(trailing) = &parts.head_trailing {
        out.push(Item::TrailingComment(trailing.clone()));
    }
    out.push(Item::Indent);
    let mut first_break = if nested_let { Item::Hard } else { Item::Space };
    for (unit, items) in units {
        for leading in &unit.leading {
            out.push(std::mem::replace(&mut first_break, Item::Space));
            out.push(Item::Comment(leading.clone()));
        }
        out.push(std::mem::replace(&mut first_break, Item::Space));
        out.extend(items);
        if let Some(trailing) = &unit.trailing {
            out.push(Item::TrailingComment(trailing.clone()));
        }
    }
    for closing in &parts.closing {
        out.push(std::mem::replace(&mut first_break, Item::Space));
        out.push(Item::Comment(closing.clone()));
    }
    out.push(Item::Dedent);
    out.push(first_break);
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
fn type_items(nodes: &[&CstNode], text: &str) -> Option<Vec<Item>> {
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
fn flat(nodes: &[&CstNode], text: &str) -> Option<String> {
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

    /// The layout of a document, whatever its diagnostics: the forms other
    /// than `use`, `type`, and `interface` are `@tool.unavailable` in this
    /// step, so their layout is reachable only here.
    fn layout(source: &str) -> String {
        let document = parse_source(None, source.as_bytes());
        format(document.root().unwrap(), document.text().unwrap()).unwrap()
    }

    #[test]
    fn sections_are_written_in_the_canonical_order() {
        assert_eq!(
            layout("fn f returns r: I32 takes {x: I32} do return x endfn"),
            "fn f takes {x: I32} returns r: I32 do return x endfn\n"
        );
        assert_eq!(
            layout("type A visibility @public of U8 endtype"),
            "type A of U8 visibility @public endtype\n"
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
            "fn f\n  returns r: I32\n  do\n    let x: I32 be 1i32 endlet return x\nendfn\n"
        );
    }

    #[test]
    fn members_follow_the_sections_and_do_comes_last() {
        assert_eq!(
            layout(
                "interface A impl B endimpl fn g returns r: I32 endfn visibility @public type C endtype endinterface"
            ),
            "interface A\n  visibility @public\n  type C endtype\n  fn g returns r: I32 endfn\n  impl B endimpl\nendinterface\n"
        );
        assert_eq!(
            layout("fn f fn g returns r: I32 endfn returns r: I32 do return x endfn"),
            "fn f returns r: I32 fn g returns r: I32 endfn do return x endfn\n"
        );
    }

    #[test]
    fn an_operand_keeps_its_token_order_and_its_anonymous_constructs() {
        assert_eq!(
            layout(
                "let f: Fn({} I32) be fn returns r: I32 do return 1i32 endfn visibility @public endlet"
            ),
            "let f: Fn({} I32) visibility @public be fn returns r: I32 do return 1i32 endfn endlet\n"
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
