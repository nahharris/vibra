//! Shaping of `fn`, `proc`, and `test`, named and anonymous.
//!
//! `docs/spec/01-source-language.md`, "Grammar" and "Declarations":
//!
//! ```text
//! fn-decl     = "fn", identifier, [ type-params ], signature,
//!               { given | requires | ensures | visibility | external
//!               | symbol | native | doc },
//!               [ "do", body ], "endfn" ;
//! proc-decl   = "proc", identifier, [ type-params ], signature,
//!               { given | requires | ensures | performs | visibility
//!               | external | symbol | doc },
//!               [ "do", body ], "endproc" ;
//! signature   = [ "takes", params ], [ "labelled", labelled ],
//!               "returns", identifier, ":", type-expr ;
//! test        = "test", string, [ "takes", params ], [ performs ],
//!               "do", body, "endtest" ;
//! anon-fn     = "fn", [ type-params ], signature,
//!               { given | requires | ensures }, "do", body, "endfn" ;
//! ```
//!
//! Sections come in any order between the head and the closer:
//!
//! - the `takes`, `labelled`, and `returns` sections written next to each
//!   other form one `@signature` node; `do` and its `@body` are children of
//!   the declaration;
//! - a section the form does not admit
//!   ([`super::super::keyword::ADMISSIBLE_SECTIONS`]) is
//!   `@syntax.invalidSection` at the section, and so are an external
//!   declaration and `native` outside the embedded standard library, and a
//!   guard on a member of an `impl`;
//! - the operand of `requires` and `ensures` runs to the next section and
//!   holds the expression constructs written in it; a guard that is not one
//!   expression and an optional `else` expression is `@syntax.invalidForm`
//!   over the section, like `where`;
//! - a missing name or `returns`, tokens that belong to no part, an
//!   `external` without `symbol` or with a body, a `symbol` without
//!   `external`, and a `test` whose `takes` has other than one parameter are
//!   `@syntax.invalidForm` over the form, once;
//! - a construct written between the sections is `@syntax.invalidForm` over
//!   that construct, as in a `type`; an `impl` there is already reported by
//!   the generic section rules;
//! - a `fn` or `proc` without `do` that is neither an abstract member of an
//!   `interface` nor an external definition is `@syntax.invalidForm` at its
//!   closer.
//!
//! A form closed as missing, or holding a group or construct closed as
//! missing, already has its `@syntax.unmatchedDelimiter` and is not checked
//! for completeness. A repeated section, a section after `do`, a missing
//! name before the first section, and `as` are reported by the generic
//! section rules and kept here without another report.

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};

use super::{
    Item, Level, Role, Slot, as_error, construct_keyword, error_node, flatten,
    region_span, run,
};
use crate::cst::{CstNode, SyntaxKind};
use crate::source::callable::CallableKind;
use crate::source::construct::is_trivia;
use crate::source::keyword::{
    IMPLEMENTATION_MEMBER_EXCLUDED, SIGNATURE_SECTIONS, STANDARD_LIBRARY_SECTIONS,
    admissible_sections,
};
use crate::source::type_decl::{report_declared_name, shape_type_params};
use crate::source::types::{Role as TypeRole, shape_group};

/// Where a `fn` or `proc` is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Place {
    /// A top-level form.
    TopLevel,
    /// A member of a `type`.
    TypeMember,
    /// A member of an `interface`, abstract when it has no `do`.
    InterfaceMember,
    /// A member of an `impl`, which declares no guard.
    ImplMember,
    /// An anonymous `fn` or `proc` in expression position.
    Anonymous,
}

/// The grammar a `fn` or `proc` is read in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CallableRole {
    /// `fn` or `proc`.
    pub(crate) kind: CallableKind,
    /// Where it is written.
    pub(crate) place: Place,
    /// Whether the module is an embedded standard-library module.
    pub(crate) standard_library: bool,
}

impl CallableRole {
    /// An anonymous `fn` or `proc`.
    pub(crate) const fn anonymous(kind: CallableKind) -> Self {
        Self {
            kind,
            place: Place::Anonymous,
            standard_library: false,
        }
    }

    /// The row of the admissibility table.
    const fn row(self) -> &'static str {
        match (self.kind, self.place) {
            (CallableKind::Fn, Place::Anonymous) => "anon-fn",
            (CallableKind::Proc, Place::Anonymous) => "anon-proc",
            (CallableKind::Fn, _) => "fn",
            (CallableKind::Proc, _) => "proc",
        }
    }

    const fn node_kind(self) -> SyntaxKind {
        match (self.kind, self.place) {
            (CallableKind::Fn, Place::Anonymous) => SyntaxKind::AnonFn,
            (CallableKind::Proc, Place::Anonymous) => SyntaxKind::AnonProc,
            (CallableKind::Fn, _) => SyntaxKind::FnDecl,
            (CallableKind::Proc, _) => SyntaxKind::ProcDecl,
        }
    }
}

/// The form being read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Form {
    /// A `fn` or `proc`.
    Callable(CallableRole),
    /// A `test`.
    Test,
}

impl Form {
    const fn node_kind(self) -> SyntaxKind {
        match self {
            Self::Callable(role) => role.node_kind(),
            Self::Test => SyntaxKind::Test,
        }
    }

    const fn row(self) -> &'static str {
        match self {
            Self::Callable(role) => role.row(),
            Self::Test => "test",
        }
    }

    const fn standard_library(self) -> bool {
        match self {
            Self::Callable(role) => role.standard_library,
            Self::Test => false,
        }
    }

    const fn place(self) -> Place {
        match self {
            Self::Callable(role) => role.place,
            Self::Test => Place::TopLevel,
        }
    }

    const fn message(self) -> &'static str {
        match self {
            Self::Callable(CallableRole {
                place: Place::Anonymous,
                ..
            }) => {
                "an anonymous callable is `fn` or `proc`, optional type parameters, a signature with `returns`, its guards, and `do` and a body"
            }
            Self::Callable(CallableRole {
                kind: CallableKind::Fn,
                ..
            }) => {
                "a `fn` is `fn name`, optional type parameters, a signature with `returns`, its sections, and `do` and a body"
            }
            Self::Callable(_) => {
                "a `proc` is `proc name`, optional type parameters, a signature with `returns`, its sections, and `do` and a body"
            }
            Self::Test => {
                "a test is `test \"name\"`, an optional `takes` with exactly one parameter, an optional `performs`, and `do` and a body"
            }
        }
    }
}

/// What the sections of one form said.
#[derive(Default)]
struct Parts<'t> {
    seen: Vec<&'t str>,
    returns: bool,
    body: bool,
    /// Whether an `external` section was written.
    writes_external: bool,
    /// Whether an `external` section was accepted: written in the standard
    /// library.
    external: bool,
    /// Whether a `symbol` section was accepted.
    symbol: bool,
    /// The number of parameters of a test's `takes`.
    test_params: Option<usize>,
    /// Whether the form was already reported `@syntax.invalidForm` over its
    /// span by the generic section rules.
    reported: bool,
    after_do: bool,
}

/// Whether a node is a group or a construct closed as missing.
fn closed_as_missing(node: &CstNode) -> bool {
    !node.kind().is_leaf()
        && node
            .children()
            .last()
            .is_some_and(|last| last.kind() == SyntaxKind::Missing)
}

/// The number of entries of a parameter brace before shaping: one `:` at its
/// top level per entry.
fn param_count(brace: &CstNode, text: &str) -> usize {
    brace
        .children()
        .iter()
        .filter(|child| {
            child.kind() == SyntaxKind::Punctuation && child.text(text) == ":"
        })
        .count()
}

/// Reads one `fn`, `proc`, or `test` construct in `form`: the kind it takes,
/// its slots, and where it is.
pub(super) fn visit_form(
    mut children: Vec<CstNode>,
    span: ByteSpan,
    form: Form,
    recovered: bool,
    text: &str,
    sink: &mut Vec<Diagnostic>,
) -> (SyntaxKind, Vec<Slot>, usize) {
    let closer = children.pop();
    let mut rest = children.into_iter();
    let opener = rest.next();
    let start = opener
        .as_ref()
        .map_or(span.start(), |node| node.span().end());
    let items = flatten(rest.collect());
    let inner_recovered = items.iter().any(|item| closed_as_missing(&item.node));
    let writes_external = items
        .iter()
        .any(|item| item.keyword && item.node.text(text) == "external");
    let mut level = Level::new(items, text, start);
    let mut slots: Vec<Slot> = opener.into_iter().map(Slot::Ready).collect();
    let mut parts = Parts {
        writes_external,
        ..Parts::default()
    };
    level.form_head(&mut slots, form, &mut parts);
    level.form_sections(&mut slots, form, &mut parts);
    level.rest(&mut slots);
    let closer_span = closer
        .as_ref()
        .filter(|closer| closer.kind() != SyntaxKind::Missing)
        .map(CstNode::span);
    slots.extend(closer.map(Slot::Ready));
    sink.append(&mut level.diagnostics);
    if recovered || inner_recovered {
        return (form.node_kind(), slots, span.start());
    }
    let mut incomplete = level.malformed;
    match form {
        Form::Callable(_) => {
            incomplete |= !parts.returns;
            if parts.external {
                incomplete |= !parts.symbol || parts.body;
            } else {
                incomplete |= parts.symbol;
            }
        }
        Form::Test => {
            incomplete |=
                !parts.body || parts.test_params.is_some_and(|count| count != 1);
        }
    }
    if incomplete && !parts.reported {
        sink.push(Diagnostic::new(
            DiagnosticCode::SyntaxInvalidForm,
            span,
            form.message(),
        ));
    }
    let abstract_member = form.place() == Place::InterfaceMember;
    if matches!(form, Form::Callable(_))
        && !parts.body
        && !abstract_member
        && !parts.writes_external
        && let Some(closer) = closer_span
    {
        sink.push(Diagnostic::new(
            DiagnosticCode::SyntaxInvalidForm,
            closer,
            "a `fn` or `proc` has `do` and a body unless it is an abstract interface member or an external definition",
        ));
    }
    (form.node_kind(), slots, span.start())
}

impl<'t> Level<'t> {
    /// The next significant item, when it is an operand item of `section`.
    fn peek_in(&self, section: Option<usize>) -> Option<&Item> {
        self.peek_sig()
            .filter(|item| !item.keyword && item.section == section)
    }

    /// The name of a named form, or the string of a `test`, then the type
    /// parameters of a `fn` or `proc`.
    fn form_head(&mut self, out: &mut Vec<Slot>, form: Form, parts: &mut Parts<'t>) {
        let first_is_section = self.peek_sig().is_some_and(|item| item.keyword);
        let named = match form {
            Form::Callable(role) if role.place == Place::Anonymous => true,
            Form::Callable(_) => self.peek_in(None).is_some_and(|item| {
                matches!(
                    item.node.kind(),
                    SyntaxKind::Identifier | SyntaxKind::ReservedWord
                )
            }),
            Form::Test => self
                .peek_in(None)
                .is_some_and(|item| item.node.kind() == SyntaxKind::String),
        };
        let anonymous = form.place() == Place::Anonymous;
        if named && !anonymous {
            self.trivia(out);
            if let Some(name) = self.bump() {
                if form != Form::Test {
                    report_declared_name(&name, self.text, &mut self.diagnostics);
                }
                out.push(Slot::Ready(name));
            }
        } else if !named {
            // A section before the name is reported by the generic section
            // rules, over the whole form.
            if first_is_section && form.place() == Place::TopLevel {
                parts.reported = true;
            } else {
                self.malformed = true;
            }
        }
        if matches!(form, Form::Callable(_))
            && self
                .peek_in(None)
                .is_some_and(|item| item.node.kind() == SyntaxKind::Paren)
        {
            self.trivia(out);
            if let Some(paren) = self.bump() {
                let params = shape_type_params(paren, self.text, &mut self.diagnostics);
                out.push(Slot::Ready(params));
            }
        }
    }

    /// The sections, in written order.
    fn form_sections(
        &mut self,
        out: &mut Vec<Slot>,
        form: Form,
        parts: &mut Parts<'t>,
    ) {
        let mut signature: Vec<Slot> = Vec::new();
        let mut signature_at = 0;
        let flush = |signature: &mut Vec<Slot>, at: usize, out: &mut Vec<Slot>| {
            if !signature.is_empty() {
                out.push(Slot::Wrap(
                    SyntaxKind::Signature,
                    std::mem::take(signature),
                    at,
                ));
            }
        };
        while let Some(item) = self.peek_sig() {
            if !item.keyword {
                flush(&mut signature, signature_at, out);
                if construct_keyword(&item.node, self.text) == Some("impl") {
                    // An `impl` outside a `type` or an `interface` is reported
                    // by the generic section rules.
                    self.trivia(out);
                    if let Some(node) = self.bump() {
                        out.push(Slot::Ready(as_error(node)));
                    }
                    continue;
                }
                if item.node.kind() == SyntaxKind::Construct {
                    // A construct between the sections is a form the
                    // declaration does not admit, reported over itself as in
                    // a `type`.
                    self.trivia(out);
                    if let Some(node) = self.bump() {
                        self.push_diagnostic(
                            DiagnosticCode::SyntaxInvalidForm,
                            node.span(),
                            "this form does not admit this construct",
                        );
                        out.push(Slot::Ready(as_error(node)));
                    }
                    continue;
                }
                self.strays(out, &|_, item| {
                    item.keyword || item.node.kind() == SyntaxKind::Construct
                });
                continue;
            }
            let word = item.node.text(self.text);
            let at = item.node.span().start();
            let repeated = parts.seen.contains(&word);
            if !repeated {
                parts.seen.push(word);
                if parts.after_do {
                    // `do` comes last: reported by the generic section rules.
                    parts.reported = true;
                }
            }
            let callable = matches!(form, Form::Callable(_));
            if callable && !repeated && SIGNATURE_SECTIONS.contains(&word) {
                if signature.is_empty() {
                    self.trivia(out);
                    signature_at = at;
                } else {
                    self.trivia(&mut signature);
                }
                self.signature_section(&mut signature, word, parts);
                continue;
            }
            flush(&mut signature, signature_at, out);
            self.trivia(out);
            if repeated || word == "as" {
                let region = self.region();
                out.push(Slot::Ready(error_node(region)));
                continue;
            }
            match word {
                "do" => {
                    parts.body = true;
                    parts.after_do = true;
                    let section = self.peek_sig().and_then(|item| item.section);
                    self.push_next(out);
                    let outside =
                        move |_: &Level<'_>, item: &Item| item.section != section;
                    self.body(out, &outside);
                    self.strays(out, &outside);
                }
                "takes" if form == Form::Test => self.test_takes(out, parts),
                _ => self.admitted_section(out, form, word, parts),
            }
        }
        flush(&mut signature, signature_at, out);
    }

    /// A section that is not part of the structure of the form: checked
    /// against the admissibility table, then shaped.
    fn admitted_section(
        &mut self,
        out: &mut Vec<Slot>,
        form: Form,
        word: &'t str,
        parts: &mut Parts<'t>,
    ) {
        let admitted = admissible_sections(form.row()).unwrap_or_default();
        let guard = matches!(word, "requires" | "ensures");
        if !admitted.contains(&word) {
            let region = if guard {
                self.guard_region()
            } else {
                self.region()
            };
            self.reject(out, region, "this form does not admit this section");
            return;
        }
        if STANDARD_LIBRARY_SECTIONS.contains(&word) && !form.standard_library() {
            let region = self.region();
            // An external declaration is reported once, at `external`.
            if word == "symbol" && parts.writes_external {
                out.push(Slot::Ready(error_node(region)));
            } else {
                self.reject(
                    out,
                    region,
                    "this section is admitted only in the embedded standard library",
                );
            }
            return;
        }
        if guard
            && form.place() == Place::ImplMember
            && IMPLEMENTATION_MEMBER_EXCLUDED.contains(&word)
        {
            let region = self.guard_region();
            self.reject(out, region, "a member of an `impl` declares no guard");
            return;
        }
        match word {
            "requires" | "ensures" => self.guard(out, word),
            "given" | "performs" => {
                let region = self.region();
                let node = self.group_section(word, region);
                out.push(Slot::Ready(node));
            }
            _ => {
                if word == "external" {
                    parts.external = true;
                }
                if word == "symbol" {
                    parts.symbol = true;
                }
                let region = self.region();
                let node = self.tag_section(word, region);
                out.push(Slot::Ready(node));
            }
        }
    }

    /// Reports `region` `@syntax.invalidSection` and keeps it in an `@error`
    /// node.
    fn reject(&mut self, out: &mut Vec<Slot>, region: Vec<CstNode>, message: &str) {
        let span = region_span(&region);
        self.push_diagnostic(DiagnosticCode::SyntaxInvalidSection, span, message);
        out.push(Slot::Ready(error_node(region)));
    }

    /// Takes a guard section and the items after it up to the next section,
    /// without the trivia before that section.
    fn guard_region(&mut self) -> Vec<CstNode> {
        let mut nodes = self.region();
        let end = (self.pos..self.items.len())
            .take_while(|&index| self.item(index).is_none_or(|item| !item.keyword))
            .filter(|&index| {
                self.item(index).is_some_and(|item| !is_trivia(&item.node))
            })
            .last();
        if let Some(end) = end {
            while self.pos <= end {
                if let Some(node) = self.bump() {
                    nodes.push(node);
                }
            }
        }
        nodes
    }

    /// `requires expr [else expr]` or `ensures expr [else expr]`: the
    /// operand runs to the next section.
    fn guard(&mut self, out: &mut Vec<Slot>, word: &str) {
        let at = self
            .item(self.pos)
            .map_or(0, |item| item.node.span().start());
        let kind = if word == "requires" {
            SyntaxKind::Requires
        } else {
            SyntaxKind::Ensures
        };
        let outer = std::mem::replace(&mut self.malformed, false);
        let mut slots: Vec<Slot> = Vec::new();
        self.push_next(&mut slots);
        if !self.expr(&mut slots) {
            self.malformed = true;
        }
        if self.next_is_word("else") {
            self.push_next(&mut slots);
            if !self.expr(&mut slots) {
                self.malformed = true;
            }
        }
        self.strays(&mut slots, &|_, item| item.keyword);
        let invalid = std::mem::replace(&mut self.malformed, outer);
        if invalid {
            let span = ByteSpan::new(at, self.last_end);
            self.push_diagnostic(
                DiagnosticCode::SyntaxInvalidForm,
                span,
                "a guard is one predicate expression and an optional `else` expression",
            );
        }
        out.push(Slot::Wrap(kind, slots, at));
    }

    /// `given {…}` or `performs {…}`: one brace, shaped as type parameter
    /// bounds or as a row. Any other operand is `@syntax.invalidForm` over
    /// the section.
    fn group_section(&mut self, word: &str, mut region: Vec<CstNode>) -> CstNode {
        let span = region_span(&region);
        let operand: Vec<SyntaxKind> = region
            .iter()
            .skip(1)
            .filter(|node| !is_trivia(node))
            .map(CstNode::kind)
            .collect();
        let (kind, role, message) = if word == "given" {
            (
                SyntaxKind::Given,
                TypeRole::Given,
                "a `given` section is one brace of `Parameter: Bound` entries",
            )
        } else {
            (
                SyntaxKind::Performs,
                TypeRole::Row,
                "a `performs` section is one row, a brace of paths",
            )
        };
        let position = region
            .iter()
            .position(|node| node.kind() == SyntaxKind::Brace);
        match (operand.as_slice(), position) {
            ([SyntaxKind::Brace], Some(position)) => {
                let brace = region.remove(position);
                let tail = region.split_off(position);
                let shaped = shape_group(brace, role, self.text, &mut self.diagnostics);
                region.extend(shaped);
                region.extend(tail);
            }
            _ => self.push_diagnostic(DiagnosticCode::SyntaxInvalidForm, span, message),
        }
        CstNode::interior(kind, region, span.start())
    }

    /// One section of the signature, into the `@signature` slots.
    fn signature_section(
        &mut self,
        out: &mut Vec<Slot>,
        word: &str,
        parts: &mut Parts<'t>,
    ) {
        let section = self.peek_sig().and_then(|item| item.section);
        self.push_next(out);
        match word {
            "takes" | "labelled" => {
                let brace = self
                    .peek_in(section)
                    .is_some_and(|item| item.node.kind() == SyntaxKind::Brace);
                if brace {
                    self.trivia(out);
                    if let Some(node) = self.bump() {
                        let role = if word == "takes" {
                            Role::Params
                        } else {
                            Role::Labelled
                        };
                        out.push(Slot::Visit(node, role));
                    }
                } else {
                    self.malformed = true;
                }
            }
            _ => {
                parts.returns = true;
                let named = self.peek_in(section).is_some_and(|item| {
                    matches!(
                        item.node.kind(),
                        SyntaxKind::Identifier | SyntaxKind::ReservedWord
                    )
                });
                if named {
                    self.trivia(out);
                    if let Some(name) = self.bump() {
                        report_declared_name(&name, self.text, &mut self.diagnostics);
                        out.push(Slot::Ready(name));
                    }
                } else {
                    self.malformed = true;
                }
                if self
                    .peek_in(section)
                    .is_some_and(|item| self.is_mark(item, ":"))
                {
                    self.push_next(out);
                    if self.peek_in(section).is_some() {
                        self.written_type(out);
                    } else {
                        self.malformed = true;
                    }
                } else {
                    self.malformed = true;
                }
            }
        }
        self.strays(out, &move |_, item| item.section != section);
    }

    /// `takes {…}` of a `test`: the keyword and the `@params` node, directly
    /// in the test.
    fn test_takes(&mut self, out: &mut Vec<Slot>, parts: &mut Parts<'t>) {
        let section = self.peek_sig().and_then(|item| item.section);
        self.push_next(out);
        let brace = self
            .peek_in(section)
            .is_some_and(|item| item.node.kind() == SyntaxKind::Brace);
        if brace {
            self.trivia(out);
            if let Some(node) = self.bump() {
                parts.test_params = Some(param_count(&node, self.text));
                out.push(Slot::Visit(node, Role::Params));
            }
        } else {
            self.malformed = true;
        }
        self.strays(out, &move |_, item| item.section != section);
    }

    // Groups.

    /// `pattern: Type` entries.
    pub(super) fn param_entries(&mut self, out: &mut Vec<Slot>) {
        while self.next_sig().is_some() {
            if !self.pattern(out) || !self.next_is_mark(":") {
                self.malformed = true;
                return;
            }
            self.push_next(out);
            self.written_type(out);
            if self.malformed {
                return;
            }
        }
    }

    /// `name: Type be default` entries.
    pub(super) fn labelled_entries(&mut self, out: &mut Vec<Slot>) {
        while self.next_sig().is_some() {
            if !self.next_is_label() {
                self.malformed = true;
                return;
            }
            self.label(out);
            self.written_type(out);
            if self.malformed || !self.next_is_word("be") {
                self.malformed = true;
                return;
            }
            self.push_next(out);
            if !self.expr(out) {
                self.malformed = true;
                return;
            }
        }
    }
}

/// Shapes a named `fn` or `proc` construct, at the top level or a member.
pub(crate) fn shape_callable(
    node: CstNode,
    role: CallableRole,
    text: &str,
    sink: &mut Vec<Diagnostic>,
) -> CstNode {
    shape_one(node, Role::Callable(role), text, sink)
}

/// Shapes a top-level `test` construct.
pub(crate) fn shape_test(
    node: CstNode,
    text: &str,
    sink: &mut Vec<Diagnostic>,
) -> CstNode {
    shape_one(node, Role::Test, text, sink)
}

fn shape_one(
    node: CstNode,
    role: Role,
    text: &str,
    sink: &mut Vec<Diagnostic>,
) -> CstNode {
    let span = node.span();
    run(vec![Slot::Visit(node, role)], text, sink)
        .pop()
        .unwrap_or_else(|| {
            CstNode::interior(SyntaxKind::Error, Vec::new(), span.start())
        })
}
