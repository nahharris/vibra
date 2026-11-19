//! Expressions and bodies: checking and lowering in one pass.
//!
//! `docs/spec/02-type-system.md`, "Application", "Functions as values",
//! "Inference and checking", and "Control flow and failure", and
//! `docs/spec/01-source-language.md`, "Bodies and statements" and
//! "Patterns". Expected types flow down; each expression yields its type and,
//! while no fault was found in it, its checked IR node. A type of `None` is
//! the error type: it was already reported and reports nothing again.
//!
//! Expressions, bodies, and patterns nest without bound, so the checker is a
//! machine over an explicit stack of tasks: no host stack grows with the
//! depth of the program.

use std::sync::Arc;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::{
    Arguments, CallTarget, Constant, Construction, ContractCall, Expr as IrExpr,
    FnType, MatchArm, NamedTypes, Node, Pattern as IrPattern, SourceOrigin, Type,
    Value,
};
use vibra_syntax::source::{
    Argument, Body, Expr, ExprForm, If, Match, MatchArm as ArmView, Pattern,
    PatternForm, Postfix, Primary, Statement, Suffix, TypeExpr,
};
use vibra_syntax::{CstNode, SyntaxKind};

use super::declare::{Entity, NoType, Program, Scope, TypeKind, substitute_self};
use super::literal::{self, LiteralFault};

/// Where each supported declaration lands in the checked module set.
#[derive(Debug, Default)]
pub(crate) struct Layout {
    /// The global index of each module value of the program.
    pub(crate) globals: Vec<Option<usize>>,
    /// The function index of each function of the program.
    pub(crate) functions: Vec<Option<usize>>,
}

/// A declaration a body refers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Target {
    /// A module value, by its program index.
    Value(usize),
    /// A function, by its program index.
    Function(usize),
}

/// One reference of a body: what it names and where.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Reference {
    pub(crate) target: Target,
    pub(crate) span: ByteSpan,
}

/// The type and the lowered node of one checked expression.
#[derive(Debug)]
struct Out {
    ty: Option<Type>,
    node: Option<Node>,
}

impl Out {
    const fn error() -> Self {
        Self {
            ty: None,
            node: None,
        }
    }
}

/// How the value of an expression is used.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Ctx {
    /// The value is not used: the expression is a statement.
    Stmt,
    /// The value is used, at the expected type when one is known.
    Value(Option<Type>),
}

impl Ctx {
    const fn expected(&self) -> Option<&Type> {
        match self {
            Self::Stmt => None,
            Self::Value(expected) => expected.as_ref(),
        }
    }
}

/// What a body is.
#[derive(Clone, Debug, PartialEq, Eq)]
enum BodyMode {
    /// The body of a callable: every path MUST leave.
    Callable,
    /// A branch, an arm, or a `do` block, whose value is used or not.
    Branch(Ctx),
}

/// What a postfix form has denoted so far.
#[derive(Debug)]
enum Head {
    /// A value arrives from the task just pushed.
    Pending,
    /// A value.
    Value(Box<Out>),
    /// A function, by its program index, not yet applied or read as a value.
    Function(usize),
    /// A case of a variant type: the type's index and the case.
    Case(usize, String),
    /// A member of an interface: the interface's index and the member.
    Contract(usize, String),
    /// A type, an interface, or a module.
    Static,
    /// Already reported.
    Error,
}

/// What a call calls.
#[derive(Debug)]
enum Callee {
    Function(usize),
    Value(Option<Box<Node>>, Box<FnType>),
    Case(usize, String),
    Contract(usize, String),
}

struct PostfixState<'a> {
    ctx: Ctx,
    suffixes: Vec<(Suffix<'a>, ByteSpan)>,
    next: usize,
    head: Head,
    start: usize,
    /// The end of what the head spans so far: the callee of the next call.
    end: usize,
}

struct CallState<'a> {
    callee: Callee,
    args: Vec<Argument<'a>>,
    next: usize,
    params: Option<Vec<Type>>,
    result: Option<Type>,
    receiver: Option<Type>,
    nodes: Vec<Option<Node>>,
    application: ByteSpan,
    callee_span: ByteSpan,
    failed: bool,
    never_reported: bool,
}

struct IfState<'a> {
    view: If<'a>,
    ctx: Ctx,
    stage: u8,
    condition: Option<Node>,
    then: Option<Out>,
    failed: bool,
}

/// What one arm of a `match` covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cover {
    All,
    Case(usize),
    Nothing,
}

struct MatchState<'a> {
    view: Match<'a>,
    ctx: Ctx,
    arms: Vec<ArmView<'a>>,
    next: usize,
    subject: Option<Type>,
    subject_node: Option<Node>,
    stage: u8,
    lowered: Vec<Option<MatchArm>>,
    pending_pattern: Option<IrPattern>,
    mark: usize,
    /// The cases of the subject's variant type, when it is one.
    cases: Option<(String, Vec<String>)>,
    covered: Vec<Option<ByteSpan>>,
    covered_all: Option<ByteSpan>,
    first_cover: Option<ByteSpan>,
    types: Vec<Option<Type>>,
    join: Option<Type>,
    failed: bool,
}

/// One lowered statement of a body.
#[derive(Debug)]
enum Piece {
    Expr(Node),
    Let(Option<usize>, Node),
}

#[derive(Debug)]
enum Pending {
    Nothing,
    Let {
        binder: Option<String>,
        written: Option<Type>,
        value_span: ByteSpan,
    },
    Expr {
        span: ByteSpan,
        last: bool,
    },
}

struct BodyState<'a> {
    span: ByteSpan,
    statements: Vec<Statement<'a>>,
    next: usize,
    mode: BodyMode,
    mark: usize,
    pending: Pending,
    never_at: Option<ByteSpan>,
    unreachable_reported: bool,
    pieces: Vec<Option<Piece>>,
    last_type: Option<Option<Type>>,
    failed: bool,
}

enum Task<'a> {
    Expr(Expr<'a>, Ctx),
    Return(ByteSpan),
    Postfix(Box<PostfixState<'a>>),
    Call(Box<CallState<'a>>),
    If(Box<IfState<'a>>),
    Match(Box<MatchState<'a>>),
    Body(Box<BodyState<'a>>),
}

enum Step<'a> {
    Done(Out),
    Child(Task<'a>, Task<'a>),
}

/// One local binding.
#[derive(Debug)]
struct Local {
    name: String,
    slot: usize,
    ty: Option<Type>,
}

/// Checks and lowers the bodies of one module.
pub(crate) struct BodyChecker<'p, 'a> {
    program: &'p Program<'a>,
    layout: &'p Layout,
    module: usize,
    source_id: Arc<str>,
    text: &'a str,
    result: Option<Type>,
    /// The type parameters in scope of the callable being checked.
    binders: Vec<String>,
    /// What `Self` denotes in it.
    self_type: Option<Type>,
    locals: Vec<Local>,
    slots: Vec<Type>,
    /// The diagnostics found.
    pub(crate) diagnostics: Vec<Diagnostic>,
    /// The declarations the checked bodies refer to.
    pub(crate) references: Vec<Reference>,
}

/// The value of one checked callable body or initializer.
pub(crate) struct Checked {
    /// The node, when no fault was found.
    pub(crate) node: Option<Node>,
    /// The number of slots its activation has.
    pub(crate) slot_count: usize,
}

impl<'p, 'a> BodyChecker<'p, 'a> {
    /// A checker for bodies of the module `module` with the document `text`.
    pub(crate) fn new(
        program: &'p Program<'a>,
        layout: &'p Layout,
        module: usize,
        text: &'a str,
    ) -> Self {
        let source_id = program
            .modules
            .get(module)
            .map_or_else(|| Arc::from(""), |data| Arc::clone(&data.source_id));
        Self {
            program,
            layout,
            module,
            source_id,
            text,
            result: None,
            binders: Vec::new(),
            self_type: None,
            locals: Vec::new(),
            slots: Vec::new(),
            diagnostics: Vec::new(),
            references: Vec::new(),
        }
    }

    /// Checks the body of a callable of type `params -> result` whose
    /// parameters are named `names`, with the type parameters `binders` and
    /// `self_type` for `Self` in scope.
    pub(crate) fn callable(
        &mut self,
        signature: &FnType,
        names: &[(String, ByteSpan)],
        binders: &[String],
        self_type: Option<Type>,
        body: Body<'a>,
    ) -> Checked {
        let params = &signature.parameters;
        let result = &signature.result;
        self.locals.clear();
        self.slots.clear();
        self.binders = binders.to_vec();
        self.self_type = self_type;
        self.result = Some(result.clone());
        for (slot, (ty, (name, _))) in params.iter().zip(names).enumerate() {
            self.slots.push(ty.clone());
            self.locals.push(Local {
                name: name.clone(),
                slot,
                ty: Some(ty.clone()),
            });
        }
        let out = self.run(Task::Body(Box::new(
            self.body_state(body, BodyMode::Callable),
        )));
        Checked {
            node: out.node,
            slot_count: self.slots.len(),
        }
    }

    /// Checks the initializer of a module value of type `ty`.
    pub(crate) fn initializer(&mut self, ty: &Type, value: Expr<'a>) -> Checked {
        self.locals.clear();
        self.slots.clear();
        self.binders.clear();
        self.self_type = None;
        self.result = None;
        let span = expr_span(&value);
        let out = self.run(Task::Expr(value, Ctx::Value(Some(ty.clone()))));
        let node = match out.ty {
            Some(Type::Never) => {
                self.report(
                    DiagnosticCode::TypeUnreachableCode,
                    span,
                    "this initializer never completes, so it has no value",
                );
                None
            }
            Some(actual) => {
                self.conform(ty, &actual, out.node, span, DiagnosticCode::TypeMismatch)
            }
            None => None,
        };
        Checked {
            node,
            slot_count: self.slots.len(),
        }
    }

    fn origin(&self, span: ByteSpan) -> SourceOrigin {
        SourceOrigin::new(Arc::clone(&self.source_id), span)
    }

    fn node(&self, expr: IrExpr, span: ByteSpan) -> Node {
        Node::new(expr, self.origin(span))
    }

    fn report(
        &mut self,
        code: DiagnosticCode,
        span: ByteSpan,
        message: impl Into<String>,
    ) {
        self.diagnostics.push(
            Diagnostic::new(code, span, message)
                .with_source_id(self.source_id.as_ref()),
        );
    }

    fn unavailable(&mut self, span: ByteSpan, what: &str) {
        self.report(
            DiagnosticCode::ToolUnavailable,
            span,
            format!("{what} is not supported by this checker yet"),
        );
    }

    /// Checks `actual` against `expected`, reporting `code` at `span` when it
    /// does not fit; widens an atom singleton to `Atom`.
    fn conform(
        &mut self,
        expected: &Type,
        actual: &Type,
        node: Option<Node>,
        span: ByteSpan,
        code: DiagnosticCode,
    ) -> Option<Node> {
        if actual == expected || *actual == Type::Never {
            return node;
        }
        if *expected == Type::Atom && matches!(actual, Type::AtomSingleton(_)) {
            return node.map(|node| self.node(IrExpr::WidenAtom(Box::new(node)), span));
        }
        self.report(
            code,
            span,
            format!("expected `{expected}`, found `{actual}`"),
        );
        None
    }

    /// The checked type of a type written in a body, reporting why it has
    /// none.
    fn written(&mut self, written: TypeExpr<'_>) -> Option<Type> {
        let scope = Scope {
            self_type: self.self_type.as_ref(),
            binders: &self.binders,
        };
        match self.program.written(self.module, written, scope) {
            Ok(ty) => Some(ty),
            Err(NoType::Unsupported) => {
                self.unavailable(written.span(), "this type");
                None
            }
            Err(NoType::Unresolved) => None,
            Err(NoType::Fault(diagnostic)) => {
                self.diagnostics
                    .push(diagnostic.with_source_id(self.source_id.as_ref()));
                None
            }
        }
    }

    fn new_slot(&mut self, ty: Option<&Type>) -> usize {
        let slot = self.slots.len();
        self.slots.push(ty.cloned().unwrap_or(Type::Void));
        slot
    }

    fn local(&self, name: &str) -> Option<&Local> {
        self.locals.iter().rev().find(|local| local.name == name)
    }

    fn run(&mut self, root: Task<'a>) -> Out {
        let mut tasks = vec![root];
        let mut input: Option<Out> = None;
        while let Some(task) = tasks.pop() {
            let step = match task {
                Task::Expr(expr, ctx) => self.start_expr(expr, ctx),
                Task::Return(span) => self.finish_return(span, input.take()),
                Task::Postfix(state) => self.step_postfix(state, input.take()),
                Task::Call(state) => self.step_call(state, input.take()),
                Task::If(state) => self.step_if(state, input.take()),
                Task::Match(state) => self.step_match(state, input.take()),
                Task::Body(state) => self.step_body(state, input.take()),
            };
            match step {
                Step::Done(out) => input = Some(out),
                Step::Child(parent, child) => {
                    tasks.push(parent);
                    tasks.push(child);
                    input = None;
                }
            }
        }
        input.unwrap_or_else(Out::error)
    }

    fn start_expr(&mut self, expr: Expr<'a>, ctx: Ctx) -> Step<'a> {
        let span = expr_span(&expr);
        match expr.form() {
            ExprForm::Return(operand) => {
                let Some(result) = self.result.clone() else {
                    self.report(
                        DiagnosticCode::TypeInvalidReturn,
                        span,
                        "`return` is valid only in a callable body",
                    );
                    return Step::Done(Out::error());
                };
                Step::Child(
                    Task::Return(span),
                    Task::Expr(operand, Ctx::Value(Some(result))),
                )
            }
            ExprForm::Try(_) => {
                self.unavailable(span, "`try`");
                Step::Done(Out::error())
            }
            ExprForm::Await(_) => {
                self.unavailable(span, "`await`");
                Step::Done(Out::error())
            }
            ExprForm::Let(_) => Step::Done(Out::error()),
            ExprForm::Postfix(postfix) => self.start_postfix(postfix, ctx),
        }
    }

    fn finish_return(&mut self, span: ByteSpan, input: Option<Out>) -> Step<'a> {
        let out = input.unwrap_or_else(Out::error);
        let Some(result) = self.result.clone() else {
            return Step::Done(Out::error());
        };
        let operand_span =
            ByteSpan::new(span.start().saturating_add("return".len()), span.end());
        let operand_span = trim_start(self.text, operand_span);
        let node = match out.ty {
            Some(Type::Never) => {
                self.report(
                    DiagnosticCode::TypeUnreachableCode,
                    operand_span,
                    "this operand never completes, so `return` has no value to return",
                );
                None
            }
            Some(actual) => self.conform(
                &result,
                &actual,
                out.node,
                operand_span,
                DiagnosticCode::TypeMismatch,
            ),
            None => None,
        };
        let node = node.map(|mut operand| {
            if let IrExpr::Call { .. } = &operand.expr {
                let call = std::mem::replace(&mut operand.expr, IrExpr::Global(0));
                if let IrExpr::Call { target, arguments } = call {
                    operand.expr = IrExpr::TailCall { target, arguments };
                }
            }
            self.node(IrExpr::Return(Box::new(operand)), span)
        });
        Step::Done(Out {
            ty: Some(Type::Never),
            node,
        })
    }

    fn start_postfix(&mut self, postfix: Postfix<'a>, ctx: Ctx) -> Step<'a> {
        let suffixes = suffix_spans(postfix, self.text);
        let start = postfix.node().span().start();
        let Some(primary) = postfix.primary() else {
            return Step::Done(Out::error());
        };
        let primary_span =
            primary_node(postfix).map_or(postfix.node().span(), CstNode::span);
        let inner_ctx = if suffixes.is_empty() {
            ctx.clone()
        } else {
            Ctx::Value(None)
        };
        let mut state = Box::new(PostfixState {
            ctx,
            suffixes,
            next: 0,
            head: Head::Pending,
            start,
            end: primary_span.end(),
        });
        match primary {
            Primary::Literal(node) => {
                state.head =
                    Head::Value(Box::new(self.literal(node, inner_ctx.expected())));
                self.step_postfix(state, None)
            }
            Primary::Name(name) => {
                let (head, consumed) =
                    self.name_head(name.text(), name.span(), &state.suffixes);
                if let Some((_, span)) = consumed
                    .checked_sub(1)
                    .and_then(|at| state.suffixes.get(at))
                {
                    state.end = span.end();
                }
                state.head = head;
                state.next = consumed;
                self.step_postfix(state, None)
            }
            Primary::If(view) => {
                let child = Task::If(Box::new(IfState {
                    view,
                    ctx: inner_ctx,
                    stage: 0,
                    condition: None,
                    then: None,
                    failed: false,
                }));
                Step::Child(Task::Postfix(state), child)
            }
            Primary::Match(view) => {
                let child = Task::Match(Box::new(MatchState {
                    view,
                    ctx: inner_ctx,
                    arms: view.arms(),
                    next: 0,
                    subject: None,
                    subject_node: None,
                    stage: 0,
                    lowered: Vec::new(),
                    pending_pattern: None,
                    mark: 0,
                    cases: None,
                    covered: Vec::new(),
                    covered_all: None,
                    first_cover: None,
                    types: Vec::new(),
                    join: None,
                    failed: false,
                }));
                Step::Child(Task::Postfix(state), child)
            }
            Primary::Do(block) => {
                let Some(body) = block.body() else {
                    return Step::Done(Out::error());
                };
                let child = Task::Body(Box::new(
                    self.body_state(body, BodyMode::Branch(inner_ctx)),
                ));
                Step::Child(Task::Postfix(state), child)
            }
            Primary::Tuple(_) => self.unavailable_primary(primary_span, "a tuple"),
            Primary::Record(_) => self.unavailable_primary(primary_span, "a record"),
            Primary::List(_) => self.unavailable_primary(primary_span, "a list"),
            Primary::Dict(_) => self.unavailable_primary(primary_span, "a dict"),
            Primary::Callable(_) => {
                self.unavailable_primary(primary_span, "an anonymous callable")
            }
        }
    }

    fn unavailable_primary(&mut self, span: ByteSpan, what: &str) -> Step<'a> {
        self.unavailable(span, what);
        Step::Done(Out::error())
    }

    /// The head a name and its leading field reads denote, and how many field
    /// reads it consumed.
    fn name_head(
        &mut self,
        name: &str,
        span: ByteSpan,
        suffixes: &[(Suffix<'a>, ByteSpan)],
    ) -> (Head, usize) {
        if let Some(local) = self.local(name) {
            let node = local.ty.clone().map(|value_type| {
                self.node(
                    IrExpr::Variable {
                        slot: local.slot,
                        value_type,
                    },
                    span,
                )
            });
            return (
                Head::Value(Box::new(Out {
                    ty: local.ty.clone(),
                    node,
                })),
                0,
            );
        }
        let program = self.program;
        let Some(mut entity) = program.lookup(self.module, name) else {
            return (Head::Error, 0);
        };
        let mut consumed = 0;
        let mut end = span.end();
        for (suffix, suffix_span) in suffixes {
            let Suffix::Field(field) = suffix else { break };
            match entity {
                Entity::Module(module) => {
                    let Some(next) = program
                        .modules
                        .get(module)
                        .and_then(|data| data.scope.get(field.text()).copied())
                    else {
                        return (Head::Error, consumed + 1);
                    };
                    entity = next;
                    consumed += 1;
                    end = suffix_span.end();
                }
                Entity::Type(index) => {
                    let Some(data) = program.types.get(index) else {
                        return (Head::Error, consumed + 1);
                    };
                    if let Some(&function) = data.members.get(field.text()) {
                        let head = if program.supported_fn(function) {
                            Head::Function(function)
                        } else {
                            self.skipped(
                                data.module,
                                ByteSpan::new(span.start(), suffix_span.end()),
                            )
                        };
                        return (head, consumed + 1);
                    }
                    if data.case(field.text()).is_some() {
                        return (
                            Head::Case(index, field.text().to_owned()),
                            consumed + 1,
                        );
                    }
                    return (Head::Error, consumed + 1);
                }
                Entity::Interface(index) => {
                    let supported = program.interfaces.get(index).is_some_and(|data| {
                        data.id.is_some()
                            && data
                                .members
                                .iter()
                                .any(|member| member.name == field.text())
                    });
                    let head = if supported {
                        Head::Contract(index, field.text().to_owned())
                    } else {
                        Head::Error
                    };
                    return (head, consumed + 1);
                }
                Entity::Fn(_) | Entity::Value(_) | Entity::Other(..) => break,
            }
        }
        let whole = ByteSpan::new(span.start(), end);
        let head = match entity {
            Entity::Fn(index) if program.supported_fn(index) => Head::Function(index),
            Entity::Value(index) if program.supported_value(index) => {
                let ty = program.values.get(index).and_then(|data| data.ty.clone());
                let global = self.layout.globals.get(index).copied().flatten();
                self.references.push(Reference {
                    target: Target::Value(index),
                    span: whole,
                });
                let node =
                    global.map(|global| self.node(IrExpr::Global(global), whole));
                Head::Value(Box::new(Out { ty, node }))
            }
            Entity::Type(_) | Entity::Interface(_) | Entity::Module(_) => Head::Static,
            Entity::Fn(index) => {
                let module = program.functions.get(index).map(|data| data.module);
                module.map_or(Head::Error, |module| self.skipped(module, whole))
            }
            Entity::Value(index) => {
                let module = program.values.get(index).map(|data| data.module);
                module.map_or(Head::Error, |module| self.skipped(module, whole))
            }
            Entity::Other(..) => Head::Error,
        };
        (head, consumed)
    }

    /// A reference to a declaration this checker does not lower: one of the
    /// standard library it skipped is reported here, since nothing else
    /// reports it; a local one was reported at its declaration.
    fn skipped(&mut self, module: usize, span: ByteSpan) -> Head {
        let embedded = self
            .program
            .modules
            .get(module)
            .is_some_and(|data| data.embedded);
        if embedded {
            self.unavailable(span, "this standard-library declaration");
        }
        Head::Error
    }

    fn step_postfix(
        &mut self,
        mut state: Box<PostfixState<'a>>,
        input: Option<Out>,
    ) -> Step<'a> {
        if matches!(state.head, Head::Pending) {
            state.head = Head::Value(Box::new(input.unwrap_or_else(Out::error)));
        }
        while let Some((suffix, suffix_span)) = state.suffixes.get(state.next).cloned()
        {
            state.next += 1;
            let callee_span = ByteSpan::new(state.start, state.end);
            let whole = ByteSpan::new(state.start, suffix_span.end());
            state.end = suffix_span.end();
            let head = std::mem::replace(&mut state.head, Head::Error);
            match suffix {
                Suffix::Field(_) => {
                    if matches!(&head, Head::Value(out) if out.ty.is_some()) {
                        self.unavailable(whole, "a field read");
                    }
                }
                Suffix::Call(args) => {
                    let callee = match head {
                        Head::Function(index) => Callee::Function(index),
                        Head::Case(ty, case) => Callee::Case(ty, case),
                        Head::Contract(interface, member) => {
                            Callee::Contract(interface, member)
                        }
                        Head::Value(out) => {
                            let Out { ty, node } = *out;
                            match &ty {
                                Some(Type::Fn(signature)) => {
                                    Callee::Value(node.map(Box::new), signature.clone())
                                }
                                Some(Type::Never) => {
                                    self.report(
                                        DiagnosticCode::TypeUnreachableCode,
                                        callee_span,
                                        "this callee never completes",
                                    );
                                    continue;
                                }
                                Some(ty) => {
                                    self.report(
                                        DiagnosticCode::TypeNotApplicable,
                                        callee_span,
                                        format!(
                                            "a value of type `{ty}` cannot be applied"
                                        ),
                                    );
                                    continue;
                                }
                                None => continue,
                            }
                        }
                        Head::Static => {
                            self.unavailable(callee_span, "applying a type");
                            continue;
                        }
                        Head::Pending | Head::Error => continue,
                    };
                    let call = self.call_state(callee, args, whole, callee_span);
                    state.head = Head::Pending;
                    return Step::Child(Task::Postfix(state), Task::Call(call));
                }
            }
        }
        let whole = ByteSpan::new(state.start, state.end);
        let out = match std::mem::replace(&mut state.head, Head::Error) {
            Head::Value(out) => *out,
            Head::Function(index) => self.function_value(index, whole),
            Head::Case(..) => {
                self.unavailable(whole, "a constructor used as a value");
                Out::error()
            }
            Head::Contract(..) => {
                self.unavailable(whole, "an interface member used as a value");
                Out::error()
            }
            Head::Static => {
                self.unavailable(whole, "a type used as a value");
                Out::error()
            }
            Head::Pending | Head::Error => Out::error(),
        };
        let _ = &state.ctx;
        Step::Done(out)
    }

    fn function_value(&mut self, index: usize, span: ByteSpan) -> Out {
        let program = self.program;
        let Some(signature) = program
            .functions
            .get(index)
            .and_then(|data| data.signature.clone())
        else {
            return Out::error();
        };
        self.references.push(Reference {
            target: Target::Function(index),
            span,
        });
        let node =
            self.layout
                .functions
                .get(index)
                .copied()
                .flatten()
                .map(|function| {
                    self.node(
                        IrExpr::Function {
                            function,
                            type_arguments: Vec::new(),
                        },
                        span,
                    )
                });
        Out {
            ty: Some(Type::function(signature)),
            node,
        }
    }

    fn call_state(
        &mut self,
        callee: Callee,
        args: Vec<Argument<'a>>,
        application: ByteSpan,
        callee_span: ByteSpan,
    ) -> Box<CallState<'a>> {
        let program = self.program;
        let (params, result) = match &callee {
            Callee::Function(index) => match program
                .functions
                .get(*index)
                .and_then(|data| data.signature.clone())
            {
                Some(signature) => (Some(signature.parameters), Some(signature.result)),
                None => (None, None),
            },
            Callee::Value(_, signature) => (
                Some(signature.parameters.clone()),
                Some(signature.result.clone()),
            ),
            Callee::Case(ty, _) => (
                Some(Vec::new()),
                program.types.get(*ty).and_then(|data| data.self_type()),
            ),
            Callee::Contract(..) => (None, None),
        };
        let mut state = Box::new(CallState {
            callee,
            args,
            next: 0,
            params,
            result,
            receiver: None,
            nodes: Vec::new(),
            application,
            callee_span,
            failed: false,
            never_reported: false,
        });
        for arg in &state.args {
            if let Some(label) = arg.label {
                let span =
                    ByteSpan::new(label.span().start(), expr_span(&arg.value).end());
                self.report(
                    DiagnosticCode::TypeArgumentMismatch,
                    span,
                    format!("the callee takes no labelled argument `{}`", label.text()),
                );
                state.failed = true;
            }
        }
        state
    }

    fn check_arity(&mut self, state: &mut CallState<'a>) {
        if let Some(params) = &state.params
            && params.len() != state.args.len()
        {
            let message = format!(
                "the callee takes {} argument(s), but {} are written",
                params.len(),
                state.args.len()
            );
            self.report(
                DiagnosticCode::TypeArgumentMismatch,
                state.application,
                message,
            );
            state.failed = true;
        }
    }

    fn step_call(
        &mut self,
        mut state: Box<CallState<'a>>,
        input: Option<Out>,
    ) -> Step<'a> {
        if state.next == 0
            && input.is_none()
            && !matches!(state.callee, Callee::Contract(..))
        {
            self.check_arity(&mut state);
        }
        if let Some(out) = input {
            let at = state.next.saturating_sub(1);
            let span = state
                .args
                .get(at)
                .map_or(state.application, |arg| expr_span(&arg.value));
            let node = match out.ty {
                Some(Type::Never) => {
                    if !state.never_reported {
                        self.report(
                            DiagnosticCode::TypeUnreachableCode,
                            span,
                            "this argument never completes, so the call is never made",
                        );
                        state.never_reported = true;
                    }
                    state.failed = true;
                    None
                }
                Some(actual) => {
                    if let Callee::Contract(interface, member) = &state.callee
                        && at == 0
                        && state.receiver.is_none()
                    {
                        let (interface, member) = (*interface, member.clone());
                        self.contract_receiver(
                            &mut state, interface, &member, &actual, span,
                        );
                    }
                    match state
                        .params
                        .as_ref()
                        .and_then(|params| params.get(at))
                        .cloned()
                    {
                        Some(expected) => self.conform(
                            &expected,
                            &actual,
                            out.node,
                            span,
                            DiagnosticCode::TypeArgumentMismatch,
                        ),
                        None => {
                            state.failed = true;
                            None
                        }
                    }
                }
                None => {
                    state.failed = true;
                    None
                }
            };
            if node.is_none() {
                state.failed = true;
            }
            state.nodes.push(node);
        }
        if let Some(arg) = state.args.get(state.next) {
            let value = arg.value.clone();
            let at = state.next;
            state.next += 1;
            let expected = state
                .params
                .as_ref()
                .and_then(|params| params.get(at))
                .cloned();
            let skip_rest = matches!(state.callee, Callee::Contract(..))
                && at > 0
                && state.receiver.is_none();
            if skip_rest {
                state.failed = true;
                state.nodes.push(None);
                return self.step_call(state, None);
            }
            return Step::Child(
                Task::Call(state),
                Task::Expr(value, Ctx::Value(expected)),
            );
        }
        if let Callee::Contract(..) = state.callee
            && state.args.is_empty()
        {
            self.report(
                DiagnosticCode::TypeArgumentMismatch,
                state.application,
                "an interface member is applied to its receiver first",
            );
            state.failed = true;
        }
        Step::Done(self.finish_call(*state))
    }

    fn contract_receiver(
        &mut self,
        state: &mut CallState<'a>,
        interface: usize,
        member: &str,
        receiver: &Type,
        span: ByteSpan,
    ) {
        let program = self.program;
        let Some(data) = program.interfaces.get(interface) else {
            return;
        };
        let Some(declared) =
            data.members.iter().find(|declared| declared.name == member)
        else {
            return;
        };
        if declared.receiver != Some(0) {
            self.unavailable(
                state.application,
                "an interface member without a receiver",
            );
            state.failed = true;
            return;
        }
        match program.implementation(interface, receiver) {
            Some(_) => {
                let signature = substitute_self(&declared.signature, receiver);
                state.params = Some(signature.parameters);
                state.result = Some(signature.result);
                state.receiver = Some(receiver.clone());
                self.check_arity(state);
            }
            None => {
                self.report(
                    DiagnosticCode::TypeUnsatisfiedBound,
                    span,
                    format!("`{receiver}` does not implement `{}`", data.name),
                );
                state.failed = true;
            }
        }
    }

    fn finish_call(&mut self, state: CallState<'a>) -> Out {
        let program = self.program;
        let CallState {
            callee,
            nodes,
            result,
            receiver,
            application,
            callee_span,
            failed,
            ..
        } = state;
        let mut positional = Vec::new();
        let mut complete = !failed;
        for node in nodes {
            match node {
                Some(node) => positional.push(node),
                None => complete = false,
            }
        }
        let expr =
            match callee {
                Callee::Function(index) => {
                    self.references.push(Reference {
                        target: Target::Function(index),
                        span: callee_span,
                    });
                    self.layout.functions.get(index).copied().flatten().map(
                        |function| IrExpr::Call {
                            target: CallTarget::Direct {
                                function,
                                type_arguments: Vec::new(),
                            },
                            arguments: Arguments {
                                positional,
                                labelled: Vec::new(),
                            },
                        },
                    )
                }
                Callee::Value(node, _) => node.map(|node| IrExpr::Call {
                    target: CallTarget::Value(node),
                    arguments: Arguments {
                        positional,
                        labelled: Vec::new(),
                    },
                }),
                Callee::Case(ty, case) => {
                    let value_type =
                        program.types.get(ty).and_then(|data| data.self_type());
                    value_type.map(|value_type| IrExpr::Construct {
                        value_type,
                        construction: Construction::Case {
                            case,
                            fields: Vec::new(),
                        },
                    })
                }
                Callee::Contract(interface, member) => {
                    let id = program
                        .interfaces
                        .get(interface)
                        .and_then(|data| data.id.clone());
                    if let Some(receiver) = &receiver
                        && let Some(implementation) =
                            program.implementation(interface, receiver)
                        && let Some((_, function)) = implementation
                            .members
                            .iter()
                            .find(|(name, _)| *name == member)
                    {
                        self.references.push(Reference {
                            target: Target::Function(*function),
                            span: callee_span,
                        });
                    }
                    match (id, receiver) {
                        (Some(interface), Some(receiver)) => Some(IrExpr::Call {
                            target: CallTarget::Contract(Box::new(ContractCall {
                                interface,
                                member,
                                receiver,
                                arguments: Vec::new(),
                                choices: NamedTypes::empty(),
                                member_arguments: Vec::new(),
                            })),
                            arguments: Arguments {
                                positional,
                                labelled: Vec::new(),
                            },
                        }),
                        _ => None,
                    }
                }
            };
        let node = if complete {
            expr.map(|expr| self.node(expr, application))
        } else {
            None
        };
        Out { ty: result, node }
    }

    fn literal(&mut self, node: &CstNode, expected: Option<&Type>) -> Out {
        let span = node.span();
        let Some(literal) = literal::read(node, self.text) else {
            return Out::error();
        };
        match literal::evaluate(&literal, expected, self.program.str_type.as_ref()) {
            Ok((ty, value)) => match Constant::new(ty.clone(), value) {
                Ok(constant) => Out {
                    node: Some(self.node(IrExpr::Literal(constant), span)),
                    ty: Some(ty),
                },
                Err(_) => Out::error(),
            },
            Err(LiteralFault::Ambiguous) => {
                let diagnostic = Diagnostic::new(
                    DiagnosticCode::TypeAmbiguousInference,
                    span,
                    "nothing fixes the type of this literal",
                )
                .with_source_id(self.source_id.as_ref())
                .with_note(
                    "a suffix on the literal, a written type on its binding, or an \
                     expected type from its position would fix its type",
                );
                self.diagnostics.push(diagnostic);
                Out::error()
            }
            Err(LiteralFault::OutOfRange(ty)) => {
                self.report(
                    DiagnosticCode::TypeNumericOutOfRange,
                    span,
                    format!("the literal does not fit `{ty}`"),
                );
                Out::error()
            }
            Err(LiteralFault::Mismatch(ty)) => {
                self.report(
                    DiagnosticCode::TypeMismatch,
                    span,
                    format!("expected `{ty}`, found a numeric literal"),
                );
                Out::error()
            }
            Err(LiteralFault::Malformed) => Out::error(),
        }
    }

    fn step_if(&mut self, mut state: Box<IfState<'a>>, input: Option<Out>) -> Step<'a> {
        let span = state.view.node().span();
        match state.stage {
            0 => {
                state.stage = 1;
                let Some(condition) = state.view.condition() else {
                    return Step::Done(Out::error());
                };
                if state.view.else_body().is_none()
                    && matches!(state.ctx, Ctx::Value(_))
                {
                    self.report(
                        DiagnosticCode::TypeMismatch,
                        span,
                        "an `if` whose value is used needs an `else`",
                    );
                    state.failed = true;
                }
                let expected = self.program.bool_type.clone();
                Step::Child(
                    Task::If(state),
                    Task::Expr(condition, Ctx::Value(expected)),
                )
            }
            1 => {
                state.stage = 2;
                let out = input.unwrap_or_else(Out::error);
                let condition_span = state
                    .view
                    .condition()
                    .map_or(span, |condition| expr_span(&condition));
                state.condition = match (out.ty, self.program.bool_type.clone()) {
                    (Some(Type::Never), _) => {
                        self.report(
                            DiagnosticCode::TypeUnreachableCode,
                            condition_span,
                            "this condition never completes",
                        );
                        None
                    }
                    (Some(actual), Some(expected)) => self.conform(
                        &expected,
                        &actual,
                        out.node,
                        condition_span,
                        DiagnosticCode::TypeMismatch,
                    ),
                    _ => None,
                };
                if state.condition.is_none() {
                    state.failed = true;
                }
                let Some(then_body) = state.view.then_body() else {
                    return Step::Done(Out::error());
                };
                let mode = BodyMode::Branch(state.ctx.clone());
                let child = Task::Body(Box::new(self.body_state(then_body, mode)));
                Step::Child(Task::If(state), child)
            }
            2 => {
                state.stage = 3;
                let then = input.unwrap_or_else(Out::error);
                let Some(else_body) = state.view.else_body() else {
                    let node = match (state.failed, state.condition.take(), then.node) {
                        (false, Some(condition), Some(then_node)) => Some(self.node(
                            IrExpr::If {
                                condition: Box::new(condition),
                                then_branch: Box::new(then_node),
                                else_branch: None,
                            },
                            span,
                        )),
                        _ => None,
                    };
                    let ty = match state.ctx {
                        Ctx::Stmt => Some(Type::Void),
                        Ctx::Value(_) => None,
                    };
                    return Step::Done(Out { ty, node });
                };
                let expected = match &state.ctx {
                    Ctx::Stmt => Ctx::Stmt,
                    Ctx::Value(Some(expected)) => Ctx::Value(Some(expected.clone())),
                    Ctx::Value(None) => {
                        Ctx::Value(then.ty.clone().filter(|ty| *ty != Type::Never))
                    }
                };
                state.then = Some(then);
                let child = Task::Body(Box::new(
                    self.body_state(else_body, BodyMode::Branch(expected)),
                ));
                Step::Child(Task::If(state), child)
            }
            _ => {
                let otherwise = input.unwrap_or_else(Out::error);
                let then = state.then.take().unwrap_or_else(Out::error);
                let ty = join(&state.ctx, &[then.ty.clone(), otherwise.ty.clone()]);
                let node = match (
                    state.failed,
                    state.condition.take(),
                    then.node,
                    otherwise.node,
                ) {
                    (false, Some(condition), Some(then_node), Some(else_node)) => {
                        Some(self.node(
                            IrExpr::If {
                                condition: Box::new(condition),
                                then_branch: Box::new(then_node),
                                else_branch: Some(Box::new(else_node)),
                            },
                            span,
                        ))
                    }
                    _ => None,
                };
                Step::Done(Out { ty, node })
            }
        }
    }

    fn step_match(
        &mut self,
        mut state: Box<MatchState<'a>>,
        input: Option<Out>,
    ) -> Step<'a> {
        let span = state.view.node().span();
        if state.stage == 0 {
            state.stage = 1;
            let Some(subject) = state.view.subject() else {
                return Step::Done(Out::error());
            };
            if state.arms.is_empty() {
                return Step::Done(Out::error());
            }
            return Step::Child(
                Task::Match(state),
                Task::Expr(subject, Ctx::Value(None)),
            );
        }
        let Some(out) = input else {
            return Step::Done(Out::error());
        };
        if state.stage == 1 {
            state.stage = 2;
            let subject_span = state
                .view
                .subject()
                .map_or(span, |subject| expr_span(&subject));
            match out.ty {
                Some(Type::Never) => {
                    self.report(
                        DiagnosticCode::TypeUnreachableCode,
                        subject_span,
                        "this subject never completes",
                    );
                    state.failed = true;
                }
                Some(ty) => {
                    state.cases = self.variant_cases(&ty);
                    if let Some((_, cases)) = &state.cases {
                        state.covered = vec![None; cases.len()];
                    }
                    state.subject = Some(ty);
                }
                None => state.failed = true,
            }
            state.subject_node = out.node;
            if state.subject_node.is_none() {
                state.failed = true;
            }
        } else {
            // An arm's body arrived.
            self.locals.truncate(state.mark);
            let pattern = state.pending_pattern.take();
            state.types.push(out.ty.clone());
            if let Ctx::Value(None) = state.ctx
                && state.join.is_none()
                && let Some(ty) = out.ty.clone().filter(|ty| *ty != Type::Never)
            {
                state.join = Some(ty);
            }
            let arm = match (pattern, out.node) {
                (Some(pattern), Some(body)) => Some(MatchArm { pattern, body }),
                _ => None,
            };
            if arm.is_none() {
                state.failed = true;
            }
            state.lowered.push(arm);
        }
        if let Some(arm) = state.arms.get(state.next).cloned() {
            state.next += 1;
            state.mark = self.locals.len();
            let pattern_span = arm_pattern_span(&arm).unwrap_or(arm.node().span());
            let pattern = arm
                .pattern()
                .and_then(|pattern| self.pattern(pattern, pattern_span, &mut state));
            if pattern.is_none() {
                state.failed = true;
            }
            state.pending_pattern = pattern;
            let Some(body) = arm.body() else {
                state.failed = true;
                return self.step_match(state, Some(Out::error()));
            };
            let ctx = match &state.ctx {
                Ctx::Stmt => Ctx::Stmt,
                Ctx::Value(Some(expected)) => Ctx::Value(Some(expected.clone())),
                Ctx::Value(None) => Ctx::Value(state.join.clone()),
            };
            let child =
                Task::Body(Box::new(self.body_state(body, BodyMode::Branch(ctx))));
            return Step::Child(Task::Match(state), child);
        }
        // Coverage.
        if state.subject.is_some() {
            let uncovered = if state.covered_all.is_some() {
                None
            } else if let Some((owner, cases)) = &state.cases {
                state
                    .covered
                    .iter()
                    .position(Option::is_none)
                    .and_then(|at| cases.get(at))
                    .map(|case| format!("{owner}.{case}()"))
            } else {
                Some("_".to_owned())
            };
            if let Some(shape) = uncovered {
                let diagnostic = Diagnostic::new(
                    DiagnosticCode::PatternNonExhaustive,
                    span,
                    "this `match` does not cover its subject",
                )
                .with_source_id(self.source_id.as_ref())
                .with_note(shape);
                self.diagnostics.push(diagnostic);
                state.failed = true;
            }
        }
        let ty = join(&state.ctx, &state.types);
        let node = match (state.failed, state.subject_node.take(), ty.clone()) {
            (false, Some(subject), Some(value_type)) => {
                let arms = std::mem::take(&mut state.lowered)
                    .into_iter()
                    .collect::<Option<Vec<_>>>();
                arms.map(|arms| {
                    self.node(
                        IrExpr::Match {
                            subject: Box::new(subject),
                            arms,
                            value_type,
                        },
                        span,
                    )
                })
            }
            _ => None,
        };
        Step::Done(Out { ty, node })
    }

    /// The owner name and the cases of a variant subject type.
    fn variant_cases(&self, ty: &Type) -> Option<(String, Vec<String>)> {
        let Type::Declared(id, _) = ty else {
            return None;
        };
        self.program
            .types
            .iter()
            .find_map(|data| match (&data.kind, &data.id) {
                (TypeKind::Variant(cases), Some(own)) if own == id => Some((
                    data.name.clone(),
                    cases.iter().map(|case| case.name.clone()).collect(),
                )),
                _ => None,
            })
    }

    /// Checks one arm's pattern against the subject, records its coverage,
    /// and binds its binder.
    fn pattern(
        &mut self,
        pattern: Pattern<'a>,
        span: ByteSpan,
        state: &mut MatchState<'a>,
    ) -> Option<IrPattern> {
        let (cover, lowered) = match pattern.form() {
            PatternForm::Discard => (Cover::All, Some(IrPattern::Discard)),
            PatternForm::Path(path) => {
                let segments = path.segments();
                match segments.as_slice() {
                    [name] => match self.program.lookup(self.module, name.text()) {
                        Some(Entity::Value(index)) => {
                            self.constant_pattern(index, span, state)
                        }
                        Some(Entity::Fn(_) | Entity::Other(..)) => {
                            (Cover::Nothing, None)
                        }
                        _ => {
                            let slot = self.new_slot(state.subject.as_ref());
                            self.locals.push(Local {
                                name: name.text().to_owned(),
                                slot,
                                ty: state.subject.clone(),
                            });
                            (Cover::All, Some(IrPattern::Bind(slot)))
                        }
                    },
                    _ => {
                        let names = segments
                            .iter()
                            .map(|segment| segment.text())
                            .collect::<Vec<_>>();
                        self.path_pattern(&names, span, state)
                    }
                }
            }
            PatternForm::Apply {
                path,
                positional,
                labelled,
            } => {
                if !positional.is_empty() || !labelled.is_empty() {
                    self.unavailable(span, "a pattern with fields");
                    (Cover::Nothing, None)
                } else {
                    let segments = path.segments();
                    let names = segments
                        .iter()
                        .map(|segment| segment.text())
                        .collect::<Vec<_>>();
                    match self.case_of(&names) {
                        Some((ty, case)) => self.case_pattern(ty, &case, span, state),
                        None => (Cover::Nothing, None),
                    }
                }
            }
            PatternForm::Literal(_) => {
                self.unavailable(span, "a literal pattern");
                (Cover::Nothing, None)
            }
            PatternForm::Tuple(_) | PatternForm::Record(_) | PatternForm::List(_) => {
                self.unavailable(span, "a product or list pattern");
                (Cover::Nothing, None)
            }
        };
        if state.subject.is_some() {
            self.cover(cover, span, state);
        }
        lowered
    }

    fn cover(&mut self, cover: Cover, span: ByteSpan, state: &mut MatchState<'a>) {
        let earlier = match cover {
            Cover::Nothing => return,
            Cover::All => state.covered_all.or_else(|| {
                let all_cases = !state.covered.is_empty()
                    && state.covered.iter().all(Option::is_some);
                all_cases.then_some(state.first_cover).flatten()
            }),
            Cover::Case(at) => state
                .covered_all
                .or_else(|| state.covered.get(at).copied().flatten()),
        };
        if let Some(earlier) = earlier {
            let diagnostic = Diagnostic::new(
                DiagnosticCode::PatternUnreachableArm,
                span,
                "no value reaches this arm",
            )
            .with_source_id(self.source_id.as_ref())
            .with_related(earlier, "an earlier arm already covers it");
            self.diagnostics.push(diagnostic);
            state.failed = true;
            return;
        }
        state.first_cover.get_or_insert(span);
        match cover {
            Cover::All => state.covered_all = Some(span),
            Cover::Case(at) => {
                if let Some(slot) = state.covered.get_mut(at) {
                    *slot = Some(span);
                }
            }
            Cover::Nothing => {}
        }
    }

    /// The type and case a dotted constructor path names.
    fn case_of(&self, names: &[&str]) -> Option<(usize, String)> {
        let (case, owner) = names.split_last()?;
        let entity = self.static_path(owner)?;
        let Entity::Type(ty) = entity else {
            return None;
        };
        let data = self.program.types.get(ty)?;
        data.case(case).map(|_| (ty, (*case).to_owned()))
    }

    fn static_path(&self, names: &[&str]) -> Option<Entity> {
        let (first, rest) = names.split_first()?;
        let mut entity = self.program.lookup(self.module, first)?;
        for name in rest {
            let Entity::Module(module) = entity else {
                return None;
            };
            entity = self
                .program
                .modules
                .get(module)?
                .scope
                .get(*name)
                .copied()?;
        }
        Some(entity)
    }

    fn path_pattern(
        &mut self,
        names: &[&str],
        span: ByteSpan,
        state: &mut MatchState<'a>,
    ) -> (Cover, Option<IrPattern>) {
        if let Some((ty, case)) = self.case_of(names) {
            return self.case_pattern(ty, &case, span, state);
        }
        match self.static_path(names) {
            Some(Entity::Value(index)) => self.constant_pattern(index, span, state),
            _ => (Cover::Nothing, None),
        }
    }

    fn case_pattern(
        &mut self,
        ty: usize,
        case: &str,
        span: ByteSpan,
        state: &MatchState<'a>,
    ) -> (Cover, Option<IrPattern>) {
        let Some(data) = self.program.types.get(ty) else {
            return (Cover::Nothing, None);
        };
        let own = data.self_type();
        if let (Some(subject), Some(own)) = (&state.subject, &own)
            && subject != own
        {
            self.report(
                DiagnosticCode::TypeMismatch,
                span,
                format!("expected a pattern of `{subject}`, found one of `{own}`"),
            );
            return (Cover::Nothing, None);
        }
        let at = data
            .cases()
            .and_then(|cases| cases.iter().position(|known| known.name == case));
        let cover = at.map_or(Cover::Nothing, Cover::Case);
        (
            cover,
            Some(IrPattern::Case {
                case: case.to_owned(),
                fields: Vec::new(),
            }),
        )
    }

    fn constant_pattern(
        &mut self,
        index: usize,
        span: ByteSpan,
        state: &MatchState<'a>,
    ) -> (Cover, Option<IrPattern>) {
        let Some((ty, case)) = self.program.constant_case(index) else {
            self.unavailable(span, "this constant pattern");
            return (Cover::Nothing, None);
        };
        let (cover, _) = self.case_pattern(ty, &case, span, state);
        if cover == Cover::Nothing {
            return (Cover::Nothing, None);
        }
        let Some(own) = self.program.types.get(ty).and_then(|data| data.self_type())
        else {
            return (Cover::Nothing, None);
        };
        let value = Value::Variant {
            case,
            fields: Vec::new(),
        };
        match Constant::new(own, value) {
            Ok(constant) => (cover, Some(IrPattern::Literal(constant))),
            Err(_) => (Cover::Nothing, None),
        }
    }

    fn body_state(&self, body: Body<'a>, mode: BodyMode) -> BodyState<'a> {
        BodyState {
            span: body.node().span(),
            statements: body.statements(),
            next: 0,
            mode,
            mark: self.locals.len(),
            pending: Pending::Nothing,
            never_at: None,
            unreachable_reported: false,
            pieces: Vec::new(),
            last_type: None,
            failed: false,
        }
    }

    fn step_body(
        &mut self,
        mut state: Box<BodyState<'a>>,
        input: Option<Out>,
    ) -> Step<'a> {
        if let Some(out) = input {
            self.arrive(&mut state, out);
        }
        if let Some(statement) = state.statements.get(state.next).cloned() {
            let last = state.next + 1 == state.statements.len();
            state.next += 1;
            let span = statement_span(&statement);
            if let Some(never) = state.never_at
                && !state.unreachable_reported
            {
                let diagnostic = Diagnostic::new(
                    DiagnosticCode::TypeUnreachableCode,
                    span,
                    "this statement is never reached",
                )
                .with_source_id(self.source_id.as_ref())
                .with_related(never, "this statement cannot complete");
                self.diagnostics.push(diagnostic);
                state.unreachable_reported = true;
                state.failed = true;
            }
            match statement {
                Statement::Let(view) => {
                    let binder = match view.pattern().map(|pattern| pattern.form()) {
                        Some(PatternForm::Discard) => None,
                        Some(PatternForm::Path(path)) if path.segments().len() == 1 => {
                            path.segments().first().map(|name| name.text().to_owned())
                        }
                        Some(_) => {
                            let pattern_span = view
                                .pattern()
                                .map_or(span, |pattern| pattern.node().span());
                            self.unavailable(pattern_span, "this binding pattern");
                            state.failed = true;
                            None
                        }
                        None => {
                            state.failed = true;
                            None
                        }
                    };
                    if view.fallback().is_some() {
                        self.unavailable(span, "`let … else`");
                        state.failed = true;
                    }
                    let written = match view.written_type() {
                        Some(written) => self.written(written),
                        None => None,
                    };
                    if view.written_type().is_some() && written.is_none() {
                        state.failed = true;
                    }
                    let Some(value) = view.value() else {
                        state.failed = true;
                        state.pieces.push(None);
                        return self.step_body(state, None);
                    };
                    state.pending = Pending::Let {
                        binder,
                        written: written.clone(),
                        value_span: expr_span(&value),
                    };
                    Step::Child(
                        Task::Body(state),
                        Task::Expr(value, Ctx::Value(written)),
                    )
                }
                Statement::Expr(expr) => {
                    let ctx = match (&state.mode, last) {
                        (BodyMode::Branch(Ctx::Value(expected)), true) => {
                            Ctx::Value(expected.clone())
                        }
                        _ => Ctx::Stmt,
                    };
                    state.pending = Pending::Expr {
                        span: expr_span(&expr),
                        last,
                    };
                    Step::Child(Task::Body(state), Task::Expr(expr, ctx))
                }
            }
        } else {
            Step::Done(self.finish_body(*state))
        }
    }

    fn arrive(&mut self, state: &mut BodyState<'a>, out: Out) {
        match std::mem::replace(&mut state.pending, Pending::Nothing) {
            Pending::Nothing => {}
            Pending::Let {
                binder,
                written,
                value_span,
            } => {
                let (ty, node) = match out.ty {
                    Some(Type::Never) => {
                        self.report(
                            DiagnosticCode::TypeUnreachableCode,
                            value_span,
                            "this initializer never completes, so nothing is bound",
                        );
                        (None, None)
                    }
                    Some(actual) => match &written {
                        Some(expected) => {
                            let node = self.conform(
                                expected,
                                &actual,
                                out.node,
                                value_span,
                                DiagnosticCode::TypeMismatch,
                            );
                            (Some(expected.clone()), node)
                        }
                        None => (Some(actual), out.node),
                    },
                    None => (written, None),
                };
                let slot = binder.as_ref().map(|_| self.new_slot(ty.as_ref()));
                if let (Some(name), Some(slot)) = (binder, slot) {
                    self.locals.push(Local {
                        name,
                        slot,
                        ty: ty.clone(),
                    });
                }
                state.last_type = None;
                if node.is_none() {
                    state.failed = true;
                }
                state.pieces.push(node.map(|node| Piece::Let(slot, node)));
            }
            Pending::Expr { span, last } => {
                let mut node = out.node;
                let mut ty = out.ty;
                if ty == Some(Type::Never) && state.never_at.is_none() {
                    state.never_at = Some(span);
                }
                if last {
                    match &state.mode {
                        BodyMode::Branch(Ctx::Value(Some(expected))) => {
                            if let Some(actual) = &ty {
                                node = self.conform(
                                    expected,
                                    actual,
                                    node,
                                    span,
                                    DiagnosticCode::TypeMismatch,
                                );
                                if *actual != Type::Never {
                                    ty = Some(expected.clone());
                                }
                            }
                        }
                        BodyMode::Branch(Ctx::Stmt) => {
                            if let Some(actual) = &ty
                                && !matches!(actual, Type::Void | Type::Never)
                            {
                                self.report(
                                    DiagnosticCode::TypeMismatch,
                                    span,
                                    format!(
                                        "a branch whose value is not used has type `Void` or \
                                         `Never`, found `{actual}`"
                                    ),
                                );
                                node = None;
                            }
                        }
                        BodyMode::Branch(Ctx::Value(None)) | BodyMode::Callable => {}
                    }
                    state.last_type = Some(ty);
                } else {
                    state.last_type = None;
                }
                if node.is_none() {
                    state.failed = true;
                }
                state.pieces.push(node.map(Piece::Expr));
            }
        }
    }

    fn finish_body(&mut self, state: BodyState<'a>) -> Out {
        self.locals.truncate(state.mark);
        let mut failed = state.failed;
        let ty = if state.never_at.is_some() {
            Some(Type::Never)
        } else {
            match &state.mode {
                BodyMode::Callable => {
                    self.report(
                        DiagnosticCode::TypeMissingReturn,
                        state.span,
                        "this body can run off its end without a `return`",
                    );
                    failed = true;
                    None
                }
                BodyMode::Branch(Ctx::Stmt) => Some(Type::Void),
                BodyMode::Branch(Ctx::Value(expected)) => match state.last_type.clone()
                {
                    Some(ty) => ty,
                    None => {
                        if let Some(expected) = expected
                            && *expected != Type::Void
                        {
                            self.report(
                                DiagnosticCode::TypeMismatch,
                                state.span,
                                format!("expected `{expected}`, found `Void`: the body has no final expression"),
                            );
                            failed = true;
                            Some(expected.clone())
                        } else {
                            Some(Type::Void)
                        }
                    }
                },
            }
        };
        if failed {
            return Out { ty, node: None };
        }
        let pieces = state.pieces.into_iter().collect::<Option<Vec<_>>>();
        let node = pieces.map(|pieces| self.sequence(pieces, state.span));
        Out { ty, node }
    }

    /// Lowers the statements of a body, from the last to the first, into
    /// sequences and `let` nodes.
    fn sequence(&self, pieces: Vec<Piece>, span: ByteSpan) -> Node {
        let mut tail: Vec<Node> = Vec::new();
        for piece in pieces.into_iter().rev() {
            match piece {
                Piece::Expr(node) => tail.push(node),
                Piece::Let(slot, value) => {
                    let body = self.close(std::mem::take(&mut tail), span);
                    let let_span = value.origin.span();
                    tail.push(self.node(
                        IrExpr::Let {
                            slot,
                            value: Box::new(value),
                            body: Box::new(body),
                        },
                        let_span,
                    ));
                }
            }
        }
        self.close(tail, span)
    }

    /// One node for statements collected in reverse order.
    fn close(&self, mut reversed: Vec<Node>, span: ByteSpan) -> Node {
        reversed.reverse();
        if reversed.len() == 1
            && let Some(node) = reversed.pop()
        {
            return node;
        }
        if reversed.is_empty() {
            let void = Constant::new(Type::Void, Value::Void);
            return match void {
                Ok(constant) => self.node(IrExpr::Literal(constant), span),
                Err(_) => self.node(IrExpr::Sequence(Vec::new()), span),
            };
        }
        self.node(IrExpr::Sequence(reversed), span)
    }
}

/// The type of an `if` or `match` from its branches' types.
fn join(ctx: &Ctx, branches: &[Option<Type>]) -> Option<Type> {
    if branches.iter().all(|ty| *ty == Some(Type::Never)) {
        return Some(Type::Never);
    }
    match ctx {
        Ctx::Stmt => Some(Type::Void),
        Ctx::Value(Some(expected)) => Some(expected.clone()),
        Ctx::Value(None) => {
            if branches.iter().any(Option::is_none) {
                return None;
            }
            branches
                .iter()
                .flatten()
                .find(|ty| **ty != Type::Never)
                .cloned()
        }
    }
}

/// The span of an expression: its first prefix to the end of its form.
pub(crate) fn expr_span(expr: &Expr<'_>) -> ByteSpan {
    let end = expr.node().span().end();
    let start = expr
        .prefixes()
        .first()
        .map_or(expr.node().span().start(), |prefix| prefix.span().start());
    ByteSpan::new(start, end)
}

fn statement_span(statement: &Statement<'_>) -> ByteSpan {
    match statement {
        Statement::Let(view) => view.node().span(),
        Statement::Expr(expr) => expr_span(expr),
    }
}

/// `span` without leading whitespace.
fn trim_start(text: &str, span: ByteSpan) -> ByteSpan {
    let slice = text.get(span.start()..span.end()).unwrap_or_default();
    let skipped = slice.len() - slice.trim_start().len();
    ByteSpan::new(span.start().saturating_add(skipped), span.end())
}

fn significant(node: &CstNode) -> Vec<&CstNode> {
    node.children()
        .iter()
        .filter(|child| {
            !matches!(
                child.kind(),
                SyntaxKind::Whitespace
                    | SyntaxKind::Comment
                    | SyntaxKind::Error
                    | SyntaxKind::Missing
            )
        })
        .collect()
}

fn primary_node<'a>(postfix: Postfix<'a>) -> Option<&'a CstNode> {
    significant(postfix.node()).first().copied()
}

/// The suffixes of a postfix form, each with its span: the parentheses of a
/// call, or the `.` and name of a field read.
fn suffix_spans<'a>(
    postfix: Postfix<'a>,
    text: &'a str,
) -> Vec<(Suffix<'a>, ByteSpan)> {
    let suffixes = postfix.suffixes();
    let items = significant(postfix.node());
    let mut spans = Vec::new();
    let mut at = 1;
    while let Some(item) = items.get(at) {
        if item.kind() == SyntaxKind::Paren {
            spans.push(item.span());
            at += 1;
        } else if let Some(name) = items.get(at + 1) {
            spans.push(ByteSpan::new(item.span().start(), name.span().end()));
            at += 2;
        } else {
            break;
        }
    }
    let _ = text;
    suffixes.into_iter().zip(spans).collect()
}

/// The span of an arm's pattern: from after `case` to the end of the last
/// node before the arm's body.
fn arm_pattern_span(arm: &ArmView<'_>) -> Option<ByteSpan> {
    let items = significant(arm.node());
    let pattern = items
        .iter()
        .skip(1)
        .take_while(|item| item.kind() != SyntaxKind::Body)
        .collect::<Vec<_>>();
    let first = pattern.first()?;
    let last = pattern.last()?;
    Some(ByteSpan::new(first.span().start(), last.span().end()))
}
