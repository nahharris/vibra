//! Structural source-position facts over the lossless reader tree.
//!
//! The query deliberately consumes the existing CST rather than reparsing
//! source text. It reports grammar slots and continuation vocabulary only;
//! name resolution, typing, and semantic identities belong to later phases.

use std::fmt;

use vibra_diagnostics::ByteSpan;

use crate::name::{NameClassification, NameKind};
use crate::reader::{CstNode, Document, DocumentMode, SyntaxKind};

const TOP_LEVEL_FORMS: &[&str] = &[
    "import", "deftype", "defint", "deffect", "def", "defn", "test",
];
const EXPRESSION_FORMS: &[&str] = &["lambda", "do", "let", "if", "match", "as", "try"];
const TYPE_FORMS: &[&str] = &["tuple", "array", "map", "fn"];
const PATTERN_FORMS: &[&str] = &["tuple", "array", "as"];
const DATA_FORMS: &[&str] = &["record", "array", "tuple", "map"];
const TYPE_ATTRIBUTE_LABELS: &[&str] = &["where", "visibility", "doc"];
const DECLARATION_ATTRIBUTE_LABELS: &[&str] = &["visibility", "doc"];
const FUNCTION_ATTRIBUTE_LABELS: &[&str] = &[
    "where",
    "labelled",
    "variadic",
    "visibility",
    "effects",
    "external",
    "symbol",
    "doc",
];
const LAMBDA_ATTRIBUTE_LABELS: &[&str] = &["labelled", "variadic", "effects"];

/// The availability of structural facts at a queried position.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FactStatus {
    /// The selected node and its grammar slot are complete enough for the
    /// reported structural facts.
    Exact,
    /// The selected node is a recovery marker inserted by the reader.
    Recovered,
    /// The position is known, but the reader cannot provide the requested
    /// structural facts for it (for example, trivia).
    Unavailable,
}

impl FactStatus {
    /// The stable wire spelling for this status.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Recovered => "recovered",
            Self::Unavailable => "unavailable",
        }
    }
}

/// The grammar category owning a queried CST node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GrammarCategory {
    /// A source module or the top-level continuation position.
    Module,
    /// A native declaration or declaration child.
    Declaration,
    /// A type expression or type child.
    Type,
    /// A pattern or pattern child.
    Pattern,
    /// An expression or expression child.
    Expression,
    /// A declaration, function, or lambda attribute.
    DeclarationAttribute,
    /// An effect-row value or effect reference.
    EffectRow,
    /// A source type field or VIBON record/data slot.
    DataField,
    /// Whitespace or a line comment.
    Trivia,
    /// A recovery marker.
    Recovery,
}

impl GrammarCategory {
    /// The stable kebab-case wire spelling for this category.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Module => "module",
            Self::Declaration => "declaration",
            Self::Type => "type",
            Self::Pattern => "pattern",
            Self::Expression => "expression",
            Self::DeclarationAttribute => "declaration-attribute",
            Self::EffectRow => "effect-row",
            Self::DataField => "data-field",
            Self::Trivia => "trivia",
            Self::Recovery => "recovery",
        }
    }
}

/// The result of one structural source-position query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructuralQuery {
    offset: usize,
    span: ByteSpan,
    mode: DocumentMode,
    syntax_kind: SyntaxKind,
    category: GrammarCategory,
    status: FactStatus,
    permitted_forms: Option<Vec<String>>,
    permitted_labels: Option<Vec<String>>,
}

impl StructuralQuery {
    /// The queried UTF-8 byte offset.
    #[must_use]
    pub const fn offset(&self) -> usize {
        self.offset
    }

    /// The half-open span selected by the query.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }

    /// The document mode used for the query.
    #[must_use]
    pub const fn mode(&self) -> DocumentMode {
        self.mode
    }

    /// The selected lossless CST kind.
    #[must_use]
    pub const fn syntax_kind(&self) -> SyntaxKind {
        self.syntax_kind
    }

    /// The grammar category owning the selected node.
    #[must_use]
    pub const fn category(&self) -> GrammarCategory {
        self.category
    }

    /// Whether the reported facts are exact, recovered, or unavailable.
    #[must_use]
    pub const fn status(&self) -> FactStatus {
        self.status
    }

    /// Permitted child forms, when the grammar slot is known.
    ///
    /// `Some(&[])` means the slot is known and has no named forms. `None`
    /// means the reader cannot provide this fact.
    #[must_use]
    pub fn permitted_forms(&self) -> Option<&[String]> {
        self.permitted_forms.as_deref()
    }

    /// Permitted labels, when the grammar slot is known.
    ///
    /// `Some(&[])` means the slot is known and has no labels. `None` means the
    /// reader cannot provide this fact.
    #[must_use]
    pub fn permitted_labels(&self) -> Option<&[String]> {
        self.permitted_labels.as_deref()
    }
}

/// A rejected source-position query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueryError {
    /// The offset is beyond the document's byte length.
    OffsetOutOfBounds {
        /// The requested byte offset.
        offset: usize,
        /// The document byte length.
        length: usize,
    },
    /// The offset points into the middle of a UTF-8 scalar.
    InteriorUtf8Offset {
        /// The requested byte offset.
        offset: usize,
    },
    /// No CST node could be selected. A normal parsed document always has a
    /// root, so this is reserved for future document implementations.
    NoNode {
        /// The requested byte offset.
        offset: usize,
    },
}

impl fmt::Display for QueryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OffsetOutOfBounds { offset, length } => {
                write!(
                    formatter,
                    "query offset {offset} is beyond document length {length}"
                )
            }
            Self::InteriorUtf8Offset { offset } => {
                write!(formatter, "query offset {offset} is inside a UTF-8 scalar")
            }
            Self::NoNode { offset } => {
                write!(formatter, "no syntax node contains offset {offset}")
            }
        }
    }
}

impl std::error::Error for QueryError {}

/// Queries the smallest deterministic CST node containing `offset`.
///
/// Offsets are UTF-8 byte offsets and may point at EOF, but must be character
/// boundaries. At EOF, non-empty nodes ending at EOF are considered to contain
/// the caret. A zero-width recovery marker at the caret wins over every
/// non-empty node; otherwise the shortest containing node wins, with depth and
/// source order providing deterministic ties.
pub fn query_position(
    document: &Document,
    offset: usize,
) -> Result<StructuralQuery, QueryError> {
    let source = document.source();
    if offset > source.len() {
        return Err(QueryError::OffsetOutOfBounds {
            offset,
            length: source.len(),
        });
    }
    if !source.is_char_boundary(offset) {
        return Err(QueryError::InteriorUtf8Offset { offset });
    }

    let root = document.root();
    let mut stack = vec![TraversalFrame {
        node: root,
        next_child: 0,
        entered: false,
    }];
    let mut path = Vec::new();
    let mut order = 0usize;
    let mut best: Option<Candidate<'_>> = None;

    while !stack.is_empty() {
        let needs_enter = stack.last().is_some_and(|frame| !frame.entered);
        if needs_enter {
            let Some(node) = stack.last().map(|frame| frame.node) else {
                break;
            };
            if let Some(frame) = stack.last_mut() {
                frame.entered = true;
            }
            path.push(node);
            let span = node.span();
            let is_recovery = node.kind() == SyntaxKind::Error
                && span.is_empty()
                && span.start() == offset;
            let is_containing = !span.is_empty()
                && span.start() <= offset
                && (offset < span.end()
                    || (offset == source.len() && span.end() == source.len()));
            if is_recovery || is_containing {
                let candidate = Candidate {
                    node,
                    path: path.clone(),
                    depth: path.len(),
                    order,
                    is_recovery,
                };
                if best
                    .as_ref()
                    .is_none_or(|current| candidate_is_better(&candidate, current))
                {
                    best = Some(candidate);
                }
            }
            order = order.saturating_add(1);
        }

        let child = {
            let Some(frame) = stack.last_mut() else {
                break;
            };
            let child = frame.node.children().get(frame.next_child);
            if child.is_some() {
                frame.next_child = frame.next_child.saturating_add(1);
            }
            child
        };
        if let Some(child) = child {
            stack.push(TraversalFrame {
                node: child,
                next_child: 0,
                entered: false,
            });
        } else {
            stack.pop();
            path.pop();
        }
    }

    let Some(candidate) = best else {
        // The root is zero-width only for an empty document. Keep this
        // fallback explicit so a future CST implementation cannot silently
        // turn a missing selection into a fabricated fact.
        if root.span().is_empty() && offset == 0 {
            return Ok(render_query(document, root, &[root], offset));
        }
        return Err(QueryError::NoNode { offset });
    };

    Ok(render_query(
        document,
        candidate.node,
        &candidate.path,
        offset,
    ))
}

struct TraversalFrame<'source> {
    node: &'source CstNode,
    next_child: usize,
    entered: bool,
}

struct Candidate<'source> {
    node: &'source CstNode,
    path: Vec<&'source CstNode>,
    depth: usize,
    order: usize,
    is_recovery: bool,
}

fn candidate_is_better(left: &Candidate<'_>, right: &Candidate<'_>) -> bool {
    if left.is_recovery != right.is_recovery {
        return left.is_recovery;
    }
    if left.node.span().len() != right.node.span().len() {
        return left.node.span().len() < right.node.span().len();
    }
    if left.depth != right.depth {
        return left.depth > right.depth;
    }
    left.order < right.order
}

fn render_query(
    document: &Document,
    node: &CstNode,
    path: &[&CstNode],
    offset: usize,
) -> StructuralQuery {
    let (category, status, permitted_forms, permitted_labels) =
        if node.kind() == SyntaxKind::Error {
            (GrammarCategory::Recovery, FactStatus::Recovered, None, None)
        } else if matches!(
            node.kind(),
            SyntaxKind::Whitespace | SyntaxKind::LineComment
        ) {
            (GrammarCategory::Trivia, FactStatus::Unavailable, None, None)
        } else if document.mode() == DocumentMode::Data {
            render_data_facts(node, path)
        } else {
            render_source_facts(path)
        };

    StructuralQuery {
        offset,
        span: node.span(),
        mode: document.mode(),
        syntax_kind: node.kind(),
        category,
        status,
        permitted_forms,
        permitted_labels,
    }
}

fn render_data_facts(
    node: &CstNode,
    path: &[&CstNode],
) -> (
    GrammarCategory,
    FactStatus,
    Option<Vec<String>>,
    Option<Vec<String>>,
) {
    let forms = if node.kind() == SyntaxKind::Root
        || node.kind() == SyntaxKind::List
        || node
            .leaf_text()
            .is_some_and(|text| DATA_FORMS.contains(&text))
    {
        Some(to_owned(DATA_FORMS))
    } else {
        Some(Vec::new())
    };
    let labels = Some(Vec::new());
    let status = if path
        .iter()
        .any(|ancestor| ancestor.kind() == SyntaxKind::Error)
    {
        FactStatus::Recovered
    } else {
        FactStatus::Exact
    };
    (GrammarCategory::DataField, status, forms, labels)
}

fn render_source_facts(
    path: &[&CstNode],
) -> (
    GrammarCategory,
    FactStatus,
    Option<Vec<String>>,
    Option<Vec<String>>,
) {
    let context = path_context(path);
    let category = context.category();
    let (forms, labels) = permitted_for(context, path);
    let status = if matches!(context, Context::Unknown) {
        FactStatus::Unavailable
    } else {
        FactStatus::Exact
    };
    (category, status, forms, labels)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Context {
    Module,
    Declaration,
    Type,
    Pattern,
    Expression,
    DeclarationAttribute,
    EffectRow,
    DataField,
    ParameterList,
    TypeArgumentList,
    LabelledList,
    WhereList,
    Unknown,
}

impl Context {
    fn category(self) -> GrammarCategory {
        match self {
            Self::Module => GrammarCategory::Module,
            Self::Declaration => GrammarCategory::Declaration,
            Self::Type => GrammarCategory::Type,
            Self::Pattern | Self::ParameterList => GrammarCategory::Pattern,
            Self::Expression => GrammarCategory::Expression,
            Self::DeclarationAttribute | Self::LabelledList | Self::WhereList => {
                GrammarCategory::DeclarationAttribute
            }
            Self::EffectRow => GrammarCategory::EffectRow,
            Self::DataField => GrammarCategory::DataField,
            Self::TypeArgumentList => GrammarCategory::Type,
            Self::Unknown => GrammarCategory::Expression,
        }
    }
}

fn path_context(path: &[&CstNode]) -> Context {
    let Some(root) = path.first().copied() else {
        return Context::Unknown;
    };
    if root.kind() != SyntaxKind::Root {
        return Context::Unknown;
    }
    let mut context = Context::Module;
    for pair in path.windows(2) {
        let Some(parent) = pair.first().copied() else {
            continue;
        };
        let Some(child) = pair.get(1).copied() else {
            continue;
        };
        let Some(index) = meaningful_index(parent, child) else {
            continue;
        };
        context = child_context(parent, child, context, index);
    }
    context
}

fn child_context(
    parent: &CstNode,
    child: &CstNode,
    parent_context: Context,
    index: usize,
) -> Context {
    if parent.kind() == SyntaxKind::Root {
        if child.kind() == SyntaxKind::List && is_declaration_head(head_text(child)) {
            return Context::Declaration;
        }
        return Context::Expression;
    }
    if parent_context == Context::EffectRow {
        return Context::EffectRow;
    }
    if parent_context == Context::DataField {
        return Context::DataField;
    }
    if matches!(
        parent_context,
        Context::ParameterList
            | Context::TypeArgumentList
            | Context::LabelledList
            | Context::WhereList
    ) {
        return match parent_context {
            Context::ParameterList => {
                if index.is_multiple_of(2) {
                    Context::Pattern
                } else {
                    Context::Type
                }
            }
            Context::TypeArgumentList => Context::Type,
            Context::LabelledList => match index % 3 {
                1 => Context::Type,
                2 => Context::Expression,
                _ => Context::DeclarationAttribute,
            },
            Context::WhereList => Context::DeclarationAttribute,
            _ => parent_context,
        };
    }

    let head = head_text(parent);
    if head.is_none() {
        return match parent_context {
            Context::ParameterList => {
                if index.is_multiple_of(2) {
                    Context::Pattern
                } else {
                    Context::Type
                }
            }
            Context::TypeArgumentList => Context::Type,
            Context::LabelledList => match index % 3 {
                1 => Context::Type,
                2 => Context::Expression,
                _ => Context::DeclarationAttribute,
            },
            Context::WhereList => Context::DeclarationAttribute,
            _ => parent_context,
        };
    }
    let head = head.unwrap_or_default();

    if index == 0 {
        return parent_context;
    }

    match parent_context {
        Context::Declaration => declaration_child_context(parent, head, child, index),
        Context::Type => type_child_context(parent, head, child, index),
        Context::Pattern => pattern_child_context(parent, head, child, index),
        Context::Expression => expression_child_context(parent, head, child, index),
        Context::DeclarationAttribute => {
            attribute_child_context(parent, head, child, index)
        }
        Context::ParameterList
        | Context::TypeArgumentList
        | Context::LabelledList
        | Context::WhereList
        | Context::Module
        | Context::EffectRow
        | Context::DataField
        | Context::Unknown => parent_context,
    }
}

fn declaration_child_context(
    parent: &CstNode,
    head: &str,
    child: &CstNode,
    index: usize,
) -> Context {
    match head {
        "import" => Context::Declaration,
        "deftype" => {
            if index == 2 {
                Context::Type
            } else if index >= 3 {
                declaration_tail_context(
                    parent,
                    child,
                    index,
                    TYPE_ATTRIBUTE_LABELS,
                    true,
                )
            } else {
                Context::Declaration
            }
        }
        "defint" => {
            if index >= 2 {
                declaration_tail_context(
                    parent,
                    child,
                    index,
                    TYPE_ATTRIBUTE_LABELS,
                    true,
                )
            } else {
                Context::Declaration
            }
        }
        "deffect" => {
            if index >= 2 {
                declaration_tail_context(
                    parent,
                    child,
                    index,
                    DECLARATION_ATTRIBUTE_LABELS,
                    true,
                )
            } else {
                Context::Declaration
            }
        }
        "def" => match index {
            2 => Context::Type,
            3 => Context::Expression,
            _ if index >= 4 => declaration_tail_context(
                parent,
                child,
                index,
                DECLARATION_ATTRIBUTE_LABELS,
                false,
            ),
            _ => Context::Declaration,
        },
        "defn" => match index {
            2 => Context::ParameterList,
            3 => Context::Type,
            _ if index >= 4 => declaration_tail_context(
                parent,
                child,
                index,
                FUNCTION_ATTRIBUTE_LABELS,
                false,
            ),
            _ => Context::Declaration,
        },
        "test" => {
            if index >= 2 && is_label(child, "effects") {
                Context::DeclarationAttribute
            } else if index >= 3
                && previous_label(parent, index).is_some_and(|label| label == "effects")
            {
                Context::EffectRow
            } else if index >= 2
                && (is_any_label(child) || previous_label(parent, index).is_some())
            {
                Context::DeclarationAttribute
            } else {
                Context::Expression
            }
        }
        "impl" => {
            if index == 1 {
                Context::Type
            } else {
                Context::Declaration
            }
        }
        _ => Context::Declaration,
    }
}

fn declaration_tail_context(
    parent: &CstNode,
    child: &CstNode,
    index: usize,
    allowed: &[&str],
    members_are_declarations: bool,
) -> Context {
    if is_label(child, "effects") || is_label_in(child, allowed) {
        return Context::DeclarationAttribute;
    }
    if let Some(label) = previous_label(parent, index) {
        return attribute_value_context(label);
    }
    if is_any_label(child) {
        return Context::DeclarationAttribute;
    }
    if members_are_declarations
        && child.kind() == SyntaxKind::List
        && matches!(head_text(child), Some("defn" | "impl"))
    {
        return Context::Declaration;
    }
    if index > 0 && !members_are_declarations {
        return Context::Expression;
    }
    Context::DeclarationAttribute
}

fn type_child_context(
    parent: &CstNode,
    head: &str,
    child: &CstNode,
    index: usize,
) -> Context {
    if let Some(label) = previous_label(parent, index) {
        return attribute_value_context(label);
    }
    if is_any_label(child) {
        return Context::DeclarationAttribute;
    }
    match head {
        "record" | "enum" => {
            if index.is_multiple_of(2) {
                Context::Type
            } else {
                Context::DataField
            }
        }
        "tuple" | "union" => Context::Type,
        "array" | "newtype" => Context::Type,
        "map" => Context::Type,
        "fn" => match index {
            1 => Context::TypeArgumentList,
            2 => Context::Type,
            _ => Context::DeclarationAttribute,
        },
        _ => Context::Type,
    }
}

fn pattern_child_context(
    parent: &CstNode,
    head: &str,
    child: &CstNode,
    index: usize,
) -> Context {
    if let Some(label) = previous_label(parent, index) {
        return if label == "types" {
            Context::TypeArgumentList
        } else {
            Context::Pattern
        };
    }
    if is_label(child, "types") {
        return Context::DeclarationAttribute;
    }
    match head {
        "as" if index == 1 => Context::Type,
        "as" => Context::Pattern,
        _ => Context::Pattern,
    }
}

fn expression_child_context(
    parent: &CstNode,
    head: &str,
    child: &CstNode,
    index: usize,
) -> Context {
    match head {
        "lambda" => match index {
            1 => Context::ParameterList,
            2 => Context::Type,
            _ => lambda_tail_context(parent, child, index),
        },
        "let" if index == 1 => Context::Pattern,
        "let" => Context::Expression,
        "if" | "do" | "try" => Context::Expression,
        "match" if index == 1 => Context::Expression,
        "match" if index.is_multiple_of(2) => Context::Pattern,
        "match" => Context::Expression,
        "as" if index == 1 => Context::Type,
        "as" => Context::Expression,
        _ if is_label(child, "types") => Context::Expression,
        _ if previous_label(parent, index).is_some_and(|label| label == "types") => {
            Context::TypeArgumentList
        }
        _ if is_any_label(child) => Context::Expression,
        _ if previous_label(parent, index).is_some() => Context::Expression,
        _ => Context::Expression,
    }
}

fn attribute_child_context(
    parent: &CstNode,
    head: &str,
    child: &CstNode,
    index: usize,
) -> Context {
    if index == 0 {
        return Context::DeclarationAttribute;
    }
    if let Some(label) = previous_label(parent, index) {
        return attribute_value_context(label);
    }
    if is_any_label(child) {
        return Context::DeclarationAttribute;
    }
    let _ = head;
    Context::DeclarationAttribute
}

fn lambda_tail_context(parent: &CstNode, child: &CstNode, index: usize) -> Context {
    if is_label_in(child, LAMBDA_ATTRIBUTE_LABELS) {
        Context::DeclarationAttribute
    } else if let Some(label) = previous_label(parent, index) {
        attribute_value_context(label)
    } else {
        Context::Expression
    }
}

fn attribute_value_context(label: &str) -> Context {
    match label {
        "effects" => Context::EffectRow,
        "labelled" => Context::LabelledList,
        "where" => Context::WhereList,
        "types" => Context::TypeArgumentList,
        _ => Context::DeclarationAttribute,
    }
}

fn permitted_for(
    context: Context,
    path: &[&CstNode],
) -> (Option<Vec<String>>, Option<Vec<String>>) {
    match context {
        Context::Module => (Some(to_owned(TOP_LEVEL_FORMS)), Some(Vec::new())),
        Context::Declaration => (Some(to_owned(TOP_LEVEL_FORMS)), Some(Vec::new())),
        Context::Type => (Some(to_owned(TYPE_FORMS)), Some(Vec::new())),
        Context::Pattern => (Some(to_owned(PATTERN_FORMS)), Some(Vec::new())),
        Context::Expression => (
            Some(to_owned(EXPRESSION_FORMS)),
            Some(vec!["types".to_owned()]),
        ),
        Context::DeclarationAttribute => {
            (Some(Vec::new()), Some(attribute_labels(path)))
        }
        Context::EffectRow => (Some(Vec::new()), Some(Vec::new())),
        Context::DataField => (Some(Vec::new()), Some(Vec::new())),
        Context::ParameterList => (Some(Vec::new()), Some(Vec::new())),
        Context::TypeArgumentList => (Some(to_owned(TYPE_FORMS)), Some(Vec::new())),
        Context::LabelledList | Context::WhereList => {
            (Some(Vec::new()), Some(Vec::new()))
        }
        Context::Unknown => (None, None),
    }
}

fn attribute_labels(path: &[&CstNode]) -> Vec<String> {
    for node in path.iter().rev() {
        let Some(head) = head_text(node) else {
            continue;
        };
        let labels = match head {
            "deftype" | "defint" => TYPE_ATTRIBUTE_LABELS,
            "deffect" | "def" | "test" => DECLARATION_ATTRIBUTE_LABELS,
            "defn" => FUNCTION_ATTRIBUTE_LABELS,
            "lambda" => LAMBDA_ATTRIBUTE_LABELS,
            "fn" => &["labelled", "variadic", "effects"][..],
            _ => continue,
        };
        return to_owned(labels);
    }
    to_owned(FUNCTION_ATTRIBUTE_LABELS)
}

fn to_owned(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn meaningful_children(node: &CstNode) -> Vec<&CstNode> {
    node.children()
        .iter()
        .filter(|child| matches!(child.kind(), SyntaxKind::Atom | SyntaxKind::List))
        .collect()
}

fn meaningful_index(parent: &CstNode, child: &CstNode) -> Option<usize> {
    meaningful_children(parent)
        .iter()
        .position(|candidate| std::ptr::eq(*candidate, child))
}

fn head_text(node: &CstNode) -> Option<&str> {
    meaningful_children(node)
        .first()
        .and_then(|head| head.leaf_text())
}

fn is_declaration_head(head: Option<&str>) -> bool {
    head.is_some_and(|head| TOP_LEVEL_FORMS.contains(&head))
}

fn is_label(node: &CstNode, expected: &str) -> bool {
    node.name().is_some_and(|classification| {
        matches!(
            classification,
            NameClassification::Name(name)
                if name.kind() == NameKind::Label && name.value() == expected
        )
    })
}

fn is_any_label(node: &CstNode) -> bool {
    node.name().is_some_and(|classification| {
        matches!(
            classification,
            NameClassification::Name(name) if name.kind() == NameKind::Label
        )
    })
}

fn is_label_in(node: &CstNode, labels: &[&str]) -> bool {
    labels.iter().any(|label| is_label(node, label))
}

fn previous_label(parent: &CstNode, index: usize) -> Option<&str> {
    if index == 0 {
        return None;
    }
    meaningful_children(parent)
        .get(index.saturating_sub(1))
        .and_then(|node| is_any_label(node).then(|| node.leaf_text()))
        .flatten()
        .and_then(|text| text.strip_suffix(':'))
}
