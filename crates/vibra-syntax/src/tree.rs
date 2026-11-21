//! The `@tree.v2` observation of a document.
//!
//! `docs/spec/07-diagnostics-and-conformance.md`, "Persistent observations":
//! the document is `{format: @tree.v2 root: N}`, an interior node is
//! `{kind: K children: C}` and a leaf is `{kind: K text: Str}`. The texts of
//! the leaves, concatenated in order, equal the input. The root is the
//! document node, `@data` or `@source`; a choice production is never a node,
//! and neither is a generic bracket group, whose children take its place.
//!
//! The writer is iterative and writes one line of VIBON. A snapshot is
//! compared as a decoded value, so its layout is free.

use crate::cst::CstNode;
use crate::document::Document;
use crate::literal::canonical_string;

/// The format atom of the tree observation.
pub const TREE_FORMAT: &str = "@tree.v2";

/// The `@tree.v2` document of `document`, or `None` when it has no tree
/// because it was never read or is not valid UTF-8.
#[must_use]
pub fn tree_document(document: &Document) -> Option<String> {
    let text = document.text()?;
    let root = document.root()?;
    Some(tree_text(root, text))
}

/// The `@tree.v2` document of the tree `root` over `text`.
#[must_use]
pub fn tree_text(root: &CstNode, text: &str) -> String {
    enum Work<'a> {
        Node(&'a CstNode),
        Close,
    }
    let mut out = format!("{{format: {TREE_FORMAT} root: ");
    // Whether the list being written already has an element.
    let mut started = vec![false];
    let mut work = vec![Work::Node(root)];
    while let Some(step) = work.pop() {
        match step {
            Work::Node(node) => {
                let Some(kind) = node.kind().tree_kind() else {
                    work.extend(node.children().iter().rev().map(Work::Node));
                    continue;
                };
                if let Some(started) = started.last_mut() {
                    if *started {
                        out.push(' ');
                    }
                    *started = true;
                }
                if node.kind().is_leaf() {
                    out.push_str("{kind: ");
                    out.push_str(kind);
                    out.push_str(" text: ");
                    out.push_str(&canonical_string(node.text(text)));
                    out.push('}');
                } else {
                    out.push_str("{kind: ");
                    out.push_str(kind);
                    out.push_str(" children: [");
                    started.push(false);
                    work.push(Work::Close);
                    work.extend(node.children().iter().rev().map(Work::Node));
                }
            }
            Work::Close => {
                started.pop();
                out.push_str("]}");
            }
        }
    }
    out.push('}');
    out
}
