//! Type expressions: `type-expr`, `record-type`, `inputs`, `row`, and `path`.
//!
//! `docs/spec/01-source-language.md`, "Grammar":
//!
//! ```text
//! type-expr   = path, [ "(", { type-expr }, { identifier, ":", type-expr }, ")" ]
//!             | atom | "{", { type-expr }, "}" | record-type
//!             | "Fn", "(", inputs, type-expr, ")"
//!             | "Proc", "(", inputs, type-expr, [ row ], ")"
//!             | "Intent", "(", type-expr, [ row ], ")" ;
//! ```
//!
//! The construct matcher has already matched every bracket. This module types
//! the bracket groups of a type position and wraps each dotted name in a
//! `@path` node:
//!
//! - labelled entries decide a record type against a tuple type; mixing them
//!   is `@syntax.invalidForm` over the brace form;
//! - in type arguments and in an input vector, the labelled entries follow
//!   the positional ones; a positional entry after a labelled one is
//!   `@syntax.invalidForm` over the bracket form;
//! - `Fn`, `Proc`, `Intent`, and `Intrinsic` are reserved type heads: a type
//!   expression that begins with one is parsed by that head's production and
//!   never as an applied path; `Intrinsic` is a type body only;
//! - `(` is never the start of a type: a `(…)` there is `@syntax.invalidForm`
//!   over the parenthesized form;
//! - a reserved word written as a label is the label, with
//!   `@name.reservedLabel` at the word.
//!
//! An invalid form is kept, unshaped, in an `@error` node. A bracket form
//! closed as missing already carries its `@syntax.unmatchedDelimiter` and is
//! kept as written, with no further report. Shaping is iterative, so a type
//! nested to any depth is bounded by memory, not by the stack. The views
//! ([`TypeExpr`], [`Path`], [`Inputs`], [`Row`]) read one level at a time.

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};

use super::construct::is_trivia;
use super::declaration::{Atom, Ident, name_of};
use crate::cst::{CstNode, SyntaxKind};

/// A reserved type head.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Head {
    Fn,
    Proc,
    Intent,
    Intrinsic,
}

impl Head {
    /// The head an identifier spells, if any.
    pub(crate) fn of(word: &str) -> Option<Self> {
        match word {
            "Fn" => Some(Self::Fn),
            "Proc" => Some(Self::Proc),
            "Intent" => Some(Self::Intent),
            "Intrinsic" => Some(Self::Intrinsic),
            _ => None,
        }
    }
}

/// How a reserved word written as an entry's label is reported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LabelKind {
    /// A label position: `@name.reservedLabel` at the word.
    Label,
    /// A `given` key, which names a type parameter: the chapter names no
    /// code for a reserved word there, so it is a form the grammar does not
    /// admit, `@syntax.invalidForm` at the word.
    GivenKey,
}

/// What a bracket group of a type position is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Role {
    /// `{…}` in a type position: a tuple type or a record type.
    Product,
    /// `{…}` after a case name: a record type.
    Record,
    /// `{…}`, the first operand of `Fn` or `Proc`.
    Inputs,
    /// `{…}`, the optional last operand of `Proc` or `Intent`.
    Row,
    /// `{…}` after `given`.
    Given,
    /// `(…)` after a path: the type arguments.
    Arguments,
    /// `(…)` after `Fn`, `Proc`, or `Intent`.
    Callable(Head),
    /// `(…)` after `Intrinsic`, in a type body.
    Intrinsic,
}

/// One item of a shaped list: a finished node, or a bracket group still to
/// be shaped in its role.
enum Slot {
    Ready(CstNode),
    Visit(CstNode, Role),
}

/// The shape of one entry, by indices into its item list.
#[derive(Clone, Copy, Debug)]
enum Form {
    /// A label with no type after it.
    Missing,
    /// `path [ (…) ]`; `dangling` when the path ends in `.`.
    Path {
        first: usize,
        last: usize,
        dangling: bool,
        applied: Option<usize>,
    },
    /// A reserved type head and the `(…)` written after it.
    Head {
        head: usize,
        kind: Head,
        applied: Option<usize>,
    },
    /// Any other single item.
    Single(usize),
}

/// One entry of a list: an optional label (the word and the `:`) and its
/// type.
#[derive(Clone, Copy, Debug)]
struct Entry {
    label: Option<(usize, usize)>,
    form: Form,
}

impl Entry {
    fn is_bare_path(&self) -> bool {
        self.label.is_none()
            && matches!(
                self.form,
                Form::Path {
                    dangling: false,
                    applied: None,
                    ..
                }
            )
    }
}

fn is_punctuation(node: &CstNode, text: &str, mark: &str) -> bool {
    node.kind() == SyntaxKind::Punctuation && node.text(text) == mark
}

/// Whether an item takes part in an entry: trivia, the `@error` nodes of an
/// earlier recovery, and `@missing` leaves do not.
fn is_entry_item(node: &CstNode) -> bool {
    !is_trivia(node) && !matches!(node.kind(), SyntaxKind::Error | SyntaxKind::Missing)
}

/// Splits a list of items into entries.
fn split_entries(items: &[CstNode], text: &str) -> Vec<Entry> {
    let positions: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, node)| is_entry_item(node))
        .map(|(index, _)| index)
        .collect();
    let at = |k: usize| {
        positions
            .get(k)
            .and_then(|&index| items.get(index).map(|node| (index, node)))
    };
    let is_label_at = |k: usize| {
        at(k).is_some_and(|(_, node)| {
            matches!(
                node.kind(),
                SyntaxKind::Identifier | SyntaxKind::ReservedWord
            )
        }) && at(k + 1).is_some_and(|(_, node)| is_punctuation(node, text, ":"))
    };
    let applied_at = |k: usize| {
        at(k)
            .filter(|(_, node)| node.kind() == SyntaxKind::Paren)
            .map(|(index, _)| index)
    };
    let mut entries = Vec::new();
    let mut k = 0;
    while at(k).is_some() {
        let mut label = None;
        if is_label_at(k)
            && let (Some((word, _)), Some((colon, _))) = (at(k), at(k + 1))
        {
            label = Some((word, colon));
            k += 2;
        }
        let Some((index, node)) =
            at(k).filter(|_| !(label.is_some() && is_label_at(k)))
        else {
            entries.push(Entry {
                label,
                form: Form::Missing,
            });
            continue;
        };
        let form = match node.kind() {
            SyntaxKind::Identifier => {
                if let Some(head) = Head::of(node.text(text)) {
                    let applied = applied_at(k + 1);
                    k += 1 + usize::from(applied.is_some());
                    Form::Head {
                        head: index,
                        kind: head,
                        applied,
                    }
                } else {
                    let mut last_k = k;
                    while at(last_k + 1)
                        .is_some_and(|(_, dot)| is_punctuation(dot, text, "."))
                        && at(last_k + 2).is_some_and(|(_, part)| {
                            part.kind() == SyntaxKind::Identifier
                        })
                    {
                        last_k += 2;
                    }
                    let dangling = at(last_k + 1)
                        .is_some_and(|(_, dot)| is_punctuation(dot, text, "."));
                    if dangling {
                        last_k += 1;
                    }
                    let last = at(last_k).map_or(index, |(last, _)| last);
                    let applied = if dangling {
                        None
                    } else {
                        applied_at(last_k + 1)
                    };
                    k = last_k + 1 + usize::from(applied.is_some());
                    Form::Path {
                        first: index,
                        last,
                        dangling,
                        applied,
                    }
                }
            }
            _ => {
                k += 1;
                Form::Single(index)
            }
        };
        entries.push(Entry { label, form });
    }
    entries
}

/// Whether every labelled entry follows every positional one.
fn positional_then_labelled(entries: &[Entry]) -> bool {
    let mut labelled = false;
    for entry in entries {
        if entry.label.is_some() {
            labelled = true;
        } else if labelled {
            return false;
        }
    }
    true
}

/// Moves items out of a list in order, wrapping and scheduling as it goes.
struct Builder {
    items: Vec<Option<CstNode>>,
    next: usize,
    slots: Vec<Slot>,
}

impl Builder {
    fn new(items: Vec<CstNode>) -> Self {
        Self {
            items: items.into_iter().map(Some).collect(),
            next: 0,
            slots: Vec::new(),
        }
    }

    fn take(&mut self, index: usize) -> Option<CstNode> {
        self.items.get_mut(index).and_then(Option::take)
    }

    fn span(&self, from: usize, to: usize) -> ByteSpan {
        let start = self
            .items
            .get(from)
            .and_then(Option::as_ref)
            .map_or(0, |node| node.span().start());
        let end = self
            .items
            .get(to)
            .and_then(Option::as_ref)
            .map_or(start, |node| node.span().end());
        ByteSpan::new(start, end)
    }

    /// Emits every item before `index` as it is.
    fn flush(&mut self, index: usize) {
        while self.next < index {
            if let Some(node) = self.take(self.next) {
                self.slots.push(Slot::Ready(node));
            }
            self.next += 1;
        }
    }

    fn range(&mut self, from: usize, to: usize) -> Vec<CstNode> {
        self.flush(from);
        let nodes = (from..=to).filter_map(|index| self.take(index)).collect();
        self.next = self.next.max(to + 1);
        nodes
    }

    fn ready(&mut self, index: usize) {
        for node in self.range(index, index) {
            self.slots.push(Slot::Ready(node));
        }
    }

    fn visit(&mut self, index: usize, role: Role) {
        for node in self.range(index, index) {
            self.slots.push(Slot::Visit(node, role));
        }
    }

    fn wrap(&mut self, from: usize, to: usize, kind: SyntaxKind) {
        let at = self.span(from, to).start();
        let nodes = self.range(from, to);
        self.slots
            .push(Slot::Ready(CstNode::interior(kind, nodes, at)));
    }

    /// Keeps the items `from` to `to` unshaped in an `@error` node, with one
    /// `@syntax.invalidForm` over them.
    fn invalid(
        &mut self,
        from: usize,
        to: usize,
        message: &str,
        sink: &mut Vec<Diagnostic>,
    ) {
        let span = self.span(from, to);
        sink.push(Diagnostic::new(
            DiagnosticCode::SyntaxInvalidForm,
            span,
            message,
        ));
        self.wrap(from, to, SyntaxKind::Error);
    }

    fn finish(mut self) -> Vec<Slot> {
        self.flush(self.items.len());
        self.slots
    }
}

/// How one list is built.
struct ListRules<'r> {
    /// The role of a brace written as a whole entry, by entry position.
    overrides: &'r [(usize, Role)],
    labels: LabelKind,
    /// Whether `Intrinsic(atom)` is admitted: a type body.
    body: bool,
}

/// Builds the slots of one list and returns the span of an `Intrinsic(…)`
/// entry, when the rules admit one.
fn build(
    items: Vec<CstNode>,
    entries: &[Entry],
    rules: &ListRules<'_>,
    text: &str,
    sink: &mut Vec<Diagnostic>,
) -> (Vec<Slot>, Option<ByteSpan>) {
    let kinds: Vec<SyntaxKind> = items.iter().map(CstNode::kind).collect();
    // `as` outside a `use` is reported by the generic section rules.
    let is_as: Vec<bool> = items
        .iter()
        .map(|node| node.kind() == SyntaxKind::ReservedWord && node.text(text) == "as")
        .collect();
    let mut builder = Builder::new(items);
    let mut intrinsic = None;
    for (position, entry) in entries.iter().enumerate() {
        if let Some((word, colon)) = entry.label {
            if matches!(entry.form, Form::Missing) {
                builder.invalid(word, colon, "a label is followed by a type", sink);
                continue;
            }
            if kinds.get(word) == Some(&SyntaxKind::ReservedWord) {
                let span = builder.span(word, word);
                sink.push(match rules.labels {
                    LabelKind::Label => Diagnostic::new(
                        DiagnosticCode::NameReservedLabel,
                        span,
                        "a reserved word cannot be a label",
                    ),
                    LabelKind::GivenKey => Diagnostic::new(
                        DiagnosticCode::SyntaxInvalidForm,
                        span,
                        "a `given` key names a type parameter",
                    ),
                });
            }
            builder.ready(word);
            builder.ready(colon);
        }
        match entry.form {
            Form::Missing => {}
            Form::Path {
                first,
                last,
                dangling,
                applied,
            } => {
                if dangling {
                    builder.invalid(first, last, "a path ends with a name", sink);
                } else {
                    builder.wrap(first, last, SyntaxKind::Path);
                    if let Some(applied) = applied {
                        builder.visit(applied, Role::Arguments);
                    }
                }
            }
            Form::Head {
                head,
                kind,
                applied,
            } => match (kind, applied) {
                (Head::Intrinsic, Some(applied)) if rules.body => {
                    intrinsic = Some(builder.span(head, applied));
                    builder.ready(head);
                    builder.visit(applied, Role::Intrinsic);
                }
                (Head::Intrinsic, _) => builder.invalid(
                    head,
                    applied.unwrap_or(head),
                    "`Intrinsic` is written only as a type body",
                    sink,
                ),
                (_, None) => builder.invalid(
                    head,
                    head,
                    "a reserved type head is applied to its operands",
                    sink,
                ),
                (head_kind, Some(applied)) => {
                    builder.ready(head);
                    builder.visit(applied, Role::Callable(head_kind));
                }
            },
            Form::Single(index) => match kinds.get(index) {
                Some(SyntaxKind::Atom) => builder.ready(index),
                Some(SyntaxKind::Brace) => {
                    let role = rules
                        .overrides
                        .iter()
                        .find(|(at, _)| *at == position)
                        .map_or(Role::Product, |(_, role)| *role);
                    builder.visit(index, role);
                }
                Some(SyntaxKind::Paren) => {
                    builder.invalid(index, index, "`(` never starts a type", sink);
                }
                _ if is_as.get(index) == Some(&true) => {
                    builder.wrap(index, index, SyntaxKind::Error);
                }
                _ => builder.invalid(index, index, "this is not a type", sink),
            },
        }
    }
    (builder.finish(), intrinsic)
}

/// The kind a group takes, the roles of its entry braces, and how its labels
/// are reported; `None` when its entries do not fit its role.
type Accepted = (SyntaxKind, Vec<(usize, Role)>, LabelKind);

fn accept(
    role: Role,
    group: SyntaxKind,
    entries: &[Entry],
    contents: &[CstNode],
) -> Result<Accepted, &'static str> {
    let labelled = entries.iter().filter(|entry| entry.label.is_some()).count();
    let is_brace = |entry: Option<&Entry>| {
        entry.is_some_and(|entry| {
            entry.label.is_none()
                && matches!(entry.form, Form::Single(index)
                    if contents.get(index).is_some_and(|node| node.kind() == SyntaxKind::Brace))
        })
    };
    let fits = |ok: bool, kind: SyntaxKind, message: &'static str| {
        if ok {
            Ok((kind, Vec::new(), LabelKind::Label))
        } else {
            Err(message)
        }
    };
    match role {
        Role::Product if group == SyntaxKind::Brace => {
            if labelled == 0 {
                Ok((SyntaxKind::Brace, Vec::new(), LabelKind::Label))
            } else {
                fits(
                    labelled == entries.len(),
                    SyntaxKind::RecordType,
                    "a brace type is all positional (a tuple) or all labelled (a record)",
                )
            }
        }
        Role::Record if group == SyntaxKind::Brace => fits(
            !entries.is_empty() && labelled == entries.len(),
            SyntaxKind::RecordType,
            "the fields of a case are a record type: one or more `name: Type` entries",
        ),
        Role::Inputs if group == SyntaxKind::Brace => fits(
            positional_then_labelled(entries),
            SyntaxKind::Inputs,
            "labelled inputs follow the positional ones",
        ),
        Role::Row if group == SyntaxKind::Brace => fits(
            entries.iter().all(Entry::is_bare_path),
            SyntaxKind::Row,
            "a row is a brace of paths",
        ),
        Role::Given if group == SyntaxKind::Brace => {
            if labelled == entries.len() {
                Ok((SyntaxKind::Brace, Vec::new(), LabelKind::GivenKey))
            } else {
                Err("a `given` entry is `Parameter: Bound`")
            }
        }
        Role::Arguments if group == SyntaxKind::Paren => fits(
            positional_then_labelled(entries),
            SyntaxKind::Paren,
            "labelled type arguments follow the positional ones",
        ),
        Role::Callable(head) if group == SyntaxKind::Paren => {
            let (inputs, results, row) = match head {
                Head::Fn => (true, 2..=2, None),
                Head::Proc => (true, 2..=3, Some(2)),
                Head::Intent | Head::Intrinsic => (false, 1..=2, Some(1)),
            };
            let mut overrides = Vec::new();
            let mut ok = labelled == 0 && results.contains(&entries.len());
            if inputs {
                ok &= is_brace(entries.first());
                overrides.push((0, Role::Inputs));
            }
            if let Some(row) = row
                && entries.len() > row
            {
                ok &= is_brace(entries.get(row));
                overrides.push((row, Role::Row));
            }
            if ok {
                Ok((SyntaxKind::Paren, overrides, LabelKind::Label))
            } else {
                Err(match head {
                    Head::Fn => "a function type is `Fn(inputs Result)`",
                    Head::Proc => "a procedure type is `Proc(inputs Result [row])`",
                    Head::Intent | Head::Intrinsic => "a plan type is `Intent(Result [row])`",
                })
            }
        }
        Role::Intrinsic if group == SyntaxKind::Paren => fits(
            entries.len() == 1
                && entries.first().is_some_and(|entry| {
                    entry.label.is_none()
                        && matches!(entry.form, Form::Single(index)
                            if contents.get(index).is_some_and(|node| node.kind() == SyntaxKind::Atom))
                }),
            SyntaxKind::Paren,
            "`Intrinsic` takes one atom",
        ),
        _ => Err("this bracket form is not a type"),
    }
}

/// Shapes one bracket group in its role: the kind it takes and the slots of
/// its children.
fn visit(
    kind: SyntaxKind,
    span: ByteSpan,
    mut children: Vec<CstNode>,
    role: Role,
    text: &str,
    sink: &mut Vec<Diagnostic>,
) -> (SyntaxKind, Vec<Slot>) {
    let recovered = children.len() < 2
        || children
            .last()
            .is_some_and(|last| last.kind() == SyntaxKind::Missing);
    if recovered {
        return (kind, children.into_iter().map(Slot::Ready).collect());
    }
    let closer = children.pop();
    let mut contents = children;
    let opener = contents.remove(0);
    let entries = split_entries(&contents, text);
    match accept(role, kind, &entries, &contents) {
        Ok((accepted, overrides, labels)) => {
            let rules = ListRules {
                overrides: &overrides,
                labels,
                body: false,
            };
            let (slots, _) = build(contents, &entries, &rules, text, sink);
            let mut all = Vec::with_capacity(slots.len() + 2);
            all.push(Slot::Ready(opener));
            all.extend(slots);
            all.extend(closer.map(Slot::Ready));
            (accepted, all)
        }
        Err(message) => {
            sink.push(Diagnostic::new(
                DiagnosticCode::SyntaxInvalidForm,
                span,
                message,
            ));
            let all = std::iter::once(opener)
                .chain(contents)
                .chain(closer)
                .map(Slot::Ready)
                .collect();
            (SyntaxKind::Error, all)
        }
    }
}

/// Runs the slots to finished nodes; every group is shaped before its
/// parent is assembled. Iterative.
fn run(slots: Vec<Slot>, text: &str, sink: &mut Vec<Diagnostic>) -> Vec<CstNode> {
    enum Work {
        Slot(Slot),
        Assemble {
            kind: SyntaxKind,
            count: usize,
            at: usize,
        },
    }
    let mut work: Vec<Work> = slots.into_iter().rev().map(Work::Slot).collect();
    let mut out: Vec<CstNode> = Vec::new();
    while let Some(step) = work.pop() {
        match step {
            Work::Slot(Slot::Ready(node)) => out.push(node),
            Work::Slot(Slot::Visit(mut node, role)) => {
                let span = node.span();
                let children = node.take_children();
                let (kind, slots) =
                    visit(node.kind(), span, children, role, text, sink);
                work.push(Work::Assemble {
                    kind,
                    count: slots.len(),
                    at: span.start(),
                });
                work.extend(slots.into_iter().rev().map(Work::Slot));
            }
            Work::Assemble { kind, count, at } => {
                let children = out.split_off(out.len().saturating_sub(count));
                out.push(CstNode::interior(kind, children, at));
            }
        }
    }
    out
}

/// A shaped operand of type expressions.
pub(crate) struct TypeList {
    /// The shaped items, trivia included.
    pub(crate) nodes: Vec<CstNode>,
    /// The number of entries, `@error` nodes of earlier recoveries excluded.
    pub(crate) entries: usize,
    /// Whether an entry is labelled.
    pub(crate) labelled: bool,
    /// The span of an `Intrinsic(…)` entry, when `body` admitted one.
    pub(crate) intrinsic: Option<ByteSpan>,
}

/// Shapes the items of an operand as a list of type expressions. `body`
/// admits `Intrinsic(atom)`, which only a type body may be.
pub(crate) fn shape_type_list(
    items: Vec<CstNode>,
    body: bool,
    text: &str,
    sink: &mut Vec<Diagnostic>,
) -> TypeList {
    let entries = split_entries(&items, text);
    let rules = ListRules {
        overrides: &[],
        labels: LabelKind::Label,
        body,
    };
    let (slots, intrinsic) = build(items, &entries, &rules, text, sink);
    TypeList {
        nodes: run(slots, text, sink),
        entries: entries.len(),
        labelled: entries.iter().any(|entry| entry.label.is_some()),
        intrinsic,
    }
}

/// The number of entries of an operand and whether one is labelled, before
/// it is shaped.
pub(crate) fn entry_count(items: &[CstNode], text: &str) -> (usize, bool) {
    let entries = split_entries(items, text);
    (
        entries.len(),
        entries.iter().any(|entry| entry.label.is_some()),
    )
}

/// Shapes one bracket group in `role`; the result is the group's node.
pub(crate) fn shape_group(
    node: CstNode,
    role: Role,
    text: &str,
    sink: &mut Vec<Diagnostic>,
) -> Vec<CstNode> {
    run(vec![Slot::Visit(node, role)], text, sink)
}

/// The children of a bracket node between its opener and its closer.
fn inner(node: &CstNode) -> &[CstNode] {
    let children = node.children();
    children
        .get(1..children.len().saturating_sub(1))
        .unwrap_or_default()
}

/// A type expression: a view over its head node and the `(…)` applied to it.
#[derive(Clone, Copy, Debug)]
pub struct TypeExpr<'a> {
    head: &'a CstNode,
    applied: Option<&'a CstNode>,
    text: &'a str,
}

/// One level of a type expression, as [`TypeExpr::form`] reads it.
#[derive(Clone, Debug)]
pub enum TypeForm<'a> {
    /// `path` or `path(positional… label: T…)`.
    Path {
        /// The dotted name.
        path: Path<'a>,
        /// The positional type arguments, in written order.
        positional: Vec<TypeExpr<'a>>,
        /// The labelled type arguments, in written order.
        labelled: Vec<(Ident<'a>, TypeExpr<'a>)>,
    },
    /// An atom's singleton type.
    Atom(Atom<'a>),
    /// `{T U}`, the components in written order.
    Tuple(Vec<TypeExpr<'a>>),
    /// `{name: T …}`, the fields in written order.
    Record(Vec<(Ident<'a>, TypeExpr<'a>)>),
    /// `Fn(inputs Result)`.
    Fn {
        /// The input vector.
        inputs: Inputs<'a>,
        /// The result type.
        result: TypeExpr<'a>,
    },
    /// `Proc(inputs Result [row])`.
    Proc {
        /// The input vector.
        inputs: Inputs<'a>,
        /// The result type.
        result: TypeExpr<'a>,
        /// The row, when written.
        row: Option<Row<'a>>,
    },
    /// `Intent(Result [row])`.
    Intent {
        /// The result type.
        result: TypeExpr<'a>,
        /// The row, when written.
        row: Option<Row<'a>>,
    },
}

impl<'a> TypeExpr<'a> {
    /// The head node: a `@path`, an atom, a brace form, or a reserved type
    /// head.
    #[must_use]
    pub const fn head(&self) -> &'a CstNode {
        self.head
    }

    /// The `(…)` applied to the head, when written.
    #[must_use]
    pub const fn applied(&self) -> Option<&'a CstNode> {
        self.applied
    }

    /// The bytes of the type expression.
    #[must_use]
    pub fn span(&self) -> ByteSpan {
        let end = self.applied.unwrap_or(self.head).span().end();
        ByteSpan::new(self.head.span().start(), end)
    }

    /// The type expression's form, one level deep; `None` when it was not
    /// shaped (recovery kept it as written).
    #[must_use]
    pub fn form(&self) -> Option<TypeForm<'a>> {
        let text = self.text;
        match self.head.kind() {
            SyntaxKind::Path => {
                let (positional, labelled) = self
                    .applied
                    .map(|applied| split_labelled(type_entries(inner(applied), text)))
                    .unwrap_or_default();
                Some(TypeForm::Path {
                    path: Path {
                        node: self.head,
                        text,
                    },
                    positional,
                    labelled,
                })
            }
            SyntaxKind::Atom => Atom::of(self.head, text).map(TypeForm::Atom),
            SyntaxKind::Brace => Some(TypeForm::Tuple(
                type_entries(inner(self.head), text)
                    .into_iter()
                    .map(|(_, entry)| entry)
                    .collect(),
            )),
            SyntaxKind::RecordType => Some(TypeForm::Record(
                type_entries(inner(self.head), text)
                    .into_iter()
                    .filter_map(|(label, entry)| Some((label?, entry)))
                    .collect(),
            )),
            SyntaxKind::Identifier => {
                let operands: Vec<TypeExpr<'a>> =
                    type_entries(inner(self.applied?), text)
                        .into_iter()
                        .map(|(_, entry)| entry)
                        .collect();
                let row = |index: usize| {
                    operands
                        .get(index)
                        .filter(|operand| operand.head.kind() == SyntaxKind::Row)
                        .map(|operand| Row {
                            node: operand.head,
                            text,
                        })
                };
                let inputs = || {
                    operands
                        .first()
                        .filter(|operand| operand.head.kind() == SyntaxKind::Inputs)
                        .map(|operand| Inputs {
                            node: operand.head,
                            text,
                        })
                };
                match Head::of(self.head.text(text))? {
                    Head::Fn => Some(TypeForm::Fn {
                        inputs: inputs()?,
                        result: *operands.get(1)?,
                    }),
                    Head::Proc => Some(TypeForm::Proc {
                        inputs: inputs()?,
                        result: *operands.get(1)?,
                        row: row(2),
                    }),
                    Head::Intent => Some(TypeForm::Intent {
                        result: *operands.first()?,
                        row: row(1),
                    }),
                    Head::Intrinsic => None,
                }
            }
            _ => None,
        }
    }
}

fn split_labelled<'a>(
    entries: Vec<(Option<Ident<'a>>, TypeExpr<'a>)>,
) -> (Vec<TypeExpr<'a>>, Vec<(Ident<'a>, TypeExpr<'a>)>) {
    let mut positional = Vec::new();
    let mut labelled = Vec::new();
    for (label, entry) in entries {
        match label {
            Some(label) => labelled.push((label, entry)),
            None => positional.push(entry),
        }
    }
    (positional, labelled)
}

/// The entries of a shaped list: an optional label and a type expression
/// each. `@error` nodes are skipped.
pub(crate) fn type_entries<'a>(
    nodes: &'a [CstNode],
    text: &'a str,
) -> Vec<(Option<Ident<'a>>, TypeExpr<'a>)> {
    let items: Vec<&'a CstNode> =
        nodes.iter().filter(|node| is_entry_item(node)).collect();
    let mut entries = Vec::new();
    let mut index = 0;
    while let Some(&first) = items.get(index) {
        let mut label = None;
        if items
            .get(index + 1)
            .is_some_and(|colon| is_punctuation(colon, text, ":"))
        {
            label = name_of(first, text);
            index += 2;
        }
        let Some(&head) = items.get(index) else {
            break;
        };
        let appliable = head.kind() == SyntaxKind::Path
            || (head.kind() == SyntaxKind::Identifier
                && Head::of(head.text(text)).is_some());
        let applied = items
            .get(index + 1)
            .copied()
            .filter(|next| appliable && next.kind() == SyntaxKind::Paren);
        index += 1 + usize::from(applied.is_some());
        entries.push((
            label,
            TypeExpr {
                head,
                applied,
                text,
            },
        ));
    }
    entries
}

/// The type expression at `items[*at]`, a shaped type: its head and the
/// `(…)` applied to it; moves past it.
pub(crate) fn read_type<'a>(
    items: &[&'a CstNode],
    at: &mut usize,
    text: &'a str,
) -> Option<TypeExpr<'a>> {
    let head = *items.get(*at)?;
    let fits = match head.kind() {
        SyntaxKind::Path
        | SyntaxKind::Atom
        | SyntaxKind::Brace
        | SyntaxKind::RecordType => true,
        SyntaxKind::Identifier => Head::of(head.text(text)).is_some(),
        _ => false,
    };
    if !fits {
        return None;
    }
    let appliable = matches!(head.kind(), SyntaxKind::Path | SyntaxKind::Identifier);
    let applied = items
        .get(*at + 1)
        .copied()
        .filter(|next| appliable && next.kind() == SyntaxKind::Paren);
    *at += 1 + usize::from(applied.is_some());
    Some(TypeExpr {
        head,
        applied,
        text,
    })
}

/// A dotted name: `identifier { "." identifier }`.
#[derive(Clone, Copy, Debug)]
pub struct Path<'a> {
    node: &'a CstNode,
    text: &'a str,
}

impl<'a> Path<'a> {
    /// The view of a `@path` node of the document `text`.
    pub(crate) const fn of(node: &'a CstNode, text: &'a str) -> Self {
        Self { node, text }
    }

    /// The `@path` node.
    #[must_use]
    pub const fn node(&self) -> &'a CstNode {
        self.node
    }

    /// The names, in written order.
    #[must_use]
    pub fn segments(&self) -> Vec<Ident<'a>> {
        self.node
            .children()
            .iter()
            .filter_map(|child| name_of(child, self.text))
            .collect()
    }

    /// The dotted name without trivia: `io.Stdout`.
    #[must_use]
    pub fn dotted(&self) -> String {
        self.segments()
            .iter()
            .map(Ident::text)
            .collect::<Vec<_>>()
            .join(".")
    }
}

/// The input vector of a callable type: positional types, then labelled
/// ones.
#[derive(Clone, Copy, Debug)]
pub struct Inputs<'a> {
    node: &'a CstNode,
    text: &'a str,
}

impl<'a> Inputs<'a> {
    /// The `@inputs` node.
    #[must_use]
    pub const fn node(&self) -> &'a CstNode {
        self.node
    }

    /// The positional input types, in written order.
    #[must_use]
    pub fn positional(&self) -> Vec<TypeExpr<'a>> {
        split_labelled(type_entries(inner(self.node), self.text)).0
    }

    /// The labelled inputs, in written order.
    #[must_use]
    pub fn labelled(&self) -> Vec<(Ident<'a>, TypeExpr<'a>)> {
        split_labelled(type_entries(inner(self.node), self.text)).1
    }
}

/// An effect row: a brace of paths.
#[derive(Clone, Copy, Debug)]
pub struct Row<'a> {
    node: &'a CstNode,
    text: &'a str,
}

impl<'a> Row<'a> {
    /// The view of a `@row` node of the document `text`.
    pub(crate) fn of(node: &'a CstNode, text: &'a str) -> Option<Self> {
        (node.kind() == SyntaxKind::Row).then_some(Self { node, text })
    }

    /// The `@row` node.
    #[must_use]
    pub const fn node(&self) -> &'a CstNode {
        self.node
    }

    /// The paths, in written order.
    #[must_use]
    pub fn paths(&self) -> Vec<Path<'a>> {
        inner(self.node)
            .iter()
            .filter(|child| child.kind() == SyntaxKind::Path)
            .map(|node| Path {
                node,
                text: self.text,
            })
            .collect()
    }
}
