//! Canonical member order of anonymous structural types.
//!
//! `docs/spec/02-type-system.md` makes anonymous `record`, `enum`, and `union`
//! types order-insensitive and gives each one canonical spelling: record
//! fields and enum variants sorted by name, union members sorted by their
//! canonical text. A declared body keeps its written order, because its
//! identity is the declaration.
//!
//! The reordering is a source-to-source pre-pass that permutes member texts
//! within their own slots and keeps every separator where it was, so the byte
//! length of each reordered type is unchanged and no span outside it moves.
//! Binding facts keyed by application spans therefore stay valid. A type that
//! contains a comment is left in written order, since a comment has no
//! unambiguous member to travel with.

use vibra_syntax::{CstNode, SyntaxKind};

/// Returns the source with every anonymous structural type in canonical order,
/// or `None` when the source is already canonical.
pub(crate) fn canonical_structural_order(
    root: &CstNode,
    source: &str,
) -> Option<String> {
    let mut replacements = Vec::new();
    // Each entry is a node and whether it is a list that cannot itself be a
    // structural type: a declared body, or a list in a position that holds
    // names, such as a parameter list whose first parameter is named `union`.
    let mut pending = vec![(root, false)];
    while let Some((node, not_a_type)) = pending.pop() {
        if node.kind() != SyntaxKind::List {
            // The document root is not a list; descend through it.
            pending.extend(meaningful(node).into_iter().map(|child| (child, false)));
            continue;
        }
        if !not_a_type && structural_head(node).is_some() {
            let text = canonical_text(node, source);
            if text != slice(source, node) {
                replacements.push((node.span().start(), node.span().end(), text));
            }
            continue;
        }
        let head = head_text(node);
        let children = meaningful(node);
        for (index, child) in children.iter().enumerate() {
            // The body of a `deftype` keeps its written order; a parameter
            // list and the entries after `labelled:`, `variadic:`, and
            // `where:` bind names, though the types inside them are types.
            let binds_names =
                matches!((head, index), (Some("defn"), 2) | (Some("lambda"), 1))
                    || index
                        .checked_sub(1)
                        .and_then(|before| children.get(before))
                        .and_then(|before| before.leaf_text())
                        .is_some_and(|label| {
                            matches!(label, "labelled:" | "variadic:" | "where:")
                        });
            pending.push((
                child,
                (head == Some("deftype") && index == 2) || binds_names,
            ));
        }
    }
    if replacements.is_empty() {
        return None;
    }
    replacements.sort_by_key(|(start, _, _)| *start);
    let mut output = String::with_capacity(source.len());
    let mut cursor = 0;
    for (start, end, text) in replacements {
        output.push_str(source.get(cursor..start)?);
        output.push_str(&text);
        cursor = end;
    }
    output.push_str(source.get(cursor..)?);
    Some(output)
}

/// The head of an anonymous structural type whose members may be reordered.
fn structural_head(node: &CstNode) -> Option<&'static str> {
    match head_text(node)? {
        "record" => Some("record"),
        "enum" => Some("enum"),
        "union" => Some("union"),
        _ => None,
    }
}

/// The canonical text of `node`, with the same byte length as its source.
fn canonical_text(node: &CstNode, source: &str) -> String {
    if node.kind() != SyntaxKind::List || contains_comment(node) {
        return slice(source, node).to_owned();
    }
    let children = meaningful(node);
    let (Some(head), Some((_, members))) =
        (structural_head(node), children.split_first())
    else {
        return rebuild(node, source, &children);
    };
    let items: Vec<(usize, usize)> = if head == "union" {
        members
            .iter()
            .map(|member| (member.span().start(), member.span().end()))
            .collect()
    } else {
        if !members.len().is_multiple_of(2) {
            return slice(source, node).to_owned();
        }
        members
            .chunks_exact(2)
            .filter_map(|pair| match pair {
                [name, value] => Some((name.span().start(), value.span().end())),
                _ => None,
            })
            .collect()
    };
    let mut keyed: Vec<(String, String)> = items
        .iter()
        .map(|(start, end)| {
            let text = item_text(node, source, *start, *end);
            let key = if head == "union" {
                normalized(&text)
            } else {
                text.split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .to_owned()
            };
            (key, text)
        })
        .collect();
    keyed.sort_by(|left, right| left.0.cmp(&right.0));
    let permuted: Vec<String> = keyed.into_iter().map(|(_, text)| text).collect();
    rebuild_items(node, source, &items, &permuted)
}

/// The canonical text of one member span: its nested lists canonicalized and
/// the trivia between them kept.
fn item_text(node: &CstNode, source: &str, start: usize, end: usize) -> String {
    let mut output = String::new();
    let mut cursor = start;
    for child in meaningful(node) {
        let span = child.span();
        if span.start() < start || span.end() > end {
            continue;
        }
        output.push_str(source.get(cursor..span.start()).unwrap_or_default());
        output.push_str(&canonical_text(child, source));
        cursor = span.end();
    }
    output.push_str(source.get(cursor..end).unwrap_or_default());
    output
}

/// Rebuilds a non-reordered list from its children's canonical texts.
fn rebuild(node: &CstNode, source: &str, children: &[&CstNode]) -> String {
    let mut output = String::new();
    let mut cursor = node.span().start();
    for child in children {
        let span = child.span();
        output.push_str(source.get(cursor..span.start()).unwrap_or_default());
        output.push_str(&canonical_text(child, source));
        cursor = span.end();
    }
    output.push_str(source.get(cursor..node.span().end()).unwrap_or_default());
    output
}

/// Places permuted member texts into the original member slots.
fn rebuild_items(
    node: &CstNode,
    source: &str,
    items: &[(usize, usize)],
    permuted: &[String],
) -> String {
    let mut output = String::new();
    let mut cursor = node.span().start();
    for ((start, end), text) in items.iter().zip(permuted) {
        output.push_str(source.get(cursor..*start).unwrap_or_default());
        output.push_str(text);
        cursor = *end;
    }
    output.push_str(source.get(cursor..node.span().end()).unwrap_or_default());
    output
}

/// `text` as the layout pass will write it: single spaces between tokens and
/// none inside a delimiter. The sort key must not depend on written spacing,
/// or a second pass over the laid-out text would order the members again.
fn normalized(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("( ", "(")
        .replace(" )", ")")
}

fn slice<'source>(source: &'source str, node: &CstNode) -> &'source str {
    source
        .get(node.span().start()..node.span().end())
        .unwrap_or_default()
}

fn meaningful(node: &CstNode) -> Vec<&CstNode> {
    node.children()
        .iter()
        .filter(|child| matches!(child.kind(), SyntaxKind::Atom | SyntaxKind::List))
        .collect()
}

fn head_text(node: &CstNode) -> Option<&str> {
    meaningful(node).first().and_then(|head| head.leaf_text())
}

fn contains_comment(node: &CstNode) -> bool {
    let mut nodes = vec![node];
    while let Some(current) = nodes.pop() {
        if current.kind() == SyntaxKind::LineComment {
            return true;
        }
        nodes.extend(current.children().iter());
    }
    false
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::canonical_structural_order;

    fn reorder(source: &str) -> Option<String> {
        let document =
            vibra_syntax::parse_document(std::path::Path::new("x.vib"), source)
                .expect("source mode");
        canonical_structural_order(document.root(), document.source())
    }

    #[test]
    fn anonymous_members_are_sorted_recursively_without_changing_length() {
        let source =
            "(defn f (v (record b (union str i64) a (enum y void x void))) i32 0i32)";
        let reordered = reorder(source).expect("reordered");
        assert_eq!(
            reordered,
            "(defn f (v (record a (enum x void y void) b (union i64 str))) i32 0i32)"
        );
        assert_eq!(reordered.len(), source.len());
    }

    #[test]
    fn declared_bodies_and_canonical_types_are_left_alone() {
        assert_eq!(reorder("(deftype p (record y i32 x i32))"), None);
        assert_eq!(reorder("(defn f (v (record a i32 b i32)) i32 0i32)"), None);
    }

    #[test]
    fn a_declared_body_still_orders_its_nested_anonymous_types() {
        assert_eq!(
            reorder("(deftype p (record y (record b i32 a i32) x i32))").as_deref(),
            Some("(deftype p (record y (record a i32 b i32) x i32))")
        );
    }

    #[test]
    fn a_commented_anonymous_type_keeps_written_order() {
        assert_eq!(
            reorder("(defn f (v (record b i32\n ; note\n a i32)) i32 0i32)"),
            None
        );
    }
}
