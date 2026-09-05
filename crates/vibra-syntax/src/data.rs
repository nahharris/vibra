//! Generic VIBON decoding and canonical data values for Step 7.
//!
//! Generic decoding is intentionally independent of project schemas and name
//! resolution. It validates only the closed VIBON value grammar and retains
//! each decoded node's source span and original bytes.

use std::collections::{BTreeMap, BTreeSet};

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};

use crate::literal::{Literal, LiteralClassification};
use crate::name::{Name, NameClassification, NameKind};
use crate::reader::{CstNode, SyntaxKind};

/// One decoded generic VIBON node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataNode {
    value: DataValue,
    span: ByteSpan,
    raw: String,
}

impl DataNode {
    /// The decoded value.
    #[must_use]
    pub const fn value(&self) -> &DataValue {
        &self.value
    }

    /// The exact source span covered by this node.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }

    /// The exact source bytes covered by this node.
    #[must_use]
    pub fn raw(&self) -> &str {
        &self.raw
    }
}

/// A generic VIBON value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DataValue {
    /// A Step 5 literal.
    Literal(Literal),
    /// A non-discard atom value.
    Atom(Name),
    /// A record with labelled fields in source order.
    Record(Vec<DataField>),
    /// An ordered array.
    Array(Vec<DataNode>),
    /// An ordered tuple.
    Tuple(Vec<DataNode>),
    /// A map with key/value pairs in canonicalizable order.
    Map(Vec<(DataNode, DataNode)>),
}

/// One labelled record field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataField {
    label: Name,
    value: DataNode,
    span: ByteSpan,
}

impl DataField {
    /// The field label, including its lexical value but not the trailing `:`.
    #[must_use]
    pub const fn label(&self) -> &Name {
        &self.label
    }

    /// The field value.
    #[must_use]
    pub const fn value(&self) -> &DataNode {
        &self.value
    }

    /// The field's complete source span.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }
}

/// The result of generic data decoding.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DataDecode {
    value: Option<DataNode>,
    diagnostics: Vec<Diagnostic>,
}

impl DataDecode {
    /// The decoded root value when no data diagnostic was emitted.
    #[must_use]
    pub const fn value(&self) -> Option<&DataNode> {
        self.value.as_ref()
    }

    /// Diagnostics produced by generic data validation.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Whether decoding produced one valid root value.
    #[must_use]
    pub const fn accepted(&self) -> bool {
        self.value.is_some()
    }
}

/// Decodes the root of a `.vibon` CST as exactly one generic data value.
#[must_use]
pub fn decode_data_root(root: &CstNode) -> DataDecode {
    let forms = meaningful_children(root, false);
    let mut diagnostics = Vec::new();
    let value = match forms.as_slice() {
        [form] => decode_node_iterative(form, &mut diagnostics),
        _ => {
            diagnostics.push(Diagnostic::new(
                DiagnosticCode::DataInvalidShape,
                root.span(),
                "a VIBON document must contain exactly one data value",
            ));
            None
        }
    };

    DataDecode {
        value: value.filter(|_| diagnostics.is_empty()),
        diagnostics,
    }
}

enum DecodeTask<'source> {
    Visit(&'source CstNode),
    BuildSequence {
        node: &'source CstNode,
        tuple: bool,
        count: usize,
    },
    BuildRecord {
        node: &'source CstNode,
        labels: Vec<(Name, ByteSpan)>,
    },
    BuildMap {
        node: &'source CstNode,
        count: usize,
    },
}

/// Decodes one node without using the host call stack for nested data.
fn decode_node_iterative(
    root: &CstNode,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<DataNode> {
    let mut tasks = vec![DecodeTask::Visit(root)];
    let mut results = Vec::new();

    while let Some(task) = tasks.pop() {
        match task {
            DecodeTask::Visit(node) => match node.kind() {
                SyntaxKind::Atom => results.push(decode_atom(node, diagnostics)),
                SyntaxKind::List => {
                    if !schedule_list(node, &mut tasks, diagnostics) {
                        results.push(None);
                    }
                }
                SyntaxKind::Error
                | SyntaxKind::Whitespace
                | SyntaxKind::LineComment
                | SyntaxKind::OpenParen
                | SyntaxKind::CloseParen
                | SyntaxKind::Root => results.push(None),
            },
            DecodeTask::BuildSequence { node, tuple, count } => {
                let values = take_results(&mut results, count);
                let value =
                    values
                        .into_iter()
                        .collect::<Option<Vec<_>>>()
                        .map(|values| {
                            if tuple {
                                DataValue::Tuple(values)
                            } else {
                                DataValue::Array(values)
                            }
                        });
                results.push(value.map(|value| DataNode {
                    value,
                    span: node.span(),
                    raw: node.to_source(),
                }));
            }
            DecodeTask::BuildRecord { node, labels } => {
                let values = take_results(&mut results, labels.len());
                let fields = values
                    .into_iter()
                    .zip(labels)
                    .map(|(value, label)| value.map(|value| (value, label)))
                    .collect::<Option<Vec<_>>>()
                    .map(|pairs| {
                        pairs
                            .into_iter()
                            .map(|(value, (label, label_span))| DataField {
                                span: ByteSpan::new(
                                    label_span.start(),
                                    value.span().end(),
                                ),
                                label,
                                value,
                            })
                            .collect::<Vec<_>>()
                    });
                results.push(fields.map(|fields| DataNode {
                    value: DataValue::Record(fields),
                    span: node.span(),
                    raw: node.to_source(),
                }));
            }
            DecodeTask::BuildMap { node, count } => {
                let values = take_results(&mut results, count.saturating_mul(2));
                let pairs = values
                    .chunks_exact(2)
                    .map(|pair| {
                        pair.first()
                            .and_then(|value| value.clone())
                            .zip(pair.get(1).and_then(|value| value.clone()))
                    })
                    .collect::<Option<Vec<_>>>()
                    .and_then(|pairs| {
                        let mut seen = BTreeSet::new();
                        for (key, _) in &pairs {
                            if !seen.insert(canonical_node(key)) {
                                diagnostics.push(Diagnostic::new(
                                    DiagnosticCode::DataDuplicateKey,
                                    key.span(),
                                    "a map key occurs more than once",
                                ));
                                return None;
                            }
                        }
                        Some(pairs)
                    });
                results.push(pairs.map(|pairs| DataNode {
                    value: DataValue::Map(pairs),
                    span: node.span(),
                    raw: node.to_source(),
                }));
            }
        }
    }

    results.pop().flatten()
}

fn schedule_list<'source>(
    node: &'source CstNode,
    tasks: &mut Vec<DecodeTask<'source>>,
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    let forms = meaningful_children(node, true);
    let Some(head) = forms.first() else {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::DataInvalidShape,
            node.span(),
            "a VIBON container requires a data head",
        ));
        return false;
    };
    if head.kind() != SyntaxKind::Atom {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::DataInvalidValue,
            head.span(),
            "a VIBON container head must be a symbol",
        ));
        return false;
    }
    let Some(head_text) = head.leaf_text() else {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::DataInvalidValue,
            head.span(),
            "a VIBON container head must be a symbol",
        ));
        return false;
    };
    let Some(items) = forms.get(1..) else {
        return false;
    };

    match head_text {
        "record" => {
            if !items.len().is_multiple_of(2) {
                diagnostics.push(Diagnostic::new(
                    DiagnosticCode::DataInvalidShape,
                    items.last().map_or(node.span(), |item| item.span()),
                    "a record requires a value after every label",
                ));
                return false;
            }
            let mut labels = Vec::with_capacity(items.len() / 2);
            let mut seen = BTreeSet::new();
            for label_node in items.iter().step_by(2) {
                let Some(label) = record_label(label_node, diagnostics) else {
                    return false;
                };
                if !seen.insert(label.value().to_owned()) {
                    diagnostics.push(Diagnostic::new(
                        DiagnosticCode::DataDuplicateField,
                        label_node.span(),
                        "a record label occurs more than once",
                    ));
                    return false;
                }
                labels.push((label, label_node.span()));
            }
            let values = items.iter().skip(1).step_by(2).copied().collect::<Vec<_>>();
            tasks.push(DecodeTask::BuildRecord { node, labels });
            tasks.extend(values.into_iter().rev().map(DecodeTask::Visit));
        }
        "array" | "tuple" => {
            let tuple = head_text == "tuple";
            tasks.push(DecodeTask::BuildSequence {
                node,
                tuple,
                count: items.len(),
            });
            tasks.extend(items.iter().rev().copied().map(DecodeTask::Visit));
        }
        "map" => {
            if !items.len().is_multiple_of(2) {
                diagnostics.push(Diagnostic::new(
                    DiagnosticCode::DataInvalidShape,
                    items.last().map_or(node.span(), |item| item.span()),
                    "a map requires an even number of key and value forms",
                ));
                return false;
            }
            tasks.push(DecodeTask::BuildMap {
                node,
                count: items.len() / 2,
            });
            tasks.extend(items.iter().rev().copied().map(DecodeTask::Visit));
        }
        _ => {
            diagnostics.push(Diagnostic::new(
                DiagnosticCode::DataInvalidValue,
                head.span(),
                "an executable or source form is not a VIBON value",
            ));
            return false;
        }
    }
    true
}

fn take_results(
    results: &mut Vec<Option<DataNode>>,
    count: usize,
) -> Vec<Option<DataNode>> {
    let start = results.len().saturating_sub(count);
    results.split_off(start)
}

fn record_label(node: &CstNode, diagnostics: &mut Vec<Diagnostic>) -> Option<Name> {
    match node.name() {
        Some(NameClassification::Name(name)) if name.kind() == NameKind::Label => {
            Some(name)
        }
        Some(NameClassification::Name(_)) => {
            diagnostics.push(Diagnostic::new(
                DiagnosticCode::DataInvalidShape,
                node.span(),
                "a record field must use a label",
            ));
            None
        }
        Some(NameClassification::Invalid) => None,
        None if matches!(node.literal(), Some(LiteralClassification::Literal(_))) => {
            diagnostics.push(Diagnostic::new(
                DiagnosticCode::DataInvalidShape,
                node.span(),
                "a record field must use a label",
            ));
            None
        }
        None => None,
    }
}

fn decode_atom(node: &CstNode, diagnostics: &mut Vec<Diagnostic>) -> Option<DataNode> {
    let text = node.leaf_text()?;
    match node.literal()? {
        LiteralClassification::Literal(literal) => Some(DataNode {
            value: DataValue::Literal(literal),
            span: node.span(),
            raw: text.to_owned(),
        }),
        LiteralClassification::Invalid(_) => None,
        LiteralClassification::Opaque => match node.name()? {
            NameClassification::Name(name) if name.kind() == NameKind::Atom => {
                Some(DataNode {
                    value: DataValue::Atom(name),
                    span: node.span(),
                    raw: text.to_owned(),
                })
            }
            NameClassification::Name(_) => {
                diagnostics.push(Diagnostic::new(
                    DiagnosticCode::DataInvalidValue,
                    node.span(),
                    "a bare symbol, label, or discard is not a VIBON value",
                ));
                None
            }
            NameClassification::Invalid => None,
        },
    }
}

fn meaningful_children(node: &CstNode, list: bool) -> Vec<&CstNode> {
    node.children()
        .iter()
        .filter(|child| {
            !(matches!(
                child.kind(),
                SyntaxKind::Whitespace | SyntaxKind::LineComment | SyntaxKind::Error
            ) || list
                && matches!(
                    child.kind(),
                    SyntaxKind::OpenParen | SyntaxKind::CloseParen
                ))
        })
        .collect()
}

/// Canonical text for a decoded value, without the document trailing newline.
#[must_use]
pub fn canonical_data(node: &DataNode) -> String {
    canonical_node(node)
}

enum CanonicalTask<'source> {
    Node(&'source DataNode),
    Field(&'source str, &'source DataNode),
    Text(&'static str),
    Owned(String),
}

fn canonical_node(node: &DataNode) -> String {
    let mut output = String::new();
    let mut tasks = vec![CanonicalTask::Node(node)];
    while let Some(task) = tasks.pop() {
        match task {
            CanonicalTask::Node(node) => match &node.value {
                DataValue::Literal(Literal::Character(character)) => output.push_str(
                    &crate::literal::canonical_character_spelling(character.value()),
                ),
                DataValue::Literal(literal) => output.push_str(literal.raw()),
                DataValue::Atom(name) => output.push_str(name.raw()),
                DataValue::Record(fields) => {
                    schedule_container(
                        &mut tasks,
                        "record",
                        fields.iter().map(|field| {
                            CanonicalPart::Field(field.label.raw(), &field.value)
                        }),
                    );
                }
                DataValue::Array(values) => {
                    schedule_container(
                        &mut tasks,
                        "array",
                        values.iter().map(CanonicalPart::Node),
                    );
                }
                DataValue::Tuple(values) => {
                    schedule_container(
                        &mut tasks,
                        "tuple",
                        values.iter().map(CanonicalPart::Node),
                    );
                }
                DataValue::Map(pairs) => {
                    let mut pairs = pairs.iter().collect::<Vec<_>>();
                    pairs.sort_by(|left, right| {
                        let left_key = canonical_node(&left.0);
                        let right_key = canonical_node(&right.0);
                        left_key.as_bytes().cmp(right_key.as_bytes())
                    });
                    schedule_container(
                        &mut tasks,
                        "map",
                        pairs.into_iter().flat_map(|(key, value)| {
                            [CanonicalPart::Node(key), CanonicalPart::Node(value)]
                        }),
                    );
                }
            },
            CanonicalTask::Field(label, value) => {
                output.push_str(label);
                output.push(' ');
                tasks.push(CanonicalTask::Node(value));
            }
            CanonicalTask::Text(text) => output.push_str(text),
            CanonicalTask::Owned(text) => output.push_str(&text),
        }
    }
    output
}

enum CanonicalPart<'source> {
    Node(&'source DataNode),
    Field(&'source str, &'source DataNode),
}

fn schedule_container<'source, I>(
    tasks: &mut Vec<CanonicalTask<'source>>,
    head: &'static str,
    parts: I,
) where
    I: IntoIterator<Item = CanonicalPart<'source>>,
{
    let parts = parts.into_iter().collect::<Vec<_>>();
    if parts.is_empty() {
        tasks.push(CanonicalTask::Owned(format!("({head})")));
        return;
    }

    tasks.push(CanonicalTask::Text(")"));
    for (index, part) in parts.into_iter().enumerate().rev() {
        tasks.push(match part {
            CanonicalPart::Node(node) => CanonicalTask::Node(node),
            CanonicalPart::Field(label, value) => CanonicalTask::Field(label, value),
        });
        if index != 0 {
            tasks.push(CanonicalTask::Text(" "));
        }
    }
    tasks.push(CanonicalTask::Owned(format!("({head} ")));
}

/// The role declared for an atom slot by a typed data adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AtomRole {
    /// Keep the atom as a value; never resolve it.
    Value,
    /// Treat the atom as a reference of a schema-declared kind later.
    Reference,
}

/// A small typed adapter contract layered over generic data.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TypedDataSchema {
    field_order: Vec<String>,
    atom_roles: BTreeMap<String, AtomRole>,
}

impl TypedDataSchema {
    /// Creates a schema with an explicit record field order.
    #[must_use]
    pub fn new(field_order: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            field_order: field_order.into_iter().map(Into::into).collect(),
            atom_roles: BTreeMap::new(),
        }
    }

    /// Declares the role of an atom-bearing field without resolving it.
    pub fn set_atom_role(&mut self, field: impl Into<String>, role: AtomRole) {
        self.atom_roles.insert(field.into(), role);
    }

    /// The explicit record field order.
    #[must_use]
    pub fn field_order(&self) -> &[String] {
        &self.field_order
    }

    /// The declared atom role for a field, if any.
    #[must_use]
    pub fn atom_role(&self, field: &str) -> Option<AtomRole> {
        self.atom_roles.get(field).copied()
    }
}
