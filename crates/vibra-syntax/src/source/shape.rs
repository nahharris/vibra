//! Shaping of expressions, statements, bodies, patterns, and `let`.
//!
//! `docs/spec/01-source-language.md`, "Grammar":
//!
//! ```text
//! module-let  = "let", identifier, ":", type-expr, { visibility | doc },
//!               "be", expr, "endlet" ;
//! body        = { statement } ;
//! statement   = let | expr ;
//! let         = "let", pattern, [ ":", type-expr ], "be", expr,
//!               [ "else", body ], "endlet" ;
//! expr        = "return", expr | unary ;
//! unary       = "try", unary | "await", unary | postfix ;
//! postfix     = primary, { "(", { argument }, ")" | ".", identifier } ;
//! argument    = expr | identifier, ":", expr ;
//! if          = "if", expr, "then", body, [ "else", body ], "endif" ;
//! match       = "match", expr, arm, { arm }, "endmatch" ;
//! arm         = "case", pattern, body ;
//! do-block    = "do", body, "enddo" ;
//! pattern     = discard | pattern-literal | path | path, "(", … ")"
//!             | "{", … "}" | "[", { pattern }, "]" ;
//! ```
//!
//! The construct matcher has already matched every construct and bracket.
//! Shaping reads each one again in the grammar of its position and builds the
//! nodes of its productions. The choice productions (`statement`, `expr`,
//! `unary`, `primary`, `literal`, `argument`, `pattern`, `pattern-literal`)
//! are never nodes: `return`, `try`, and `await` are tokens of the enclosing
//! node, and every operand is one `@postfix` node, a bare name or literal
//! included. A bracket form of an expression or a pattern is a generic group,
//! which the tree observation writes as its children.
//!
//! - `(` always applies the form before it, with or without whitespace; a
//!   `(` where an expression or a statement is expected is
//!   `@syntax.invalidForm` once over the balanced parenthesized form.
//! - `{…}` is a record value when its first entry is labelled, then every
//!   entry is; `[…]` is a dict when its first entry has a key, then every
//!   entry has; `[:]` is the empty dict. A pattern brace follows the same
//!   rule, and a constructor pattern is positional or labelled. A form that
//!   breaks its rule is `@syntax.invalidForm` over the form and is kept
//!   unshaped in an `@error` node.
//! - A `let` in an operand position is `@syntax.misplacedBinding` at its
//!   keyword and is still read as a `let`.
//! - A reserved word written as a label is the label, with
//!   `@name.reservedLabel` at it; as a pattern binder or a module value's
//!   name it is the name, with `@name.reservedDeclaration` at it.
//! - An anonymous `fn` or `proc` is read in the callable grammar
//!   ([`callable`]), and so are the named `fn`, `proc`, and `test`
//!   declarations; a named declaration in expression position is
//!   `@syntax.invalidForm` over it.
//! - Any other token a construct does not admit (a missing `then`, `be`, or
//!   expression, a stray separator, a doc string, a float pattern) makes the
//!   smallest enclosing construct `@syntax.invalidForm`, once, over the
//!   construct; the tokens are kept in an `@error` node. A construct closed
//!   as missing already has its `@syntax.unmatchedDelimiter` and is not
//!   checked for completeness.
//!
//! Shaping is iterative: each construct or group is read one level at a
//! time and its nested forms are scheduled on a work stack, so nesting depth
//! is bounded by memory, not by the stack.

mod callable;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};

pub(crate) use callable::{CallableRole, Place, shape_callable, shape_test};

use super::callable::CallableKind;
use super::construct::is_trivia;
use super::keyword::{admissible_sections, tag_set};
use super::types::shape_type_list;
use crate::cst::{CstNode, SyntaxKind};

/// The grammar a construct or a bracket group is read in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    /// A `fn` or `proc`, named or anonymous.
    Callable(CallableRole),
    /// `test … endtest`.
    Test,
    /// `{…}` after `takes`: the parameter patterns and their types.
    Params,
    /// `{…}` after `labelled`: the labelled parameters and their defaults.
    Labelled,
    /// A top-level `let`.
    ModuleLet,
    /// A local `let`.
    Let,
    /// `if … endif`.
    If,
    /// `match … endmatch`.
    Match,
    /// `do … enddo`.
    Do,
    /// `(…)` after a callee.
    Arguments,
    /// `{…}` in expression position: a tuple or a record value.
    Product,
    /// `[…]` in expression position: a list or a dict.
    Collection,
    /// `(…)` after a constructor path in a pattern.
    PatternArguments,
    /// `{…}` in a pattern: a tuple or a record pattern.
    PatternProduct,
    /// `[…]` in a pattern.
    PatternList,
}

/// One item of a shaped list: a finished node, a node still to be read in
/// its role, or nodes still to be wrapped in one interior node.
enum Slot {
    Ready(CstNode),
    Visit(CstNode, Role),
    Wrap(SyntaxKind, Vec<Slot>, usize),
}

impl Slot {
    /// The nodes of the slot as they were written, unshaped.
    fn unshape(self, out: &mut Vec<CstNode>) {
        match self {
            Self::Ready(node) | Self::Visit(node, _) => out.push(node),
            Self::Wrap(_, slots, _) => {
                for slot in slots {
                    slot.unshape(out);
                }
            }
        }
    }
}

/// One item of a construct's contents with its sections opened.
struct Item {
    node: CstNode,
    /// The section the item was written in, numbered in written order.
    section: Option<usize>,
    /// Whether the item is that section's keyword.
    keyword: bool,
}

/// Opens the sections of a construct's contents into one stream.
fn flatten(children: Vec<CstNode>) -> Vec<Item> {
    let mut items = Vec::with_capacity(children.len());
    let mut sections = 0;
    for mut child in children {
        if child.kind() == SyntaxKind::Section {
            for (index, part) in child.take_children().into_iter().enumerate() {
                items.push(Item {
                    node: part,
                    section: Some(sections),
                    keyword: index == 0,
                });
            }
            sections += 1;
        } else {
            items.push(Item {
                node: child,
                section: None,
                keyword: false,
            });
        }
    }
    items
}

/// An interior `@error` node holding `nodes`.
fn error_node(nodes: Vec<CstNode>) -> CstNode {
    let at = nodes.first().map_or(0, |node| node.span().start());
    CstNode::interior(SyntaxKind::Error, nodes, at)
}

/// The same tokens, in an `@error` node: a construct or a group consumed
/// without building a form.
fn as_error(mut node: CstNode) -> CstNode {
    let at = node.span().start();
    let children = node.take_children();
    CstNode::interior(SyntaxKind::Error, children, at)
}

/// The opener keyword of a generic construct node.
fn construct_keyword<'t>(node: &CstNode, text: &'t str) -> Option<&'t str> {
    (node.kind() == SyntaxKind::Construct)
        .then(|| node.children().first().map(|keyword| keyword.text(text)))
        .flatten()
}

/// Whether a generic `fn` or `proc` construct is named.
fn is_named_callable(node: &CstNode) -> bool {
    let children = node.children();
    children
        .get(1..children.len().saturating_sub(1))
        .unwrap_or_default()
        .iter()
        .find(|child| !is_trivia(child))
        .is_some_and(|first| {
            matches!(
                first.kind(),
                SyntaxKind::Identifier | SyntaxKind::ReservedWord
            )
        })
}

/// How an expression's first item starts it.
enum Start {
    /// A primary, which suffixes may follow.
    Primary(Slot),
    /// A form that takes the place of the expression and admits no suffix:
    /// a misplaced `let`, or a form kept in an `@error` node.
    Alone(Slot),
}

/// One level being read: the items of one construct, group, or operand.
struct Level<'t> {
    text: &'t str,
    items: Vec<Option<Item>>,
    pos: usize,
    /// The end of the last significant item read, where an empty body is.
    last_end: usize,
    diagnostics: Vec<Diagnostic>,
    malformed: bool,
}

impl<'t> Level<'t> {
    fn new(items: Vec<Item>, text: &'t str, start: usize) -> Self {
        Self {
            text,
            items: items.into_iter().map(Some).collect(),
            pos: 0,
            last_end: start,
            diagnostics: Vec::new(),
            malformed: false,
        }
    }

    fn item(&self, index: usize) -> Option<&Item> {
        self.items.get(index).and_then(Option::as_ref)
    }

    /// The index of the first significant item at or after `from`.
    fn sig_from(&self, from: usize) -> Option<usize> {
        (from..self.items.len())
            .find(|&index| self.item(index).is_some_and(|item| !is_trivia(&item.node)))
    }

    /// The next significant item.
    fn next_sig(&self) -> Option<usize> {
        self.sig_from(self.pos)
    }

    fn peek_sig(&self) -> Option<&Item> {
        self.next_sig().and_then(|index| self.item(index))
    }

    /// Takes the item at the read position.
    fn bump(&mut self) -> Option<CstNode> {
        let item = self.items.get_mut(self.pos).and_then(Option::take)?;
        self.pos += 1;
        if !is_trivia(&item.node) {
            self.last_end = item.node.span().end();
        }
        Some(item.node)
    }

    /// Moves the trivia at the read position to `out`.
    fn trivia(&mut self, out: &mut Vec<Slot>) {
        while self
            .item(self.pos)
            .is_some_and(|item| is_trivia(&item.node))
        {
            if let Some(node) = self.bump() {
                out.push(Slot::Ready(node));
            }
        }
    }

    /// Moves the trivia and the next significant item to `out`.
    fn push_next(&mut self, out: &mut Vec<Slot>) {
        self.trivia(out);
        if let Some(node) = self.bump() {
            out.push(Slot::Ready(node));
        }
    }

    fn is_word(&self, item: &Item, word: &str) -> bool {
        !item.keyword
            && item.node.kind() == SyntaxKind::ReservedWord
            && item.node.text(self.text) == word
    }

    fn is_mark(&self, item: &Item, mark: &str) -> bool {
        !item.keyword
            && item.node.kind() == SyntaxKind::Punctuation
            && item.node.text(self.text) == mark
    }

    fn next_is_word(&self, word: &str) -> bool {
        self.peek_sig().is_some_and(|item| self.is_word(item, word))
    }

    fn next_is_mark(&self, mark: &str) -> bool {
        self.peek_sig().is_some_and(|item| self.is_mark(item, mark))
    }

    /// Whether the next significant item is a name followed by `:`: a label.
    fn next_is_label(&self) -> bool {
        let Some(at) = self.next_sig() else {
            return false;
        };
        self.item(at).is_some_and(|item| {
            !item.keyword
                && matches!(
                    item.node.kind(),
                    SyntaxKind::Identifier | SyntaxKind::ReservedWord
                )
        }) && self
            .sig_from(at + 1)
            .and_then(|colon| self.item(colon))
            .is_some_and(|colon| self.is_mark(colon, ":"))
    }

    fn push_diagnostic(&mut self, code: DiagnosticCode, span: ByteSpan, message: &str) {
        self.diagnostics.push(Diagnostic::new(code, span, message));
    }

    /// Reads a label and its `:`; a reserved word is the label, reported.
    fn label(&mut self, out: &mut Vec<Slot>) {
        self.trivia(out);
        if let Some(word) = self.bump() {
            if word.kind() == SyntaxKind::ReservedWord {
                self.push_diagnostic(
                    DiagnosticCode::NameReservedLabel,
                    word.span(),
                    "a reserved word cannot be a label",
                );
            }
            out.push(Slot::Ready(word));
        }
        self.push_next(out);
    }

    /// Takes the items of the section at the read position.
    fn region(&mut self) -> Vec<CstNode> {
        let section = self.item(self.pos).and_then(|item| item.section);
        let mut nodes = Vec::new();
        while section.is_some()
            && self
                .item(self.pos)
                .is_some_and(|item| item.section == section)
        {
            if let Some(node) = self.bump() {
                nodes.push(node);
            }
        }
        nodes
    }

    /// Reads tokens the level does not admit up to the next item `stop`
    /// accepts, into one `@error` node; the level is malformed when there
    /// were any. An `as` section, already reported where it is written, is
    /// kept the same way without making the level malformed.
    fn strays(&mut self, out: &mut Vec<Slot>, stop: &dyn Fn(&Self, &Item) -> bool) {
        let mut stray: Vec<CstNode> = Vec::new();
        while let Some(at) = self.next_sig() {
            let Some(item) = self.item(at) else {
                break;
            };
            if stop(self, item) {
                break;
            }
            if stray.is_empty() {
                self.trivia(out);
            } else {
                while self.pos < at {
                    if let Some(node) = self.bump() {
                        stray.push(node);
                    }
                }
            }
            self.take_stray(&mut stray);
        }
        if !stray.is_empty() {
            out.push(Slot::Ready(error_node(stray)));
        }
    }

    /// Takes one stray item, or the whole section it starts, into `stray`.
    fn take_stray(&mut self, stray: &mut Vec<CstNode>) {
        let Some(item) = self.item(self.pos) else {
            return;
        };
        let earlier =
            matches!(item.node.kind(), SyntaxKind::Error | SyntaxKind::Missing);
        if item.keyword {
            let as_section = item.node.text(self.text) == "as";
            stray.extend(self.region());
            self.malformed |= !as_section;
            return;
        }
        if let Some(node) = self.bump() {
            stray.push(node);
        }
        self.malformed |= !earlier;
    }

    /// Reads every remaining significant item into one `@error` node.
    fn rest(&mut self, out: &mut Vec<Slot>) {
        self.strays(out, &|_, _| false);
        self.trivia(out);
    }

    /// Reads one expression at the next significant item; trivia before it
    /// goes to `out`. Returns whether an expression was read. A `return`,
    /// `try`, or `await` with no operand is kept in an `@error` node and
    /// makes the level malformed, and so does a `return` after `try` or
    /// `await`: `return` starts an `expr`, never a `unary`.
    fn expr(&mut self, out: &mut Vec<Slot>) -> bool {
        let mut prefix: Vec<Slot> = Vec::new();
        let mut unary = false;
        while let Some(item) = self.peek_sig() {
            let returns = self.is_word(item, "return");
            let is_unary = self.is_word(item, "try") || self.is_word(item, "await");
            if !returns && !is_unary {
                break;
            }
            if returns && unary {
                self.malformed = true;
            }
            unary |= is_unary;
            if prefix.is_empty() {
                self.trivia(out);
            } else {
                self.trivia(&mut prefix);
            }
            if let Some(node) = self.bump() {
                prefix.push(Slot::Ready(node));
            }
        }
        if prefix.is_empty() {
            self.trivia(out);
        }
        let start = match self.peek_sig() {
            Some(item) if self.can_start_primary(item) => {
                out.append(&mut prefix);
                self.trivia(out);
                self.primary()
            }
            _ => None,
        };
        let Some(start) = start else {
            if !prefix.is_empty() {
                let mut nodes = Vec::new();
                for slot in prefix {
                    slot.unshape(&mut nodes);
                }
                out.push(Slot::Ready(error_node(nodes)));
                self.malformed = true;
            }
            return false;
        };
        match start {
            Start::Alone(slot) => out.push(slot),
            Start::Primary(slot) => {
                let at = self.last_end;
                let mut chain = vec![slot];
                self.suffixes(&mut chain);
                out.push(Slot::Wrap(SyntaxKind::Postfix, chain, at));
            }
        }
        true
    }

    /// Whether `item` starts a primary or a form that stands for one.
    fn can_start_primary(&self, item: &Item) -> bool {
        if item.keyword {
            return false;
        }
        match item.node.kind() {
            SyntaxKind::String
            | SyntaxKind::Character
            | SyntaxKind::Integer
            | SyntaxKind::Float
            | SyntaxKind::Identifier
            | SyntaxKind::Atom
            | SyntaxKind::Brace
            | SyntaxKind::Bracket
            | SyntaxKind::Paren
            | SyntaxKind::Construct => true,
            SyntaxKind::ReservedWord => item.node.text(self.text) == "void",
            _ => false,
        }
    }

    /// Reads the primary at the read position, which
    /// [`Self::can_start_primary`] accepted.
    fn primary(&mut self) -> Option<Start> {
        let node = self.bump()?;
        let text = self.text;
        Some(match node.kind() {
            SyntaxKind::Brace => Start::Primary(Slot::Visit(node, Role::Product)),
            SyntaxKind::Bracket => Start::Primary(Slot::Visit(node, Role::Collection)),
            SyntaxKind::Paren => {
                self.push_diagnostic(
                    DiagnosticCode::SyntaxInvalidForm,
                    node.span(),
                    "`(` never starts an expression: there are no grouping parentheses",
                );
                Start::Alone(Slot::Ready(as_error(node)))
            }
            SyntaxKind::Construct => match construct_keyword(&node, text) {
                Some("if") => Start::Primary(Slot::Visit(node, Role::If)),
                Some("match") => Start::Primary(Slot::Visit(node, Role::Match)),
                Some("do") => Start::Primary(Slot::Visit(node, Role::Do)),
                Some("let") => {
                    let keyword =
                        node.children().first().map_or(node.span(), CstNode::span);
                    self.push_diagnostic(
                        DiagnosticCode::SyntaxMisplacedBinding,
                        keyword,
                        "a `let` is a statement; a `do` block holds one where an expression is required",
                    );
                    Start::Alone(Slot::Visit(node, Role::Let))
                }
                Some(word @ ("fn" | "proc")) if !is_named_callable(&node) => {
                    let kind = if word == "fn" {
                        CallableKind::Fn
                    } else {
                        CallableKind::Proc
                    };
                    let role = CallableRole::anonymous(kind);
                    Start::Primary(Slot::Visit(node, Role::Callable(role)))
                }
                // An `impl` outside a `type` or an `interface` is reported by
                // the generic section rules.
                Some("impl") => Start::Alone(Slot::Ready(as_error(node))),
                _ => {
                    self.push_diagnostic(
                        DiagnosticCode::SyntaxInvalidForm,
                        node.span(),
                        "a declaration is not an expression",
                    );
                    Start::Alone(Slot::Ready(as_error(node)))
                }
            },
            _ => Start::Primary(Slot::Ready(node)),
        })
    }

    /// Reads the applications and field reads after a primary into `chain`.
    fn suffixes(&mut self, chain: &mut Vec<Slot>) {
        while let Some(at) = self.next_sig() {
            let Some(item) = self.item(at) else {
                break;
            };
            if item.keyword {
                break;
            }
            if item.node.kind() == SyntaxKind::Paren {
                self.trivia(chain);
                if let Some(paren) = self.bump() {
                    chain.push(Slot::Visit(paren, Role::Arguments));
                }
                continue;
            }
            let field = self.is_mark(item, ".")
                && self
                    .sig_from(at + 1)
                    .and_then(|name| self.item(name))
                    .is_some_and(|name| {
                        !name.keyword && name.node.kind() == SyntaxKind::Identifier
                    });
            if !field {
                break;
            }
            self.push_next(chain);
            self.push_next(chain);
        }
    }

    /// Whether `item` starts a statement.
    fn starts_statement(&self, item: &Item) -> bool {
        self.can_start_primary(item)
            || ["return", "try", "await"]
                .iter()
                .any(|word| self.is_word(item, word))
    }

    /// Reads one statement at the next significant item.
    fn statement(&mut self, out: &mut Vec<Slot>) -> bool {
        let is_let = self.peek_sig().is_some_and(|item| {
            !item.keyword && construct_keyword(&item.node, self.text) == Some("let")
        });
        if is_let {
            self.trivia(out);
            if let Some(node) = self.bump() {
                out.push(Slot::Visit(node, Role::Let));
            }
            return true;
        }
        self.expr(out)
    }

    /// Reads a body up to an item `stop` accepts, or to the end, into one
    /// `@body` slot. The trivia before the first statement goes to `out`
    /// and the trivia after the last stays unread, so the body runs from its
    /// first statement to its last; an empty body sits after the token
    /// before it.
    fn body(&mut self, out: &mut Vec<Slot>, stop: &dyn Fn(&Self, &Item) -> bool) {
        let starts = self.peek_sig().is_some_and(|item| !stop(self, item));
        if !starts {
            out.push(Slot::Wrap(SyntaxKind::Body, Vec::new(), self.last_end));
            return;
        }
        self.trivia(out);
        let mut statements: Vec<Slot> = Vec::new();
        while let Some(item) = self.peek_sig() {
            if stop(self, item) {
                break;
            }
            let statement = self.starts_statement(item);
            if !statements.is_empty() {
                self.trivia(&mut statements);
            }
            if statement {
                self.statement(&mut statements);
            } else {
                let not_statement = |level: &Self, item: &Item| {
                    stop(level, item) || level.starts_statement(item)
                };
                self.strays(&mut statements, &not_statement);
            }
        }
        out.push(Slot::Wrap(SyntaxKind::Body, statements, self.last_end));
    }

    /// Reads one pattern at the next significant item; trivia before it goes
    /// to `out`. Returns whether a pattern was read.
    fn pattern(&mut self, out: &mut Vec<Slot>) -> bool {
        let Some(item) = self.peek_sig() else {
            return false;
        };
        if item.keyword {
            return false;
        }
        let text = self.text;
        let kind = item.node.kind();
        let word = item.node.text(text);
        match kind {
            SyntaxKind::Punctuation if word == "_" => {}
            SyntaxKind::String
            | SyntaxKind::Character
            | SyntaxKind::Integer
            | SyntaxKind::Atom => {}
            SyntaxKind::Identifier => {
                self.path(out);
                return true;
            }
            SyntaxKind::ReservedWord if word != "void" => {
                self.trivia(out);
                if let Some(name) = self.bump() {
                    self.push_diagnostic(
                        DiagnosticCode::NameReservedDeclaration,
                        name.span(),
                        "a reserved word cannot be bound",
                    );
                    let at = name.span().start();
                    out.push(Slot::Wrap(SyntaxKind::Path, vec![Slot::Ready(name)], at));
                }
                return true;
            }
            SyntaxKind::Brace | SyntaxKind::Bracket => {
                self.trivia(out);
                if let Some(group) = self.bump() {
                    let role = if kind == SyntaxKind::Brace {
                        Role::PatternProduct
                    } else {
                        Role::PatternList
                    };
                    out.push(Slot::Visit(group, role));
                }
                return true;
            }
            _ => return false,
        }
        self.push_next(out);
        true
    }

    /// Reads a dotted name as a `@path`, and the `(…)` applied to it.
    fn path(&mut self, out: &mut Vec<Slot>) {
        self.trivia(out);
        let at = self
            .item(self.pos)
            .map_or(0, |item| item.node.span().start());
        let mut parts: Vec<Slot> = Vec::new();
        if let Some(first) = self.bump() {
            parts.push(Slot::Ready(first));
        }
        while let Some(dot) = self.next_sig() {
            let continues = self.item(dot).is_some_and(|item| self.is_mark(item, "."))
                && self
                    .sig_from(dot + 1)
                    .and_then(|name| self.item(name))
                    .is_some_and(|name| {
                        !name.keyword && name.node.kind() == SyntaxKind::Identifier
                    });
            if !continues {
                break;
            }
            self.push_next(&mut parts);
            self.push_next(&mut parts);
        }
        out.push(Slot::Wrap(SyntaxKind::Path, parts, at));
        let applied = self
            .peek_sig()
            .is_some_and(|item| !item.keyword && item.node.kind() == SyntaxKind::Paren);
        if applied {
            self.trivia(out);
            if let Some(paren) = self.bump() {
                out.push(Slot::Visit(paren, Role::PatternArguments));
            }
        }
    }

    /// The end of the type expression at the next significant item: a
    /// dotted name and the `(…)` applied to it, or one atom or brace form.
    fn type_end(&self) -> Option<usize> {
        let first = self.next_sig()?;
        let item = self.item(first)?;
        if item.keyword {
            return None;
        }
        match item.node.kind() {
            SyntaxKind::Atom | SyntaxKind::Brace => Some(first + 1),
            SyntaxKind::Identifier => {
                let mut last = first;
                while let Some(dot) = self.sig_from(last + 1) {
                    let Some(name) = self.sig_from(dot + 1) else {
                        break;
                    };
                    let continues = self
                        .item(dot)
                        .is_some_and(|dot| self.is_mark(dot, "."))
                        && self.item(name).is_some_and(|name| {
                            !name.keyword && name.node.kind() == SyntaxKind::Identifier
                        });
                    if !continues {
                        break;
                    }
                    last = name;
                }
                if let Some(paren) = self.sig_from(last + 1)
                    && self.item(paren).is_some_and(|item| {
                        !item.keyword && item.node.kind() == SyntaxKind::Paren
                    })
                {
                    last = paren;
                }
                Some(last + 1)
            }
            _ => None,
        }
    }

    /// Reads one type expression into `out`; the level is malformed when
    /// there is none.
    fn written_type(&mut self, out: &mut Vec<Slot>) {
        self.trivia(out);
        let Some(end) = self.type_end() else {
            self.malformed = true;
            return;
        };
        let mut nodes = Vec::new();
        while self.pos < end {
            if let Some(node) = self.bump() {
                nodes.push(node);
            }
        }
        let list = shape_type_list(nodes, false, self.text, &mut self.diagnostics);
        out.extend(list.nodes.into_iter().map(Slot::Ready));
    }

    // Constructs.

    fn if_parts(&mut self, out: &mut Vec<Slot>) {
        if !self.expr(out) {
            self.malformed = true;
        }
        self.strays(out, &|level, item| {
            level.is_word(item, "then") || level.is_word(item, "else")
        });
        if self.next_is_word("then") {
            self.push_next(out);
        } else {
            self.malformed = true;
        }
        self.body(out, &|level, item| level.is_word(item, "else"));
        if self.next_is_word("else") {
            self.push_next(out);
            self.body(out, &|_, _| false);
        }
    }

    fn match_parts(&mut self, out: &mut Vec<Slot>) {
        if !self.expr(out) {
            self.malformed = true;
        }
        let is_case = |level: &Self, item: &Item| level.is_word(item, "case");
        self.strays(out, &is_case);
        let mut arms = 0;
        while self.next_is_word("case") {
            self.trivia(out);
            let at = self
                .item(self.pos)
                .map_or(0, |item| item.node.span().start());
            let mut arm: Vec<Slot> = Vec::new();
            self.push_next(&mut arm);
            if !self.pattern(&mut arm) {
                self.malformed = true;
                let not_pattern = self
                    .peek_sig()
                    .is_some_and(|item| !self.is_word(item, "case"));
                if not_pattern {
                    self.trivia(&mut arm);
                    let mut stray = Vec::new();
                    self.take_stray(&mut stray);
                    arm.push(Slot::Ready(error_node(stray)));
                    self.malformed = true;
                }
            }
            self.body(&mut arm, &is_case);
            out.push(Slot::Wrap(SyntaxKind::Arm, arm, at));
            arms += 1;
        }
        if arms == 0 {
            self.malformed = true;
        }
    }

    fn do_parts(&mut self, out: &mut Vec<Slot>) {
        self.body(out, &|_, _| false);
    }

    fn let_parts(&mut self, out: &mut Vec<Slot>) {
        if !self.pattern(out) {
            self.malformed = true;
            self.strays(out, &|level, item| item.keyword || level.is_mark(item, ":"));
        }
        if self.next_is_mark(":") {
            self.push_next(out);
            self.written_type(out);
        }
        let has_be = self
            .peek_sig()
            .is_some_and(|item| item.keyword && item.node.text(self.text) == "be");
        if !has_be {
            self.malformed = true;
            self.rest(out);
            return;
        }
        self.push_next(out);
        if !self.expr(out) {
            self.malformed = true;
        }
        if self.next_is_word("else") {
            self.push_next(out);
            self.body(out, &|_, _| false);
            return;
        }
        self.sections(out, &[]);
    }

    fn module_let_parts(&mut self, out: &mut Vec<Slot>) {
        let named = self.peek_sig().is_some_and(|item| {
            !item.keyword
                && matches!(
                    item.node.kind(),
                    SyntaxKind::Identifier | SyntaxKind::ReservedWord
                )
        });
        if !named {
            self.malformed = true;
            self.rest(out);
            return;
        }
        self.trivia(out);
        if let Some(name) = self.bump() {
            if name.kind() == SyntaxKind::ReservedWord {
                self.push_diagnostic(
                    DiagnosticCode::NameReservedDeclaration,
                    name.span(),
                    "a reserved word cannot be declared",
                );
            }
            out.push(Slot::Ready(name));
        }
        if !self.next_is_mark(":") {
            self.malformed = true;
            self.rest(out);
            return;
        }
        self.push_next(out);
        self.written_type(out);
        if self.malformed {
            self.rest(out);
            return;
        }
        self.sections(out, admissible_sections("let").unwrap_or_default());
    }

    /// Reads the sections of a `let` after its head: `be` and its
    /// expression, the sections of `admitted`, and nothing else. A section
    /// that is repeated is already `@syntax.duplicateSection`, and `as` is
    /// already `@syntax.invalidForm`; another section is
    /// `@syntax.invalidSection` over the section.
    fn sections(&mut self, out: &mut Vec<Slot>, admitted: &[&str]) {
        let mut seen: Vec<String> = Vec::new();
        // A local `let` has read its `be` already.
        if admitted.is_empty() {
            seen.push("be".to_owned());
        }
        while let Some(item) = self.peek_sig() {
            if !item.keyword {
                self.strays(out, &|_, item| item.keyword);
                continue;
            }
            let word = item.node.text(self.text).to_owned();
            self.trivia(out);
            let repeated = seen.contains(&word);
            if word == "be" && !repeated {
                seen.push(word);
                self.push_next(out);
                if !self.expr(out) {
                    self.malformed = true;
                }
                continue;
            }
            let region = self.region();
            if repeated || word == "as" {
                out.push(Slot::Ready(error_node(region)));
            } else if admitted.contains(&word.as_str()) {
                seen.push(word.clone());
                out.push(Slot::Ready(self.tag_section(&word, region)));
            } else {
                let span = region_span(&region);
                self.push_diagnostic(
                    DiagnosticCode::SyntaxInvalidSection,
                    span,
                    "a `let` admits only `be`, and a module value `visibility` and `doc`",
                );
                out.push(Slot::Ready(error_node(region)));
            }
        }
        if !seen.iter().any(|word| word == "be") {
            self.malformed = true;
        }
    }

    /// Shapes a section whose operand is one token: `doc` and its doc
    /// string, `symbol` or `native` and a string, or a tag section
    /// (`visibility`, `external`) and one atom of its closed set. Any other
    /// operand is `@syntax.invalidForm` over the section.
    fn tag_section(&mut self, keyword: &str, region: Vec<CstNode>) -> CstNode {
        let span = region_span(&region);
        let text = self.text;
        let operand: Vec<&CstNode> = region
            .iter()
            .skip(1)
            .filter(|node| !is_trivia(node))
            .collect();
        let one = |kind: SyntaxKind| matches!(operand.as_slice(), [token] if token.kind() == kind);
        let (kind, fits, message) = match keyword {
            "doc" => (
                SyntaxKind::Doc,
                one(SyntaxKind::DocString),
                "a `doc` section is one doc string",
            ),
            "symbol" | "native" => (
                if keyword == "symbol" {
                    SyntaxKind::Symbol
                } else {
                    SyntaxKind::Native
                },
                one(SyntaxKind::String),
                "this section is one string",
            ),
            _ => {
                let tags = tag_set(keyword).unwrap_or_default();
                (
                    if keyword == "external" {
                        SyntaxKind::External
                    } else {
                        SyntaxKind::Visibility
                    },
                    matches!(operand.as_slice(),
                        [tag] if tag.kind() == SyntaxKind::Atom && tags.contains(&tag.text(text))),
                    "the operand is one atom of this section's closed tag set",
                )
            }
        };
        if !fits {
            self.push_diagnostic(DiagnosticCode::SyntaxInvalidForm, span, message);
        }
        CstNode::interior(kind, region, span.start())
    }

    // Groups.

    fn argument_entries(&mut self, out: &mut Vec<Slot>) {
        while self.next_sig().is_some() {
            if self.next_is_label() {
                self.label(out);
            }
            if !self.expr(out) {
                self.malformed = true;
                return;
            }
        }
    }

    fn product_entries(&mut self, out: &mut Vec<Slot>) {
        let record = self.next_is_label();
        while self.next_sig().is_some() {
            if self.next_is_label() != record {
                self.malformed = true;
                return;
            }
            if record {
                self.label(out);
            }
            if !self.expr(out) {
                self.malformed = true;
                return;
            }
        }
    }

    fn collection_entries(&mut self, out: &mut Vec<Slot>) {
        if self.next_is_mark(":") {
            self.push_next(out);
            if self.next_sig().is_some() {
                self.malformed = true;
            }
            return;
        }
        let mut dict = None;
        while self.next_sig().is_some() {
            if !self.expr(out) {
                self.malformed = true;
                return;
            }
            let keyed = self.next_is_mark(":");
            if *dict.get_or_insert(keyed) != keyed {
                self.malformed = true;
                return;
            }
            if keyed {
                self.push_next(out);
                if !self.expr(out) {
                    self.malformed = true;
                    return;
                }
            }
        }
    }

    fn pattern_entries(&mut self, out: &mut Vec<Slot>, labels: bool) {
        let labelled = self.next_is_label();
        while self.next_sig().is_some() {
            if self.next_is_label() != labelled || (labelled && !labels) {
                self.malformed = true;
                return;
            }
            if labelled {
                self.label(out);
            }
            if !self.pattern(out) {
                self.malformed = true;
                return;
            }
        }
    }
}

/// The span of a section's items.
fn region_span(region: &[CstNode]) -> ByteSpan {
    let start = region.first().map_or(0, |node| node.span().start());
    let end = region
        .iter()
        .rev()
        .find(|node| !is_trivia(node))
        .map_or(start, |node| node.span().end());
    ByteSpan::new(start, end)
}

/// The node kind and the message of a construct role.
const fn construct_role(role: Role) -> Option<(SyntaxKind, &'static str)> {
    Some(match role {
        Role::ModuleLet => (
            SyntaxKind::ModuleLet,
            "a module value is `let name: Type`, its `visibility` and `doc`, `be` and one expression, and `endlet`",
        ),
        Role::Let => (
            SyntaxKind::Let,
            "a `let` is `let pattern [: Type] be expression [else body] endlet`",
        ),
        Role::If => (
            SyntaxKind::If,
            "an `if` is `if condition then body [else body] endif`",
        ),
        Role::Match => (
            SyntaxKind::Match,
            "a `match` is `match subject`, one or more `case pattern body` arms, and `endmatch`",
        ),
        Role::Do => (SyntaxKind::DoBlock, "a `do` block holds statements only"),
        _ => return None,
    })
}

/// The message of a group role that breaks its rule.
const fn group_message(role: Role) -> &'static str {
    match role {
        Role::Product => {
            "a brace value is all positional (a tuple) or all labelled (a record)"
        }
        Role::Collection => {
            "a bracket value is all elements (a list) or all `key: value` entries (a dict)"
        }
        Role::PatternArguments => {
            "a constructor pattern is all positional or all labelled patterns"
        }
        Role::PatternProduct => {
            "a brace pattern is all positional (a tuple) or all labelled (a record)"
        }
        Role::PatternList => "a list pattern holds patterns only",
        Role::Params => "parameters are `pattern: Type` entries",
        Role::Labelled => "labelled parameters are `name: Type be default` entries",
        _ => "arguments are expressions, each with an optional `label:`",
    }
}

/// Reads one construct or group in `role`: the kind it takes, its slots,
/// and where it is.
fn visit(
    mut node: CstNode,
    role: Role,
    text: &str,
    sink: &mut Vec<Diagnostic>,
) -> (SyntaxKind, Vec<Slot>, usize) {
    let span = node.span();
    let kind = node.kind();
    let mut children = node.take_children();
    let recovered = children
        .last()
        .is_none_or(|last| last.kind() == SyntaxKind::Missing);
    match role {
        Role::Callable(callable) => {
            let form = callable::Form::Callable(callable);
            return callable::visit_form(children, span, form, recovered, text, sink);
        }
        Role::Test => {
            return callable::visit_form(
                children,
                span,
                callable::Form::Test,
                recovered,
                text,
                sink,
            );
        }
        _ => {}
    }
    if let Some((construct, message)) = construct_role(role) {
        let closer = children.pop();
        let mut rest = children.into_iter();
        let opener = rest.next();
        let start = opener
            .as_ref()
            .map_or(span.start(), |node| node.span().end());
        let mut level = Level::new(flatten(rest.collect()), text, start);
        let mut slots: Vec<Slot> = opener.into_iter().map(Slot::Ready).collect();
        match role {
            Role::ModuleLet => level.module_let_parts(&mut slots),
            Role::Let => level.let_parts(&mut slots),
            Role::If => level.if_parts(&mut slots),
            Role::Match => level.match_parts(&mut slots),
            _ => level.do_parts(&mut slots),
        }
        level.rest(&mut slots);
        slots.extend(closer.map(Slot::Ready));
        sink.append(&mut level.diagnostics);
        if level.malformed && !recovered {
            sink.push(Diagnostic::new(
                DiagnosticCode::SyntaxInvalidForm,
                span,
                message,
            ));
        }
        return (construct, slots, span.start());
    }
    if recovered || children.len() < 2 {
        return (
            kind,
            children.into_iter().map(Slot::Ready).collect(),
            span.start(),
        );
    }
    let closer = children.pop();
    let opener = children.remove(0);
    let start = opener.span().end();
    let items = children
        .into_iter()
        .map(|node| Item {
            node,
            section: None,
            keyword: false,
        })
        .collect();
    let mut level = Level::new(items, text, start);
    let mut slots = vec![Slot::Ready(opener)];
    match role {
        Role::Arguments => level.argument_entries(&mut slots),
        Role::Product => level.product_entries(&mut slots),
        Role::Collection => level.collection_entries(&mut slots),
        Role::PatternList => level.pattern_entries(&mut slots, false),
        Role::Params => level.param_entries(&mut slots),
        Role::Labelled => level.labelled_entries(&mut slots),
        _ => level.pattern_entries(&mut slots, true),
    }
    level.trivia(&mut slots);
    if !level.malformed && level.next_sig().is_none() {
        sink.append(&mut level.diagnostics);
        slots.extend(closer.map(Slot::Ready));
        let kind = match role {
            Role::Params => SyntaxKind::Params,
            Role::Labelled => SyntaxKind::Labelled,
            _ => kind,
        };
        return (kind, slots, span.start());
    }
    sink.push(Diagnostic::new(
        DiagnosticCode::SyntaxInvalidForm,
        span,
        group_message(role),
    ));
    let mut nodes = Vec::new();
    for slot in slots {
        slot.unshape(&mut nodes);
    }
    nodes.extend(level.items.into_iter().flatten().map(|item| item.node));
    nodes.extend(closer);
    (
        SyntaxKind::Error,
        nodes.into_iter().map(Slot::Ready).collect(),
        span.start(),
    )
}

/// Runs the slots to finished nodes; every construct and group is read
/// before its parent is assembled. Iterative.
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
        let (kind, slots, at) = match step {
            Work::Slot(Slot::Ready(node)) => {
                out.push(node);
                continue;
            }
            Work::Assemble { kind, count, at } => {
                let children = out.split_off(out.len().saturating_sub(count));
                out.push(CstNode::interior(kind, children, at));
                continue;
            }
            Work::Slot(Slot::Wrap(kind, slots, at)) => (kind, slots, at),
            Work::Slot(Slot::Visit(node, role)) => visit(node, role, text, sink),
        };
        work.push(Work::Assemble {
            kind,
            count: slots.len(),
            at,
        });
        work.extend(slots.into_iter().rev().map(Work::Slot));
    }
    out
}

/// Shapes a top-level `let` construct as a module value.
pub(crate) fn shape_module_let(
    node: CstNode,
    text: &str,
    sink: &mut Vec<Diagnostic>,
) -> CstNode {
    let span = node.span();
    run(vec![Slot::Visit(node, Role::ModuleLet)], text, sink)
        .pop()
        .unwrap_or_else(|| {
            CstNode::interior(SyntaxKind::Error, Vec::new(), span.start())
        })
}

/// Shapes the operand of a `where` section, the items after its keyword:
/// a predicate and an optional `else` consequence. Returns the shaped items
/// and whether the operand broke that rule.
pub(crate) fn shape_where_operand(
    items: Vec<CstNode>,
    text: &str,
    sink: &mut Vec<Diagnostic>,
) -> (Vec<CstNode>, bool) {
    let start = items.first().map_or(0, |node| node.span().start());
    let items = items
        .into_iter()
        .map(|node| Item {
            node,
            section: None,
            keyword: false,
        })
        .collect();
    let mut level = Level::new(items, text, start);
    let mut slots = Vec::new();
    if !level.expr(&mut slots) {
        level.malformed = true;
    }
    if level.next_is_word("else") {
        level.push_next(&mut slots);
        if !level.expr(&mut slots) {
            level.malformed = true;
        }
    }
    level.rest(&mut slots);
    sink.append(&mut level.diagnostics);
    (run(slots, text, sink), level.malformed)
}
