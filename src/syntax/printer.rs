use std::fmt::Write;

use super::{Atom, Document, Node, NodeKind};

const INDENT: usize = 2;
const INLINE_WIDTH: usize = 88;

/// Render deterministic canonical source. Reader comments are retained.
pub fn print(document: &Document) -> String {
    let mut output = String::new();
    for (index, node) in document.nodes.iter().enumerate() {
        if index > 0 {
            output.push('\n');
        }
        print_node(node, 0, &mut output);
    }
    if !output.is_empty() {
        output.push('\n');
    }
    output
}

fn print_node(node: &Node, indent: usize, output: &mut String) {
    match &node.kind {
        NodeKind::Atom(atom) => print_atom(atom, output),
        NodeKind::Comment(comment) => {
            output.push(';');
            output.push_str(comment);
        }
        NodeKind::List(children) if can_inline(children, indent) => {
            output.push('(');
            for (index, child) in children.iter().enumerate() {
                if index > 0 {
                    output.push(' ');
                }
                print_node(child, indent + INDENT, output);
            }
            output.push(')');
        }
        NodeKind::List(children) => {
            output.push('(');
            let (head, tail) = match children.split_first() {
                Some((head, tail)) if matches!(head.kind, NodeKind::Atom(_)) => (Some(head), tail),
                _ => (None, children.as_slice()),
            };
            if let Some(head) = head {
                print_node(head, indent + 1, output);
            }
            for child in tail {
                output.push('\n');
                output.push_str(&" ".repeat(indent + INDENT));
                print_node(child, indent + INDENT, output);
            }
            if !children.is_empty() {
                output.push('\n');
                output.push_str(&" ".repeat(indent));
            }
            output.push(')');
        }
    }
}

fn can_inline(children: &[Node], indent: usize) -> bool {
    if children.iter().any(|child| {
        matches!(child.kind, NodeKind::Comment(_))
            || matches!(&child.kind, NodeKind::List(nested) if !can_inline(nested, indent + INDENT))
    }) {
        return false;
    }
    indent + inline_len(children) + 2 <= INLINE_WIDTH
}

fn inline_len(children: &[Node]) -> usize {
    children
        .iter()
        .map(|child| match &child.kind {
            NodeKind::Atom(atom) => atom_len(atom),
            NodeKind::List(nested) => inline_len(nested) + nested.len().saturating_sub(1) + 2,
            NodeKind::Comment(comment) => comment.len() + 1,
        })
        .sum::<usize>()
        + children.len().saturating_sub(1)
}

fn atom_len(atom: &Atom) -> usize {
    match atom {
        Atom::Symbol(value) => value.len(),
        Atom::String(value) => escaped_string(value).len(),
        Atom::Int(value) => value.to_string().len(),
        Atom::Float(value) => float_text(*value).len(),
        Atom::Bool(true) => 4,
        Atom::Bool(false) => 5,
        Atom::Unit => 4,
    }
}

fn print_atom(atom: &Atom, output: &mut String) {
    match atom {
        Atom::Symbol(value) => output.push_str(value),
        Atom::String(value) => output.push_str(&escaped_string(value)),
        Atom::Int(value) => write!(output, "{value}").expect("write to String"),
        Atom::Float(value) => output.push_str(&float_text(*value)),
        Atom::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
        Atom::Unit => output.push_str("unit"),
    }
}

fn escaped_string(value: &str) -> String {
    let mut output = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            ch if ch.is_control() => {
                write!(output, "\\u{{{:x}}}", ch as u32).expect("write to String");
            }
            ch => output.push(ch),
        }
    }
    output.push('"');
    output
}

fn float_text(value: f64) -> String {
    let mut output = value.to_string();
    if !output.contains(['.', 'e', 'E']) {
        output.push_str(".0");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::parse;

    #[test]
    fn canonical_print_is_idempotent_and_preserves_values() {
        let source = "(say \"line\\nλ\" 1 2.0 true unit)\n; keep me\n";
        let once = print(&parse(source).unwrap());
        let twice = print(&parse(&once).unwrap());
        assert_eq!(once, twice);
        assert_eq!(once, "(say \"line\\nλ\" 1 2.0 true unit)\n; keep me\n");
    }

    #[test]
    fn comments_and_long_lists_force_deterministic_multiline_layout() {
        let source = "(module ; note\n (a very-long-symbol-name-that-makes-this-list-exceed-the-canonical-inline-width another-long-symbol-name))";
        assert_eq!(
            print(&parse(source).unwrap()),
            "(module\n  ; note\n  (a\n    very-long-symbol-name-that-makes-this-list-exceed-the-canonical-inline-width\n    another-long-symbol-name\n  )\n)\n"
        );
    }

    #[test]
    fn canonical_inline_limit_is_eighty_eight_columns() {
        let symbol = "a".repeat(80);
        let source = format!("(head {symbol})");
        assert_eq!(source.len(), 87);
        assert_eq!(print(&parse(&source).unwrap()), format!("{source}\n"));

        let over_limit = format!("(head {symbol} x)");
        assert_eq!(
            print(&parse(&over_limit).unwrap()),
            format!("(head\n  {symbol}\n  x\n)\n")
        );
    }

    #[test]
    fn printer_escapes_control_characters() {
        let printed = print(&parse("\"\\u{1f}\"").unwrap());
        assert_eq!(printed, "\"\\u{1f}\"\n");
        assert_eq!(print(&parse(&printed).unwrap()), printed);
    }
}
