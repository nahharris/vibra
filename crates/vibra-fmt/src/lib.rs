//! The canonical Vibra formatter.
//!
//! `docs/spec/01-source-language.md` defines exactly one canonical
//! representation. Formatting is idempotent and semantics-preserving: it may
//! normalize recoverable presentation but must never guess through a syntax,
//! binding, or type ambiguity.
//!
//! # Position in the architecture
//!
//! Depends on [`vibra_syntax`] and [`vibra_diagnostics`]. Nothing in the
//! language semantics may depend on this crate.
//!
//! # Status
//!
//! Milestone 1 step 4 supplies the syntax-only formatter. It canonicalizes
//! whitespace, delimiters, comments, line endings, and list layout while
//! leaving opaque leaf text untouched. Literal, name, declaration, and VIBON
//! schema rules arrive in later steps; see
//! `docs/roadmap/milestone-1/README.md`.
//!
//! A recovered document is returned byte-for-byte unchanged because applying
//! canonical whitespace to incomplete or opaque leaf text would be a guess.

use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use vibra_syntax::{CstNode, Document, DocumentModeError, SyntaxKind, parse_document};

/// An error selecting or formatting a document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormatError {
    /// The filename does not select `.vib` or `.vibon`.
    UnsupportedExtension(DocumentModeError),
}

impl fmt::Display for FormatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedExtension(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for FormatError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::UnsupportedExtension(error) => Some(error),
        }
    }
}

impl From<DocumentModeError> for FormatError {
    fn from(error: DocumentModeError) -> Self {
        Self::UnsupportedExtension(error)
    }
}

/// The syntax-only canonical formatter.
#[derive(Clone, Copy, Debug, Default)]
pub struct Formatter;

impl Formatter {
    /// Formats an already parsed document.
    #[must_use]
    pub fn format(self, document: &Document) -> String {
        format_document(document)
    }

    /// Selects the mode from `path`, parses, and formats the document.
    pub fn format_source(
        self,
        path: impl AsRef<Path>,
        source: &str,
    ) -> Result<String, FormatError> {
        format_source(path, source)
    }
}

/// Formats a document that has already been parsed by the shared reader.
#[must_use]
pub fn format_document(document: &Document) -> String {
    if document.recovered() {
        // A recovered tree contains an explicit error marker. In that state a
        // syntax-only formatter cannot safely choose a semantic layout, and
        // must not rewrite opaque or incomplete leaf text.
        return document.source().to_owned();
    }

    let Some(groups) = root_groups(document.root()) else {
        return normalize_recovery(&document.root().to_source());
    };
    if groups.is_empty() {
        return "\n".to_owned();
    }
    let layouts = build_layouts(document.root());
    let mut output = String::new();
    for (group_index, group) in groups.iter().enumerate() {
        if group_index != 0 {
            output.push_str("\n\n");
        }
        for (comment_index, comment) in group.comments.iter().enumerate() {
            if comment_index != 0 {
                output.push('\n');
            }
            output.push_str(&comment_text(comment));
        }
        if let Some(form) = group.form {
            if !group.comments.is_empty() {
                output.push('\n');
            }
            render_node(form, 0, &layouts, &mut output);
        }
    }
    output.push('\n');
    output
}

/// Selects a document mode from `path`, parses, and returns canonical text.
pub fn format_source(
    path: impl AsRef<Path>,
    source: &str,
) -> Result<String, FormatError> {
    let document = parse_document(path, source)?;
    Ok(format_document(&document))
}

/// Alias for [`format_source`] for callers that use the shorter operation name.
pub fn format(path: impl AsRef<Path>, source: &str) -> Result<String, FormatError> {
    format_source(path, source)
}

struct RootGroup<'source> {
    comments: Vec<&'source CstNode>,
    form: Option<&'source CstNode>,
}

fn root_groups(root: &CstNode) -> Option<Vec<RootGroup<'_>>> {
    let mut groups = Vec::new();
    let mut comments = Vec::new();
    for child in root.children() {
        match child.kind() {
            SyntaxKind::Whitespace => {}
            SyntaxKind::LineComment => comments.push(child),
            SyntaxKind::Atom | SyntaxKind::List => {
                groups.push(RootGroup {
                    comments: std::mem::take(&mut comments),
                    form: Some(child),
                });
            }
            SyntaxKind::Root
            | SyntaxKind::OpenParen
            | SyntaxKind::CloseParen
            | SyntaxKind::Error => {
                // `format_document` handles recovery trees above. Keeping
                // this branch makes the formatter conservative if a future
                // reader exposes a new root child kind.
                return None;
            }
        }
    }
    if !comments.is_empty() {
        groups.push(RootGroup {
            comments,
            form: None,
        });
    }
    Some(groups)
}

#[derive(Clone, Copy)]
struct NodeLayout {
    inline: bool,
    inline_width: usize,
}

fn node_key(node: &CstNode) -> *const CstNode {
    std::ptr::from_ref(node)
}

enum LayoutTask<'source> {
    Visit(&'source CstNode, usize, bool),
}

fn build_layouts(root: &CstNode) -> HashMap<*const CstNode, NodeLayout> {
    let mut layouts = HashMap::new();
    let mut tasks = Vec::new();
    for child in root.children().iter().rev() {
        if matches!(child.kind(), SyntaxKind::Atom | SyntaxKind::List) {
            tasks.push(LayoutTask::Visit(child, 0, false));
        }
    }

    while let Some(LayoutTask::Visit(node, indent, expanded)) = tasks.pop() {
        if !expanded {
            tasks.push(LayoutTask::Visit(node, indent, true));
            if node.kind() == SyntaxKind::List {
                for child in node.children().iter().rev() {
                    if matches!(child.kind(), SyntaxKind::Atom | SyntaxKind::List) {
                        tasks.push(LayoutTask::Visit(
                            child,
                            indent.saturating_add(2),
                            false,
                        ));
                    }
                }
            }
            continue;
        }

        let layout = match node.kind() {
            SyntaxKind::Atom => leaf_layout(node.leaf_text().unwrap_or_default()),
            SyntaxKind::List => list_layout(node, indent, &layouts),
            _ => NodeLayout {
                inline: false,
                inline_width: 0,
            },
        };
        layouts.insert(node_key(node), layout);
    }
    layouts
}

fn leaf_layout(text: &str) -> NodeLayout {
    let (has_newline, inline_width) = normalized_leaf_shape(text);
    NodeLayout {
        inline: !has_newline,
        inline_width,
    }
}

fn normalized_leaf_shape(text: &str) -> (bool, usize) {
    let mut has_newline = false;
    let mut inline_width: usize = 0;
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\r' {
            has_newline = true;
            if characters.peek() == Some(&'\n') {
                characters.next();
            }
        } else if character == '\n' {
            has_newline = true;
        }
        inline_width = inline_width.saturating_add(1);
    }
    (has_newline, inline_width)
}

fn list_layout(
    node: &CstNode,
    indent: usize,
    layouts: &HashMap<*const CstNode, NodeLayout>,
) -> NodeLayout {
    let mut has_comment = false;
    let mut all_inline = true;
    let mut item_count = 0;
    let mut inline_width: usize = 2;
    for child in node.children() {
        match child.kind() {
            SyntaxKind::Whitespace | SyntaxKind::OpenParen | SyntaxKind::CloseParen => {
            }
            SyntaxKind::LineComment => has_comment = true,
            SyntaxKind::Atom | SyntaxKind::List => {
                let child_layout =
                    layouts
                        .get(&node_key(child))
                        .copied()
                        .unwrap_or(NodeLayout {
                            inline: false,
                            inline_width: 0,
                        });
                if item_count != 0 {
                    inline_width = inline_width.saturating_add(1);
                }
                inline_width = inline_width.saturating_add(child_layout.inline_width);
                item_count += 1;
                all_inline &= child_layout.inline;
            }
            SyntaxKind::Root | SyntaxKind::Error => {}
        }
    }
    NodeLayout {
        inline: !has_comment && all_inline && indent.saturating_add(inline_width) <= 88,
        inline_width,
    }
}

enum LineComponent<'source> {
    Node(&'source CstNode),
    Comment(&'source CstNode),
}

enum RenderTask<'source> {
    Node(&'source CstNode, usize),
    Raw(&'static str),
    LineNode(&'source CstNode, usize),
    LineComment(&'source CstNode, usize),
    CloseList(usize),
}

fn render_node(
    node: &CstNode,
    indent: usize,
    layouts: &HashMap<*const CstNode, NodeLayout>,
    output: &mut String,
) {
    let mut tasks = vec![RenderTask::Node(node, indent)];
    while let Some(task) = tasks.pop() {
        match task {
            RenderTask::Node(node, indent) => match node.kind() {
                SyntaxKind::Atom => output
                    .push_str(&normalize_leaf(node.leaf_text().unwrap_or_default())),
                SyntaxKind::List => {
                    let layout =
                        layouts.get(&node_key(node)).copied().unwrap_or(NodeLayout {
                            inline: false,
                            inline_width: 0,
                        });
                    if layout.inline {
                        output.push('(');
                        let items = node
                            .children()
                            .iter()
                            .filter(|child| {
                                matches!(
                                    child.kind(),
                                    SyntaxKind::Atom | SyntaxKind::List
                                )
                            })
                            .collect::<Vec<_>>();
                        tasks.push(RenderTask::Raw(")"));
                        for (index, item) in items.into_iter().enumerate().rev() {
                            tasks
                                .push(RenderTask::Node(item, indent.saturating_add(2)));
                            if index != 0 {
                                tasks.push(RenderTask::Raw(" "));
                            }
                        }
                    } else {
                        output.push('(');
                        tasks.push(RenderTask::CloseList(indent));
                        for component in multiline_components(node).into_iter().rev() {
                            match component {
                                LineComponent::Node(child) => {
                                    tasks.push(RenderTask::LineNode(
                                        child,
                                        indent.saturating_add(2),
                                    ))
                                }
                                LineComponent::Comment(child) => {
                                    tasks.push(RenderTask::LineComment(
                                        child,
                                        indent.saturating_add(2),
                                    ))
                                }
                            }
                        }
                    }
                }
                SyntaxKind::LineComment => output.push_str(&comment_text(node)),
                SyntaxKind::Whitespace
                | SyntaxKind::Root
                | SyntaxKind::OpenParen
                | SyntaxKind::CloseParen
                | SyntaxKind::Error => {
                    output.push_str(&normalize_leaf(&node.to_source()))
                }
            },
            RenderTask::Raw(text) => output.push_str(text),
            RenderTask::LineNode(node, indent) => {
                output.push('\n');
                output.push_str(&" ".repeat(indent));
                tasks.push(RenderTask::Node(node, indent));
            }
            RenderTask::LineComment(node, indent) => {
                output.push('\n');
                output.push_str(&" ".repeat(indent));
                output.push_str(&comment_text(node));
            }
            RenderTask::CloseList(indent) => {
                output.push('\n');
                output.push_str(&" ".repeat(indent));
                output.push(')');
            }
        }
    }
}

fn multiline_components(node: &CstNode) -> Vec<LineComponent<'_>> {
    let mut components = Vec::new();
    let mut comments = Vec::new();
    for child in node.children() {
        match child.kind() {
            SyntaxKind::Whitespace | SyntaxKind::OpenParen | SyntaxKind::CloseParen => {
            }
            SyntaxKind::LineComment => comments.push(child),
            SyntaxKind::Atom | SyntaxKind::List => {
                components.extend(comments.drain(..).map(LineComponent::Comment));
                components.push(LineComponent::Node(child));
            }
            SyntaxKind::Root | SyntaxKind::Error => {}
        }
    }
    components.extend(comments.into_iter().map(LineComponent::Comment));
    components
}

fn comment_text(node: &CstNode) -> String {
    normalize_leaf(node.leaf_text().unwrap_or_default())
        .trim_end()
        .to_owned()
}

fn normalize_leaf(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn normalize_recovery(source: &str) -> String {
    let normalized = source.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines = normalized
        .split('\n')
        .map(str::trim_end)
        .collect::<Vec<_>>();
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    if lines.is_empty() {
        return "\n".to_owned();
    }
    let mut result = lines.join("\n");
    result.push('\n');
    result
}
