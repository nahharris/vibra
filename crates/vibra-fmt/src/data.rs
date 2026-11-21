//! The source-preserving VIBON formatter.
//!
//! `docs/spec/04-programs-and-packages.md`, "Decoded values and canonical
//! bytes": the source-preserving formatter keeps comments, written spellings,
//! and the written order of fields, elements, and entries, and applies only
//! the layout rules. It never produces canonical bytes.
//!
//! The layout rules are those of `docs/spec/01-source-language.md`,
//! "Canonical format": one space between tokens, none inside brackets or
//! before `:`, one after `:`; a brace or bracket form on one line when it fits
//! and holds no comment, otherwise one entry per line, one step in, with the
//! closer on its own line; a comment on a construct's line stays at the end
//! of that line, and any other comment stays on its own line before the form
//! that follows it.

use vibra_syntax::{CstNode, SyntaxKind};

use crate::layout::{Item, trailing_comment};

/// One line of a form's contents, before layout.
struct Line<'a> {
    /// An entry, or an own-line comment.
    node: &'a CstNode,
    /// The comment at the end of the line, if any.
    trailing: Option<&'a CstNode>,
}

/// Work left for the lowering, newest last.
enum Work<'a> {
    Node(&'a CstNode),
    Emit(Item),
}

fn comment(node: &CstNode, text: &str) -> String {
    node.text(text).trim_end().to_owned()
}

/// Splits the contents of a form or a document into lines: each entry with
/// the comment written after it on its line, and each other comment alone.
/// Also returns the comment written on the opener's line.
fn lines<'a>(
    contents: &'a [CstNode],
    text: &str,
) -> (Option<&'a CstNode>, Vec<Line<'a>>) {
    let mut opener_comment = None;
    let mut lines: Vec<Line<'a>> = Vec::new();
    // Whether a line break was written since the last entry or the opener.
    let mut broke = false;
    for node in contents {
        match node.kind() {
            SyntaxKind::Whitespace => broke |= node.text(text).contains(['\n', '\r']),
            SyntaxKind::Comment if !broke => match lines.last_mut() {
                Some(line) if line.node.kind() != SyntaxKind::Comment => {
                    line.trailing = Some(node);
                }
                Some(_) => lines.push(Line {
                    node,
                    trailing: None,
                }),
                None => opener_comment = Some(node),
            },
            _ => {
                lines.push(Line {
                    node,
                    trailing: None,
                });
                broke = false;
            }
        }
    }
    (opener_comment, lines)
}

/// The layout items of a brace or bracket form.
fn form<'a>(node: &'a CstNode, text: &str, out: &mut Vec<Work<'a>>) {
    let children = node.children();
    let (Some(opener), Some(closer)) = (children.first(), children.last()) else {
        return;
    };
    let contents = children
        .get(1..children.len().saturating_sub(1))
        .unwrap_or_default();
    let (opener_comment, lines) = lines(contents, text);
    out.push(Work::Emit(Item::Begin));
    out.push(Work::Emit(Item::Text(opener.text(text).to_owned())));
    if let Some(opener_comment) = opener_comment {
        out.push(Work::Emit(trailing_comment(
            text,
            opener_comment.span().start(),
        )));
    }
    out.push(Work::Emit(Item::Indent));
    for (index, line) in lines.iter().enumerate() {
        out.push(Work::Emit(if index == 0 {
            Item::Soft
        } else {
            Item::Space
        }));
        push_line(line, text, out);
    }
    out.push(Work::Emit(Item::Dedent));
    out.push(Work::Emit(Item::Soft));
    out.push(Work::Emit(Item::Text(closer.text(text).to_owned())));
    out.push(Work::Emit(Item::End));
}

fn push_line<'a>(line: &Line<'a>, text: &str, out: &mut Vec<Work<'a>>) {
    if line.node.kind() == SyntaxKind::Comment {
        out.push(Work::Emit(Item::Comment(comment(line.node, text))));
    } else {
        out.push(Work::Node(line.node));
    }
    if let Some(trailing) = line.trailing {
        out.push(Work::Emit(trailing_comment(text, trailing.span().start())));
    }
}

/// The layout items of a field or an entry: key, `:`, one space, value. A
/// comment inside one breaks the line after it, and the rest continues one
/// step in.
fn pair<'a>(node: &'a CstNode, text: &str, out: &mut Vec<Work<'a>>) {
    let mut broke = false;
    let mut after_break = false;
    let mut indented = false;
    let mut seen_colon = false;
    for child in node.children() {
        match child.kind() {
            SyntaxKind::Whitespace => broke |= child.text(text).contains(['\n', '\r']),
            SyntaxKind::Comment => {
                if !indented {
                    out.push(Work::Emit(Item::Indent));
                    indented = true;
                }
                if broke {
                    if !after_break {
                        out.push(Work::Emit(Item::Hard));
                    }
                    out.push(Work::Emit(Item::Comment(comment(child, text))));
                } else {
                    out.push(Work::Emit(trailing_comment(text, child.span().start())));
                }
                out.push(Work::Emit(Item::Hard));
                after_break = true;
                broke = false;
            }
            SyntaxKind::Punctuation if !seen_colon && child.text(text) == ":" => {
                seen_colon = true;
                out.push(Work::Emit(Item::Text(":".to_owned())));
                after_break = false;
                broke = false;
            }
            _ => {
                if seen_colon && !after_break {
                    out.push(Work::Emit(Item::Text(" ".to_owned())));
                }
                out.push(Work::Node(child));
                after_break = false;
                broke = false;
            }
        }
    }
    if indented {
        out.push(Work::Emit(Item::Dedent));
    }
}

/// The layout items of a valid data document.
pub(crate) fn document_items(root: &CstNode, text: &str) -> Vec<Item> {
    let (leading, lines) = lines(root.children(), text);
    let mut top = Vec::new();
    if let Some(leading) = leading {
        // A comment first in the document is on a line of its own.
        top.push(Work::Emit(Item::Comment(comment(leading, text))));
        top.push(Work::Emit(Item::Hard));
    }
    for line in &lines {
        push_line(line, text, &mut top);
        top.push(Work::Emit(Item::Hard));
    }
    let mut items = Vec::new();
    let mut work = top;
    work.reverse();
    while let Some(step) = work.pop() {
        match step {
            Work::Emit(item) => items.push(item),
            Work::Node(node) => {
                let mut expanded = Vec::new();
                match node.kind() {
                    SyntaxKind::Tuple
                    | SyntaxKind::Record
                    | SyntaxKind::List
                    | SyntaxKind::Dict => {
                        form(node, text, &mut expanded);
                    }
                    SyntaxKind::Field | SyntaxKind::Entry => {
                        pair(node, text, &mut expanded)
                    }
                    _ => items.push(Item::Text(node.text(text).to_owned())),
                }
                work.extend(expanded.into_iter().rev());
            }
        }
    }
    items
}
