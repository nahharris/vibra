//! Lexical scopes and binders.
//!
//! `docs/spec/01-source-language.md`, "Bodies and statements": a callable
//! body is one scope together with its parameters, and each `if` branch,
//! `match` arm, `do` block, and `let` fallback is its own scope, whose
//! bindings end with it. A `let`'s names are visible after `endlet` to the
//! end of the directly enclosing body and not in its initializer or its
//! fallback; a `let` opens no scope. "`fn` and `proc`": the result name is
//! visible only in `ensures`.
//!
//! "Patterns": a bare name that resolves to a visible module value whose
//! initializer is a constant expression is a constant pattern and binds
//! nothing, `true` and `false` among them; any other bare name binds; a
//! dotted name never binds; a name that resolves to a function or to a
//! non-constant module value is `@name.redeclaration` at the name, related
//! to that declaration. A pattern binding one name twice, or a binder that
//! shadows a visible name, is `@name.redeclaration` at the later name,
//! related to the earlier (`docs/spec/02-type-system.md`, "Namespaces and
//! resolution"); the later binder still binds. `_` binds nothing and may
//! repeat. "The prelude": binding a prelude name is
//! `@name.reservedDeclaration`; a reserved word in a binder position is the
//! reader's diagnostic.

use std::collections::BTreeMap;

use vibra_diagnostics::{ByteSpan, DiagnosticCode};
use vibra_syntax::source::{
    Body, Callable, Expr, ExprForm, Guard, Ident, Pattern, PatternForm, Postfix,
    Primary, Statement, Suffix,
};

use crate::declare::reserve;
use crate::graph::Binding;
use crate::identity::DeclarationKind;
use crate::namespaces::at;
use crate::paths::{Entity, Position, Scope, Walker};

/// The lexical scopes open at a position, outermost first. Each frame holds
/// the names it bound, in the order they were bound: a parameter, a labelled
/// parameter, a result name, or a pattern binder.
///
/// The cost of a binding and of a lookup does not grow with the length of a
/// body: [`Self::named`] holds the visible bindings of each name, and every
/// binding is also a link of [`Self::chain`], which points at the binding
/// visible just before it, so that what is visible at a position is one
/// index, [`Self::top`].
#[derive(Clone, Debug, Default)]
pub(crate) struct Frames {
    frames: Vec<Vec<String>>,
    /// The visible bindings of each name, latest last.
    named: BTreeMap<String, Vec<ByteSpan>>,
    /// The newest visible binding when each open scope was opened.
    opened: Vec<Option<usize>>,
    /// The newest visible binding, a link of `chain`.
    top: Option<usize>,
    /// Every binding made, with the link of the binding visible before it.
    chain: Vec<(String, Option<usize>)>,
}

/// The scopes [`Frames::take`] took.
#[derive(Debug)]
pub(crate) struct Taken {
    frames: Vec<Vec<String>>,
    named: BTreeMap<String, Vec<ByteSpan>>,
    opened: Vec<Option<usize>>,
    top: Option<usize>,
}

impl Frames {
    /// Opens a scope.
    pub(crate) fn push(&mut self) {
        self.frames.push(Vec::new());
        self.opened.push(self.top);
    }

    /// Closes the innermost scope and every binding made in it.
    pub(crate) fn pop(&mut self) {
        for name in self.frames.pop().unwrap_or_default() {
            if let Some(spans) = self.named.get_mut(&name) {
                spans.pop();
                if spans.is_empty() {
                    self.named.remove(&name);
                }
            }
        }
        if let Some(top) = self.opened.pop() {
            self.top = top;
        }
    }

    /// Binds `name` at `span` in the innermost scope.
    pub(crate) fn bind(&mut self, name: &str, span: ByteSpan) {
        if let Some(frame) = self.frames.last_mut() {
            frame.push(name.to_owned());
            self.named.entry(name.to_owned()).or_default().push(span);
            self.chain.push((name.to_owned(), self.top));
            self.top = Some(self.chain.len() - 1);
        }
    }

    /// The newest visible binding, as a link of [`Self::chain`]: what is
    /// visible here is it and the bindings it links to.
    pub(crate) const fn top(&self) -> Option<usize> {
        self.top
    }

    /// Every binding made, each with the link of the binding visible before
    /// it, taken out at the end of the walk.
    pub(crate) fn take_chain(&mut self) -> Vec<(String, Option<usize>)> {
        std::mem::take(&mut self.chain)
    }

    /// The span of the latest visible binding of `name`.
    pub(crate) fn find(&self, name: &str) -> Option<ByteSpan> {
        self.named.get(name).and_then(|spans| spans.last()).copied()
    }

    /// Takes every open scope away, leaving none; [`Self::restore`] puts
    /// them back. A named callable sees no enclosing local.
    pub(crate) fn take(&mut self) -> Taken {
        Taken {
            frames: std::mem::take(&mut self.frames),
            named: std::mem::take(&mut self.named),
            opened: std::mem::take(&mut self.opened),
            top: self.top.take(),
        }
    }

    /// Puts back the scopes [`Self::take`] took.
    pub(crate) fn restore(&mut self, taken: Taken) {
        self.frames = taken.frames;
        self.named = taken.named;
        self.opened = taken.opened;
        self.top = taken.top;
    }
}

impl<'a> Walker<'_, 'a, '_> {
    /// The scope of a callable: its labelled defaults, resolved before any
    /// parameter is bound; its parameters and labelled names; `requires`;
    /// `ensures`, which alone sees the result name; and its body, whose
    /// statements share the parameters' scope.
    pub(crate) fn callable_scope(&mut self, scope: &Scope, view: Callable<'a>) {
        for (_, _, default) in view.labelled() {
            self.expr(scope, default);
        }
        self.frames.push();
        for (pattern, _) in view.takes() {
            self.pattern(scope, pattern, &mut Vec::new());
        }
        for (name, _, _) in view.labelled() {
            self.bind(scope, name, &mut Vec::new());
        }
        if let Some(guard) = view.requires() {
            self.guard(scope, guard);
        }
        self.frames.push();
        if let Some((result, _)) = view.result() {
            self.bind(scope, result, &mut Vec::new());
        }
        if let Some(guard) = view.ensures() {
            self.guard(scope, guard);
        }
        self.frames.pop();
        if let Some(body) = view.body() {
            self.statements(scope, body);
        }
        self.frames.pop();
    }

    /// A guard's predicate and its `else` consequence.
    fn guard(&mut self, scope: &Scope, guard: Guard<'a>) {
        if let Some(predicate) = guard.predicate() {
            self.expr(scope, predicate);
        }
        if let Some(consequence) = guard.consequence() {
            self.expr(scope, consequence);
        }
    }

    /// A body that is its own scope: an `if` branch, a `do` block, or a
    /// `let` fallback.
    fn scoped(&mut self, scope: &Scope, body: Body<'a>) {
        self.frames.push();
        self.statements(scope, body);
        self.frames.pop();
    }

    /// The statements of `body`, in the innermost scope. A `let`'s names are
    /// bound after its type, its initializer, and its fallback are resolved,
    /// so they are visible from the next statement on.
    pub(crate) fn statements(&mut self, scope: &Scope, body: Body<'a>) {
        for statement in body.statements() {
            match statement {
                Statement::Let(view) => {
                    self.record_visible(view.node().span());
                    if let Some(written) = view.written_type() {
                        self.type_expr(scope, written, Position::Type);
                    }
                    if let Some(value) = view.value() {
                        self.expr(scope, value);
                    }
                    if let Some(fallback) = view.fallback() {
                        self.scoped(scope, fallback);
                    }
                    if let Some(pattern) = view.pattern() {
                        self.pattern(scope, pattern, &mut Vec::new());
                    }
                }
                Statement::Expr(expr) => {
                    let end = expr.node().span().end();
                    let start = expr
                        .prefixes()
                        .first()
                        .map_or(expr.node().span().start(), |prefix| {
                            prefix.span().start()
                        });
                    self.record_visible(ByteSpan::new(start, end));
                    self.expr(scope, expr);
                }
            }
        }
    }

    /// Resolves every name of an expression.
    pub(crate) fn expr(&mut self, scope: &Scope, expr: Expr<'a>) {
        match expr.form() {
            ExprForm::Return(inner) | ExprForm::Try(inner) | ExprForm::Await(inner) => {
                self.expr(scope, inner);
            }
            ExprForm::Postfix(postfix) => self.postfix(scope, postfix),
            ExprForm::Let(view) => {
                if let Some(value) = view.value() {
                    self.expr(scope, value);
                }
            }
        }
    }

    /// A primary and its applications and field reads.
    fn postfix(&mut self, scope: &Scope, postfix: Postfix<'a>) {
        let suffixes = postfix.suffixes();
        let mut applied = 0;
        match postfix.primary() {
            Some(Primary::Name(first)) => {
                let fields = suffixes
                    .iter()
                    .take_while(|suffix| matches!(suffix, Suffix::Field(_)))
                    .count();
                let parts: Vec<Ident<'a>> = std::iter::once(first)
                    .chain(suffixes.iter().filter_map(|suffix| match suffix {
                        Suffix::Field(name) => Some(*name),
                        Suffix::Call(_) => None,
                    }))
                    .take(fields + 1)
                    .collect();
                self.value_path(scope, &parts);
                applied = fields;
            }
            Some(Primary::Literal(_)) | None => {}
            Some(Primary::Tuple(items) | Primary::List(items)) => {
                for item in items {
                    self.expr(scope, item);
                }
            }
            Some(Primary::Record(fields)) => {
                for (_, value) in fields {
                    self.expr(scope, value);
                }
            }
            Some(Primary::Dict(entries)) => {
                for (key, value) in entries {
                    self.expr(scope, key);
                    self.expr(scope, value);
                }
            }
            Some(Primary::If(view)) => {
                if let Some(condition) = view.condition() {
                    self.expr(scope, condition);
                }
                if let Some(body) = view.then_body() {
                    self.scoped(scope, body);
                }
                if let Some(body) = view.else_body() {
                    self.scoped(scope, body);
                }
            }
            Some(Primary::Match(view)) => {
                if let Some(subject) = view.subject() {
                    self.expr(scope, subject);
                }
                for arm in view.arms() {
                    self.frames.push();
                    if let Some(pattern) = arm.pattern() {
                        self.pattern(scope, pattern, &mut Vec::new());
                    }
                    if let Some(body) = arm.body() {
                        self.statements(scope, body);
                    }
                    self.frames.pop();
                }
            }
            Some(Primary::Do(view)) => {
                if let Some(body) = view.body() {
                    self.scoped(scope, body);
                }
            }
            Some(Primary::Callable(view)) => self.callable(scope, view),
        }
        for suffix in suffixes.into_iter().skip(applied) {
            if let Suffix::Call(arguments) = suffix {
                for argument in arguments {
                    self.expr(scope, argument.value);
                }
            }
        }
    }

    /// The source and span of the introduction of `name` that a binder would
    /// shadow: a local, a type parameter, an associated name, or a name of the
    /// module scope.
    fn shadowed(&self, scope: &Scope, name: &str) -> Option<(String, ByteSpan)> {
        let here = self.source_id().to_owned();
        if let Some(span) = self.frames.find(name) {
            return Some((here, span));
        }
        if let Some((_, span)) =
            scope.params.iter().rev().find(|(param, _)| param == name)
        {
            return Some((here, *span));
        }
        if let Some((_, _, span)) =
            scope.associated.iter().find(|(assoc, _, _)| assoc == name)
        {
            return Some((here, *span));
        }
        let binding = *self.graph.modules.get(self.module)?.scope.get(name)?;
        let span = match binding {
            Binding::Decl(index) => self.graph.decls.get(index)?.name_span,
            Binding::Import(index) => self.graph.imports.get(index)?.name.as_ref()?.1,
        };
        Some((here, span))
    }

    /// Introduces the binder `name` in the innermost scope. `pattern` holds
    /// the names the same pattern bound before it.
    pub(crate) fn bind(
        &mut self,
        scope: &Scope,
        name: Ident<'a>,
        pattern: &mut Vec<(String, ByteSpan)>,
    ) {
        if !name.is_reserved() {
            if self.graph.is_prelude_name(name.text()) {
                let source_id = self.source_id().to_owned();
                reserve(
                    self.graph,
                    &source_id,
                    name,
                    DiagnosticCode::NameReservedDeclaration,
                    self.out,
                );
            } else if let Some((_, earlier)) =
                pattern.iter().find(|(bound, _)| bound == name.text())
            {
                let diagnostic = at(
                    DiagnosticCode::NameRedeclaration,
                    self.source_id(),
                    name.span(),
                    format!("the pattern already binds `{}`", name.text()),
                )
                .with_related(*earlier, "the earlier binder");
                self.report(diagnostic);
            } else if let Some((source_id, earlier)) = self.shadowed(scope, name.text())
            {
                let diagnostic = at(
                    DiagnosticCode::NameRedeclaration,
                    self.source_id(),
                    name.span(),
                    format!(
                        "`{}` is already visible here; a binder may not shadow it",
                        name.text()
                    ),
                )
                .with_related_source(
                    &source_id,
                    earlier,
                    "the visible name",
                );
                self.report(diagnostic);
            }
        }
        pattern.push((name.text().to_owned(), name.span()));
        self.frames.bind(name.text(), name.span());
    }

    /// Resolves a pattern and binds its binders in the innermost scope.
    /// `pattern` holds the names the enclosing pattern bound before it.
    pub(crate) fn pattern(
        &mut self,
        scope: &Scope,
        written: Pattern<'a>,
        pattern: &mut Vec<(String, ByteSpan)>,
    ) {
        let mut stack = vec![written];
        while let Some(current) = stack.pop() {
            match current.form() {
                PatternForm::Discard | PatternForm::Literal(_) => {}
                PatternForm::Path(path) => match path.segments().as_slice() {
                    [name] => self.pattern_name(scope, *name, pattern),
                    parts => {
                        self.value_path(scope, parts);
                    }
                },
                PatternForm::Apply {
                    path,
                    positional,
                    labelled,
                } => {
                    self.value_path(scope, &path.segments());
                    stack
                        .extend(labelled.into_iter().rev().map(|(_, pattern)| pattern));
                    stack.extend(positional.into_iter().rev());
                }
                PatternForm::Tuple(items) | PatternForm::List(items) => {
                    stack.extend(items.into_iter().rev());
                }
                PatternForm::Record(fields) => {
                    stack.extend(fields.into_iter().rev().map(|(_, pattern)| pattern));
                }
            }
        }
    }

    /// A bare name in a pattern: a constant pattern, a name that is neither
    /// a binder nor a constant, or a binder.
    fn pattern_name(
        &mut self,
        scope: &Scope,
        name: Ident<'a>,
        pattern: &mut Vec<(String, ByteSpan)>,
    ) {
        if name.is_reserved() {
            self.bind(scope, name, pattern);
            return;
        }
        let entity = if self.frames.find(name.text()).is_some() {
            None
        } else {
            self.lookup(scope, name.text())
        };
        let decl = match entity {
            Some(Entity::Failed) => return,
            Some(Entity::Decl(index)) => {
                self.graph.decls.get(index).map(|decl| (index, decl.kind()))
            }
            _ => None,
        };
        match decl {
            Some((index, DeclarationKind::Value)) if self.is_constant(index) => {
                self.reference(name.span(), index);
            }
            Some((
                index,
                kind @ (DeclarationKind::Value
                | DeclarationKind::Fn
                | DeclarationKind::Proc),
            )) => {
                let Some(decl) = self.graph.decls.get(index) else {
                    return;
                };
                let what = match kind {
                    DeclarationKind::Value => {
                        "a module value whose initializer is not constant"
                    }
                    DeclarationKind::Proc => "a procedure",
                    _ => "a function",
                };
                let diagnostic = at(
                    DiagnosticCode::NameRedeclaration,
                    self.source_id(),
                    name.span(),
                    format!(
                        "`{}` names {what}: it is neither a binder nor a constant pattern",
                        name.text()
                    ),
                )
                .with_related_source(
                    self.graph.source_id(decl.module),
                    decl.name_span,
                    "the declaration",
                );
                self.report(diagnostic);
            }
            _ => self.bind(scope, name, pattern),
        }
    }
}
