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

mod patterns;

use std::rc::Rc;
use std::sync::Arc;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::external::compiler::{CompilerIntrinsic, is_assertion};
use vibra_ir::{
    Arguments, CallTarget, Constant, Construction, ContractCall, Expr as IrExpr,
    FnType, Library, MatchArm, NamedTypes, Node, Pattern as IrPattern, SourceOrigin,
    Type, Value,
};
use vibra_syntax::source::{
    Argument, Body, Expr, ExprForm, If, Match, MatchArm as ArmView, Pattern, Postfix,
    Primary, Statement, Suffix, TypeExpr,
};
use vibra_syntax::{CstNode, SyntaxKind};

use super::declare::{
    Entity, NoType, Program, Scope, TypeKind, key_diagnostic, substitute_self,
};
use super::literal::{self, LiteralFault};
use super::pattern::{Coverage, LimitReached, Pat};
use patterns::pattern_span;

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
    /// A test, by its program index. Nothing refers to a test.
    Test(usize),
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
    /// The fallback of a `let … else`: every path MUST leave.
    Fallback,
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
    /// A type, by its program index.
    Type(usize),
    /// An interface or a module.
    Static,
    /// Already reported.
    Error,
}

/// What a call calls.
#[derive(Debug)]
enum Callee {
    Function(usize),
    Value(Option<Box<Node>>, Box<FnType>),
    /// A case of a variant type.
    Case(usize, String),
    /// A wrapper, declared tuple, or declared record.
    Construct(usize),
    Contract(usize, String),
    /// A lookup in a collection of the given type.
    Lookup(Option<Box<Node>>, Box<Type>),
}

/// What a tuple, record, list, or dict literal builds.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Product {
    Tuple,
    /// A record, with each field's name in written order.
    Record(Vec<String>),
    List,
    /// A dict; its operands are its keys and values, alternating.
    Dict,
}

struct ProductState<'a> {
    product: Product,
    operands: Vec<Expr<'a>>,
    expected: Option<Type>,
    next: usize,
    /// The element type of a list, or the key and value types of a dict,
    /// once known.
    element: Option<Type>,
    value: Option<Type>,
    types: Vec<Option<Type>>,
    nodes: Vec<Option<Node>>,
    span: ByteSpan,
    failed: bool,
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
    /// The parameter types, in which a type parameter of the callee is a
    /// [`Type::Binder`] into `assignment`.
    params: Option<Vec<Type>>,
    /// The field names of a shape with labels, in declaration order.
    labels: Option<Vec<String>>,
    /// The parameter each written argument binds.
    binding: Vec<Option<usize>>,
    result: Option<Type>,
    /// The callee's type parameters and what each is inferred to be.
    binder_names: Vec<String>,
    assignment: Vec<Option<Type>>,
    /// Arguments whose parameter type was still open when they arrived.
    deferred: Vec<(usize, Type, ByteSpan)>,
    receiver: Option<Type>,
    nodes: Vec<Option<Node>>,
    application: ByteSpan,
    callee_span: ByteSpan,
    failed: bool,
    never_reported: bool,
}

impl CallState<'_> {
    /// `pattern` with every type parameter of the callee replaced by what it
    /// was inferred to be, when each one it mentions is known.
    fn instance(&self, pattern: &Type) -> Option<Type> {
        if self.assignment.is_empty() {
            return Some(pattern.clone());
        }
        let mut open = false;
        pattern.visit(&mut |ty| {
            if let Type::Binder(at) = ty {
                open |= !matches!(self.assignment.get(*at), Some(Some(_)));
            }
        });
        if open {
            return None;
        }
        let arguments = self
            .assignment
            .iter()
            .enumerate()
            .map(|(at, bound)| bound.clone().unwrap_or(Type::Binder(at)))
            .collect::<Vec<_>>();
        Some(pattern.instantiate(&arguments))
    }

    /// The parameter type the written argument `at` binds.
    fn slot(&self, at: usize) -> Option<&Type> {
        let slot = self.binding.get(at).copied().flatten()?;
        self.params.as_ref()?.get(slot)
    }
}

struct IfState<'a> {
    view: If<'a>,
    ctx: Ctx,
    stage: u8,
    condition: Option<Node>,
    then: Option<Out>,
    failed: bool,
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
    /// Each arm's shape for coverage and the span of its pattern.
    covers: Vec<(Rc<Pat>, ByteSpan)>,
    /// Whether a pattern had a fault, so that coverage is not decided.
    patterns_failed: bool,
    types: Vec<Option<Type>>,
    join: Option<Type>,
    failed: bool,
}

/// One lowered statement of a body.
#[derive(Debug)]
enum Piece {
    Expr(Node),
    Let(Option<usize>, Node),
    /// An irrefutable `let` whose pattern takes the value apart.
    Destructure(Box<(IrPattern, Node)>),
    /// A `let … else`: the pattern, the initializer, and the fallback.
    LetElse(Box<(IrPattern, Node, Node)>),
}

#[derive(Debug)]
enum Pending<'a> {
    Nothing,
    Let {
        pattern: Option<Pattern<'a>>,
        pattern_span: ByteSpan,
        fallback: Option<Body<'a>>,
        written: Option<Type>,
        value_span: ByteSpan,
    },
    /// A `let … else` whose fallback is being checked: the pattern's
    /// binders, the pattern, and the initializer.
    Fallback(Box<(Vec<Local>, Option<IrPattern>, Option<Node>)>),
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
    pending: Pending<'a>,
    never_at: Option<ByteSpan>,
    unreachable_reported: bool,
    pieces: Vec<Option<Piece>>,
    last_type: Option<Option<Type>>,
    failed: bool,
}

enum Task<'a> {
    Expr(Expr<'a>, Ctx),
    Return(ByteSpan),
    /// A `try`, and its operand's span.
    Try(ByteSpan, ByteSpan),
    Postfix(Box<PostfixState<'a>>),
    Call(Box<CallState<'a>>),
    Product(Box<ProductState<'a>>),
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
    /// Each call of, and reference to, a generic function: the function,
    /// its type arguments, and where.
    pub(crate) instantiations: Vec<(usize, Vec<Type>, ByteSpan)>,
    /// The span of the argument being checked, so that a literal written
    /// there reports a mismatch as an argument that does not fit.
    argument: Option<ByteSpan>,
    /// The span of a `let` initializer whose written type is faulty and
    /// already reported, so that nothing fixing its type is not reported.
    unfixed: Option<ByteSpan>,
    /// The number of patterns coverage does not take apart so far.
    opaque: usize,
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
            instantiations: Vec::new(),
            argument: None,
            unfixed: None,
            opaque: 0,
        }
    }

    /// Checks the body of a callable of type `params -> result` whose
    /// parameters are written as `patterns`, with the type parameters
    /// `binders` and `self_type` for `Self` in scope. Each parameter is
    /// bound to its slot, and a pattern that takes it apart is a `match` of
    /// one arm around the body.
    pub(crate) fn callable(
        &mut self,
        signature: &FnType,
        patterns: &[Pattern<'a>],
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
        self.slots.extend(params.iter().cloned());
        let mut failed = false;
        let mut destructured = Vec::new();
        for (slot, (ty, pattern)) in params.iter().zip(patterns).enumerate() {
            let span = pattern_span(*pattern, self.text);
            let typed = self.type_pattern(*pattern, span, Some(ty));
            if typed.lowered.is_some()
                && self.irrefutable(&typed.cover, ty, span) == Some(false)
            {
                self.report(
                    DiagnosticCode::PatternRefutableBinding,
                    span,
                    "this parameter pattern does not cover every value of its type",
                );
                failed = true;
            }
            match typed.lowered {
                Some(IrPattern::Bind(fresh)) if fresh + 1 == self.slots.len() => {
                    // A bare name binds the parameter's own slot.
                    self.slots.pop();
                    self.locals.extend(
                        typed
                            .bindings
                            .into_iter()
                            .map(|local| Local { slot, ..local }),
                    );
                }
                Some(IrPattern::Discard) => {}
                Some(lowered) => {
                    self.locals.extend(typed.bindings);
                    destructured.push((slot, ty.clone(), lowered, span));
                }
                None => {
                    self.locals.extend(typed.bindings);
                    failed = true;
                }
            }
        }
        let out = self.run(Task::Body(Box::new(
            self.body_state(body, BodyMode::Callable),
        )));
        let node = out.node.filter(|_| !failed).map(|body| {
            destructured.into_iter().rev().fold(
                body,
                |body, (slot, value_type, pattern, span)| {
                    let subject =
                        self.node(IrExpr::Variable { slot, value_type }, span);
                    self.node(
                        IrExpr::Match {
                            subject: Box::new(subject),
                            arms: vec![MatchArm { pattern, body }],
                            value_type: Type::Never,
                        },
                        span,
                    )
                },
            )
        });
        Checked {
            node,
            slot_count: self.slots.len(),
        }
    }

    /// Checks the body of a test: a body of type `Void` outside any callable,
    /// so that a `return` or a `try` written directly in it is reported.
    pub(crate) fn test_body(&mut self, body: Body<'a>) -> Checked {
        self.locals.clear();
        self.slots.clear();
        self.binders.clear();
        self.self_type = None;
        self.result = None;
        let out = self.run(Task::Body(Box::new(
            self.body_state(body, BodyMode::Branch(Ctx::Stmt)),
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
                Task::Try(span, operand) => {
                    self.finish_try(span, operand, input.take())
                }
                Task::Postfix(state) => self.step_postfix(state, input.take()),
                Task::Call(state) => self.step_call(state, input.take()),
                Task::Product(state) => self.step_product(state, input.take()),
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
            ExprForm::Try(operand) => {
                let carrier = self
                    .result
                    .as_ref()
                    .filter(|result| self.carrier(result))
                    .and_then(|result| match result {
                        Type::Declared(id, arguments) => {
                            Some((id.clone(), arguments.clone()))
                        }
                        _ => None,
                    });
                let Some((id, mut arguments)) = carrier else {
                    let found = self.result.as_ref().map_or_else(
                        || "no result".to_owned(),
                        |result| format!("`{result}`"),
                    );
                    self.report(
                        DiagnosticCode::TypeInvalidTry,
                        span,
                        format!(
                            "`try` needs an enclosing callable whose result is an \
                             `Option` or a `Result`, but it has {found}"
                        ),
                    );
                    return Step::Done(Out::error());
                };
                // The operand is the carrier of the value the context
                // expects, with the enclosing error type.
                let expected = ctx.expected().and_then(|value| {
                    let first = arguments.first_mut()?;
                    *first = value.clone();
                    Some(Type::Declared(id, arguments))
                });
                let operand_span = expr_span(&operand);
                Step::Child(
                    Task::Try(span, operand_span),
                    Task::Expr(operand, Ctx::Value(expected)),
                )
            }
            ExprForm::Await(_) => {
                self.report(
                    DiagnosticCode::EffectInvalidAwait,
                    span,
                    "`await` is valid only in a `proc`",
                );
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

    /// Whether `ty` is a `try` carrier: an `Option` or a `Result`.
    fn carrier(&self, ty: &Type) -> bool {
        let Type::Declared(id, _) = ty else {
            return false;
        };
        matches!(
            self.program.library_of(id),
            Some(Library::Option | Library::Result)
        )
    }

    /// A `try` at `span` whose operand at `operand_span` arrived: the
    /// operand is a carrier of the enclosing result's kind, and a `Result`
    /// has its error type.
    fn finish_try(
        &mut self,
        span: ByteSpan,
        operand_span: ByteSpan,
        input: Option<Out>,
    ) -> Step<'a> {
        let out = input.unwrap_or_else(Out::error);
        let (Some(actual), Some(result)) = (out.ty, self.result.clone()) else {
            return Step::Done(Out::error());
        };
        if actual == Type::Never {
            self.report(
                DiagnosticCode::TypeUnreachableCode,
                operand_span,
                "this operand never completes, so `try` has nothing to unwrap",
            );
            return Step::Done(Out::error());
        }
        let fits = match (&actual, &result) {
            (Type::Declared(own, values), Type::Declared(enclosing, errors)) => {
                let kind = self.program.library_of(own);
                kind.is_some()
                    && kind == self.program.library_of(enclosing)
                    && values.get(1) == errors.get(1)
            }
            _ => false,
        };
        let value = match &actual {
            Type::Declared(_, values) if fits => values.first().cloned(),
            _ => None,
        };
        let Some(value) = value else {
            self.report(
                DiagnosticCode::TypeInvalidTry,
                span,
                format!(
                    "`try` on `{actual}` does not fit the enclosing result `{result}`"
                ),
            );
            return Step::Done(Out::error());
        };
        let node = out
            .node
            .map(|operand| self.node(IrExpr::Try(Box::new(operand)), span));
        Step::Done(Out {
            ty: Some(value),
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
                    covers: Vec::new(),
                    patterns_failed: false,
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
            Primary::Tuple(operands) => self.start_product(
                state,
                Product::Tuple,
                operands,
                inner_ctx,
                primary_span,
            ),
            Primary::Record(fields) => {
                let names = fields
                    .iter()
                    .map(|(name, _)| name.text().to_owned())
                    .collect();
                let operands = fields.into_iter().map(|(_, value)| value).collect();
                self.start_product(
                    state,
                    Product::Record(names),
                    operands,
                    inner_ctx,
                    primary_span,
                )
            }
            Primary::List(operands) => self.start_product(
                state,
                Product::List,
                operands,
                inner_ctx,
                primary_span,
            ),
            Primary::Dict(entries) => {
                let operands = entries
                    .into_iter()
                    .flat_map(|(key, value)| [key, value])
                    .collect();
                self.start_product(
                    state,
                    Product::Dict,
                    operands,
                    inner_ctx,
                    primary_span,
                )
            }
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
                        let module =
                            program.interfaces.get(index).map(|data| data.module);
                        let whole = ByteSpan::new(span.start(), suffix_span.end());
                        module.map_or(Head::Error, |module| self.skipped(module, whole))
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
            Entity::Type(index) => Head::Type(index),
            Entity::Interface(_) | Entity::Module(_) => Head::Static,
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
            let last = state.next == state.suffixes.len();
            let callee_span = ByteSpan::new(state.start, state.end);
            let whole = ByteSpan::new(state.start, suffix_span.end());
            state.end = suffix_span.end();
            let head = std::mem::replace(&mut state.head, Head::Error);
            match suffix {
                Suffix::Field(name) => {
                    let value = match head {
                        Head::Value(out) => Some(*out),
                        Head::Function(index) => {
                            Some(self.function_value(index, callee_span, None))
                        }
                        _ => None,
                    };
                    if let Some(value) = value {
                        let out =
                            self.field_read(value, name.text(), name.span(), whole);
                        state.head = Head::Value(Box::new(out));
                    }
                }
                Suffix::Call(args) => {
                    let expected = if last {
                        state.ctx.expected().cloned()
                    } else {
                        None
                    };
                    let callee = match head {
                        Head::Function(index) => Some(Callee::Function(index)),
                        Head::Case(ty, case) => Some(Callee::Case(ty, case)),
                        Head::Contract(interface, member) => {
                            Some(Callee::Contract(interface, member))
                        }
                        Head::Type(index) => self.constructor(index, callee_span),
                        Head::Value(out) => self.applied_value(*out, callee_span),
                        Head::Static => {
                            self.report(
                                DiagnosticCode::TypeNotApplicable,
                                callee_span,
                                "a module or an interface cannot be applied",
                            );
                            None
                        }
                        Head::Pending | Head::Error => None,
                    };
                    let Some(callee) = callee else { continue };
                    let call =
                        self.call_state(callee, args, whole, callee_span, expected);
                    state.head = Head::Pending;
                    return Step::Child(Task::Postfix(state), Task::Call(call));
                }
            }
        }
        let whole = ByteSpan::new(state.start, state.end);
        let out = match std::mem::replace(&mut state.head, Head::Error) {
            Head::Value(out) => *out,
            Head::Function(index) => {
                let expected = state.ctx.expected().cloned();
                self.function_value(index, whole, expected.as_ref())
            }
            Head::Case(..) => {
                self.unavailable(whole, "a constructor used as a value");
                Out::error()
            }
            Head::Contract(..) => {
                self.unavailable(whole, "an interface member used as a value");
                Out::error()
            }
            Head::Type(_) | Head::Static => {
                self.unavailable(whole, "a type used as a value");
                Out::error()
            }
            Head::Pending | Head::Error => Out::error(),
        };
        Step::Done(out)
    }

    /// The constructor a type applied as a callee denotes, reporting a type
    /// that has none.
    fn constructor(&mut self, index: usize, callee_span: ByteSpan) -> Option<Callee> {
        let program = self.program;
        let data = program.types.get(index)?;
        match data.kind {
            TypeKind::Wrapper(_) | TypeKind::Tuple(_) | TypeKind::Record(_)
                if data.invariant =>
            {
                self.unavailable(
                    callee_span,
                    "constructing a value of a type with an invariant",
                );
                None
            }
            TypeKind::Wrapper(_) | TypeKind::Tuple(_) | TypeKind::Record(_)
                if data.id.is_some() =>
            {
                Some(Callee::Construct(index))
            }
            TypeKind::Unsupported => {
                self.skipped(data.module, callee_span);
                None
            }
            TypeKind::Faulty | TypeKind::Pending => None,
            _ => {
                self.report(
                    DiagnosticCode::TypeNotApplicable,
                    callee_span,
                    format!(
                        "the type `{}` has no constructor of its own; a variant type is built by one of its cases",
                        data.name
                    ),
                );
                None
            }
        }
    }

    /// The callee a value applied as a callee denotes: a function value or
    /// a collection; any other value is reported.
    fn applied_value(&mut self, out: Out, callee_span: ByteSpan) -> Option<Callee> {
        let Out { ty, node } = out;
        match ty {
            Some(Type::Fn(ref signature)) => {
                Some(Callee::Value(node.map(Box::new), signature.clone()))
            }
            Some(Type::Never) => {
                self.report(
                    DiagnosticCode::TypeUnreachableCode,
                    callee_span,
                    "this callee never completes",
                );
                None
            }
            Some(ty) if self.lookup_types(&ty).is_some() => {
                Some(Callee::Lookup(node.map(Box::new), Box::new(ty)))
            }
            Some(ty) => {
                self.report(
                    DiagnosticCode::TypeNotApplicable,
                    callee_span,
                    format!("a value of type `{ty}` cannot be applied"),
                );
                None
            }
            None => None,
        }
    }

    /// The key and item types of a lookup in a value of `collection`.
    fn lookup_types(&self, collection: &Type) -> Option<(Type, Type)> {
        match collection {
            Type::List(element) => Some((Type::U64, element.as_ref().clone())),
            Type::Declared(id, arguments) => match self.program.library_of(id)? {
                Library::Str => Some((Type::U64, Type::Char)),
                Library::Bytes => Some((Type::U64, Type::U8)),
                Library::Dict => match arguments.as_slice() {
                    [key, value] => Some((key.clone(), value.clone())),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        }
    }

    /// The key and value types of a `Dict` type.
    fn dict_arguments(&self, ty: &Type) -> Option<(Type, Type)> {
        match ty {
            Type::Declared(id, arguments)
                if self.program.library_of(id) == Some(Library::Dict) =>
            {
                match arguments.as_slice() {
                    [key, value] => Some((key.clone(), value.clone())),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Reports a use at `span`, in another module, of the representation of
    /// the type `index` when that module cannot see it.
    fn hidden(&mut self, index: usize, span: ByteSpan) -> bool {
        let program = self.program;
        let Some(data) = program.types.get(index) else {
            return false;
        };
        if data.module == self.module || data.representation_public {
            return false;
        }
        let mut diagnostic = Diagnostic::new(
            DiagnosticCode::NameInaccessibleRepresentation,
            span,
            format!(
                "the representation of `{}` is private to the module that declares it",
                data.name
            ),
        )
        .with_source_id(self.source_id.as_ref());
        if let Some(module) = program.modules.get(data.module) {
            let declared = data
                .view
                .name()
                .map_or(data.view.construct().span(), |name| name.span());
            diagnostic = diagnostic.with_related_source(
                module.source_id.as_ref(),
                declared,
                "the type is declared here",
            );
        }
        self.diagnostics.push(diagnostic);
        true
    }

    /// A field read of `name` on `value`.
    fn field_read(
        &mut self,
        value: Out,
        name: &str,
        name_span: ByteSpan,
        whole: ByteSpan,
    ) -> Out {
        let Out { ty, node } = value;
        let Some(ty) = ty else {
            return Out::error();
        };
        if ty == Type::Never {
            self.report(
                DiagnosticCode::TypeUnreachableCode,
                whole,
                "this value never completes, so it has no field to read",
            );
            return Out::error();
        }
        let program = self.program;
        let field = match &ty {
            Type::Record(fields) => fields.get(name).cloned(),
            Type::Declared(id, arguments) => {
                match program
                    .type_index_of(id)
                    .map(|index| (index, program.types.get(index)))
                {
                    Some((index, Some(data))) => match &data.kind {
                        TypeKind::Record(fields) => {
                            let found = fields
                                .iter()
                                .find(|(field, _)| field == name)
                                .map(|(_, field)| field.instantiate(arguments));
                            if found.is_some() && self.hidden(index, name_span) {
                                return Out::error();
                            }
                            found
                        }
                        _ => None,
                    },
                    _ => None,
                }
            }
            _ => None,
        };
        let Some(field) = field else {
            self.report(
                DiagnosticCode::TypeUnknownRecordField,
                name_span,
                format!("a value of type `{ty}` has no field `{name}`"),
            );
            return Out::error();
        };
        let node = node.map(|record| {
            self.node(
                IrExpr::Field {
                    record: Box::new(record),
                    field: name.to_owned(),
                },
                whole,
            )
        });
        Out {
            ty: Some(field),
            node,
        }
    }

    /// Reports that the type parameters `missing` of the call or reference
    /// at `span` are not determined.
    fn ambiguous(&mut self, span: ByteSpan, missing: &[String]) {
        let mut diagnostic = Diagnostic::new(
            DiagnosticCode::TypeAmbiguousInference,
            span,
            "nothing fixes every type argument here",
        )
        .with_source_id(self.source_id.as_ref());
        for name in missing {
            diagnostic = diagnostic.with_note(format!(
                "nothing fixes `{name}`: an operand of a type that mentions it, or a written expected type, would"
            ));
        }
        self.diagnostics.push(diagnostic);
    }

    fn function_value(
        &mut self,
        index: usize,
        span: ByteSpan,
        expected: Option<&Type>,
    ) -> Out {
        let program = self.program;
        let Some(data) = program.functions.get(index) else {
            return Out::error();
        };
        let Some(signature) = data.signature.clone() else {
            return Out::error();
        };
        self.references.push(Reference {
            target: Target::Function(index),
            span,
        });
        let outside = self.assertion_outside_tests(index, span);
        let pattern = Type::function(signature);
        let (ty, type_arguments) = if data.binders.is_empty() {
            (pattern, Vec::new())
        } else {
            let mut assignment = vec![None; data.binders.len()];
            if let Some(expected) = expected {
                let mut trial = assignment.clone();
                if unify(&pattern, expected, &mut trial) {
                    assignment = trial;
                }
            }
            let missing = data
                .binders
                .iter()
                .zip(&assignment)
                .filter(|(_, bound)| bound.is_none())
                .map(|(name, _)| name.clone())
                .collect::<Vec<_>>();
            if !missing.is_empty() {
                self.ambiguous(span, &missing);
                return Out::error();
            }
            let arguments = assignment.into_iter().flatten().collect::<Vec<_>>();
            self.instantiations.push((index, arguments.clone(), span));
            (pattern.instantiate(&arguments), arguments)
        };
        let node = self
            .layout
            .functions
            .get(index)
            .copied()
            .flatten()
            .map(|function| {
                self.node(
                    IrExpr::Function {
                        function,
                        type_arguments,
                    },
                    span,
                )
            })
            .filter(|_| !outside);
        Out { ty: Some(ty), node }
    }

    /// Reports `@module.assertionOutsideTests` at `span` when the function
    /// `index` is bound to an assertion row of the registry and this module
    /// is not a module of `@tests`.
    fn assertion_outside_tests(&mut self, index: usize, span: ByteSpan) -> bool {
        let program = self.program;
        let in_tests = program
            .modules
            .get(self.module)
            .is_some_and(|module| module.tests);
        let assertion = program
            .functions
            .get(index)
            .and_then(|data| data.external)
            .is_some_and(|row| is_assertion(&row.symbol()));
        if in_tests || !assertion {
            return false;
        }
        self.report(
            DiagnosticCode::ModuleAssertionOutsideTests,
            span,
            "an assertion is used only in a module of `@tests`",
        );
        true
    }

    /// Reports `@type.functionNotEquatable` at each operand of an
    /// `assert.equal` call whose type contains a callable.
    fn unequatable_operands(&mut self, state: &CallState<'a>) -> bool {
        let Callee::Function(index) = state.callee else {
            return false;
        };
        let row = self
            .program
            .functions
            .get(index)
            .and_then(|data| data.external);
        if row != Some(CompilerIntrinsic::AssertEqual) {
            return false;
        }
        let Some(Some(operand)) = state.assignment.first() else {
            return false;
        };
        if self.program.key_fault(operand)
            != Some(DiagnosticCode::TypeFunctionNotEquatable)
        {
            return false;
        }
        for arg in &state.args {
            self.report(
                DiagnosticCode::TypeFunctionNotEquatable,
                expr_span(&arg.value),
                format!(
                    "an operand of `assert.equal` has the type `{operand}`, which contains a callable"
                ),
            );
        }
        true
    }

    fn call_state(
        &mut self,
        callee: Callee,
        args: Vec<Argument<'a>>,
        application: ByteSpan,
        callee_span: ByteSpan,
        expected: Option<Type>,
    ) -> Box<CallState<'a>> {
        let program = self.program;
        let mut labels = None;
        let mut binder_names = Vec::new();
        let (params, result) = match &callee {
            Callee::Function(index) => {
                match program.functions.get(*index).and_then(|data| {
                    data.signature.clone().map(|signature| (data, signature))
                }) {
                    Some((data, signature)) => {
                        binder_names.clone_from(&data.binders);
                        (Some(signature.parameters), Some(signature.result))
                    }
                    None => (None, None),
                }
            }
            Callee::Value(_, signature) => (
                Some(signature.parameters.clone()),
                Some(signature.result.clone()),
            ),
            Callee::Case(ty, case) => {
                match program
                    .types
                    .get(*ty)
                    .and_then(|data| data.case(case).map(|case| (data, case)))
                {
                    Some((data, case)) => {
                        binder_names.clone_from(&data.params);
                        let params = match &case.fields {
                            Some(fields) => {
                                labels = Some(
                                    fields
                                        .iter()
                                        .map(|(name, _)| name.clone())
                                        .collect(),
                                );
                                fields.iter().map(|(_, ty)| ty.clone()).collect()
                            }
                            None => Vec::new(),
                        };
                        (Some(params), data.self_type())
                    }
                    None => (None, None),
                }
            }
            Callee::Construct(ty) => match program.types.get(*ty) {
                Some(data) => {
                    binder_names.clone_from(&data.params);
                    let params = match &data.kind {
                        TypeKind::Record(fields) => {
                            labels = Some(
                                fields.iter().map(|(name, _)| name.clone()).collect(),
                            );
                            Some(fields.iter().map(|(_, ty)| ty.clone()).collect())
                        }
                        TypeKind::Tuple(components) => Some(components.clone()),
                        TypeKind::Wrapper(representation) => {
                            Some(vec![representation.clone()])
                        }
                        _ => None,
                    };
                    (params, data.self_type())
                }
                None => (None, None),
            },
            Callee::Contract(..) => (None, None),
            Callee::Lookup(_, collection) => match self.lookup_types(collection) {
                Some((key, item)) => {
                    let result = program.library_type(Library::Option, vec![item]);
                    if result.is_none() {
                        self.unavailable(application, "a lookup without `Option`");
                    }
                    (Some(vec![key]), result)
                }
                None => (None, None),
            },
        };
        let mut state = Box::new(CallState {
            callee,
            args,
            next: 0,
            params,
            labels,
            binding: Vec::new(),
            result,
            assignment: vec![None; binder_names.len()],
            binder_names,
            deferred: Vec::new(),
            receiver: None,
            nodes: Vec::new(),
            application,
            callee_span,
            failed: false,
            never_reported: false,
        });
        self.bind_arguments(&mut state);
        if !matches!(state.callee, Callee::Contract(..)) {
            self.check_arity(&mut state);
        }
        if !state.assignment.is_empty()
            && let (Some(result), Some(expected)) = (&state.result, &expected)
        {
            let mut trial = state.assignment.clone();
            if unify(result, expected, &mut trial) {
                state.assignment = trial;
            }
        }
        if let Callee::Case(ty, _) | Callee::Construct(ty) = state.callee
            && self.hidden(ty, callee_span)
        {
            state.failed = true;
        }
        state
    }

    /// Binds each written argument to its parameter: by label in a shape
    /// with field names, by position in any other.
    fn bind_arguments(&mut self, state: &mut CallState<'a>) {
        let span_of = |arg: &Argument<'_>| match arg.label {
            Some(label) => {
                ByteSpan::new(label.span().start(), expr_span(&arg.value).end())
            }
            None => expr_span(&arg.value),
        };
        let mut binding = Vec::new();
        match state.labels.clone() {
            Some(names) => {
                let mut used = vec![false; names.len()];
                let mut positional = false;
                for arg in &state.args {
                    let Some(label) = arg.label else {
                        positional = true;
                        binding.push(None);
                        continue;
                    };
                    match names.iter().position(|name| name == label.text()) {
                        Some(at) if used.get(at) == Some(&false) => {
                            if let Some(slot) = used.get_mut(at) {
                                *slot = true;
                            }
                            binding.push(Some(at));
                        }
                        Some(_) => {
                            self.report(
                                DiagnosticCode::TypeArgumentMismatch,
                                span_of(arg),
                                format!("the field `{}` is bound twice", label.text()),
                            );
                            state.failed = true;
                            binding.push(None);
                        }
                        None => {
                            self.report(
                                DiagnosticCode::TypeArgumentMismatch,
                                span_of(arg),
                                format!("the shape has no field `{}`", label.text()),
                            );
                            state.failed = true;
                            binding.push(None);
                        }
                    }
                }
                if positional {
                    self.report(
                        DiagnosticCode::TypeArgumentMismatch,
                        state.application,
                        "this shape has field names, so each argument names its field",
                    );
                    state.failed = true;
                } else {
                    let missing = names
                        .iter()
                        .zip(&used)
                        .filter(|(_, used)| !**used)
                        .map(|(name, _)| format!("`{name}`"))
                        .collect::<Vec<_>>();
                    if !missing.is_empty() {
                        self.report(
                            DiagnosticCode::TypeArgumentMismatch,
                            state.application,
                            format!(
                                "no argument binds the field(s) {}",
                                missing.join(", ")
                            ),
                        );
                        state.failed = true;
                    }
                }
            }
            None => {
                let mut next = 0;
                for arg in &state.args {
                    if let Some(label) = arg.label {
                        self.report(
                            DiagnosticCode::TypeArgumentMismatch,
                            span_of(arg),
                            format!(
                                "the callee takes no labelled argument `{}`",
                                label.text()
                            ),
                        );
                        state.failed = true;
                        binding.push(None);
                    } else {
                        binding.push(Some(next));
                        next += 1;
                    }
                }
            }
        }
        state.binding = binding;
    }

    fn check_arity(&mut self, state: &mut CallState<'a>) {
        if state.labels.is_some() {
            return;
        }
        // A labelled argument to a callee without field names is reported
        // where it is written, and still counts here, so that it is not
        // reported again as a wrong arity.
        let positional = state.args.len();
        if let Some(params) = &state.params
            && params.len() != positional
        {
            let message = format!(
                "the callee takes {} argument(s), but {positional} are written",
                params.len(),
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
                    match state.slot(at).cloned() {
                        Some(pattern) => {
                            let mut trial = state.assignment.clone();
                            let fits = trial.is_empty()
                                || unify(&pattern, &actual, &mut trial);
                            if fits {
                                state.assignment = trial;
                            }
                            match state.instance(&pattern) {
                                None if !fits => {
                                    self.report(
                                        DiagnosticCode::TypeArgumentMismatch,
                                        span,
                                        format!(
                                            "an argument of type `{actual}` does not fit this parameter"
                                        ),
                                    );
                                    state.failed = true;
                                    None
                                }
                                Some(expected) => self.conform(
                                    &expected,
                                    &actual,
                                    out.node,
                                    span,
                                    DiagnosticCode::TypeArgumentMismatch,
                                ),
                                None => {
                                    state.deferred.push((at, actual, span));
                                    out.node
                                }
                            }
                        }
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
            let expected = state.slot(at).and_then(|pattern| state.instance(pattern));
            let skip_rest = matches!(state.callee, Callee::Contract(..))
                && at > 0
                && state.receiver.is_none();
            if skip_rest {
                state.failed = true;
                state.nodes.push(None);
                return self.step_call(state, None);
            }
            self.argument = Some(expr_span(&value));
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

    fn finish_call(&mut self, mut state: CallState<'a>) -> Out {
        let program = self.program;
        let missing = state
            .binder_names
            .iter()
            .zip(&state.assignment)
            .filter(|(_, bound)| bound.is_none())
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();
        if !missing.is_empty() && state.params.is_some() && !state.failed {
            self.ambiguous(state.application, &missing);
            state.failed = true;
        }
        if let Callee::Function(index) = state.callee
            && self.assertion_outside_tests(index, state.callee_span)
        {
            state.failed = true;
        }
        if !state.failed && self.unequatable_operands(&state) {
            state.failed = true;
        }
        for (at, actual, span) in std::mem::take(&mut state.deferred) {
            let expected = state.slot(at).and_then(|pattern| state.instance(pattern));
            let node = state.nodes.get_mut(at).and_then(Option::take);
            let node = match expected {
                Some(expected) => self.conform(
                    &expected,
                    &actual,
                    node,
                    span,
                    DiagnosticCode::TypeArgumentMismatch,
                ),
                None => None,
            };
            if node.is_none() {
                state.failed = true;
            }
            if let Some(slot) = state.nodes.get_mut(at) {
                *slot = node;
            }
        }
        let mut result = state
            .result
            .as_ref()
            .and_then(|result| state.instance(result));
        if let Some(ty) = &result
            && ty.depth() > Type::MAX_DEPTH
        {
            self.report(
                DiagnosticCode::TypeDepthExceeded,
                state.application,
                format!(
                    "the type of this expression nests more than {} levels",
                    Type::MAX_DEPTH
                ),
            );
            result = None;
            state.failed = true;
        }
        let type_arguments = state
            .assignment
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        let complete_arguments = type_arguments.len() == state.assignment.len();
        let CallState {
            callee,
            nodes,
            binding,
            labels,
            receiver,
            application,
            callee_span,
            failed,
            ..
        } = state;
        let mut complete = !failed && result.is_some();
        let mut written = Vec::new();
        for node in nodes {
            match node {
                Some(node) => written.push(node),
                None => complete = false,
            }
        }
        let named = |written: Vec<Node>| {
            let names = labels.clone().unwrap_or_default();
            binding
                .iter()
                .zip(written)
                .filter_map(|(slot, node)| {
                    slot.and_then(|slot| names.get(slot).cloned())
                        .map(|name| (name, node))
                })
                .collect::<Vec<_>>()
        };
        let expr = match callee {
            Callee::Function(index) => {
                self.references.push(Reference {
                    target: Target::Function(index),
                    span: callee_span,
                });
                if !type_arguments.is_empty() && complete_arguments {
                    self.instantiations.push((
                        index,
                        type_arguments.clone(),
                        application,
                    ));
                }
                self.layout
                    .functions
                    .get(index)
                    .copied()
                    .flatten()
                    .map(|function| IrExpr::Call {
                        target: CallTarget::Direct {
                            function,
                            type_arguments,
                        },
                        arguments: Arguments {
                            positional: written,
                            labelled: Vec::new(),
                        },
                    })
            }
            Callee::Value(node, _) => node.map(|node| IrExpr::Call {
                target: CallTarget::Value(node),
                arguments: Arguments {
                    positional: written,
                    labelled: Vec::new(),
                },
            }),
            Callee::Case(_, case) => {
                result.clone().map(|value_type| IrExpr::Construct {
                    value_type,
                    construction: Construction::Case {
                        case,
                        fields: named(written),
                    },
                })
            }
            Callee::Construct(ty) => {
                let construction = match program.types.get(ty).map(|data| &data.kind) {
                    Some(TypeKind::Record(_)) => {
                        Some(Construction::Record(named(written)))
                    }
                    Some(TypeKind::Tuple(_)) => Some(Construction::Tuple(written)),
                    Some(TypeKind::Wrapper(_)) => written
                        .into_iter()
                        .next()
                        .map(|node| Construction::Wrapper(Box::new(node))),
                    _ => None,
                };
                result
                    .clone()
                    .zip(construction)
                    .map(|(value_type, construction)| IrExpr::Construct {
                        value_type,
                        construction,
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
                            positional: written,
                            labelled: Vec::new(),
                        },
                    }),
                    _ => None,
                }
            }
            Callee::Lookup(collection, _) => collection
                .zip(written.into_iter().next())
                .map(|(collection, key)| IrExpr::Lookup {
                    collection,
                    key: Box::new(key),
                }),
        };
        let node = if complete {
            expr.map(|expr| self.node(expr, application))
        } else {
            None
        };
        Out { ty: result, node }
    }

    fn start_product(
        &mut self,
        state: Box<PostfixState<'a>>,
        product: Product,
        operands: Vec<Expr<'a>>,
        ctx: Ctx,
        span: ByteSpan,
    ) -> Step<'a> {
        let expected = ctx.expected().cloned();
        let (element, value) = match (&product, &expected) {
            (Product::List, Some(Type::List(element))) => {
                (Some(element.as_ref().clone()), None)
            }
            (Product::Dict, Some(ty)) => match self.dict_arguments(ty) {
                Some((key, value)) => (Some(key), Some(value)),
                None => (None, None),
            },
            _ => (None, None),
        };
        let child = Task::Product(Box::new(ProductState {
            product,
            operands,
            expected,
            next: 0,
            element,
            value,
            types: Vec::new(),
            nodes: Vec::new(),
            span,
            failed: false,
        }));
        Step::Child(Task::Postfix(state), child)
    }

    fn step_product(
        &mut self,
        mut state: Box<ProductState<'a>>,
        input: Option<Out>,
    ) -> Step<'a> {
        if let Some(out) = input {
            let at = state.next.saturating_sub(1);
            let span = state.operands.get(at).map_or(state.span, expr_span);
            let (ty, node) = match out.ty {
                Some(Type::Never) => {
                    self.report(
                        DiagnosticCode::TypeUnreachableCode,
                        span,
                        "this operand never completes, so nothing is built",
                    );
                    (None, None)
                }
                Some(actual) => {
                    let slot = match state.product {
                        Product::Tuple | Product::Record(_) => None,
                        Product::List => Some(&mut state.element),
                        Product::Dict if at.is_multiple_of(2) => {
                            Some(&mut state.element)
                        }
                        Product::Dict => Some(&mut state.value),
                    };
                    match slot {
                        Some(Some(expected)) => {
                            let expected = expected.clone();
                            let node = self.conform(
                                &expected,
                                &actual,
                                out.node,
                                span,
                                DiagnosticCode::TypeMismatch,
                            );
                            (Some(actual), node)
                        }
                        Some(slot) => {
                            *slot = Some(actual.clone());
                            (Some(actual), out.node)
                        }
                        None => (Some(actual), out.node),
                    }
                }
                None => (None, None),
            };
            if node.is_none() {
                state.failed = true;
            }
            state.types.push(ty);
            state.nodes.push(node);
        }
        if let Some(operand) = state.operands.get(state.next).cloned() {
            let at = state.next;
            state.next += 1;
            let expected = match &state.product {
                Product::Tuple => match &state.expected {
                    Some(Type::Tuple(components))
                        if components.len() == state.operands.len() =>
                    {
                        components.get(at).cloned()
                    }
                    _ => None,
                },
                Product::Record(names) => match &state.expected {
                    Some(Type::Record(fields)) => {
                        names.get(at).and_then(|name| fields.get(name)).cloned()
                    }
                    _ => None,
                },
                Product::List => state.element.clone(),
                Product::Dict if at.is_multiple_of(2) => state.element.clone(),
                Product::Dict => state.value.clone(),
            };
            return Step::Child(
                Task::Product(state),
                Task::Expr(operand, Ctx::Value(expected)),
            );
        }
        Step::Done(self.finish_product(*state))
    }

    fn finish_product(&mut self, state: ProductState<'a>) -> Out {
        let ProductState {
            product,
            operands,
            expected,
            element,
            value,
            types,
            nodes,
            span,
            mut failed,
            ..
        } = state;
        let components = types.into_iter().collect::<Option<Vec<_>>>();
        let empty = operands.is_empty();
        let mut ty = match &product {
            Product::Tuple => components.map(Type::Tuple),
            Product::Record(names) => components.and_then(|components| {
                match Type::record(names.iter().cloned().zip(components)) {
                    Ok(ty) => Some(ty),
                    Err(_) => {
                        self.unavailable(span, "a record value with a repeated field");
                        None
                    }
                }
            }),
            Product::List => match element {
                Some(element) => Some(Type::list(element)),
                None if empty => {
                    self.empty_collection(span, expected.as_ref(), "list");
                    None
                }
                None => None,
            },
            Product::Dict => match (element, value) {
                (Some(key), Some(value)) => {
                    let inferred = expected
                        .as_ref()
                        .and_then(|expected| self.dict_arguments(expected))
                        .is_none();
                    let key_fault = if inferred {
                        self.program.key_fault(&key)
                    } else {
                        None
                    };
                    match key_fault {
                        Some(code) => {
                            let at = operands.first().map_or(span, expr_span);
                            self.diagnostics.push(
                                key_diagnostic(code, at, &key)
                                    .with_source_id(self.source_id.as_ref()),
                            );
                            None
                        }
                        None => {
                            let dict = self
                                .program
                                .library_type(Library::Dict, vec![key, value]);
                            if dict.is_none() {
                                self.unavailable(span, "a dict without `Dict`");
                            }
                            dict
                        }
                    }
                }
                (None, None) if empty => {
                    self.empty_collection(span, expected.as_ref(), "dict");
                    None
                }
                _ => None,
            },
        };
        if let Some(built) = &ty
            && built.depth() > Type::MAX_DEPTH
        {
            self.report(
                DiagnosticCode::TypeDepthExceeded,
                span,
                format!(
                    "the type of this expression nests more than {} levels",
                    Type::MAX_DEPTH
                ),
            );
            ty = None;
        }
        if ty.is_none() {
            failed = true;
        }
        let nodes = nodes.into_iter().collect::<Option<Vec<_>>>();
        let node = match (failed, nodes, ty.clone()) {
            (false, Some(nodes), Some(value_type)) => {
                let construction = match product {
                    Product::Tuple => Construction::Tuple(nodes),
                    Product::Record(names) => {
                        Construction::Record(names.into_iter().zip(nodes).collect())
                    }
                    Product::List => Construction::List(nodes),
                    Product::Dict => {
                        let mut pairs = Vec::new();
                        let mut nodes = nodes.into_iter();
                        while let (Some(key), Some(value)) =
                            (nodes.next(), nodes.next())
                        {
                            pairs.push((key, value));
                        }
                        Construction::Dict(pairs)
                    }
                };
                Some(self.node(
                    IrExpr::Construct {
                        value_type,
                        construction,
                    },
                    span,
                ))
            }
            _ => None,
        };
        Out { ty, node }
    }

    /// Reports an empty list or dict literal whose element types nothing
    /// fixes.
    fn empty_collection(
        &mut self,
        span: ByteSpan,
        expected: Option<&Type>,
        what: &str,
    ) {
        if self.unfixed == Some(span) {
            return;
        }
        match expected {
            Some(expected) => self.report(
                DiagnosticCode::TypeMismatch,
                span,
                format!("expected `{expected}`, found an empty {what} literal"),
            ),
            None => {
                let diagnostic = Diagnostic::new(
                    DiagnosticCode::TypeAmbiguousInference,
                    span,
                    format!("nothing fixes the type of this empty {what}"),
                )
                .with_source_id(self.source_id.as_ref())
                .with_note(
                    "a written type on its binding, or an expected type from its position, would fix it",
                );
                self.diagnostics.push(diagnostic);
            }
        }
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
                let code = if self.argument == Some(span) {
                    DiagnosticCode::TypeArgumentMismatch
                } else {
                    DiagnosticCode::TypeMismatch
                };
                self.report(
                    code,
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
                Some(ty) => state.subject = Some(ty),
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
            let pattern = match arm.pattern() {
                Some(pattern) => {
                    let typed = self.type_pattern(
                        pattern,
                        pattern_span,
                        state.subject.as_ref(),
                    );
                    self.locals.extend(typed.bindings);
                    state.covers.push((typed.cover, pattern_span));
                    typed.lowered
                }
                None => None,
            };
            if pattern.is_none() {
                state.failed = true;
                state.patterns_failed = true;
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
        if let Some(subject) = state.subject.clone()
            && !state.patterns_failed
            && !self.match_coverage(&state.covers, &subject, span)
        {
            state.failed = true;
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

    /// Decides the coverage of a `match` at `span` on `subject` whose arms
    /// have the shapes `covers`, reporting each unreachable arm, the first
    /// uncovered shape, or a question too large to decide. Whether nothing
    /// was reported.
    fn match_coverage(
        &mut self,
        covers: &[(Rc<Pat>, ByteSpan)],
        subject: &Type,
        span: ByteSpan,
    ) -> bool {
        let rows = covers
            .iter()
            .map(|(cover, _)| Rc::clone(cover))
            .collect::<Vec<_>>();
        let coverage = match Coverage::new(self.program).match_coverage(&rows, subject)
        {
            Ok(coverage) => coverage,
            Err(LimitReached) => {
                self.too_large(span, "this `match`");
                return false;
            }
        };
        let mut clean = true;
        for (at, by) in coverage.unreachable {
            let Some((_, arm_span)) = covers.get(at) else {
                continue;
            };
            let mut diagnostic = Diagnostic::new(
                DiagnosticCode::PatternUnreachableArm,
                *arm_span,
                "no value reaches this arm",
            )
            .with_source_id(self.source_id.as_ref());
            if let Some((_, earlier)) = by.and_then(|by| covers.get(by)) {
                diagnostic = diagnostic
                    .with_related(*earlier, "the arms up to this one cover it");
            }
            self.diagnostics.push(diagnostic);
            clean = false;
        }
        if let Some(shape) = coverage.uncovered {
            let diagnostic = Diagnostic::new(
                DiagnosticCode::PatternNonExhaustive,
                span,
                "this `match` does not cover its subject",
            )
            .with_source_id(self.source_id.as_ref())
            .with_note(shape);
            self.diagnostics.push(diagnostic);
            clean = false;
        }
        clean
    }

    /// Whether the binding pattern of shape `cover` at `span` covers every
    /// value of `ty`, reporting a question too large to decide as `None`.
    fn irrefutable(
        &mut self,
        cover: &Rc<Pat>,
        ty: &Type,
        span: ByteSpan,
    ) -> Option<bool> {
        match Coverage::new(self.program).irrefutable(cover, ty) {
            Ok(irrefutable) => Some(irrefutable),
            Err(LimitReached) => {
                self.too_large(span, "this pattern");
                None
            }
        }
    }

    /// Reports a coverage question about `what` at `span` that needs more
    /// work than the checker gives one.
    fn too_large(&mut self, span: ByteSpan, what: &str) {
        self.report(
            DiagnosticCode::PatternUnsupportedCoverage,
            span,
            format!("the coverage of {what} is too large to decide"),
        );
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
        if let Some(out) = input
            && let Some(child) = self.arrive(&mut state, out)
        {
            return Step::Child(Task::Body(state), child);
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
                    let pattern = view.pattern();
                    let pattern_span = pattern
                        .map_or(span, |pattern| pattern_span(pattern, self.text));
                    let written = match view.written_type() {
                        Some(written) => self.written(written),
                        None => None,
                    };
                    if view.written_type().is_some() && written.is_none() {
                        state.failed = true;
                        self.unfixed = view.value().map(|value| expr_span(&value));
                    }
                    let Some(value) = view.value() else {
                        state.failed = true;
                        state.pieces.push(None);
                        return self.step_body(state, None);
                    };
                    state.pending = Pending::Let {
                        pattern,
                        pattern_span,
                        fallback: view.fallback(),
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

    /// Takes the value of the statement or fallback that `state` waits for,
    /// returning the fallback of a `let … else` to check next.
    fn arrive(&mut self, state: &mut BodyState<'a>, out: Out) -> Option<Task<'a>> {
        match std::mem::replace(&mut state.pending, Pending::Nothing) {
            Pending::Nothing => {}
            Pending::Let {
                pattern,
                pattern_span,
                fallback,
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
                state.last_type = None;
                let Some(pattern) = pattern else {
                    state.failed = true;
                    state.pieces.push(None);
                    return None;
                };
                let typed = self.type_pattern(pattern, pattern_span, ty.as_ref());
                let irrefutable = match (&ty, &typed.lowered) {
                    (Some(ty), Some(_)) => {
                        self.irrefutable(&typed.cover, ty, pattern_span)
                    }
                    _ => None,
                };
                if let Some(fallback) = fallback {
                    if irrefutable == Some(true) {
                        self.report(
                            DiagnosticCode::PatternIrrefutableLetElse,
                            pattern_span,
                            "this pattern covers every value, so the `else` never runs",
                        );
                        state.failed = true;
                    }
                    state.pending = Pending::Fallback(Box::new((
                        typed.bindings,
                        typed.lowered,
                        node,
                    )));
                    return Some(Task::Body(Box::new(
                        self.body_state(fallback, BodyMode::Fallback),
                    )));
                }
                if irrefutable == Some(false) {
                    self.report(
                        DiagnosticCode::PatternRefutableBinding,
                        pattern_span,
                        "this pattern does not cover every value; add an `else`",
                    );
                    state.failed = true;
                }
                self.locals.extend(typed.bindings);
                let piece = match (typed.lowered, node) {
                    (Some(IrPattern::Discard), Some(node)) => {
                        Some(Piece::Let(None, node))
                    }
                    (Some(IrPattern::Bind(slot)), Some(node)) => {
                        Some(Piece::Let(Some(slot), node))
                    }
                    (Some(pattern), Some(node)) => {
                        Some(Piece::Destructure(Box::new((pattern, node))))
                    }
                    _ => None,
                };
                if piece.is_none() {
                    state.failed = true;
                }
                state.pieces.push(piece);
            }
            Pending::Fallback(waiting) => {
                let (bindings, pattern, value) = *waiting;
                self.locals.extend(bindings);
                let piece = match (pattern, value, out.node) {
                    (Some(pattern), Some(value), Some(fallback)) => {
                        Some(Piece::LetElse(Box::new((pattern, value, fallback))))
                    }
                    _ => None,
                };
                if piece.is_none() {
                    state.failed = true;
                }
                state.pieces.push(piece);
            }
            Pending::Expr { span, last } => {
                let mut node = out.node;
                let mut ty = out.ty;
                if ty == Some(Type::Never) && state.never_at.is_none() {
                    state.never_at = Some(span);
                }
                let used = last && matches!(state.mode, BodyMode::Branch(_));
                if !used
                    && let Some(actual) = &ty
                    && self.fallible(actual)
                {
                    self.report(
                        DiagnosticCode::TypeUnhandledFallible,
                        span,
                        format!(
                            "this statement leaves a fallible `{actual}` unused; \
                             handle it, or bind it with `let _`"
                        ),
                    );
                    node = None;
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
                        BodyMode::Branch(Ctx::Value(None))
                        | BodyMode::Callable
                        | BodyMode::Fallback => {}
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
        None
    }

    /// Whether `ty` is fallible: a `Result` whose error type has a value.
    fn fallible(&self, ty: &Type) -> bool {
        let Type::Declared(id, arguments) = ty else {
            return false;
        };
        if self.program.library_of(id) != Some(Library::Result) {
            return false;
        }
        arguments
            .get(1)
            .is_some_and(|error| !Coverage::new(self.program).is_uninhabited(error))
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
                BodyMode::Fallback => {
                    self.report(
                        DiagnosticCode::TypeMismatch,
                        state.span,
                        "expected `Never`: the `else` of a `let … else` can run off \
                         its end, but every path of it must leave",
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
        let value_type = ty.clone().unwrap_or(Type::Void);
        let node = pieces.map(|pieces| self.sequence(pieces, state.span, &value_type));
        Out { ty, node }
    }

    /// Lowers the statements of a body of type `value_type`, from the last to
    /// the first, into sequences and `let` nodes; a `let` whose pattern
    /// takes its value apart is a `match` of one arm.
    fn sequence(&self, pieces: Vec<Piece>, span: ByteSpan, value_type: &Type) -> Node {
        let mut tail: Vec<Node> = Vec::new();
        for piece in pieces.into_iter().rev() {
            if let Piece::Expr(node) = piece {
                tail.push(node);
                continue;
            }
            let body = Box::new(self.close(std::mem::take(&mut tail), span));
            let (expr, let_span) = match piece {
                Piece::Expr(_) => continue,
                Piece::Let(slot, value) => {
                    let let_span = value.origin.span();
                    let value = Box::new(value);
                    (IrExpr::Let { slot, value, body }, let_span)
                }
                Piece::Destructure(destructure) => {
                    let (pattern, value) = *destructure;
                    let let_span = value.origin.span();
                    let expr = IrExpr::Match {
                        subject: Box::new(value),
                        arms: vec![MatchArm {
                            pattern,
                            body: *body,
                        }],
                        value_type: value_type.clone(),
                    };
                    (expr, let_span)
                }
                Piece::LetElse(let_else) => {
                    let (pattern, value, fallback) = *let_else;
                    let let_span = value.origin.span();
                    let expr = IrExpr::LetElse {
                        pattern,
                        value: Box::new(value),
                        fallback: Box::new(fallback),
                        body,
                    };
                    (expr, let_span)
                }
            };
            tail.push(self.node(expr, let_span));
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

/// Matches `pattern`, in which a [`Type::Binder`] below `assignment.len()`
/// is a type parameter of a callee, against `actual`, recording what each
/// parameter it meets must be. `Never` is never inferred. Whether every
/// part matched.
fn unify(pattern: &Type, actual: &Type, assignment: &mut [Option<Type>]) -> bool {
    let mut pending = vec![(pattern, actual)];
    while let Some((pattern, actual)) = pending.pop() {
        match (pattern, actual) {
            (Type::Binder(at), _) if *at < assignment.len() => {
                if *actual == Type::Never {
                    continue;
                }
                match assignment.get_mut(*at) {
                    Some(Some(bound)) => {
                        if bound != actual {
                            return false;
                        }
                    }
                    Some(slot) => *slot = Some(actual.clone()),
                    None => return false,
                }
            }
            (Type::List(pattern), Type::List(actual)) => {
                pending.push((pattern, actual))
            }
            (Type::Tuple(patterns), Type::Tuple(actuals))
                if patterns.len() == actuals.len() =>
            {
                pending.extend(patterns.iter().zip(actuals));
            }
            (Type::Record(patterns), Type::Record(actuals))
                if patterns.names().eq(actuals.names()) =>
            {
                pending.extend(
                    patterns
                        .entries()
                        .iter()
                        .zip(actuals.entries())
                        .map(|((_, pattern), (_, actual))| (pattern, actual)),
                );
            }
            (Type::Declared(left, patterns), Type::Declared(right, actuals))
                if left == right && patterns.len() == actuals.len() =>
            {
                pending.extend(patterns.iter().zip(actuals));
            }
            (Type::Fn(pattern), Type::Fn(actual))
                if pattern.parameters.len() == actual.parameters.len()
                    && pattern.labels.names().eq(actual.labels.names()) =>
            {
                pending.extend(pattern.parameters.iter().zip(&actual.parameters));
                pending.extend(
                    pattern
                        .labels
                        .entries()
                        .iter()
                        .zip(actual.labels.entries())
                        .map(|((_, pattern), (_, actual))| (pattern, actual)),
                );
                pending.push((&pattern.result, &actual.result));
            }
            _ => {
                if pattern != actual {
                    return false;
                }
            }
        }
    }
    true
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
