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
//!   then nested `type` declarations, then named `fn` and `proc` members,
//!   then `impl` blocks, then `do`; within each group the written order is
//!   kept, and the tokens of a section's operand are never reordered.
//!
//! The grammar of the six forms other than `use` arrives in later steps, and
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

/// One reorderable part of a declaration: a section with the constructs
/// written directly after it, a member, or the body.
struct Unit<'a> {
    rank: usize,
    leading: Vec<String>,
    nodes: Vec<&'a CstNode>,
    trailing: Option<String>,
}

/// Whether a construct nested directly in a declaration is a member: a named
/// `type`, `fn`, or `proc`, or an `impl`. An anonymous `fn` or `proc` and
/// every other construct belong to the operand written before them.
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

/// Whether a `let` construct is nested anywhere in `node`.
fn has_nested_let(node: &CstNode, text: &str) -> bool {
    let mut stack: Vec<&CstNode> = node.children().iter().collect();
    while let Some(current) = stack.pop() {
        if Construct::new(current, text).is_some_and(|nested| nested.keyword() == "let") {
            return true;
        }
        stack.extend(current.children());
    }
    false
}

/// The layout items of one top-level form.
fn declaration(node: &CstNode, text: &str, out: &mut Vec<Item>) -> Option<()> {
    let construct = Construct::new(node, text)?;
    let closer = construct.closer()?;
    let mut head: Vec<&CstNode> = Vec::new();
    let mut head_trailing: Option<String> = None;
    let mut units: Vec<Unit<'_>> = Vec::new();
    let mut pending: Vec<String> = Vec::new();
    let mut newlines = 0;
    for child in construct.contents() {
        match child.kind() {
            SyntaxKind::Whitespace => newlines += breaks(child.text(text)),
            SyntaxKind::Comment => {
                let slot = match units.last_mut() {
                    Some(unit) => &mut unit.trailing,
                    None => &mut head_trailing,
                };
                if newlines == 0 && pending.is_empty() && slot.is_none() {
                    *slot = Some(comment(child, text));
                } else {
                    pending.push(comment(child, text));
                }
            }
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
                units.push(Unit {
                    rank,
                    leading: std::mem::take(&mut pending),
                    nodes: vec![child],
                    trailing: None,
                });
                newlines = 0;
            }
            kind if is_construct(kind) && member_rank(child, text).is_some() => {
                units.push(Unit {
                    rank: member_rank(child, text)?,
                    leading: std::mem::take(&mut pending),
                    nodes: vec![child],
                    trailing: None,
                });
                newlines = 0;
            }
            _ => {
                if !pending.is_empty() {
                    return None;
                }
                match units.last_mut() {
                    Some(unit) if is_construct(child.kind()) && unit.trailing.is_none() => {
                        unit.nodes.push(child);
                    }
                    None if head_trailing.is_none() => head.push(child),
                    _ => return None,
                }
                newlines = 0;
            }
        }
    }
    units.sort_by_key(|unit| unit.rank);
    let mut opener = construct.keyword().to_owned();
    if !head.is_empty() {
        opener.push(' ');
        opener.push_str(&flat(&head, text)?);
    }
    out.push(Item::Begin);
    out.push(Item::Text(opener));
    if let Some(trailing) = head_trailing {
        out.push(Item::TrailingComment(trailing));
    }
    out.push(Item::Indent);
    let mut first_break = if has_nested_let(node, text) {
        Item::Hard
    } else {
        Item::Space
    };
    for unit in &units {
        for leading in &unit.leading {
            out.push(std::mem::replace(&mut first_break, Item::Space));
            out.push(Item::Comment(leading.clone()));
        }
        out.push(std::mem::replace(&mut first_break, Item::Space));
        unit_items(unit, text, out)?;
        if let Some(trailing) = &unit.trailing {
            out.push(Item::TrailingComment(trailing.clone()));
        }
    }
    for closing in pending {
        out.push(std::mem::replace(&mut first_break, Item::Space));
        out.push(Item::Comment(closing));
    }
    out.push(Item::Dedent);
    out.push(first_break);
    out.push(Item::Text(closer.text(text).to_owned()));
    out.push(Item::End);
    Some(())
}

/// A unit's items; the body of `do` goes on the next line, one step further
/// in, when the declaration does not fit on one line.
fn unit_items(unit: &Unit<'_>, text: &str, out: &mut Vec<Item>) -> Option<()> {
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
            let glued_before = punctuation && matches!(token, ")" | "}" | "]" | "(" | ":" | ".");
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
mod tests {
    use vibra_syntax::parse_source;

    use super::format;

    /// The layout of a document, whatever its diagnostics: the forms other
    /// than `use` are `@tool.unavailable` in this step, so their layout is
    /// reachable only here.
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
            layout("fn f\n  // the result\n  returns r: I32 takes {x: I32} // one\nendfn"),
            "fn f\n  takes {x: I32} // one\n  // the result\n  returns r: I32\nendfn\n"
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
                "type A impl B endimpl fn g returns r: I32 endfn of U8 type C endtype endtype"
            ),
            "type A of U8 type C endtype fn g returns r: I32 endfn impl B endimpl endtype\n"
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
    }
}
