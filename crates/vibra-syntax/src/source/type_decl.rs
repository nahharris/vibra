//! `type … endtype`, and the declaration shaping it shares with `interface`,
//! `impl`, and associated types.
//!
//! `docs/spec/01-source-language.md`, "Grammar" and "Declarations":
//!
//! ```text
//! type-decl   = "type", identifier, [ type-params ],
//!               ( "of", type-body | case, { case } ),
//!               { type-item }, "endtype" ;
//! type-body   = "Intrinsic", "(", atom, ")" | type-expr ;
//! case        = "case", identifier, [ record-type ] ;
//! type-item   = given | where | role | visibility | representation | doc
//!             | fn-decl | proc-decl | impl ;
//! ```
//!
//! The construct matcher gives a declaration its generic sections and
//! nested constructs. Shaping reads them again in the declaration's own
//! grammar and builds the nodes of its productions:
//!
//! - the name, then the optional type parameters, come first; a reserved
//!   word or a reserved type head as the name or a parameter is the name,
//!   with `@name.reservedDeclaration` at it;
//! - a section the declaration kind does not admit
//!   ([`super::keyword::ADMISSIBLE_SECTIONS`]) is `@syntax.invalidSection` at
//!   the section, and so are `role` and `Intrinsic` outside the embedded
//!   standard library;
//! - a tag outside its section's closed set is `@syntax.invalidForm` over
//!   the section;
//! - a missing name, a `type` with neither `of` nor `case` or with both, and
//!   tokens that belong to no part are `@syntax.invalidForm` over the
//!   declaration, once; a declaration closed as missing is not checked for
//!   completeness, because its end was never written;
//! - the operand of `where` runs to the next section, case, or member, and
//!   holds the expression constructs written in it (an anonymous `fn`, an
//!   `if`); its expressions are typed by the expression grammar;
//! - a named `fn` or `proc` member is `@tool.unavailable` over the member
//!   until the callable grammar exists; an `impl` is shaped in place; any
//!   other nested construct is `@syntax.invalidForm` over it.
//!
//! Cases keep their written order, which is the order of the variants.

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};

use super::construct::is_trivia;
use super::declaration::{Atom, Ident, name_of};
use super::keyword::{STANDARD_LIBRARY_SECTIONS, admissible_sections, tag_set};
use super::section::{Construct, Doc, Given, TagSection, Where};
use super::types::{
    Head, Role, TypeExpr, entry_count, shape_group, shape_type_list, type_entries,
};
use crate::cst::{CstNode, SyntaxKind};

/// What the reader knows about the module it parses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourceOptions {
    /// Whether the module is one of the embedded standard-library modules,
    /// where `role`, `Intrinsic`, and external declarations are admitted.
    pub standard_library: bool,
}

/// The kinds of declaration this module shapes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeclKind {
    Type,
    Interface,
    Impl,
    AssocType,
    AssocChoice,
}

impl DeclKind {
    const fn node_kind(self) -> SyntaxKind {
        match self {
            Self::Type => SyntaxKind::TypeDecl,
            Self::Interface => SyntaxKind::InterfaceDecl,
            Self::Impl => SyntaxKind::Impl,
            Self::AssocType => SyntaxKind::AssocType,
            Self::AssocChoice => SyntaxKind::AssocChoice,
        }
    }

    /// The sections the kind admits; an associated type or choice admits
    /// none.
    fn admits(self, keyword: &str) -> bool {
        let row = match self {
            Self::Type => "type",
            Self::Interface => "interface",
            Self::Impl => "impl",
            Self::AssocType | Self::AssocChoice => return false,
        };
        admissible_sections(row).is_some_and(|sections| sections.contains(&keyword))
    }

    const fn has_name(self) -> bool {
        !matches!(self, Self::Impl)
    }
}

/// One item of a declaration's contents with its sections opened: a section
/// keyword is marked, so that a reserved word recovered as the name is not
/// taken for one.
struct Item {
    node: CstNode,
    section_keyword: bool,
}

/// The part of a declaration being read.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Part {
    /// Tokens that belong to no part.
    Stray,
    /// The target of an `impl`.
    Target,
    /// A section, by keyword.
    Section(String),
    /// A `case`.
    Case,
}

/// Whether a nested construct is an expression: `if`, `match`, a `do`
/// block, or an anonymous `fn` or `proc`. An expression construct belongs
/// to the operand it is written in.
fn is_expression_construct(node: &CstNode, text: &str) -> bool {
    let Some(construct) = Construct::new(node, text) else {
        return false;
    };
    match construct.keyword() {
        "if" | "match" | "do" => true,
        "fn" | "proc" => !is_named(construct),
        _ => false,
    }
}

/// Whether a nested `fn` or `proc` is named: its first word is a name.
fn is_named(construct: Construct<'_>) -> bool {
    construct.head().first().is_some_and(|first| {
        matches!(
            first.kind(),
            SyntaxKind::Identifier | SyntaxKind::ReservedWord
        )
    })
}

fn split_trailing_trivia(items: &mut Vec<CstNode>) -> Vec<CstNode> {
    let keep = items
        .iter()
        .rposition(|item| !is_trivia(item))
        .map_or(0, |last| last + 1);
    items.split_off(keep)
}

/// The span from the first to the last node that is not trivia.
fn content_span(items: &[CstNode]) -> Option<ByteSpan> {
    let first = items.iter().find(|item| !is_trivia(item))?;
    let last = items.iter().rev().find(|item| !is_trivia(item))?;
    Some(ByteSpan::new(first.span().start(), last.span().end()))
}

/// The items that are neither trivia nor an earlier recovery.
fn significant(items: &[CstNode]) -> Vec<&CstNode> {
    items
        .iter()
        .filter(|item| !is_trivia(item) && item.kind() != SyntaxKind::Error)
        .collect()
}

/// Reports a reserved word or a reserved type head written as a declared
/// name.
fn report_declared_name(node: &CstNode, text: &str, sink: &mut Vec<Diagnostic>) {
    let reserved = node.kind() == SyntaxKind::ReservedWord;
    let head =
        node.kind() == SyntaxKind::Identifier && Head::of(node.text(text)).is_some();
    if reserved || head {
        sink.push(Diagnostic::new(
            DiagnosticCode::NameReservedDeclaration,
            node.span(),
            if reserved {
                "a reserved word cannot be declared"
            } else {
                "a reserved type head cannot be declared"
            },
        ));
    }
}

/// Whether a generic associated type or choice has its one shape:
/// `type Name endtype`, or `type Name of T endtype` with one type.
fn assoc_fits(kind: DeclKind, children: &[CstNode], text: &str) -> bool {
    let contents = children
        .get(1..children.len().saturating_sub(1))
        .unwrap_or_default();
    let items = significant(contents);
    let is_name = |node: &&CstNode| {
        matches!(
            node.kind(),
            SyntaxKind::Identifier | SyntaxKind::ReservedWord
        )
    };
    match (kind, items.as_slice()) {
        (DeclKind::AssocType, [name]) => is_name(name),
        (DeclKind::AssocChoice, [name, section]) => {
            let operand = section.children().get(1..).unwrap_or_default();
            let (count, labelled) = entry_count(operand, text);
            is_name(name)
                && section.kind() == SyntaxKind::Section
                && section
                    .children()
                    .first()
                    .is_some_and(|keyword| keyword.text(text) == "of")
                && count == 1
                && !labelled
        }
        _ => false,
    }
}

/// Shapes a generic declaration construct as a declaration of `kind`.
pub(crate) fn shape_declaration(
    mut node: CstNode,
    kind: DeclKind,
    options: SourceOptions,
    text: &str,
    sink: &mut Vec<Diagnostic>,
) -> CstNode {
    let span = node.span();
    let mut children = node.take_children();
    if matches!(kind, DeclKind::AssocType | DeclKind::AssocChoice)
        && !assoc_fits(kind, &children, text)
    {
        sink.push(Diagnostic::new(
            DiagnosticCode::SyntaxInvalidForm,
            span,
            if kind == DeclKind::AssocType {
                "an associated type is `type Name endtype`, with no body and no items"
            } else {
                "an associated type is chosen with `type Name of Type endtype`"
            },
        ));
        return CstNode::interior(SyntaxKind::Error, children, span.start());
    }
    let closer = children.pop();
    let recovered = closer
        .as_ref()
        .is_none_or(|closer| closer.kind() == SyntaxKind::Missing);
    let mut rest = children.into_iter();
    let opener = rest.next();
    let mut stream = Vec::new();
    for mut child in rest {
        if child.kind() == SyntaxKind::Section {
            let mut parts = child.take_children().into_iter();
            if let Some(keyword) = parts.next() {
                stream.push(Item {
                    node: keyword,
                    section_keyword: true,
                });
            }
            stream.extend(parts.map(|node| Item {
                node,
                section_keyword: false,
            }));
        } else {
            stream.push(Item {
                node: child,
                section_keyword: false,
            });
        }
    }
    let mut shaper = Shaper {
        kind,
        options,
        text,
        sink,
        out: Vec::new(),
        malformed: false,
        has_of: false,
        has_case: false,
    };
    shaper.read(stream);
    if kind == DeclKind::Type && shaper.has_of == shaper.has_case {
        shaper.malformed = true;
    }
    if shaper.malformed && !recovered {
        shaper.sink.push(Diagnostic::new(
            DiagnosticCode::SyntaxInvalidForm,
            span,
            match kind {
                DeclKind::Type => {
                    "a type is `type Name [params]`, then `of` and one type or one or more cases, then its items"
                }
                DeclKind::Interface => {
                    "an interface is `interface Name [params]` and its items"
                }
                DeclKind::Impl => "an `impl` names one type and holds its choices and members",
                DeclKind::AssocType | DeclKind::AssocChoice => {
                    "an associated type has one shape"
                }
            },
        ));
    }
    let mut out: Vec<CstNode> = opener.into_iter().collect();
    out.append(&mut shaper.out);
    out.extend(closer);
    CstNode::interior(kind.node_kind(), out, span.start())
}

struct Shaper<'s, 't> {
    kind: DeclKind,
    options: SourceOptions,
    text: &'t str,
    sink: &'s mut Vec<Diagnostic>,
    out: Vec<CstNode>,
    malformed: bool,
    has_of: bool,
    has_case: bool,
}

impl Shaper<'_, '_> {
    /// Whether `item` starts a new part. An expression construct belongs to
    /// the operand of a `where`; anywhere else it is a nested construct.
    fn is_starter(&self, item: &Item, part: &Part) -> bool {
        let node = &item.node;
        if item.section_keyword {
            return true;
        }
        if node.kind() == SyntaxKind::ReservedWord && node.text(self.text) == "case" {
            return true;
        }
        node.kind() == SyntaxKind::Construct
            && (!matches!(part, Part::Section(keyword) if keyword == "where")
                || !is_expression_construct(node, self.text))
    }

    fn read(&mut self, stream: Vec<Item>) {
        let mut items = stream.into_iter().peekable();
        while let Some(item) = items.next_if(|item| is_trivia(&item.node)) {
            self.out.push(item.node);
        }
        if self.kind.has_name() {
            let named = items.peek().is_some_and(|item| {
                !item.section_keyword
                    && matches!(
                        item.node.kind(),
                        SyntaxKind::Identifier | SyntaxKind::ReservedWord
                    )
            });
            if let Some(item) = items.next_if(|_| named) {
                if self.kind != DeclKind::AssocChoice {
                    report_declared_name(&item.node, self.text, self.sink);
                }
                self.out.push(item.node);
                while let Some(item) = items.next_if(|item| is_trivia(&item.node)) {
                    self.out.push(item.node);
                }
                if let Some(item) = items.next_if(|item| {
                    !item.section_keyword && item.node.kind() == SyntaxKind::Paren
                }) {
                    let params = self.params(item.node);
                    self.out.push(params);
                }
            } else {
                self.malformed = true;
            }
        }
        let mut part = if self.kind == DeclKind::Impl {
            Part::Target
        } else {
            Part::Stray
        };
        let mut current: Vec<CstNode> = Vec::new();
        for item in items {
            if !self.is_starter(&item, &part) {
                current.push(item.node);
                continue;
            }
            self.finish(&part, std::mem::take(&mut current));
            let node = item.node;
            if item.section_keyword {
                part = Part::Section(node.text(self.text).to_owned());
                current.push(node);
            } else if node.kind() == SyntaxKind::Construct {
                self.nested(node);
                part = Part::Stray;
            } else {
                part = Part::Case;
                current.push(node);
            }
        }
        self.finish(&part, current);
    }

    /// Shapes `(…)` after the name as the type parameters.
    fn params(&mut self, mut paren: CstNode) -> CstNode {
        let span = paren.span();
        if paren
            .children()
            .last()
            .is_some_and(|last| last.kind() == SyntaxKind::Missing)
        {
            return paren;
        }
        let children = paren.take_children();
        let contents = children
            .get(1..children.len().saturating_sub(1))
            .unwrap_or_default();
        let names = significant(contents);
        let ok = !names.is_empty()
            && contents.iter().all(|node| node.kind() != SyntaxKind::Error)
            && names.iter().all(|node| {
                matches!(
                    node.kind(),
                    SyntaxKind::Identifier | SyntaxKind::ReservedWord
                )
            });
        if ok {
            for name in names {
                report_declared_name(name, self.text, self.sink);
            }
            CstNode::interior(SyntaxKind::TypeParams, children, span.start())
        } else {
            self.sink.push(Diagnostic::new(
                DiagnosticCode::SyntaxInvalidForm,
                span,
                "type parameters are one or more names in `(…)`",
            ));
            CstNode::interior(SyntaxKind::Error, children, span.start())
        }
    }

    /// Ends the part being read with its items; its trailing trivia stays in
    /// the declaration, after the part's node.
    fn finish(&mut self, part: &Part, mut items: Vec<CstNode>) {
        let trivia = split_trailing_trivia(&mut items);
        match part {
            Part::Stray => self.stray(items),
            Part::Target => {
                let list = shape_type_list(items, false, self.text, self.sink);
                if list.entries != 1 || list.labelled {
                    self.malformed = true;
                }
                self.out.extend(list.nodes);
            }
            Part::Section(keyword) => self.section(keyword, items),
            Part::Case => self.case(items),
        }
        self.out.extend(trivia);
    }

    /// Tokens that belong to no part: one `@error` node, and the declaration
    /// is malformed unless they are all earlier recoveries.
    fn stray(&mut self, items: Vec<CstNode>) {
        let Some(span) = content_span(&items) else {
            self.out.extend(items);
            return;
        };
        if significant(&items).is_empty() {
            self.out.extend(items);
            return;
        }
        self.malformed = true;
        let mut items = items;
        let leading: Vec<CstNode> = {
            let keep = items.iter().position(|item| !is_trivia(item)).unwrap_or(0);
            let rest = items.split_off(keep);
            std::mem::replace(&mut items, rest)
        };
        self.out.extend(leading);
        self.out
            .push(CstNode::interior(SyntaxKind::Error, items, span.start()));
    }

    fn error(&mut self, items: Vec<CstNode>) {
        let at = items.first().map_or(0, |item| item.span().start());
        self.out
            .push(CstNode::interior(SyntaxKind::Error, items, at));
    }

    fn section(&mut self, keyword: &str, mut items: Vec<CstNode>) {
        let span = content_span(&items).unwrap_or_else(|| ByteSpan::empty_at(0));
        if keyword == "as" {
            // `as` outside a `use` is already `@syntax.invalidForm` at the `as`.
            self.error(items);
            return;
        }
        if keyword == "of"
            && matches!(self.kind, DeclKind::Type | DeclKind::AssocChoice)
        {
            self.body(items);
            return;
        }
        if !self.kind.admits(keyword)
            || (STANDARD_LIBRARY_SECTIONS.contains(&keyword)
                && !self.options.standard_library)
        {
            self.sink.push(Diagnostic::new(
                DiagnosticCode::SyntaxInvalidSection,
                span,
                if self.kind.admits(keyword) {
                    "this section is admitted only in the embedded standard library"
                } else {
                    "this declaration kind does not admit this section"
                },
            ));
            self.error(items);
            return;
        }
        let text = self.text;
        let operand: Vec<(SyntaxKind, &str)> =
            significant(items.get(1..).unwrap_or_default())
                .into_iter()
                .map(|node| (node.kind(), node.text(text)))
                .collect();
        let invalid = |message: &str| {
            Diagnostic::new(DiagnosticCode::SyntaxInvalidForm, span, message)
        };
        let kind = match keyword {
            "given" => {
                let position = items
                    .iter()
                    .position(|item| item.kind() == SyntaxKind::Brace);
                match (operand.as_slice(), position) {
                    ([(SyntaxKind::Brace, _)], Some(position)) => {
                        let brace = items.remove(position);
                        let shaped = shape_group(brace, Role::Given, text, self.sink);
                        let tail = items.split_off(position);
                        items.extend(shaped);
                        items.extend(tail);
                    }
                    _ => self.sink.push(invalid(
                        "a `given` section is one brace of `Parameter: Bound` entries",
                    )),
                }
                SyntaxKind::Given
            }
            "where" => {
                if operand.is_empty() {
                    self.sink.push(invalid("a `where` section has a predicate"));
                }
                SyntaxKind::Where
            }
            "doc" => {
                if !matches!(operand.as_slice(), [(SyntaxKind::DocString, _)]) {
                    self.sink.push(invalid("a `doc` section is one doc string"));
                }
                SyntaxKind::Doc
            }
            _ => {
                let tags = tag_set(keyword).unwrap_or_default();
                let fits = matches!(operand.as_slice(),
                    [(SyntaxKind::Atom, tag)] if tags.contains(tag));
                if !fits {
                    self.sink.push(invalid(
                        "the operand is one atom of this section's closed tag set",
                    ));
                }
                match keyword {
                    "visibility" => SyntaxKind::Visibility,
                    "representation" => SyntaxKind::Representation,
                    _ => SyntaxKind::Role,
                }
            }
        };
        self.out.push(CstNode::interior(kind, items, span.start()));
    }

    /// The operand of `of`: one type body, written inline in the
    /// declaration.
    fn body(&mut self, mut items: Vec<CstNode>) {
        let repeated = std::mem::replace(&mut self.has_of, true);
        let operand = items.split_off(1.min(items.len()));
        self.out.append(&mut items);
        let body = self.kind == DeclKind::Type;
        let list = shape_type_list(operand, body, self.text, self.sink);
        if !repeated && (list.entries != 1 || list.labelled) {
            self.malformed = true;
        }
        if let Some(intrinsic) = list.intrinsic
            && !self.options.standard_library
        {
            self.sink.push(Diagnostic::new(
                DiagnosticCode::SyntaxInvalidSection,
                intrinsic,
                "`Intrinsic` is admitted only in the embedded standard library",
            ));
        }
        self.out.extend(list.nodes);
    }

    /// `case name [record-type]`.
    fn case(&mut self, mut items: Vec<CstNode>) {
        if self.kind != DeclKind::Type {
            self.malformed = true;
            self.error(items);
            return;
        }
        self.has_case = true;
        let span = content_span(&items).unwrap_or_else(|| ByteSpan::empty_at(0));
        let operand = items.get(1..).unwrap_or_default();
        let fits = match significant(operand).as_slice() {
            [name] => matches!(
                name.kind(),
                SyntaxKind::Identifier | SyntaxKind::ReservedWord
            ),
            [name, fields] => {
                matches!(
                    name.kind(),
                    SyntaxKind::Identifier | SyntaxKind::ReservedWord
                ) && fields.kind() == SyntaxKind::Brace
            }
            _ => false,
        };
        if !fits {
            self.sink.push(Diagnostic::new(
                DiagnosticCode::SyntaxInvalidForm,
                span,
                "a case is `case name` and an optional record type",
            ));
            self.error(items);
            return;
        }
        if let Some(name) = significant(operand).first() {
            report_declared_name(name, self.text, self.sink);
        }
        if let Some(position) = items
            .iter()
            .position(|item| item.kind() == SyntaxKind::Brace)
        {
            let brace = items.remove(position);
            let shaped = shape_group(brace, Role::Record, self.text, self.sink);
            let tail = items.split_off(position);
            items.extend(shaped);
            items.extend(tail);
        }
        self.out
            .push(CstNode::interior(SyntaxKind::Case, items, span.start()));
    }

    /// A construct nested directly in the declaration.
    fn nested(&mut self, node: CstNode) {
        let (keyword, named) = Construct::new(&node, self.text)
            .map_or(("", false), |construct| {
                (construct.keyword(), is_named(construct))
            });
        let kind = match (self.kind, keyword) {
            (_, "fn" | "proc") if named => {
                self.sink.push(Diagnostic::new(
                    DiagnosticCode::ToolUnavailable,
                    node.span(),
                    "the grammar of `fn` and `proc` members is not available in this reader",
                ));
                self.out.push(node);
                return;
            }
            (DeclKind::Type | DeclKind::Interface, "impl") => DeclKind::Impl,
            (DeclKind::Interface, "type") => DeclKind::AssocType,
            (DeclKind::Impl, "type") => DeclKind::AssocChoice,
            (DeclKind::Impl, "impl") => {
                // An `impl` outside a `type` or `interface` is reported by the
                // generic section rules.
                let mut node = node;
                let at = node.span().start();
                let children = node.take_children();
                self.out
                    .push(CstNode::interior(SyntaxKind::Error, children, at));
                return;
            }
            _ => {
                self.sink.push(Diagnostic::new(
                    DiagnosticCode::SyntaxInvalidForm,
                    node.span(),
                    "this declaration kind does not admit this construct",
                ));
                let mut node = node;
                let at = node.span().start();
                let children = node.take_children();
                self.out
                    .push(CstNode::interior(SyntaxKind::Error, children, at));
                return;
            }
        };
        let shaped = shape_declaration(node, kind, self.options, self.text, self.sink);
        self.out.push(shaped);
    }
}

/// The children of a declaration node between its opener and its closer.
fn contents(node: &CstNode) -> &[CstNode] {
    let children = node.children();
    children
        .get(1..children.len().saturating_sub(1))
        .unwrap_or_default()
}

/// The declared name: the first child after the opener, when it is a name.
pub(crate) fn declared_name<'a>(node: &'a CstNode, text: &'a str) -> Option<Ident<'a>> {
    contents(node)
        .iter()
        .find(|child| !is_trivia(child))
        .and_then(|first| name_of(first, text))
}

/// The declared type parameters.
pub(crate) fn declared_params<'a>(node: &'a CstNode, text: &'a str) -> Vec<Ident<'a>> {
    contents(node)
        .iter()
        .find(|child| child.kind() == SyntaxKind::TypeParams)
        .map(|params| {
            params
                .children()
                .iter()
                .filter_map(|child| name_of(child, text))
                .collect()
        })
        .unwrap_or_default()
}

/// The first child of `kind`.
pub(crate) fn child_of_kind(node: &CstNode, kind: SyntaxKind) -> Option<&CstNode> {
    contents(node).iter().find(|child| child.kind() == kind)
}

/// Every child of `kind`, in written order.
pub(crate) fn children_of_kind(node: &CstNode, kind: SyntaxKind) -> Vec<&CstNode> {
    contents(node)
        .iter()
        .filter(|child| child.kind() == kind)
        .collect()
}

/// The named `fn` and `proc` members, as generic constructs until the
/// callable grammar exists.
pub(crate) fn members<'a>(node: &'a CstNode, text: &'a str) -> Vec<Construct<'a>> {
    contents(node)
        .iter()
        .filter_map(|child| Construct::new(child, text))
        .filter(|construct| {
            matches!(construct.keyword(), "fn" | "proc") && is_named(*construct)
        })
        .collect()
}

/// The type written after `of`, inline in a declaration.
pub(crate) fn of_operand<'a>(node: &'a CstNode, text: &'a str) -> Option<TypeExpr<'a>> {
    let items = contents(node);
    let at = items.iter().position(|child| {
        child.kind() == SyntaxKind::ReservedWord && child.text(text) == "of"
    })?;
    let rest = items.get(at + 1..).unwrap_or_default();
    let end = rest
        .iter()
        .position(|child| {
            !is_trivia(child)
                && !matches!(
                    child.kind(),
                    SyntaxKind::Path
                        | SyntaxKind::Paren
                        | SyntaxKind::Brace
                        | SyntaxKind::RecordType
                        | SyntaxKind::Identifier
                        | SyntaxKind::Atom
                )
        })
        .unwrap_or(rest.len());
    type_entries(rest.get(..end).unwrap_or_default(), text)
        .into_iter()
        .next()
        .map(|(_, entry)| entry)
}

/// `type … endtype`.
#[derive(Clone, Copy, Debug)]
pub struct TypeDecl<'a> {
    node: &'a CstNode,
    text: &'a str,
}

/// What a `type` declares.
#[derive(Clone, Debug)]
pub enum TypeBody<'a> {
    /// `of T`: a wrapper, a declared tuple, or a declared record.
    Of(TypeExpr<'a>),
    /// `of Intrinsic(@tag)`, in the embedded standard library.
    Intrinsic(Atom<'a>),
    /// One or more cases: a variant type, the cases in written order.
    Cases(Vec<Case<'a>>),
}

impl<'a> TypeDecl<'a> {
    /// The view of a `@typeDecl` node of the document `text`.
    #[must_use]
    pub fn new(node: &'a CstNode, text: &'a str) -> Option<Self> {
        (node.kind() == SyntaxKind::TypeDecl).then_some(Self { node, text })
    }

    /// The node.
    #[must_use]
    pub const fn node(&self) -> &'a CstNode {
        self.node
    }

    /// The generic construct view.
    #[must_use]
    pub fn construct(&self) -> Construct<'a> {
        Construct::of_declaration(self.node, self.text)
    }

    /// The name.
    #[must_use]
    pub fn name(&self) -> Option<Ident<'a>> {
        declared_name(self.node, self.text)
    }

    /// The type parameters, in written order.
    #[must_use]
    pub fn params(&self) -> Vec<Ident<'a>> {
        declared_params(self.node, self.text)
    }

    /// The body: the type after `of`, the intrinsic tag, or the cases.
    #[must_use]
    pub fn body(&self) -> Option<TypeBody<'a>> {
        let cases = self.cases();
        if !cases.is_empty() {
            return Some(TypeBody::Cases(cases));
        }
        let items = contents(self.node);
        let at = items.iter().position(|child| {
            child.kind() == SyntaxKind::ReservedWord && child.text(self.text) == "of"
        })?;
        let rest = items.get(at + 1..).unwrap_or_default();
        let mut significant = rest.iter().filter(|child| !is_trivia(child));
        if let (Some(head), Some(paren)) = (significant.next(), significant.next())
            && head.kind() == SyntaxKind::Identifier
            && head.text(self.text) == "Intrinsic"
        {
            return contents(paren)
                .iter()
                .find_map(|child| Atom::of(child, self.text))
                .map(TypeBody::Intrinsic);
        }
        of_operand(self.node, self.text).map(TypeBody::Of)
    }

    /// The cases, in written order.
    #[must_use]
    pub fn cases(&self) -> Vec<Case<'a>> {
        children_of_kind(self.node, SyntaxKind::Case)
            .into_iter()
            .map(|node| Case {
                node,
                text: self.text,
            })
            .collect()
    }

    /// The `given` section.
    #[must_use]
    pub fn given(&self) -> Option<Given<'a>> {
        child_of_kind(self.node, SyntaxKind::Given)
            .and_then(|node| Given::new(node, self.text))
    }

    /// The `where` section.
    #[must_use]
    pub fn invariant(&self) -> Option<Where<'a>> {
        child_of_kind(self.node, SyntaxKind::Where)
            .and_then(|node| Where::new(node, self.text))
    }

    /// The `visibility` section.
    #[must_use]
    pub fn visibility(&self) -> Option<TagSection<'a>> {
        child_of_kind(self.node, SyntaxKind::Visibility)
            .and_then(|node| TagSection::new(node, self.text))
    }

    /// The `representation` section.
    #[must_use]
    pub fn representation(&self) -> Option<TagSection<'a>> {
        child_of_kind(self.node, SyntaxKind::Representation)
            .and_then(|node| TagSection::new(node, self.text))
    }

    /// The `role` section.
    #[must_use]
    pub fn role(&self) -> Option<TagSection<'a>> {
        child_of_kind(self.node, SyntaxKind::Role)
            .and_then(|node| TagSection::new(node, self.text))
    }

    /// The `doc` section.
    #[must_use]
    pub fn doc(&self) -> Option<Doc<'a>> {
        child_of_kind(self.node, SyntaxKind::Doc)
            .and_then(|node| Doc::new(node, self.text))
    }

    /// The named `fn` and `proc` members, in written order.
    #[must_use]
    pub fn members(&self) -> Vec<Construct<'a>> {
        members(self.node, self.text)
    }

    /// The `impl` blocks, in written order.
    #[must_use]
    pub fn impls(&self) -> Vec<super::impl_block::Impl<'a>> {
        children_of_kind(self.node, SyntaxKind::Impl)
            .into_iter()
            .filter_map(|node| super::impl_block::Impl::new(node, self.text))
            .collect()
    }
}

/// `case name [record-type]`: one alternative of a variant type.
#[derive(Clone, Copy, Debug)]
pub struct Case<'a> {
    node: &'a CstNode,
    text: &'a str,
}

impl<'a> Case<'a> {
    /// The `@case` node.
    #[must_use]
    pub const fn node(&self) -> &'a CstNode {
        self.node
    }

    /// The case's name.
    #[must_use]
    pub fn name(&self) -> Option<Ident<'a>> {
        self.node
            .children()
            .iter()
            .skip(1)
            .find(|child| !is_trivia(child))
            .and_then(|name| name_of(name, self.text))
    }

    /// The named fields, in written order, or `None` for a case without
    /// fields.
    #[must_use]
    pub fn fields(&self) -> Option<Vec<(Ident<'a>, TypeExpr<'a>)>> {
        let record = self
            .node
            .children()
            .iter()
            .find(|child| child.kind() == SyntaxKind::RecordType)?;
        Some(
            type_entries(contents(record), self.text)
                .into_iter()
                .filter_map(|(label, entry)| Some((label?, entry)))
                .collect(),
        )
    }
}
