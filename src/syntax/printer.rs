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

fn symbol(node: &Node) -> Option<&str> {
    match &node.kind {
        NodeKind::Atom(Atom::Symbol(value)) => Some(value),
        _ => None,
    }
}

fn print_node(node: &Node, indent: usize, output: &mut String) {
    match &node.kind {
        NodeKind::Atom(atom) => print_atom(atom, output),
        NodeKind::Comment(comment) => {
            output.push(';');
            output.push_str(comment);
        }
        NodeKind::List(children) if is_layout_form(children) => {
            print_multiline_list(children, indent, output);
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
        NodeKind::List(children) => print_multiline_list(children, indent, output),
    }
}

fn print_multiline_list(children: &[Node], indent: usize, output: &mut String) {
    match children.first().and_then(symbol) {
        Some("defn") => print_defn(children, indent, output),
        Some("match") => print_match(children, indent, output),
        Some("if") => print_if(children, indent, output),
        Some("do") => print_do(children, indent, output),
        _ => print_generic_multiline(children, indent, output),
    }
}

fn is_layout_form(children: &[Node]) -> bool {
    matches!(
        children.first().and_then(symbol),
        Some("defn" | "match" | "if" | "do")
    )
}

fn print_defn(children: &[Node], indent: usize, output: &mut String) {
    let (header, tail) = semantic_prefix(children, 4);
    print_form_header(&header, indent, output);
    let units = trailing_units(tail);
    print_units(&units, indent, output);
    close_attached(output, indent, children.last());
}

fn print_match(children: &[Node], indent: usize, output: &mut String) {
    let (header, tail) = semantic_prefix(children, 2);
    print_form_header(&header, indent, output);
    let mut units = Vec::new();
    let mut index = 0;
    while index < tail.len() {
        if is_comment(&tail[index]) {
            units.push(vec![&tail[index]]);
            index += 1;
            continue;
        }
        let pattern = &tail[index];
        if let Some(body) = tail.get(index + 1).filter(|node| !is_comment(node)) {
            units.push(vec![pattern, body]);
            index += 2;
        } else {
            units.push(vec![pattern]);
            index += 1;
        }
    }
    for unit in units {
        output.push('\n');
        output.push_str(&" ".repeat(indent + INDENT));
        if unit.len() == 2 {
            print_arm(&unit, indent + INDENT, output);
        } else {
            print_node(unit[0], indent + INDENT, output);
        }
    }
    close_attached(output, indent, children.last());
}

fn print_if(children: &[Node], indent: usize, output: &mut String) {
    let (header, tail) = semantic_prefix(children, 2);
    print_form_header(&header, indent, output);
    for branch in tail {
        output.push('\n');
        output.push_str(&" ".repeat(indent + INDENT));
        print_node(branch, indent + INDENT, output);
    }
    close_attached(output, indent, children.last());
}

fn print_do(children: &[Node], indent: usize, output: &mut String) {
    output.push('(');
    if let Some(head) = children.first() {
        print_node(head, indent, output);
    }
    for child in children.iter().skip(1) {
        output.push('\n');
        output.push_str(&" ".repeat(indent + INDENT));
        print_node(child, indent + INDENT, output);
    }
    close_attached(output, indent, children.last());
}

fn print_arm(unit: &[&Node], indent: usize, output: &mut String) {
    let pattern = unit[0];
    let body = unit[1];
    let do_opener_fits = matches!(&body.kind, NodeKind::List(children) if children.first().and_then(symbol) == Some("do"))
        && can_inline_node(pattern, indent)
        && indent + node_inline_len(pattern, indent) + 1 + 3 <= INLINE_WIDTH;
    if is_do_form(body) && do_opener_fits {
        print_node(pattern, indent, output);
        output.push(' ');
        print_node(body, indent, output);
    } else if node_inline_len(pattern, indent) + node_inline_len(body, indent) + 1
        <= INLINE_WIDTH.saturating_sub(indent)
        && can_inline_node(pattern, indent)
        && can_inline_node(body, indent)
    {
        print_node(pattern, indent, output);
        output.push(' ');
        print_node(body, indent, output);
    } else {
        print_node(pattern, indent, output);
        output.push('\n');
        let body_indent = indent + INDENT;
        output.push_str(&" ".repeat(body_indent));
        print_node(body, body_indent, output);
    }
}

fn print_form_header(header: &[&Node], indent: usize, output: &mut String) {
    output.push('(');
    if !header.is_empty() && header_can_inline(&header, indent) {
        for (index, child) in header.iter().enumerate() {
            if index > 0 {
                output.push(' ');
            }
            print_node(child, indent + INDENT, output);
        }
    } else {
        for child in header {
            if !header.is_empty() {
                output.push('\n');
                output.push_str(&" ".repeat(indent + INDENT));
            }
            print_node(child, indent + INDENT, output);
        }
    }
}

/// Return the first semantic operands (including comments that occur among
/// them) and the remaining physical CST children. Comments are trivia for
/// operand partitioning but remain in the rendered sequence.
fn semantic_prefix<'a>(children: &'a [Node], semantic_count: usize) -> (Vec<&'a Node>, &'a [Node]) {
    let mut seen = 0;
    let mut boundary = children.len();
    for (index, child) in children.iter().enumerate() {
        if !is_comment(child) {
            seen += 1;
            if seen == semantic_count {
                boundary = index + 1;
                break;
            }
        }
    }
    (children[..boundary].iter().collect(), &children[boundary..])
}

fn header_can_inline(header: &[&Node], indent: usize) -> bool {
    !header.iter().any(|node| is_comment(node))
        && indent + inline_len_refs(header) + 2 <= INLINE_WIDTH
        && header
            .iter()
            .all(|node| can_inline_node(node, indent + INDENT))
}

fn trailing_units<'a>(children: &'a [Node]) -> Vec<Vec<&'a Node>> {
    let mut units = Vec::new();
    let mut index = 0;
    while index < children.len() {
        if is_comment(&children[index]) {
            units.push(vec![&children[index]]);
            index += 1;
        } else if matches!(children[index].kind, NodeKind::Atom(Atom::Label(_)))
            && children
                .get(index + 1)
                .is_some_and(|node| !is_comment(node))
        {
            units.push(vec![&children[index], &children[index + 1]]);
            index += 2;
        } else {
            units.push(vec![&children[index]]);
            index += 1;
        }
    }
    units
}

fn print_units(units: &[Vec<&Node>], indent: usize, output: &mut String) {
    for unit in units {
        output.push('\n');
        output.push_str(&" ".repeat(indent + INDENT));
        for (index, child) in unit.iter().enumerate() {
            if index > 0 {
                output.push(' ');
            }
            print_node(child, indent + INDENT, output);
        }
    }
}

fn print_generic_multiline(children: &[Node], indent: usize, output: &mut String) {
    if can_inline(children, indent) {
        output.push('(');
        for (index, child) in children.iter().enumerate() {
            if index > 0 {
                output.push(' ');
            }
            print_node(child, indent, output);
        }
        output.push(')');
        return;
    }

    output.push('(');
    let (head, tail) = match children.split_first() {
        Some((head, tail)) if matches!(head.kind, NodeKind::Atom(_)) => (Some(head), tail),
        _ => (None, children),
    };
    if let Some(head) = head {
        print_node(head, indent, output);
    }
    for unit in trailing_units(tail) {
        output.push('\n');
        output.push_str(&" ".repeat(indent + INDENT));
        for (index, child) in unit.iter().enumerate() {
            if index > 0 {
                output.push(' ');
            }
            print_node(child, indent + INDENT, output);
        }
    }
    if children.last().is_some_and(is_comment) {
        output.push('\n');
        output.push_str(&" ".repeat(indent));
    }
    output.push(')');
}

fn close_attached(output: &mut String, indent: usize, last_child: Option<&Node>) {
    // A closing delimiter cannot share a line with a line comment because it
    // would become part of the comment. Keep it attached to the rendered
    // child whenever that child is not a comment.
    if last_child.is_some_and(is_comment) {
        output.push('\n');
        output.push_str(&" ".repeat(indent));
    }
    output.push(')');
}

fn is_comment(node: &Node) -> bool {
    matches!(node.kind, NodeKind::Comment(_))
}

fn is_multiline_do(node: &Node) -> bool {
    matches!(&node.kind, NodeKind::List(children) if children.first().and_then(symbol) == Some("do") && children.len() > 2)
}

fn is_do_form(node: &Node) -> bool {
    matches!(&node.kind, NodeKind::List(children) if children.first().and_then(symbol) == Some("do"))
}

fn can_inline(children: &[Node], indent: usize) -> bool {
    !(children.first().and_then(symbol) == Some("do") && children.len() > 2)
        && children
            .iter()
            .all(|child| can_inline_node(child, indent + INDENT))
        && indent + inline_len(children) + 2 <= INLINE_WIDTH
}

fn can_inline_node(node: &Node, indent: usize) -> bool {
    match &node.kind {
        NodeKind::Comment(_) => false,
        NodeKind::Atom(_) => true,
        NodeKind::List(children) => {
            !is_layout_form(children)
                && !is_multiline_do(node)
                && can_inline(children, indent)
                && indent + inline_len(children) + 2 <= INLINE_WIDTH
        }
    }
}

fn node_inline_len(node: &Node, _indent: usize) -> usize {
    match &node.kind {
        NodeKind::Atom(atom) => atom_len(atom),
        NodeKind::Comment(comment) => comment.len() + 1,
        NodeKind::List(children) => inline_len(children) + 2,
    }
}

fn inline_len_refs(nodes: &[&Node]) -> usize {
    nodes
        .iter()
        .map(|node| node_inline_len(node, 0))
        .sum::<usize>()
        + nodes.len().saturating_sub(1)
}

fn inline_len(children: &[Node]) -> usize {
    children
        .iter()
        .map(|child| match &child.kind {
            NodeKind::Atom(atom) => atom_len(atom),
            NodeKind::List(nested) => inline_len(nested) + 2,
            NodeKind::Comment(comment) => comment.len() + 1,
        })
        .sum::<usize>()
        + children.len().saturating_sub(1)
}

fn atom_len(atom: &Atom) -> usize {
    match atom {
        Atom::Symbol(value) => value.len(),
        Atom::Label(value) => value.len() + 1,
        Atom::Atom(value) => value.len() + 1,
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
        Atom::Label(value) => {
            output.push_str(value);
            output.push(':');
        }
        Atom::Atom(value) => {
            output.push('@');
            output.push_str(value);
        }
        Atom::String(value) => output.push_str(&escaped_string(value)),
        Atom::Int(value) => write!(output, "{value}").expect("write to String"),
        Atom::Float(value) => output.push_str(&float_text(*value)),
        Atom::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
        Atom::Unit => output.push_str("unit"),
    }
}

#[cfg(test)]
mod atom_tests {
    use super::*;
    use crate::syntax::parse;

    #[test]
    fn prints_atom_literals_canonically() {
        assert_eq!(
            print(&parse("(@ok @http.not-found)").unwrap()),
            "(@ok @http.not-found)\n"
        );
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
        let source = "(def answer 42 doc: \"line\\nλ\")\n; keep me\n";
        let once = print(&parse(source).unwrap());
        let twice = print(&parse(&once).unwrap());
        assert_eq!(once, twice);
        assert_eq!(once, "(def answer 42 doc: \"line\\nλ\")\n; keep me\n");
    }

    #[test]
    fn comments_and_long_lists_force_deterministic_multiline_layout() {
        let source = "(module ; note\n (a very-long-symbol-name-that-makes-this-list-exceed-the-canonical-inline-width another-long-symbol-name))";
        assert_eq!(
            print(&parse(source).unwrap()),
            "(module\n  ; note\n  (a\n    very-long-symbol-name-that-makes-this-list-exceed-the-canonical-inline-width\n    another-long-symbol-name))\n"
        );
    }

    #[test]
    fn multiline_attributes_keep_each_label_with_its_value() {
        let source = "(def option (enum (some type-with-a-very-long-name-that-forces-multiline-layout) (none void)) where: (type-with-a-very-long-name-that-forces-multiline-layout any) doc: \"Maybe\")";
        let printed = print(&parse(source).unwrap());
        assert!(printed.contains("\n  where: ("));
        assert!(printed.contains("\n  doc: \"Maybe\")"));
        assert_eq!(print(&parse(&printed).unwrap()), printed);
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
            format!("(head\n  {symbol}\n  x)\n")
        );
    }

    #[test]
    fn short_generic_lists_remain_inline() {
        assert_eq!(
            print(&parse("(list first second)").unwrap()),
            "(list first second)\n"
        );
    }

    #[test]
    fn layout_forms_keep_bodies_on_their_policy_lines_even_when_short() {
        assert_eq!(
            print(&parse("(defn f () void (cmd))").unwrap()),
            "(defn f () void\n  (cmd))\n"
        );
        assert_eq!(print(&parse("(do (cmd))").unwrap()), "(do\n  (cmd))\n");
    }

    #[test]
    fn printer_escapes_control_characters() {
        let printed = print(&parse("\"\\u{1f}\"").unwrap());
        assert_eq!(printed, "\"\\u{1f}\"\n");
        assert_eq!(print(&parse(&printed).unwrap()), printed);
    }

    #[test]
    fn form_aware_layout_groups_defn_match_if_and_do() {
        let source = "(defn name ((arg int64)) return-type doc: \"Some doc comment\" (cmd1) (cmd2) (return (very-long-result-name value)))\n\
(match value (pattern) (branch) (other-pattern) (do (cmd1) (cmd2)))\n\
(if condition (branch) (do (cmd1) (cmd2)))";
        assert_eq!(
            print(&parse(source).unwrap()),
            "(defn name ((arg int64)) return-type\n  doc: \"Some doc comment\"\n  (cmd1)\n  (cmd2)\n  (return (very-long-result-name value)))\n(match value\n  (pattern) (branch)\n  (other-pattern) (do\n    (cmd1)\n    (cmd2)))\n(if condition\n  (branch)\n  (do\n    (cmd1)\n    (cmd2)))\n"
        );
    }

    #[test]
    fn long_match_arm_hangs_branch_beneath_pattern() {
        let source = "(match value (pattern-with-a-name-that-is-long-enough-to-force-the-arm-past-the-width-limit-now) (do (cmd)))";
        let printed = print(&parse(source).unwrap());
        assert!(printed.contains(
            "  (pattern-with-a-name-that-is-long-enough-to-force-the-arm-past-the-width-limit-now)\n    (do"
        ));
        assert_eq!(print(&parse(&printed).unwrap()), printed);
    }

    #[test]
    fn printer_preserves_node_order_and_only_reformats_whitespace() {
        let source = "(defn run () void (do (step-one) (step-two)) visibility: @private)";
        let before = parse(source).unwrap();
        let printed = print(&before);
        let after = parse(&printed).unwrap();
        assert_eq!(node_shape(&before), node_shape(&after));
    }

    fn node_shape(document: &Document) -> Vec<String> {
        fn visit(node: &Node, output: &mut Vec<String>) {
            match &node.kind {
                NodeKind::Atom(atom) => output.push(format!("atom:{atom:?}")),
                NodeKind::Comment(comment) => output.push(format!("comment:{comment}")),
                NodeKind::List(children) => {
                    output.push("[".to_string());
                    for child in children {
                        visit(child, output);
                    }
                    output.push("]".to_string());
                }
            }
        }

        let mut output = Vec::new();
        for node in &document.nodes {
            visit(node, &mut output);
        }
        output
    }
}
