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
//!   `performs {}` is removed, its comments moving to the line before what
//!   follows it;
//! - a module value laid out as a `let` ([`crate::expr`]); with a
//!   `visibility` or `doc` section, as a declaration whose last section is
//!   `be` and its value.
//!
//! A `fn`, `proc`, or `test`, named or anonymous, is laid out with the
//! expressions ([`crate::expr`]): its signature split into its sections, a
//! guard's `else` on the next line one step in when both do not fit, and the
//! body statements one step in from `do`. Comments inside a form go with
//! their tokens ([`crate::comments`]), so a comment moves with its section.

use vibra_syntax::source::Construct;
use vibra_syntax::source::keyword::{SECTIONS, section_rank};
use vibra_syntax::{CstNode, SyntaxKind};

use crate::comments::Comments;
use crate::expr::{
    Work, close, emit, is_colon, run, significant, token, value_group, word,
};
use crate::layout::{Item, render, trailing_comment};

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

/// The canonical text of a module with no diagnostic. `None` only when the
/// tree is not of a shape the reader accepts.
pub(crate) fn format(root: &CstNode, text: &str) -> Option<String> {
    let mut comments = Comments::of(root, text);
    let mut items = Vec::new();
    let mut last = Last::Nothing;
    let mut newlines = 0;
    for child in root.children() {
        match child.kind() {
            SyntaxKind::Whitespace => newlines += breaks(child.text(text)),
            SyntaxKind::Comment if last == Last::Form && newlines == 0 => {
                items.push(trailing_comment(text, child.span().start()));
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
                run(Work::Declaration(child), text, &mut comments, &mut items)?;
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
    nodes: Vec<&'a CstNode>,
}

/// A declaration split into its head and its units.
struct Parts<'a> {
    head: Vec<&'a CstNode>,
    units: Vec<Unit<'a>>,
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
    start: impl Fn(&CstNode) -> Option<(usize, bool)>,
) -> Option<Parts<'a>> {
    let mut parts = Parts {
        head: Vec::new(),
        units: Vec::new(),
    };
    // Whether the last unit takes the nodes that follow it.
    let mut open = false;
    for child in contents {
        if matches!(child.kind(), SyntaxKind::Whitespace | SyntaxKind::Comment) {
            continue;
        }
        if let Some((rank, takes_more)) = start(child) {
            parts.units.push(Unit {
                rank,
                nodes: vec![child],
            });
            open = takes_more;
        } else {
            match parts.units.last_mut() {
                Some(unit) if open => unit.nodes.push(child),
                None => parts.head.push(child),
                Some(_) => return None,
            }
        }
    }
    parts.units.sort_by_key(|unit| unit.rank);
    Some(parts)
}

/// The opener, the contents, and the closer of a construct.
fn frame(node: &CstNode) -> Option<(&CstNode, Vec<&CstNode>, &CstNode)> {
    let mut items = significant(node);
    let closer = items
        .pop()
        .filter(|last| last.kind() != SyntaxKind::Missing)?;
    if items.is_empty() {
        return None;
    }
    let opener = items.remove(0);
    Some((opener, items, closer))
}

/// Schedules one declaration, named member, or anonymous callable.
pub(crate) fn declaration<'a>(
    node: &'a CstNode,
    text: &str,
    comments: &mut Comments,
    next: &mut Vec<Work<'a>>,
) -> Option<()> {
    match node.kind() {
        SyntaxKind::TypeDecl
        | SyntaxKind::InterfaceDecl
        | SyntaxKind::Impl
        | SyntaxKind::AssocType
        | SyntaxKind::AssocChoice => typed(node, text, comments, next),
        SyntaxKind::ModuleLet => module_let(node, text, next),
        SyntaxKind::FnDecl
        | SyntaxKind::ProcDecl
        | SyntaxKind::Test
        | SyntaxKind::AnonFn
        | SyntaxKind::AnonProc => callable(node, text, comments, next),
        _ => generic(node, text, next),
    }
}

/// A module value: the layout of a `let` when it has only `be`, otherwise
/// the declaration layout with `be` and its value as the last section.
fn module_let<'a>(
    node: &'a CstNode,
    text: &str,
    next: &mut Vec<Work<'a>>,
) -> Option<()> {
    let (opener, contents, closer) = frame(node)?;
    let is_section = |child: &&CstNode| {
        matches!(child.kind(), SyntaxKind::Visibility | SyntaxKind::Doc)
    };
    let mut sections: Vec<&CstNode> =
        contents.iter().copied().filter(is_section).collect();
    if sections.is_empty() {
        next.push(Work::Form(node));
        return Some(());
    }
    let be = contents.iter().position(|child| {
        child.kind() == SyntaxKind::ReservedWord && child.text(text) == "be"
    })?;
    let head: Vec<&CstNode> = contents
        .get(..be)?
        .iter()
        .copied()
        .filter(|child| !is_section(child))
        .collect();
    let (name, written) = head.split_at(head.len().min(2));
    let mut opener_steps = Vec::from(word(opener, format!("{} ", opener.text(text))));
    opener_steps.extend([
        Work::Tokens(name.to_vec()),
        emit(Item::Text(" ".to_owned())),
        Work::Type(written.to_vec()),
    ]);
    sections.sort_by_key(|section| {
        section_rank(
            section
                .children()
                .first()
                .map_or("", |keyword| keyword.text(text)),
        )
    });
    let mut units: Vec<Vec<Work<'a>>> = sections
        .into_iter()
        .map(|section| vec![Work::Tokens(vec![section])])
        .collect();
    // The value on the line of `be` when it fits, otherwise on the next line
    // one step in, as in a `let`.
    let mut be_steps = Vec::from(word(contents.get(be)?, "be"));
    be_steps.extend(value_group(contents.get(be + 1..)?));
    units.push(be_steps);
    lay_out(opener_steps, units, closer, text, false, None, next);
    Some(())
}

/// A `use`: its sections by keyword, each operand written flat.
fn generic<'a>(node: &'a CstNode, text: &str, next: &mut Vec<Work<'a>>) -> Option<()> {
    let (opener, contents, closer) = frame(node)?;
    let parts = split(contents, |child| {
        if child.kind() != SyntaxKind::Section {
            return None;
        }
        let keyword = child
            .children()
            .first()
            .map_or("", |keyword| keyword.text(text));
        Some((section_rank(keyword)?, true))
    })?;
    let opener_steps = head_steps(opener, &parts.head, text);
    let units = parts
        .units
        .into_iter()
        .map(|unit| vec![Work::Tokens(unit.nodes)])
        .collect();
    lay_out(
        opener_steps,
        units,
        closer,
        text,
        has_nested_let(node, text),
        None,
        next,
    );
    Some(())
}

/// The opener of a declaration and its head on one line.
fn head_steps<'a>(
    opener: &'a CstNode,
    head: &[&'a CstNode],
    text: &str,
) -> Vec<Work<'a>> {
    let mut steps = Vec::from(token(opener, text));
    if !head.is_empty() {
        steps.extend([
            emit(Item::Text(" ".to_owned())),
            Work::Tokens(head.to_vec()),
        ]);
    }
    steps
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
fn callable<'a>(
    node: &'a CstNode,
    text: &str,
    comments: &mut Comments,
    next: &mut Vec<Work<'a>>,
) -> Option<()> {
    let (opener, contents, closer) = frame(node)?;
    let flattened = contents.into_iter().flat_map(|child| {
        if child.kind() == SyntaxKind::Signature {
            child.children().iter().collect()
        } else {
            vec![child]
        }
    });
    let mut parts = split(flattened, |child| callable_rank(child, text))?;
    drop_empty(&mut parts, text, comments, closer);
    let opener_steps = head_steps(opener, &parts.head, text);
    let broken = !parts.units.is_empty();
    let units = parts
        .units
        .iter()
        .map(|unit| callable_unit(unit, text))
        .collect::<Option<Vec<_>>>()?;
    lay_out(opener_steps, units, closer, text, broken, None, next);
    Some(())
}

/// A callable unit's steps.
fn callable_unit<'a>(unit: &Unit<'a>, text: &str) -> Option<Vec<Work<'a>>> {
    let first = *unit.nodes.first()?;
    let rest = unit.nodes.get(1..)?;
    let mut steps = Vec::new();
    match first.kind() {
        SyntaxKind::ReservedWord => match (first.text(text), rest) {
            ("do", []) => steps.extend(word(first, "do")),
            ("do", [body]) => {
                steps.extend(word(first, "do"));
                steps.extend([
                    emit(Item::Indent),
                    Work::Body(body),
                    emit(Item::Dedent),
                ]);
            }
            ("takes", [params]) => {
                steps.extend(word(first, "takes "));
                steps.push(Work::Type(vec![params]));
            }
            ("labelled", [labelled]) => {
                steps.extend(word(first, "labelled "));
                steps.push(Work::Labelled(labelled));
            }
            ("returns", [name, colon, written @ ..]) if is_colon(colon, text) => {
                steps.extend(word(first, "returns "));
                steps.push(Work::Tokens(vec![name]));
                steps.extend(word(colon, ": "));
                steps.push(Work::Type(written.to_vec()));
            }
            _ => return None,
        },
        SyntaxKind::Requires | SyntaxKind::Ensures => steps.push(Work::Guard(first)),
        SyntaxKind::Given | SyntaxKind::Performs => {
            let items = significant(first);
            let (keyword, operand) = items.split_first()?;
            steps.extend(word(keyword, format!("{} ", keyword.text(text))));
            steps.push(Work::Type(operand.to_vec()));
        }
        _ => steps.push(Work::Tokens(unit.nodes.clone())),
    }
    Some(steps)
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

/// Removes every empty section; its comments move to the line before the
/// unit after it, or before the closer.
fn drop_empty<'a>(
    parts: &mut Parts<'a>,
    text: &str,
    comments: &mut Comments,
    closer: &'a CstNode,
) {
    let mut carried: Vec<&CstNode> = Vec::new();
    let mut kept = Vec::with_capacity(parts.units.len());
    for unit in std::mem::take(&mut parts.units) {
        if is_empty_section(&unit, text) {
            carried.extend(&unit.nodes);
            continue;
        }
        if let Some(first) = unit.nodes.first() {
            comments.carry(&carried, first);
            carried.clear();
        }
        kept.push(unit);
    }
    comments.carry(&carried, closer);
    parts.units = kept;
}

/// A typed declaration: `type`, `interface`, `impl`, an associated type,
/// or an associated choice.
fn typed<'a>(
    node: &'a CstNode,
    text: &str,
    comments: &mut Comments,
    next: &mut Vec<Work<'a>>,
) -> Option<()> {
    let (opener, contents, closer) = frame(node)?;
    let mut parts = split(contents, |child| typed_rank(child, text))?;
    drop_empty(&mut parts, text, comments, closer);
    let mut opener_steps = Vec::from(token(opener, text));
    if !parts.head.is_empty() {
        opener_steps.push(emit(Item::Text(" ".to_owned())));
        if node.kind() == SyntaxKind::Impl {
            opener_steps.push(Work::Type(parts.head.clone()));
        } else {
            opener_steps.push(Work::Tokens(parts.head.clone()));
        }
    }
    let starts_with_of = |unit: &Unit<'_>| {
        unit.nodes
            .first()
            .is_some_and(|node| node.kind() == SyntaxKind::ReservedWord)
    };
    // Every unit but `of` is a case, a section other than `of`, or a member.
    let always_broken = parts.units.iter().any(|unit| !starts_with_of(unit));
    let of_first = parts
        .units
        .first()
        .filter(|unit| starts_with_of(unit))
        .and_then(|unit| unit.nodes.last().copied());
    let units = parts
        .units
        .iter()
        .map(|unit| typed_unit(unit, text))
        .collect::<Option<Vec<_>>>()?;
    lay_out(
        opener_steps,
        units,
        closer,
        text,
        always_broken || has_nested_let(node, text),
        of_first,
        next,
    );
    Some(())
}

/// A typed unit's steps.
fn typed_unit<'a>(unit: &Unit<'a>, text: &str) -> Option<Vec<Work<'a>>> {
    let first = *unit.nodes.first()?;
    let mut steps = Vec::new();
    match first.kind() {
        SyntaxKind::ReservedWord => {
            steps.extend(word(first, "of "));
            steps.push(Work::Type(unit.nodes.get(1..)?.to_vec()));
        }
        SyntaxKind::Case => {
            let (record, name): (Vec<&CstNode>, Vec<&CstNode>) = significant(first)
                .into_iter()
                .partition(|child| child.kind() == SyntaxKind::RecordType);
            steps.push(Work::Tokens(name));
            if !record.is_empty() {
                steps.extend([emit(Item::Text(" ".to_owned())), Work::Type(record)]);
            }
        }
        SyntaxKind::Given => {
            let items = significant(first);
            let keyword = items.first()?;
            let brace: Vec<&CstNode> = items
                .iter()
                .copied()
                .filter(|child| child.kind() == SyntaxKind::Brace)
                .collect();
            steps.extend(word(keyword, "given "));
            steps.push(Work::Type(brace));
        }
        SyntaxKind::Where => {
            let items = significant(first);
            let (keyword, operand) = items.split_first()?;
            let split = operand.iter().position(|node| {
                node.kind() == SyntaxKind::ReservedWord && node.text(text) == "else"
            });
            let (predicate, consequence) = match split {
                Some(at) => (operand.get(..at)?, operand.get(at..)),
                None => (operand, None),
            };
            steps.extend(word(keyword, "where "));
            steps.push(Work::Expr(predicate.to_vec()));
            if let Some((else_word, consequence)) =
                consequence.and_then(<[_]>::split_first)
            {
                steps.extend(word(else_word, " else "));
                steps.push(Work::Expr(consequence.to_vec()));
            }
        }
        SyntaxKind::AssocType
        | SyntaxKind::AssocChoice
        | SyntaxKind::Impl
        | SyntaxKind::FnDecl
        | SyntaxKind::ProcDecl => steps.push(Work::Declaration(first)),
        _ => steps.push(Work::Tokens(unit.nodes.clone())),
    }
    Some(steps)
}

/// Lays out a declaration: on one line when it fits, holds no comment, and
/// is not `broken`; otherwise its opener line, each unit on its own line one
/// step in, and the closer on its own line. With `of_first`, the last node
/// of the first unit, that unit is `of`, which stays on the opener's line
/// when that line fits.
fn lay_out<'a>(
    opener: Vec<Work<'a>>,
    units: Vec<Vec<Work<'a>>>,
    closer: &'a CstNode,
    text: &str,
    broken: bool,
    of_first: Option<&'a CstNode>,
    next: &mut Vec<Work<'a>>,
) {
    next.push(emit(Item::Begin));
    if broken {
        next.push(emit(Item::Force));
    }
    next.extend(opener);
    next.push(emit(Item::Indent));
    for (index, steps) in units.into_iter().enumerate() {
        if index == 0
            && let Some(of_last) = of_first
        {
            // A comment after the type is after the group, so it does not
            // move `of` off the opener's line.
            next.extend([Work::Hold(of_last), emit(Item::Begin), emit(Item::Space)]);
            next.extend(steps);
            next.extend([emit(Item::End), Work::Release]);
        } else {
            next.push(emit(Item::Space));
            next.extend(steps);
        }
    }
    next.extend(close(closer, text, true, Item::Space));
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
                "let f: Fn({} I32) visibility @public be fn returns r: I32 do return 1i32 endfn endlet"
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
    fn a_comment_inside_a_section_stays_with_its_token() {
        assert_eq!(
            layout("fn f returns r: // c\n I32 do return 1i32 endfn"),
            "fn f\n  returns r: // c\n  I32\n  do\n    return 1i32\nendfn\n"
        );
        assert_eq!(
            layout(
                "fn f returns r: I32 requires ready // why\n else 0i32 do return 1i32 endfn"
            ),
            "fn f\n  returns r: I32\n  requires ready // why\n    else 0i32\n  do\n    return 1i32\nendfn\n"
        );
        assert_eq!(
            layout(
                "fn f labelled {up: // c\n Bool be false} returns r: I32 do return 1i32 endfn"
            ),
            "fn f\n  labelled {\n    up: // c\n    Bool be false\n  }\n  returns r: I32\n  do\n    return 1i32\nendfn\n"
        );
        assert_eq!(
            layout("use @std.io // io\n as io enduse"),
            "use @std.io // io\n  as io\nenduse\n"
        );
    }

    #[test]
    fn the_comments_of_a_removed_empty_section_move_to_what_follows_it() {
        for (text, expected) in [
            (
                "fn f takes {} // none\n returns r: I32 do return 1i32 endfn",
                "fn f\n  // none\n  returns r: I32\n  do\n    return 1i32\nendfn\n",
            ),
            (
                "proc p returns r: Void performs {} // none\n do return void endproc",
                "proc p\n  returns r: Void\n  // none\n  do\n    return void\nendproc\n",
            ),
            (
                "type A of U8 given {} // none\nendtype",
                "type A of U8\n  // none\nendtype\n",
            ),
        ] {
            let document = parse_source(None, text.as_bytes());
            assert!(document.diagnostics().is_empty(), "{text:?} is valid");
            assert_eq!(layout(text), expected, "{text:?}");
        }
    }
}
