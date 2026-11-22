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

mod dispatch;
mod guards;
mod patterns;

use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::external::compiler::{CompilerIntrinsic, is_assertion};
use vibra_ir::{
    Arguments, BodyContext, CallTarget, ClosureBody, Constant, Construction, ExitGuard,
    Expr as IrExpr, FnType, Library, MatchArm, Node, Pattern as IrPattern,
    SourceOrigin, Type, Value,
};
use vibra_syntax::source::{
    Argument, Body, Callable, Expr, ExprForm, Ident, If, Let as LetView, Match,
    MatchArm as ArmView, Pattern, PatternForm, Postfix, Primary, Statement, Suffix,
    TypeExpr,
};
use vibra_syntax::{CstNode, SyntaxKind};

use super::callables::{NotConstant, fold, mark_tail, substitute};
use super::contract::{CheckedGuard, ClosureContract, Guards, Site, constant_operands};
use super::declare::{
    BoundData, Entity, NoType, Program, Scope, TypeKind, key_diagnostic,
};
use super::effects;
use super::literal::{self, LiteralFault};
use super::pattern::{Coverage, LimitReached, Pat};
use dispatch::Dispatch;
pub(crate) use guards::{GuardViews, fallible_construction};
use patterns::pattern_span;

/// Where each supported declaration lands in the checked module set.
#[derive(Debug, Default)]
pub(crate) struct Layout {
    /// The global index of each module value of the program.
    pub(crate) globals: Vec<Option<usize>>,
    /// The function index of each function of the program.
    pub(crate) functions: Vec<Option<usize>>,
    /// The value of each module value, by global index, whose initializer
    /// is a constant expression.
    pub(crate) constants: BTreeMap<usize, Value>,
    /// Whether `constants` is complete; before it is, a default that names a
    /// module value is left unchecked rather than called not constant.
    pub(crate) constants_known: bool,
    /// The static type of the `else` consequence of each type's `where …
    /// else`, by the type's program index, once the invariants are checked.
    pub(crate) errors: BTreeMap<usize, Type>,
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
pub(crate) struct Out {
    pub(crate) ty: Option<Type>,
    pub(crate) node: Option<Node>,
}

impl Out {
    /// The out of an expression that has a fault: no type and no node.
    pub(crate) const fn error() -> Self {
        Self {
            ty: None,
            node: None,
        }
    }
}

/// How the value of an expression is used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Ctx {
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
    /// A generic anonymous callable bound by `let`, not yet instantiated.
    Generic(Rc<GenericClosure>),
    /// Already reported.
    Error,
}

/// What a call calls.
#[derive(Debug)]
pub(crate) enum Callee {
    Function(usize),
    /// A callable value: its node, its signature, and the row of its `Proc`
    /// type when it is a procedure value, whose application builds a plan.
    Value(Option<Box<Node>>, Box<FnType>, Option<vibra_ir::Row>),
    /// A generic anonymous callable bound by `let`, instantiated at the
    /// call's type arguments.
    Generic(Rc<GenericClosure>),
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

/// A tuple, record, list, or dict literal being checked, one operand at a
/// time.
pub(crate) struct ProductState<'a> {
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
    /// Whether a list or dict literal is an argument, so that an element
    /// that does not fit is an argument that does not fit its slot.
    argument: bool,
    failed: bool,
}

/// A postfix form being checked, one suffix at a time.
pub(crate) struct PostfixState<'a> {
    ctx: Ctx,
    suffixes: Vec<(Suffix<'a>, ByteSpan)>,
    next: usize,
    head: Head,
    start: usize,
    /// The end of what the head spans so far: the callee of the next call.
    end: usize,
}

/// An application being checked, one argument at a time.
pub(crate) struct CallState<'a> {
    callee: Callee,
    args: Vec<Argument<'a>>,
    next: usize,
    /// The written argument being checked.
    current: usize,
    /// Whether its parameter type was still open when it was started, and
    /// the lengths of the diagnostics, references, and instantiations then.
    open: bool,
    marks: (usize, usize, usize),
    /// Arguments set aside to be checked again once the other operands
    /// have fixed more type arguments, and whether they are being checked
    /// again now.
    retry: Vec<usize>,
    retrying: bool,
    /// The parameter types, in which a type parameter of the callee is a
    /// [`Type::Binder`] into `assignment`: the positional ones, then the
    /// labelled ones in canonical label order.
    params: Option<Vec<Type>>,
    /// The number of positional parameters of a function, a function
    /// value, or a generic anonymous callable.
    positional: Option<usize>,
    /// The labels of its labelled parameters, in canonical order.
    named: Vec<String>,
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
    /// The order the written arguments are checked in: a contract call's
    /// receiver operand first, then the rest as written.
    order: Vec<usize>,
    /// Whether every argument was checked once.
    first_pass: bool,
    /// The dispatch of a contract call.
    dispatch: Option<Box<Dispatch>>,
    /// The bound of each type parameter of the callee, the module it is
    /// written in, and whether it was resolved.
    bounds: Vec<Option<BoundData>>,
    bound_module: usize,
    resolved: Vec<bool>,
    /// Where each type parameter of the callee was first fixed.
    sources: Vec<Option<ByteSpan>>,
    /// Whether an argument checked again still lacked a type argument.
    undetermined: bool,
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

/// An `if` being checked: its condition, then its branches.
pub(crate) struct IfState<'a> {
    view: If<'a>,
    ctx: Ctx,
    stage: u8,
    condition: Option<Node>,
    then: Option<Out>,
    failed: bool,
}

/// A `match` being checked: its subject, then each arm.
pub(crate) struct MatchState<'a> {
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
    /// A `let` that binds a generic anonymous callable, which leaves no
    /// node: each use is a closure of its own.
    Skip,
}

#[derive(Debug)]
enum Pending<'a> {
    Nothing,
    /// A `let` binding a generic anonymous callable to `name`.
    GenericLet {
        name: String,
    },
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

/// A body being checked, one statement at a time.
pub(crate) struct BodyState<'a> {
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

/// One entry of the checker's work list: an expression to start, or a form
/// that resumes with the out of the child task run before it.
pub(crate) enum Task<'a> {
    Expr(Expr<'a>, Ctx),
    Return(ByteSpan),
    /// A `try`, and its operand's span.
    Try(ByteSpan, ByteSpan),
    /// An `await` at its span, and its operand's span, whose operand
    /// [`effects::procs::await_expr`] started.
    Await(ByteSpan, ByteSpan),
    Postfix(Box<PostfixState<'a>>),
    Call(Box<CallState<'a>>),
    Product(Box<ProductState<'a>>),
    If(Box<IfState<'a>>),
    Match(Box<MatchState<'a>>),
    Body(Box<BodyState<'a>>),
    /// An anonymous callable to start, at its span, in its context.
    Anon(Callable<'a>, ByteSpan, Ctx, ClosureMode),
    Closure(Box<ClosureState<'a>>),
}

/// What starting or resuming a task gives: the out of the expression, or a
/// child task to run before the parent task resumes with its out.
pub(crate) enum Step<'a> {
    Done(Out),
    Child(Task<'a>, Task<'a>),
}

/// One local binding.
#[derive(Debug)]
struct Local {
    name: String,
    slot: usize,
    ty: Option<Type>,
    /// The callable activation it belongs to: 0 for the checked callable or
    /// initializer, one more for each anonymous callable around it.
    frame: usize,
    /// The generic anonymous callable a `let` binds to it; such a local has
    /// no slot.
    generic: Option<Rc<GenericClosure>>,
}

/// A generic anonymous callable bound by `let`, checked once over its own
/// type parameters.
#[derive(Debug)]
pub(crate) struct GenericClosure {
    /// Its body, in which its own type parameters are the binders after the
    /// enclosing ones.
    closure: Arc<ClosureBody>,
    /// The index among the locals of each local it captures, in
    /// environment order.
    capture_locals: Vec<usize>,
    /// Its type, over the same binders.
    signature: FnType,
    /// The names of the enclosing type parameters, then of its own.
    binder_names: Vec<String>,
    /// The number of enclosing type parameters.
    outer: usize,
    /// The bound of each of its own type parameters, after `None` for each
    /// enclosing one.
    bounds: Vec<Option<BoundData>>,
    /// The instantiations of generic functions in its body.
    instantiations: Vec<(usize, Vec<Type>, ByteSpan)>,
}

/// What an anonymous callable is checked as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClosureMode {
    /// A value, instantiated from the expected type of its position when it
    /// is generic.
    Value,
    /// The initializer of a `let` that binds it to a name, so it stays
    /// generic.
    Generic,
}

/// The activation enclosing an anonymous callable being checked: what the
/// checker restores when the callable is done, and what it captures.
#[derive(Debug)]
struct Frame {
    /// The length of the locals when the callable started.
    locals: usize,
    /// The slots of the enclosing activation.
    slots: Vec<Type>,
    /// The result of the enclosing callable.
    result: Option<Type>,
    /// The number of type parameters in scope around the callable.
    binders: usize,
    /// The callable's captures, in environment order.
    captures: Vec<Capture>,
}

/// One captured local: its index among the locals, its type, and the node
/// that reads it in the enclosing activation.
#[derive(Debug)]
struct Capture {
    local: usize,
    ty: Option<Type>,
    operand: Option<Node>,
}

/// One parameter that a pattern takes apart: its slot, its type, the
/// pattern, and the pattern's span.
type Destructure = (usize, Type, IrPattern, ByteSpan);

/// An anonymous callable being checked: its labelled defaults, its guards,
/// and then its body.
pub(crate) struct ClosureState<'a> {
    view: Callable<'a>,
    span: ByteSpan,
    mode: ClosureMode,
    stage: u8,
    /// The names of its own type parameters, with one after them for each
    /// associated type a bound of its own leaves open.
    own: Vec<String>,
    signature: FnType,
    /// The type arguments it is instantiated at: the enclosing binders
    /// themselves, then one for each of its own type parameters.
    instance: Option<Vec<Type>>,
    labelled: Vec<(Ident<'a>, TypeExpr<'a>, Expr<'a>)>,
    next: usize,
    defaults: Vec<(String, Constant)>,
    destructured: Vec<Destructure>,
    /// The length of the instantiations when it started.
    instantiations: usize,
    /// The argument span the enclosing checker had.
    argument: Option<ByteSpan>,
    /// Its guards, while they are checked.
    guards: guards::ClosureGuards<'a>,
    /// Its checked guards.
    checked: Guards,
    /// The slots the patterns of its parameters bind.
    bound: std::ops::Range<usize>,
    /// The bound of each of its own type parameters, after `None` for each
    /// enclosing one.
    bounds: Vec<Option<BoundData>>,
    failed: bool,
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
    /// The activations enclosing the anonymous callable being checked,
    /// innermost last.
    frames: Vec<Frame>,
    /// The generic anonymous callable a `let` initializer just produced.
    generic: Option<GenericClosure>,
    /// The bound of each type parameter in scope.
    bounds: Vec<Option<BoundData>>,
    /// The associated names in scope.
    associated: Vec<(String, Type)>,
    /// The interface whose default member is being checked.
    self_interface: Option<usize>,
    /// Each expression whose expected type is written, with the written
    /// type's span.
    destinations: Vec<(ByteSpan, ByteSpan)>,
    /// The span of the written result type of the callable being checked.
    result_span: Option<ByteSpan>,
    /// Forms the checked IR cannot hold, reported only when the body has no
    /// other fault.
    gaps: Vec<(ByteSpan, String)>,
    /// Every place a checked body states a bare-guard obligation or a
    /// pattern a constant construction decides.
    pub(crate) sites: Vec<Site>,
    /// The guards of each anonymous callable checked since the body began.
    closures: Vec<ClosureContract>,
    /// Each `await` checked since the body began, at its span, with the
    /// type of the plan it awaits and the call it runs, by the body or the
    /// anonymous `proc` it is in: what [`effects::analysis::check_bodies`]
    /// reads. [`effects::procs::finish_await`] records it.
    pub(crate) awaits: effects::values::Awaits,
    /// The context the checked callable's body runs in, as
    /// [`effects::procs::lower`] gives it.
    pub(crate) context: BodyContext,
    /// Whether the body checked is a test's.
    test: bool,
}

/// What a callable body sees besides its parameters: its type parameters
/// and their bounds, `Self`, the associated names, the interface whose
/// default member it is, and its written result type.
#[derive(Clone, Debug, Default)]
pub(crate) struct CallableScope {
    pub(crate) binders: Vec<String>,
    pub(crate) bounds: Vec<Option<BoundData>>,
    pub(crate) self_type: Option<Type>,
    pub(crate) associated: Vec<(String, Type)>,
    pub(crate) self_interface: Option<usize>,
    pub(crate) result_span: Option<ByteSpan>,
}

/// The value of one checked callable body or initializer.
pub(crate) struct Checked {
    /// The node, when no fault was found.
    pub(crate) node: Option<Node>,
    /// The number of slots its activation has.
    pub(crate) slot_count: usize,
    /// The constant default of each labelled parameter, in written order.
    pub(crate) defaults: Vec<(String, Constant)>,
    /// The guards of a callable.
    pub(crate) guards: Guards,
    /// The guards of each anonymous callable in it that has one.
    pub(crate) closures: Vec<ClosureContract>,
    /// Each `await` in it, at its span, with the type of the plan it awaits
    /// and the call it runs, by the body or the anonymous `proc` it is in.
    pub(crate) awaits: effects::values::Awaits,
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
            frames: Vec::new(),
            generic: None,
            bounds: Vec::new(),
            associated: Vec::new(),
            self_interface: None,
            destinations: Vec::new(),
            result_span: None,
            gaps: Vec::new(),
            sites: Vec::new(),
            closures: Vec::new(),
            awaits: effects::values::Awaits::default(),
            context: BodyContext::Pure,
            test: false,
        }
    }

    /// Whether the expression being checked is in the checked callable's
    /// own body: not in a guard or a labelled default, where nothing leaves
    /// the callable, and not in an anonymous callable written there.
    pub(crate) fn in_own_body(&self) -> bool {
        self.frames.is_empty() && self.result.is_some()
    }

    /// Whether the expression being checked is in a test's own body, not in
    /// an anonymous callable written there.
    pub(crate) fn in_test_body(&self) -> bool {
        self.test && self.frames.is_empty()
    }

    /// The number of anonymous callables around the expression being
    /// checked.
    pub(crate) fn depth(&self) -> usize {
        self.frames.len()
    }

    /// [`Self::depth`], when the expression being checked is in a callable's
    /// own body: not in a guard or a labelled default.
    pub(crate) fn body_depth(&self) -> Option<usize> {
        self.result.is_some().then_some(self.frames.len())
    }

    /// The program index of the module whose bodies are checked.
    pub(crate) const fn module_index(&self) -> usize {
        self.module
    }

    /// Reports `diagnostic`, found in the module whose bodies are checked.
    pub(crate) fn push_diagnostic(&mut self, diagnostic: Diagnostic) {
        self.diagnostics
            .push(diagnostic.with_source_id(self.source_id.as_ref()));
    }

    /// The program whose body is being checked.
    pub(crate) const fn program(&self) -> &'p Program<'a> {
        self.program
    }

    /// The program index of the function the checked IR holds at `at`.
    pub(crate) fn function_at(&self, at: usize) -> Option<usize> {
        self.layout
            .functions
            .iter()
            .position(|lowered| *lowered == Some(at))
    }

    /// Checks the body of a callable of type `signature` whose parameters
    /// are written as `patterns` and whose labelled parameters are written
    /// as `labelled`, with the type parameters `binders` and `self_type`
    /// for `Self` in scope. Each parameter is bound to its slot, a pattern
    /// that takes it apart is a `match` of one arm around the body, and each
    /// labelled default is checked as a constant of its label's type. The
    /// guards `views` are checked once the parameters are bound.
    pub(crate) fn callable(
        &mut self,
        signature: &FnType,
        patterns: &[Pattern<'a>],
        labelled: &[(Ident<'a>, TypeExpr<'a>, Expr<'a>)],
        scope: CallableScope,
        views: &GuardViews<'a>,
        body: Body<'a>,
    ) -> Checked {
        self.locals.clear();
        self.slots.clear();
        self.frames.clear();
        self.gaps.clear();
        self.closures.clear();
        self.awaits.clear();
        let mark = self.diagnostics.len();
        self.binders = scope.binders;
        self.self_type = scope.self_type;
        self.bounds = scope.bounds;
        self.associated = scope.associated;
        self.self_interface = scope.self_interface;
        self.result_span = scope.result_span;
        let names = labelled
            .iter()
            .map(|(name, _, _)| *name)
            .collect::<Vec<_>>();
        let (mut failed, destructured) =
            self.bind_parameters(signature, patterns, &names);
        let bound = signature.slot_count()..self.slots.len();
        let (faulty, defaults) = self.labelled_defaults(signature, labelled);
        failed |= faulty;
        let (faulty, guards) = self.guards(views, signature, &destructured, &bound);
        failed |= faulty;
        self.result = Some(signature.result.clone());
        let out = self.run(Task::Body(Box::new(
            self.body_state(body, BodyMode::Callable),
        )));
        let mut node = out
            .node
            .filter(|_| !failed)
            .map(|body| self.wrap_destructured(body, destructured));
        let gaps = std::mem::take(&mut self.gaps);
        if !gaps.is_empty() {
            node = None;
        }
        if self.diagnostics.len() == mark {
            for (span, what) in gaps {
                self.unavailable(span, &what);
            }
        }
        Checked {
            node,
            slot_count: self.slots.len(),
            defaults,
            guards,
            closures: std::mem::take(&mut self.closures),
            awaits: std::mem::take(&mut self.awaits),
        }
    }

    /// Checks each labelled default of `labelled` as a constant of its
    /// label's type in `signature`: whether one had a fault, and the
    /// constants, in written order.
    fn labelled_defaults(
        &mut self,
        signature: &FnType,
        labelled: &[(Ident<'a>, TypeExpr<'a>, Expr<'a>)],
    ) -> (bool, Vec<(String, Constant)>) {
        // A default is checked where nothing can leave a callable.
        self.result = None;
        let mut failed = false;
        let mut defaults = Vec::new();
        for (name, _, default) in labelled {
            let Some(ty) = signature.labels.get(name.text()).cloned() else {
                failed = true;
                continue;
            };
            let span = expr_span(default);
            let out =
                self.run(Task::Expr(default.clone(), Ctx::Value(Some(ty.clone()))));
            match self.default_value(out, &ty, span) {
                Some(constant) => defaults.push((name.text().to_owned(), constant)),
                None => failed = true,
            }
        }
        (failed, defaults)
    }

    /// The labelled defaults `labelled` of a contract member of type
    /// `signature`, whose parameters are written as `patterns`, checked as a
    /// callable's are under `scope`. The parameters are bound only so that a
    /// default naming one is not constant; their patterns are checked where
    /// the member's body is.
    pub(crate) fn contract_defaults(
        &mut self,
        signature: &FnType,
        patterns: &[Pattern<'a>],
        labelled: &[(Ident<'a>, TypeExpr<'a>, Expr<'a>)],
        scope: CallableScope,
    ) -> Vec<(String, Constant)> {
        self.locals.clear();
        self.slots.clear();
        self.frames.clear();
        self.gaps.clear();
        self.binders = scope.binders;
        self.self_type = scope.self_type;
        self.bounds = scope.bounds;
        self.associated = scope.associated;
        self.self_interface = scope.self_interface;
        self.result_span = scope.result_span;
        let names = labelled
            .iter()
            .map(|(name, _, _)| *name)
            .collect::<Vec<_>>();
        let mark = self.diagnostics.len();
        let _ = self.bind_parameters(signature, patterns, &names);
        self.diagnostics.truncate(mark);
        self.labelled_defaults(signature, labelled).1
    }

    /// The parameter patterns `patterns` and the guards `views` of an
    /// abstract contract member of type `signature`, whose labelled
    /// parameters are written as `labelled`, checked once under `scope` with
    /// `Self` open, so that the contract's own faults are reported where it
    /// is declared. The checked guards, which a call at a receiver no
    /// implementation member is known for is decided from, or `None` when
    /// one has a fault.
    pub(crate) fn abstract_guards(
        &mut self,
        signature: &FnType,
        patterns: &[Pattern<'a>],
        labelled: &[Ident<'a>],
        scope: CallableScope,
        views: &GuardViews<'a>,
    ) -> Option<Guards> {
        self.locals.clear();
        self.slots.clear();
        self.frames.clear();
        self.gaps.clear();
        self.binders = scope.binders;
        self.self_type = scope.self_type;
        self.bounds = scope.bounds;
        self.associated = scope.associated;
        self.self_interface = scope.self_interface;
        self.result_span = scope.result_span;
        self.result = Some(signature.result.clone());
        let mark = self.diagnostics.len();
        let (failed, _) = self.bind_parameters(signature, patterns, labelled);
        if failed || self.diagnostics.len() != mark {
            return None;
        }
        self.slots.truncate(signature.slot_count());
        self.locals.clear();
        let guards = self.member_guards(signature, patterns, labelled, views);
        // A contract guard is copied into each implementation, and an
        // obligation it states is one obligation of each implementation.
        self.sites.clear();
        guards
    }

    /// The guards `views` of the contract member that the callable just
    /// checked implements, written in the module `module` whose source is
    /// `text`, checked under the implementation's substitution: each
    /// parameter name of the contract member denotes the implementation's
    /// parameter in the same position, and each slot a guard binds comes
    /// after the body's. The guards and the slot count, or `None` when one
    /// has a fault; the faults themselves are the contract's, reported
    /// where it is declared. The obligation sites the guards state are kept,
    /// spanning the contract's module.
    pub(crate) fn implementation_guards(
        &mut self,
        signature: &FnType,
        patterns: &[Pattern<'a>],
        labelled: &[Ident<'a>],
        module: (usize, Arc<str>, &'a str),
        views: &GuardViews<'a>,
    ) -> Option<(Guards, usize)> {
        let (module, source_id, text) = module;
        let saved = (
            std::mem::replace(&mut self.module, module),
            std::mem::replace(&mut self.source_id, source_id),
            std::mem::replace(&mut self.text, text),
        );
        let (diagnostics, sites) = (self.diagnostics.len(), self.sites.len());
        let guards = self.member_guards(signature, patterns, labelled, views);
        self.diagnostics.truncate(diagnostics);
        if guards.is_none() {
            self.sites.truncate(sites);
        }
        (self.module, self.source_id, self.text) = saved;
        guards.map(|guards| (guards, self.slots.len()))
    }

    /// Checks the guards `views` of a contract member of type `signature`
    /// whose parameter slots are the first of the current ones: each
    /// positional parameter written as a name in `patterns` is bound to its
    /// slot, and each labelled one in `labelled` to its label's. A pattern
    /// that takes a parameter apart binds slots after the current ones, and
    /// a guard that reads them is a `match` of one arm around it. The guards,
    /// or `None` when one has a fault.
    fn member_guards(
        &mut self,
        signature: &FnType,
        patterns: &[Pattern<'a>],
        labelled: &[Ident<'a>],
        views: &GuardViews<'a>,
    ) -> Option<Guards> {
        if views.requires.is_none() && views.ensures.is_none() {
            return Some(Guards::default());
        }
        let mark = self.diagnostics.len();
        self.locals.clear();
        let frame = self.frames.len();
        let start = self.slots.len();
        let mut destructured = Vec::new();
        for (index, (ty, pattern)) in
            signature.parameters.iter().zip(patterns).enumerate()
        {
            let span = pattern_span(*pattern, self.text);
            let typed = self.type_pattern(*pattern, span, Some(ty));
            match typed.lowered {
                Some(IrPattern::Bind(fresh)) if fresh + 1 == self.slots.len() => {
                    self.slots.pop();
                    self.locals
                        .extend(typed.bindings.into_iter().map(|local| Local {
                            slot: index,
                            ..local
                        }));
                }
                Some(IrPattern::Discard) => {}
                Some(lowered)
                    if self.irrefutable(&typed.cover, ty, span) != Some(false) =>
                {
                    self.locals.extend(typed.bindings);
                    destructured.push((index, ty.clone(), lowered, span));
                }
                _ => return None,
            }
        }
        let entries = signature.labels.entries();
        for name in labelled {
            let Some(at) = entries.iter().position(|(label, _)| label == name.text())
            else {
                continue;
            };
            self.locals.push(Local {
                name: name.text().to_owned(),
                slot: signature.parameters.len() + at,
                ty: entries.get(at).map(|(_, ty)| ty.clone()),
                frame,
                generic: None,
            });
        }
        let bound = start..self.slots.len();
        let (failed, guards) = self.guards(views, signature, &destructured, &bound);
        (!failed && self.diagnostics.len() == mark).then_some(guards)
    }
    /// Binds the parameters of a callable of type `signature`, whose slots
    /// start at the current slot count: each positional one through its
    /// pattern, each labelled one written as `labelled` to its slot. Whether
    /// a pattern had a fault, and the parameters patterns take apart.
    fn bind_parameters(
        &mut self,
        signature: &FnType,
        patterns: &[Pattern<'a>],
        labelled: &[Ident<'a>],
    ) -> (bool, Vec<Destructure>) {
        let base = self.slots.len();
        let params = &signature.parameters;
        self.slots.extend(signature.slot_types());
        let frame = self.frames.len();
        let mut failed = false;
        let mut destructured = Vec::new();
        for (index, (ty, pattern)) in params.iter().zip(patterns).enumerate() {
            let slot = base + index;
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
        for name in labelled {
            let entries = signature.labels.entries();
            let Some(at) = entries.iter().position(|(label, _)| label == name.text())
            else {
                continue;
            };
            self.locals.push(Local {
                name: name.text().to_owned(),
                slot: base + params.len() + at,
                ty: entries.get(at).map(|(_, ty)| ty.clone()),
                frame,
                generic: None,
            });
        }
        (failed, destructured)
    }

    /// `body` inside a `match` of one arm for each parameter a pattern takes
    /// apart, the first parameter outermost.
    fn wrap_destructured(&self, body: Node, destructured: Vec<Destructure>) -> Node {
        destructured.into_iter().rev().fold(
            body,
            |body, (slot, value_type, pattern, span)| {
                let subject = self.node(IrExpr::Variable { slot, value_type }, span);
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
    }

    /// The constant a labelled default of type `ty`, written at `span`,
    /// denotes, once its value `out` arrived; reports a default that does
    /// not fit its type or is not a constant expression.
    fn default_value(
        &mut self,
        out: Out,
        ty: &Type,
        span: ByteSpan,
    ) -> Option<Constant> {
        let node = match out.ty {
            Some(Type::Never) => {
                self.report(
                    DiagnosticCode::TypeUnreachableCode,
                    span,
                    "this default never completes, so it has no value",
                );
                None
            }
            Some(actual) => {
                self.widen(ty, &actual, out.node, span, DiagnosticCode::TypeMismatch)
            }
            None => None,
        }?;
        // A construction of a type whose `where` has an `else` is no
        // constant expression.
        let program = self.program;
        let folded = if fallible_construction(program, &node) {
            Err(NotConstant::Expression)
        } else {
            fold(&node, &self.layout.constants, &|id| program.library_of(id))
        };
        match folded {
            Ok(value) => {
                // The default checked at `ty`, binders included, so it holds
                // nothing at a binder's position; the checked IR keeps a
                // constant closed, at `ty` with every binder `Never`.
                // Unreachable: the default checked at `ty`, so its value is
                // one of `ty`, and no value has an open type; kept as a
                // defect net.
                if let Ok(constant) = Constant::new(ty.at_never(), value) {
                    Some(constant)
                } else {
                    self.unavailable(span, "a labelled default of an open type");
                    None
                }
            }
            Err(NotConstant::Dict) => {
                self.report(
                    DiagnosticCode::TypeNotConstant,
                    span,
                    "a dict keyed by a type ordered by its own `Ordered` implementation \
                     is no constant expression: ordering its entries would run that \
                     implementation",
                );
                None
            }
            Err(NotConstant::Pending) if !self.layout.constants_known => None,
            Err(NotConstant::Pending | NotConstant::Expression) => {
                self.report(
                    DiagnosticCode::TypeNotConstant,
                    span,
                    "a labelled default must be a constant expression: a literal, an \
                     atom, `void`, a construction, tuple, record, list, or dict of \
                     constants, or the name of a constant module value",
                );
                None
            }
        }
    }

    /// Checks the body of a test: a body of type `Void` outside any callable,
    /// so that a `return` or a `try` written directly in it is reported.
    /// A test that takes a world binds its one bare-name parameter `world`,
    /// of its type, to the first slot.
    pub(crate) fn test_body(
        &mut self,
        body: Body<'a>,
        world: Option<(Pattern<'a>, Type)>,
    ) -> Checked {
        self.locals.clear();
        self.slots.clear();
        self.frames.clear();
        self.binders.clear();
        self.self_type = None;
        self.reset_scope();
        self.result = None;
        self.test = true;
        if let Some((pattern, ty)) = world {
            let _ = self.bind_parameters(
                &FnType::new(vec![ty], Type::Void),
                &[pattern],
                &[],
            );
        }
        let out = self.run(Task::Body(Box::new(
            self.body_state(body, BodyMode::Branch(Ctx::Stmt)),
        )));
        Checked {
            node: out.node,
            slot_count: self.slots.len(),
            defaults: Vec::new(),
            guards: Guards::default(),
            closures: std::mem::take(&mut self.closures),
            awaits: std::mem::take(&mut self.awaits),
        }
    }

    /// Checks the initializer of a module value of type `ty`.
    pub(crate) fn initializer(
        &mut self,
        ty: &Type,
        value: Expr<'a>,
        written: Option<ByteSpan>,
    ) -> Checked {
        self.locals.clear();
        self.slots.clear();
        self.frames.clear();
        self.binders.clear();
        self.self_type = None;
        self.reset_scope();
        self.result = None;
        let span = expr_span(&value);
        if let Some(written) = written {
            self.destinations.push((span, written));
        }
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
                self.widen(ty, &actual, out.node, span, DiagnosticCode::TypeMismatch)
            }
            None => None,
        };
        Checked {
            node,
            slot_count: self.slots.len(),
            defaults: Vec::new(),
            guards: Guards::default(),
            closures: std::mem::take(&mut self.closures),
            awaits: std::mem::take(&mut self.awaits),
        }
    }

    fn origin(&self, span: ByteSpan) -> SourceOrigin {
        SourceOrigin::new(Arc::clone(&self.source_id), span)
    }

    /// Clears what a callable's scope set: bounds, associated names, the
    /// default member's interface, and the written result type.
    fn reset_scope(&mut self) {
        self.bounds.clear();
        self.associated.clear();
        self.self_interface = None;
        self.result_span = None;
        self.gaps.clear();
        self.closures.clear();
        self.awaits.clear();
    }

    pub(crate) fn node(&self, expr: IrExpr, span: ByteSpan) -> Node {
        Node::new(expr, self.origin(span))
    }

    /// Reports code at span in the module being checked.
    pub(crate) fn report(
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

    /// Reports each field of a record value that repeats an earlier one:
    /// `@name.redeclaration` at the later field, related to the earlier.
    fn repeated_fields(&mut self, names: impl Iterator<Item = Ident<'a>>) {
        let mut seen: BTreeMap<&'a str, ByteSpan> = BTreeMap::new();
        for name in names {
            if let Some(first) = seen.get(name.text()) {
                let diagnostic = Diagnostic::new(
                    DiagnosticCode::NameRedeclaration,
                    name.span(),
                    format!("the field `{}` is written twice", name.text()),
                )
                .with_source_id(self.source_id.as_ref())
                .with_related(*first, "the earlier field");
                self.diagnostics.push(diagnostic);
            } else {
                seen.insert(name.text(), name.span());
            }
        }
    }

    fn unavailable(&mut self, span: ByteSpan, what: &str) {
        self.report(
            DiagnosticCode::ToolUnavailable,
            span,
            format!("{what} is not supported by this checker yet"),
        );
    }

    /// Checks `actual` against `expected`, reporting `code` at `span` when it
    /// does not fit. Nothing widens here: see [`Self::widen`].
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
            associated: &self.associated,
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

    /// The index of the innermost local named `name`.
    fn local_index(&self, name: &str) -> Option<usize> {
        self.locals.iter().rposition(|local| local.name == name)
    }

    /// A read of the local at `index` at `span`: of its slot in its own
    /// activation, or of a capture of each anonymous callable between that
    /// activation and the current one, which captures it if it does not yet.
    fn read_local(&mut self, index: usize, span: ByteSpan) -> Out {
        let Some(local) = self.locals.get(index) else {
            return Out::error();
        };
        let (slot, ty, frame) = (local.slot, local.ty.clone(), local.frame);
        let origin = self.origin(span);
        let mut node = ty.clone().map(|value_type| {
            Node::new(IrExpr::Variable { slot, value_type }, origin.clone())
        });
        for level in frame.saturating_add(1)..=self.frames.len() {
            let Some(enclosing) =
                level.checked_sub(1).and_then(|at| self.frames.get_mut(at))
            else {
                return Out::error();
            };
            let at = match enclosing
                .captures
                .iter()
                .position(|capture| capture.local == index)
            {
                Some(at) => at,
                None => {
                    enclosing.captures.push(Capture {
                        local: index,
                        ty: ty.clone(),
                        operand: node.take(),
                    });
                    enclosing.captures.len() - 1
                }
            };
            node = ty.clone().map(|value_type| {
                Node::new(
                    IrExpr::Captured {
                        slot: at,
                        value_type,
                    },
                    origin.clone(),
                )
            });
        }
        Out { ty, node }
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
                Task::Await(span, operand) => {
                    effects::procs::finish_await(self, span, operand, input.take())
                }
                Task::Postfix(state) => self.step_postfix(state, input.take()),
                Task::Call(state) => self.step_call(state, input.take()),
                Task::Product(state) => self.step_product(state, input.take()),
                Task::If(state) => self.step_if(state, input.take()),
                Task::Match(state) => self.step_match(state, input.take()),
                Task::Body(state) => self.step_body(state, input.take()),
                Task::Anon(view, span, ctx, mode) => {
                    self.start_closure(view, span, &ctx, mode)
                }
                Task::Closure(state) => self.step_closure(state, input.take()),
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
                if self.frames.is_empty()
                    && let Some(written) = self.result_span
                {
                    self.destinations.push((expr_span(&operand), written));
                }
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
            ExprForm::Await(operand) => {
                effects::procs::await_expr(self, operand, span, &ctx)
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
        let tail = out.ty.as_ref().is_none_or(effects::procs::marks_tail);
        let node = match out.ty {
            // `return spin(n)`, where `spin` returns `Never`, is a tail call.
            Some(Type::Never) if out.node.as_ref().is_some_and(ends_in_call) => {
                out.node
            }
            Some(Type::Never) => {
                self.report(
                    DiagnosticCode::TypeUnreachableCode,
                    operand_span,
                    "this operand never completes, so `return` has no value to return",
                );
                None
            }
            Some(actual) => self.widen(
                &result,
                &actual,
                out.node,
                operand_span,
                DiagnosticCode::TypeMismatch,
            ),
            None => None,
        };
        let node = node.map(|mut operand| {
            if tail {
                mark_tail(&mut operand);
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
                self.repeated_fields(fields.iter().map(|(name, _)| *name));
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
            Primary::Callable(view) => Step::Child(
                Task::Postfix(state),
                Task::Anon(view, primary_span, inner_ctx, ClosureMode::Value),
            ),
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
        if let Some(index) = self.local_index(name) {
            if let Some(generic) = self
                .locals
                .get(index)
                .and_then(|local| local.generic.clone())
            {
                return (Head::Generic(generic), 0);
            }
            return (Head::Value(Box::new(self.read_local(index, span))), 0);
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
                Entity::Fn(_) | Entity::Value(_) => break,
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
                        Head::Generic(generic) => {
                            Some(self.generic_value(&generic, callee_span, None))
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
                        Head::Generic(generic) => Some(Callee::Generic(generic)),
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
            Head::Generic(generic) => {
                let expected = state.ctx.expected().cloned();
                self.generic_value(&generic, whole, expected.as_ref())
            }
            Head::Case(..) | Head::Type(_) => {
                let expected = state.ctx.expected().cloned();
                self.not_a_function(whole, expected.as_ref());
                Out::error()
            }
            Head::Contract(interface, member) => {
                let expected = state.ctx.expected().cloned();
                self.contract_value(interface, &member, whole, expected.as_ref())
            }
            Head::Static => {
                self.unavailable(whole, "a type used as a value");
                Out::error()
            }
            Head::Pending | Head::Error => Out::error(),
        };
        Step::Done(out)
    }

    /// Reports a declared type or case named at `span` in value position
    /// without being applied: a constructor is not a value, whatever is
    /// `expected`.
    fn not_a_function(&mut self, span: ByteSpan, expected: Option<&Type>) {
        let code = if self.argument == Some(span) {
            DiagnosticCode::TypeArgumentMismatch
        } else {
            DiagnosticCode::TypeMismatch
        };
        let message = match expected {
            Some(expected) => format!(
                "expected `{expected}`, found a constructor, which is not a function \
                 value; an anonymous `fn` wraps one"
            ),
            None => "found a constructor, which is not a function value; apply it, \
                     or wrap it in an anonymous `fn`"
                .to_owned(),
        };
        self.report(code, span, message);
    }

    /// Starts the anonymous callable `view` written at `span`: reads its
    /// header, opens its activation, and binds its parameters.
    fn start_closure(
        &mut self,
        view: Callable<'a>,
        span: ByteSpan,
        ctx: &Ctx,
        mode: ClosureMode,
    ) -> Step<'a> {
        if let Some(step) = effects::values::anonymous(self, &view, span, ctx, mode) {
            return step;
        }
        let mut own = view
            .params()
            .iter()
            .map(|name| name.text().to_owned())
            .collect::<Vec<_>>();
        let outer = self.binders.len();
        let mut binders = self.binders.clone();
        binders.extend(own.iter().cloned());
        let scope = Scope {
            self_type: self.self_type.as_ref(),
            binders: &binders,
            associated: &self.associated,
        };
        let read = self
            .program
            .callable_type(self.module, &view, scope)
            .and_then(|signature| {
                let bounds = self.program.read_bounds(
                    self.module,
                    view.given(),
                    &binders,
                    outer,
                    scope,
                )?;
                Ok((signature, bounds))
            });
        let (signature, bounds) = match read {
            Ok((signature, bounds)) => {
                // An associated type its own bound leaves open is one more
                // parameter of its own.
                let (all, bounds) = self.program.open_choices(&binders, bounds);
                own.extend(all.iter().skip(binders.len()).cloned());
                binders = all;
                (signature, bounds)
            }
            Err(NoType::Unsupported) => {
                return self.unavailable_primary(span, "this callable header");
            }
            Err(NoType::Unresolved) => return Step::Done(Out::error()),
            Err(NoType::Fault(diagnostic)) => {
                self.diagnostics
                    .push(diagnostic.with_source_id(self.source_id.as_ref()));
                return Step::Done(Out::error());
            }
        };
        let instance = if own.is_empty() || mode == ClosureMode::Generic {
            None
        } else {
            match self.closure_instance(
                &signature,
                outer,
                &own,
                &bounds,
                ctx.expected(),
                span,
            ) {
                Some(instance) => Some(instance),
                None => return Step::Done(Out::error()),
            }
        };
        let argument = self.argument.take();
        self.frames.push(Frame {
            locals: self.locals.len(),
            slots: std::mem::take(&mut self.slots),
            result: self.result.take(),
            binders: outer,
            captures: Vec::new(),
        });
        self.binders = binders;
        // Its body sees its own bounds after those around it.
        self.bounds.resize(outer, None);
        self.bounds.extend(bounds.iter().skip(outer).cloned());
        let patterns = view
            .takes()
            .into_iter()
            .map(|(pattern, _)| pattern)
            .collect::<Vec<_>>();
        let labelled = view.labelled();
        let names = labelled
            .iter()
            .map(|(name, _, _)| *name)
            .collect::<Vec<_>>();
        let (failed, destructured) =
            self.bind_parameters(&signature, &patterns, &names);
        let bound = signature.slot_count()..self.slots.len();
        let guards = self.closure_guards(&GuardViews::of(&view));
        let state = Box::new(ClosureState {
            view,
            span,
            mode,
            stage: 0,
            own,
            signature,
            instance,
            labelled,
            next: 0,
            defaults: Vec::new(),
            destructured,
            instantiations: self.instantiations.len(),
            argument,
            guards,
            checked: Guards::default(),
            bound,
            bounds,
            failed,
        });
        self.step_closure(state, None)
    }

    /// The name of each argument slot of the callable `view` of type
    /// `signature`: each positional parameter's pattern as written, then
    /// the labels in canonical order.
    fn parameter_names(&self, view: &Callable<'a>, signature: &FnType) -> Vec<String> {
        let mut names = view
            .takes()
            .into_iter()
            .map(|(pattern, _)| {
                let span = pattern.node().span();
                self.text
                    .get(span.start()..span.end())
                    .unwrap_or_default()
                    .to_owned()
            })
            .collect::<Vec<_>>();
        names.extend(
            signature
                .labels
                .entries()
                .iter()
                .map(|(label, _)| label.clone()),
        );
        names
    }

    /// The type arguments a generic anonymous callable of type `signature`,
    /// written at `span` where `expected` is, is instantiated at: the
    /// `outer` enclosing binders themselves, then what `expected` fixes each
    /// of its own type parameters `own` to. Reports one it cannot fix.
    fn closure_instance(
        &mut self,
        signature: &FnType,
        outer: usize,
        own: &[String],
        bounds: &[Option<BoundData>],
        expected: Option<&Type>,
        span: ByteSpan,
    ) -> Option<Vec<Type>> {
        let pattern = effects::values::pattern(self, span, signature);
        let base = (0..outer)
            .map(|at| Some(Type::Binder(at)))
            .chain(own.iter().map(|_| None))
            .collect::<Vec<_>>();
        let mut assignment = base.clone();
        let fits = expected.is_some_and(|expected| {
            unify_expected(&pattern, expected, &mut assignment)
        });
        if let Some(expected) = expected
            && !fits
        {
            let code = if self.argument == Some(span) {
                DiagnosticCode::TypeArgumentMismatch
            } else {
                DiagnosticCode::TypeMismatch
            };
            self.report(
                code,
                span,
                format!(
                    "expected `{expected}`, found the generic callable `{pattern}`"
                ),
            );
            return None;
        }
        // The expected type fixes the type arguments, so a breach of a bound
        // is reported where that type is written.
        let source = self.written_span(span).unwrap_or(span);
        let mut resolved = vec![false; assignment.len()];
        let sources = vec![Some(source); assignment.len()];
        let module = self.module;
        if !self.resolve_bounds(
            bounds,
            module,
            &mut assignment,
            &mut resolved,
            &sources,
            span,
            true,
        ) {
            return None;
        }
        let missing = own
            .iter()
            .zip(assignment.iter().skip(outer))
            .filter(|(_, bound)| bound.is_none())
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            self.ambiguous(span, &missing);
            return None;
        }
        Some(assignment.into_iter().flatten().collect())
    }

    fn step_closure(
        &mut self,
        mut state: Box<ClosureState<'a>>,
        input: Option<Out>,
    ) -> Step<'a> {
        let mut input = input;
        if state.stage == 0 {
            if let Some(out) = input.take()
                && let Some((name, _, default)) = state
                    .next
                    .checked_sub(1)
                    .and_then(|at| state.labelled.get(at))
            {
                let (name, span) = (name.text().to_owned(), expr_span(default));
                let constant = match state.signature.labels.get(&name).cloned() {
                    Some(ty) => self.default_value(out, &ty, span),
                    None => None,
                };
                match constant {
                    Some(constant) => state.defaults.push((name, constant)),
                    None => state.failed = true,
                }
            }
            if let Some((name, _, default)) = state.labelled.get(state.next).cloned() {
                state.next += 1;
                let ty = state.signature.labels.get(name.text()).cloned();
                // A default is checked where nothing can leave a callable.
                self.result = None;
                return Step::Child(
                    Task::Closure(state),
                    Task::Expr(default, Ctx::Value(ty)),
                );
            }
            state.stage = 1;
            // Nothing leaves a callable from a guard.
            self.result = None;
        }
        if state.stage == 1 {
            if let Some(task) = self.next_closure_guard(
                &mut state.guards,
                &state.signature,
                input.take(),
            ) {
                return Step::Child(Task::Closure(state), task);
            }
            let (failed, checked) = self.finish_closure_guards(
                &mut state.guards,
                &state.signature,
                &state.destructured,
                &state.bound,
            );
            state.failed |= failed;
            state.checked = checked;
            state.stage = 2;
            self.result = Some(state.signature.result.clone());
            if let Some(body) = state.view.body() {
                let body =
                    Task::Body(Box::new(self.body_state(body, BodyMode::Callable)));
                return Step::Child(Task::Closure(state), body);
            }
            state.failed = true;
        }
        Step::Done(self.finish_closure(*state, input))
    }

    /// Closes the activation of an anonymous callable whose body arrived as
    /// `input`, and builds its closure.
    fn finish_closure(&mut self, state: ClosureState<'a>, input: Option<Out>) -> Out {
        let body = input.and_then(|out| out.node);
        let Some(frame) = self.frames.pop() else {
            return Out::error();
        };
        let slot_count = self.slots.len();
        self.slots = frame.slots;
        self.result = frame.result;
        self.binders.truncate(frame.binders);
        self.bounds.truncate(frame.binders);
        self.locals.truncate(frame.locals);
        self.argument = state.argument;
        let value_type =
            effects::values::closure_type(self, state.span, &state.signature);
        let ty = match &state.instance {
            Some(instance) => value_type.instantiate(instance),
            None => value_type.clone(),
        };
        let mut failed = state.failed;
        let mut capture_types = Vec::new();
        let mut operands = Vec::new();
        let mut locals = Vec::new();
        for capture in frame.captures {
            match (capture.ty, capture.operand) {
                (Some(ty), Some(operand)) => {
                    capture_types.push(ty);
                    operands.push(operand);
                }
                _ => failed = true,
            }
            locals.push(capture.local);
        }
        let body = match body {
            Some(body) if !failed => body,
            _ => {
                let ty = (state.mode == ClosureMode::Value).then_some(ty);
                return Out { ty, node: None };
            }
        };
        let body = self.wrap_destructured(body, state.destructured);
        // A guard with `else` runs on every call; a bare one leaves no code
        // and is the review's.
        let with_else = |guard: &CheckedGuard| {
            Some(vibra_ir::Guard {
                predicate: guard.predicate.clone(),
                consequence: guard.consequence.clone()?,
            })
        };
        let entry_guard = state.checked.entry.as_ref().and_then(with_else);
        let exit_guard = state.checked.exit.as_ref().and_then(|(slot, guard)| {
            Some(ExitGuard {
                candidate: *slot,
                guard: with_else(guard)?,
            })
        });
        if state.checked.entry.is_some() || state.checked.exit.is_some() {
            let mut guards = state.checked.clone();
            if let Some(instance) = &state.instance {
                let nodes = guards
                    .entry
                    .iter_mut()
                    .chain(guards.exit.iter_mut().map(|(_, guard)| guard));
                for guard in nodes {
                    substitute(&mut guard.predicate, instance);
                    if let Some(consequence) = &mut guard.consequence {
                        substitute(consequence, instance);
                    }
                }
            }
            let signature = match &ty {
                Type::Fn(signature) => (**signature).clone(),
                Type::Proc(signature) => FnType {
                    parameters: signature.parameters.clone(),
                    labels: signature.labels.clone(),
                    result: signature.result.clone(),
                },
                _ => state.signature.clone(),
            };
            let names = self.parameter_names(&state.view, &signature);
            self.closures.push(ClosureContract {
                plan: matches!(ty, Type::Proc(_)),
                body: (Arc::clone(&self.source_id), body.origin.span()),
                module: self.module,
                span: state.span,
                signature,
                names,
                result_name: state
                    .view
                    .result()
                    .map(|(name, _)| name.text().to_owned())
                    .unwrap_or_default(),
                defaults: state.defaults.clone(),
                guards,
            });
        }
        let closure = ClosureBody {
            value_type,
            slot_count,
            captures: capture_types,
            defaults: state.defaults,
            body,
            entry_guard,
            exit_guard,
        };
        let records = self.instantiations.split_off(state.instantiations);
        if state.mode == ClosureMode::Generic {
            let mut binder_names = self.binders.clone();
            binder_names.extend(state.own.iter().cloned());
            self.generic = Some(GenericClosure {
                closure: Arc::new(closure),
                capture_locals: locals,
                signature: state.signature,
                binder_names,
                outer: frame.binders,
                bounds: state.bounds,
                instantiations: records,
            });
            return Out::error();
        }
        let mut node = self.node(
            IrExpr::Closure {
                closure: Arc::new(closure),
                captures: Vec::new(),
            },
            state.span,
        );
        match &state.instance {
            Some(instance) => {
                substitute(&mut node, instance);
                self.instantiations.extend(records.into_iter().map(
                    |(function, arguments, span)| {
                        let arguments = arguments
                            .iter()
                            .map(|ty| ty.instantiate(instance))
                            .collect();
                        (function, arguments, span)
                    },
                ));
            }
            None => self.instantiations.extend(records),
        }
        if let IrExpr::Closure { captures, .. } = &mut node.expr {
            *captures = operands;
        }
        Out {
            ty: Some(ty),
            node: Some(node),
        }
    }

    /// A use at `span`, where `expected` is, of a generic anonymous callable
    /// bound by `let`, as a value: instantiated at what `expected` fixes.
    fn generic_value(
        &mut self,
        generic: &GenericClosure,
        span: ByteSpan,
        expected: Option<&Type>,
    ) -> Out {
        let pattern = Type::function(generic.signature.clone());
        let mut assignment = self.generic_assignment(generic);
        if let Some(expected) = expected {
            let mut trial = assignment.clone();
            if unify_expected(&pattern, expected, &mut trial) {
                assignment = trial;
            }
        }
        let source = self.written_span(span).unwrap_or(span);
        let mut resolved = vec![false; assignment.len()];
        let sources = vec![Some(source); assignment.len()];
        let module = self.module;
        if !self.resolve_bounds(
            &generic.bounds,
            module,
            &mut assignment,
            &mut resolved,
            &sources,
            span,
            true,
        ) {
            return Out::error();
        }
        let missing = generic
            .binder_names
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
        let node = self.instantiate_generic(generic, &arguments, span);
        Out {
            ty: Some(pattern.instantiate(&arguments)),
            node,
        }
    }

    /// What a use of `generic` starts from: each enclosing binder is itself,
    /// and each of its own type parameters is still open.
    fn generic_assignment(&self, generic: &GenericClosure) -> Vec<Option<Type>> {
        (0..generic.binder_names.len())
            .map(|at| (at < generic.outer).then_some(Type::Binder(at)))
            .collect()
    }

    /// The closure of `generic` at the type arguments `arguments`, at
    /// `span`, capturing what it captures from where it is used.
    fn instantiate_generic(
        &mut self,
        generic: &GenericClosure,
        arguments: &[Type],
        span: ByteSpan,
    ) -> Option<Node> {
        self.instantiations
            .extend(
                generic
                    .instantiations
                    .iter()
                    .map(|(function, records, at)| {
                        let records = records
                            .iter()
                            .map(|ty| ty.instantiate(arguments))
                            .collect();
                        (*function, records, *at)
                    }),
            );
        let mut node = self.node(
            IrExpr::Closure {
                closure: Arc::clone(&generic.closure),
                captures: Vec::new(),
            },
            span,
        );
        substitute(&mut node, arguments);
        let mut operands = Vec::new();
        for &local in &generic.capture_locals {
            operands.push(self.read_local(local, span).node?);
        }
        if let IrExpr::Closure { captures, .. } = &mut node.expr {
            *captures = operands;
        }
        Some(node)
    }

    /// The constructor a type applied as a callee denotes, reporting a type
    /// that has none.
    fn constructor(&mut self, index: usize, callee_span: ByteSpan) -> Option<Callee> {
        let program = self.program;
        let data = program.types.get(index)?;
        match data.kind {
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
                Some(Callee::Value(node.map(Box::new), signature.clone(), None))
            }
            Some(Type::Proc(ref signature)) => {
                effects::procs::apply(self, node, signature, callee_span)
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
        let pattern = effects::procs::named_as_value(program, data, signature);
        let (ty, type_arguments) = if data.binders.is_empty() {
            (pattern, Vec::new())
        } else {
            let mut assignment = vec![None; data.binders.len()];
            if let Some(expected) = expected {
                let mut trial = assignment.clone();
                if unify_expected(&pattern, expected, &mut trial) {
                    assignment = trial;
                }
            }
            // The expected type fixes the type arguments, so a breach of a
            // bound is reported where that type is written.
            let source = self.written_span(span).unwrap_or(span);
            let mut resolved = vec![false; data.binders.len()];
            let sources = vec![Some(source); data.binders.len()];
            if !self.resolve_bounds(
                &data.bounds,
                data.module,
                &mut assignment,
                &mut resolved,
                &sources,
                span,
                true,
            ) {
                return Out::error();
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
    /// `assert.equal` call whose type contains a callable, a plan, or a
    /// grant.
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
            && !effects::grants::check_equatable(self.program, operand)
        {
            return false;
        }
        for arg in &state.args {
            self.report(
                DiagnosticCode::TypeFunctionNotEquatable,
                expr_span(&arg.value),
                format!(
                    "an operand of `assert.equal` has the type `{operand}`, which contains a callable, a plan, or a grant"
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
        let mut positional = None;
        let mut named = Vec::new();
        let mut flatten = |signature: &FnType| {
            positional = Some(signature.parameters.len());
            named = signature.labels.names().map(str::to_owned).collect();
            (Some(signature.slot_types()), Some(signature.result.clone()))
        };
        let mut preset = 0;
        let mut dispatch = None;
        let mut bounds = Vec::new();
        let mut bound_module = self.module;
        let (params, result) = match &callee {
            Callee::Function(index) => {
                match program.functions.get(*index).and_then(|data| {
                    data.signature.clone().map(|signature| (data, signature))
                }) {
                    Some((data, signature)) => {
                        binder_names.clone_from(&data.binders);
                        bounds.clone_from(&data.bounds);
                        bound_module = data.module;
                        let (params, result) = flatten(&signature);
                        (
                            params,
                            result.map(|result| {
                                effects::procs::applied(program, data, result)
                            }),
                        )
                    }
                    None => (None, None),
                }
            }
            Callee::Value(_, signature, row) => {
                let (params, result) = flatten(signature);
                (
                    params,
                    result.map(|result| {
                        effects::procs::applied_value(result, row.as_ref())
                    }),
                )
            }
            Callee::Generic(generic) => {
                binder_names.clone_from(&generic.binder_names);
                bounds.clone_from(&generic.bounds);
                preset = generic.outer;
                let (params, result) = flatten(&generic.signature);
                (
                    params,
                    result.map(|result| {
                        effects::values::generic_applied(
                            &generic.closure.value_type,
                            result,
                        )
                    }),
                )
            }
            Callee::Case(ty, case) => {
                match program
                    .types
                    .get(*ty)
                    .and_then(|data| data.case(case).map(|case| (data, case)))
                {
                    Some((data, case)) => {
                        binder_names.clone_from(&data.params);
                        bounds.clone_from(&data.bounds);
                        bound_module = data.module;
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
                    bounds.clone_from(&data.bounds);
                    bound_module = data.module;
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
                    (params, self.constructed(*ty))
                }
                None => (None, None),
            },
            Callee::Contract(interface, member) => {
                match self.contract_signature(*interface, member, application) {
                    Some((names, signature, found)) => {
                        binder_names = names;
                        dispatch = Some(Box::new(found));
                        if let Some((found, module)) =
                            self.contract_bounds(*interface, member)
                        {
                            bounds = found;
                            bound_module = module;
                        }
                        flatten(&signature)
                    }
                    None => (None, None),
                }
            }
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
        let assignment = (0..binder_names.len())
            .map(|at| (at < preset).then_some(Type::Binder(at)))
            .collect();
        let mut state = Box::new(CallState {
            callee,
            args,
            next: 0,
            current: 0,
            open: false,
            marks: (0, 0, 0),
            retry: Vec::new(),
            retrying: false,
            params,
            positional,
            named,
            labels,
            binding: Vec::new(),
            result,
            assignment,
            binder_names,
            deferred: Vec::new(),
            order: Vec::new(),
            first_pass: false,
            dispatch,
            resolved: vec![false; bounds.len()],
            bounds,
            bound_module,
            sources: Vec::new(),
            undetermined: false,
            nodes: Vec::new(),
            application,
            callee_span,
            failed: false,
            never_reported: false,
        });
        state.sources = vec![None; state.assignment.len()];
        state.order = (0..state.args.len()).collect();
        self.bind_arguments(&mut state);
        self.check_arity(&mut state);
        if state.dispatch.is_some() {
            self.contract_start(&mut state, expected.as_ref());
        } else if !state.assignment.is_empty()
            && let (Some(result), Some(expected)) = (&state.result, &expected)
        {
            let mut trial = state.assignment.clone();
            if unify_expected(result, expected, &mut trial) {
                let source = self.written_span(application).unwrap_or(application);
                note_sources(&state.assignment, &trial, &mut state.sources, source);
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
                // Positional arguments bind in order, labelled ones by name
                // in any position.
                let mut next = 0;
                let mut used = vec![false; state.named.len()];
                let count = state.positional.unwrap_or(usize::MAX);
                for arg in &state.args {
                    let Some(label) = arg.label else {
                        binding.push((next < count).then_some(next));
                        next = next.saturating_add(1);
                        continue;
                    };
                    let at = state.named.iter().position(|name| name == label.text());
                    let message = match at {
                        Some(at) if used.get(at) == Some(&false) => {
                            if let Some(slot) = used.get_mut(at) {
                                *slot = true;
                            }
                            binding.push(Some(count.saturating_add(at)));
                            continue;
                        }
                        Some(_) => {
                            format!("the label `{}` is bound twice", label.text())
                        }
                        None if state.named.is_empty() => format!(
                            "the callee takes no labelled argument `{}`",
                            label.text()
                        ),
                        None => format!(
                            "the callee has no labelled parameter `{}`",
                            label.text()
                        ),
                    };
                    self.report(
                        DiagnosticCode::TypeArgumentMismatch,
                        span_of(arg),
                        message,
                    );
                    state.failed = true;
                    binding.push(None);
                }
            }
        }
        state.binding = binding;
    }

    fn check_arity(&mut self, state: &mut CallState<'a>) {
        if state.labels.is_some() {
            return;
        }
        // A labelled argument to a callee without labelled parameters is
        // reported where it is written, and still counts here, so that it is
        // not reported again as a wrong arity.
        let written = if state.named.is_empty() {
            state.args.len()
        } else {
            state.args.iter().filter(|arg| arg.label.is_none()).count()
        };
        let takes = state
            .positional
            .or_else(|| state.params.as_ref().map(Vec::len));
        if let Some(takes) = takes
            && takes != written
        {
            let message = format!(
                "the callee takes {takes} argument(s), but {written} are written"
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
            let at = state.current;
            let span = state
                .args
                .get(at)
                .map_or(state.application, |arg| expr_span(&arg.value));
            // An argument whose parameter type was open and that only lacked
            // a type argument is checked again once the other operands have
            // fixed what they can: inference is joint over every operand.
            let (diagnostics, references, instantiations) = state.marks;
            let ambiguous_only =
                self.diagnostics.get(diagnostics..).is_some_and(|added| {
                    !added.is_empty()
                        && added.iter().all(|diagnostic| {
                            diagnostic.code() == DiagnosticCode::TypeAmbiguousInference
                        })
                });
            if out.ty.is_none() && state.open && !state.retrying && ambiguous_only {
                self.diagnostics.truncate(diagnostics);
                self.references.truncate(references);
                self.instantiations.truncate(instantiations);
                state.retry.push(at);
                place(&mut state.nodes, at, None);
                return self.next_argument(state);
            }
            // Checked again, it still only lacks a type argument of the
            // callee: that parameter is undetermined, which is reported once,
            // at the application.
            if out.ty.is_none() && state.open && state.retrying && ambiguous_only {
                self.diagnostics.truncate(diagnostics);
                self.references.truncate(references);
                self.instantiations.truncate(instantiations);
                state.undetermined = true;
                place(&mut state.nodes, at, None);
                return self.next_argument(state);
            }
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
                    if state
                        .dispatch
                        .as_ref()
                        .is_some_and(|dispatch| dispatch.receiver_arg == Some(at))
                    {
                        self.receiver_arrived(&mut state, &actual, span);
                    }
                    if self.self_outside_slot(&state, at, span) {
                        state.failed = true;
                        place(&mut state.nodes, at, None);
                        return self.next_argument(state);
                    }
                    match state.slot(at).cloned() {
                        Some(pattern) => {
                            let mut trial = state.assignment.clone();
                            let fits = trial.is_empty()
                                || unify(&pattern, &actual, &mut trial);
                            if fits {
                                note_sources(
                                    &state.assignment,
                                    &trial,
                                    &mut state.sources,
                                    span,
                                );
                                state.assignment = trial;
                            } else {
                                let mut widened = state.assignment.clone();
                                if self.widening_inference(
                                    &pattern,
                                    &actual,
                                    &mut widened,
                                ) {
                                    note_sources(
                                        &state.assignment,
                                        &widened,
                                        &mut state.sources,
                                        span,
                                    );
                                    state.assignment = widened;
                                }
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
                                Some(expected)
                                    if !matches!(pattern, Type::Binder(_)) =>
                                {
                                    self.widen(
                                        &expected,
                                        &actual,
                                        out.node,
                                        span,
                                        DiagnosticCode::TypeArgumentMismatch,
                                    )
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
            place(&mut state.nodes, at, node);
        }
        self.next_argument(state)
    }

    /// Starts the next argument of a call, an argument set aside once all
    /// were checked, or finishes the call.
    fn next_argument(&mut self, mut state: Box<CallState<'a>>) -> Step<'a> {
        loop {
            let at = if let Some(at) = state.order.get(state.next).copied() {
                state.next += 1;
                at
            } else {
                if !state.first_pass {
                    state.first_pass = true;
                    self.first_pass_done(&mut state);
                    if state.failed {
                        for at in std::mem::take(&mut state.retry) {
                            place(&mut state.nodes, at, None);
                        }
                    }
                }
                if state.retry.is_empty() {
                    break;
                }
                state.retrying = true;
                state.retry.remove(0)
            };
            let Some(arg) = state.args.get(at) else { break };
            let value = arg.value.clone();
            let expected = state.slot(at).and_then(|pattern| state.instance(pattern));
            let skip_rest = state.dispatch.as_ref().is_some_and(|dispatch| {
                dispatch.receiver_arg.is_some_and(|receiver| receiver != at)
                    && !matches!(
                        state.assignment.get(dispatch.self_var()),
                        Some(Some(_))
                    )
            });
            if skip_rest {
                state.failed = true;
                place(&mut state.nodes, at, None);
                continue;
            }
            state.current = at;
            state.open = expected.is_none() && state.slot(at).is_some();
            state.marks = (
                self.diagnostics.len(),
                self.references.len(),
                self.instantiations.len(),
            );
            self.argument = Some(expr_span(&value));
            return Step::Child(
                Task::Call(state),
                Task::Expr(value, Ctx::Value(expected)),
            );
        }
        if !state.first_pass {
            state.first_pass = true;
            self.first_pass_done(&mut state);
        }
        Step::Done(self.finish_call(*state))
    }

    /// What follows the check of every argument once: a contract call's
    /// selection and the resolution of the callee's bounds, both before any
    /// argument set aside is checked again.
    fn first_pass_done(&mut self, state: &mut CallState<'a>) {
        self.select(state);
        self.call_bounds(state, false);
    }

    /// Resolves the bounds of the callee of `state`; `last` once no
    /// argument is checked again.
    fn call_bounds(&mut self, state: &mut CallState<'a>, last: bool) {
        if state.failed || state.bounds.iter().all(Option::is_none) {
            return;
        }
        let bounds = std::mem::take(&mut state.bounds);
        let mut resolved = std::mem::take(&mut state.resolved);
        let holds = self.resolve_bounds(
            &bounds,
            state.bound_module,
            &mut state.assignment,
            &mut resolved,
            &state.sources,
            state.application,
            last,
        );
        state.bounds = bounds;
        state.resolved = resolved;
        if !holds {
            state.failed = true;
        }
    }

    fn finish_call(&mut self, mut state: CallState<'a>) -> Out {
        let program = self.program;
        self.call_bounds(&mut state, true);
        let contract = if state.dispatch.is_some() && !state.failed {
            self.contract_call(&state)
        } else {
            None
        };
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
        } else if state.undetermined && !state.failed {
            self.report(
                DiagnosticCode::TypeAmbiguousInference,
                state.application,
                "an operand of this call has a type nothing determines",
            );
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
        let slot_types = (0..state.args.len())
            .map(|at| state.slot(at).and_then(|pattern| state.instance(pattern)))
            .collect::<Vec<_>>();
        let positional = state.positional.unwrap_or(usize::MAX);
        let named = state
            .named
            .iter()
            .enumerate()
            .map(|(at, name)| {
                let ty = state
                    .params
                    .as_ref()
                    .and_then(|params| params.get(positional.saturating_add(at)))
                    .and_then(|pattern| state.instance(pattern));
                (name.clone(), ty)
            })
            .collect::<Vec<_>>();
        let CallState {
            callee,
            nodes,
            binding,
            labels,
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
        let mut hoisted = Vec::new();
        let mut lowered = |this: &mut Self, written: Vec<Node>| {
            if !complete {
                return None;
            }
            let shape = Shape {
                binding: &binding,
                positional,
                named: &named,
                types: &slot_types,
                span: application,
            };
            this.lower_arguments(&shape, written, &mut hoisted)
        };
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
        let site = match &callee {
            Callee::Function(index) => Some(Ok(*index)),
            Callee::Construct(ty) => Some(Err(*ty)),
            _ => None,
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
                let function = self.layout.functions.get(index).copied().flatten();
                function
                    .zip(lowered(self, written))
                    .map(|(function, arguments)| IrExpr::Call {
                        target: CallTarget::Direct {
                            function,
                            type_arguments,
                        },
                        arguments,
                    })
            }
            Callee::Value(node, callee_type, row) => {
                let arguments = lowered(self, written);
                node.zip(arguments).map(|(node, arguments)| {
                    // Arguments hoisted to keep their written order run
                    // before the call, so the callee is bound first.
                    let node = if hoisted.is_empty() {
                        node
                    } else {
                        let value_type = effects::procs::value_type(
                            callee_type.as_ref(),
                            row.as_ref(),
                        );
                        let fresh = self.new_slot(Some(&value_type));
                        let origin = node.origin.clone();
                        hoisted.insert(0, (fresh, *node));
                        Box::new(Node::new(
                            IrExpr::Variable {
                                slot: fresh,
                                value_type,
                            },
                            origin,
                        ))
                    };
                    IrExpr::Call {
                        target: CallTarget::Value(node),
                        arguments,
                    }
                })
            }
            Callee::Generic(generic) => {
                let closure = if complete && complete_arguments {
                    self.instantiate_generic(&generic, &type_arguments, callee_span)
                } else {
                    None
                };
                closure
                    .zip(lowered(self, written))
                    .map(|(closure, arguments)| IrExpr::Call {
                        target: CallTarget::Value(Box::new(closure)),
                        arguments,
                    })
            }
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
                // The constructor of a `where … else` returns a `Result`; the
                // construction itself is of the type.
                let value_type = match program.types.get(ty) {
                    Some(data) if data.fallible => {
                        data.self_type().map(|own| own.instantiate(&type_arguments))
                    }
                    _ => result.clone(),
                };
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
                value_type
                    .zip(construction)
                    .map(|(value_type, construction)| IrExpr::Construct {
                        value_type,
                        construction,
                    })
            }
            Callee::Contract(..) => {
                contract
                    .zip(lowered(self, written))
                    .map(|(call, arguments)| IrExpr::Call {
                        target: CallTarget::Contract(Box::new(call)),
                        arguments,
                    })
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
        if let Some(node) = &node {
            self.obligation(site, node, application);
        }
        // Arguments hoisted to keep their written order bind around the
        // call, the first outermost.
        let node = node.map(|call| {
            hoisted.into_iter().rev().fold(call, |body, (slot, value)| {
                self.node(
                    IrExpr::Let {
                        slot: Some(slot),
                        value: Box::new(value),
                        body: Box::new(body),
                    },
                    application,
                )
            })
        });
        Out { ty: result, node }
    }

    /// The arguments of a call to a callable with labelled parameters, as
    /// `shape` describes it, from its written argument nodes `written`:
    /// the positional ones in order, the labelled ones in written order, and
    /// a placeholder for the default of each label not written. When a
    /// labelled argument is written before a positional one, each argument
    /// is bound to a fresh slot first, in `hoisted`, so that they are
    /// evaluated in written order.
    fn lower_arguments(
        &mut self,
        shape: &Shape<'_>,
        written: Vec<Node>,
        hoisted: &mut Vec<(usize, Node)>,
    ) -> Option<Arguments> {
        let mut labelled_seen = false;
        let mut reorder = false;
        for slot in shape.binding.iter().flatten() {
            if *slot >= shape.positional {
                labelled_seen = true;
            } else if labelled_seen {
                reorder = true;
            }
        }
        let mut positional = Vec::new();
        let mut labelled: Vec<(String, Node)> = Vec::new();
        for ((slot, node), ty) in shape.binding.iter().zip(written).zip(shape.types) {
            let slot = (*slot)?;
            let node = if reorder {
                let value_type = ty.clone()?;
                let fresh = self.new_slot(Some(&value_type));
                let origin = node.origin.clone();
                hoisted.push((fresh, node));
                Node::new(
                    IrExpr::Variable {
                        slot: fresh,
                        value_type,
                    },
                    origin,
                )
            } else {
                node
            };
            match slot.checked_sub(shape.positional) {
                None => positional.push(node),
                Some(at) => labelled.push((shape.named.get(at)?.0.clone(), node)),
            }
        }
        for (name, ty) in shape.named {
            if !labelled.iter().any(|(label, _)| label == name) {
                let placeholder = self.node(IrExpr::Default(ty.clone()?), shape.span);
                labelled.push((name.clone(), placeholder));
            }
        }
        Some(Arguments {
            positional,
            labelled,
        })
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
        let argument = matches!(product, Product::List | Product::Dict)
            && self.argument == Some(span);
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
            argument,
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
                            let code = if state.argument {
                                DiagnosticCode::TypeArgumentMismatch
                            } else {
                                DiagnosticCode::TypeMismatch
                            };
                            let node =
                                self.conform(&expected, &actual, out.node, span, code);
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
            // An element of a list or dict literal argument is in argument
            // position too.
            if state.argument {
                self.argument = Some(expr_span(&operand));
            }
            return Step::Child(
                Task::Product(state),
                Task::Expr(operand, Ctx::Value(expected)),
            );
        }
        if state.argument {
            self.argument = Some(state.span);
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
                // A repeated field is reported where it is written.
                Type::record(names.iter().cloned().zip(components)).ok()
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
                    let key_fault = match self.program.key_fault(&key) {
                        Some(DiagnosticCode::TypeInvalidDictKey)
                            if key.is_open() && !self.open_key_fault(&key) =>
                        {
                            None
                        }
                        fault if inferred => fault,
                        _ => None,
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
                    if let Some((name, callable, callable_span)) =
                        self.generic_let(&view)
                    {
                        state.pending = Pending::GenericLet { name };
                        return Step::Child(
                            Task::Body(state),
                            Task::Anon(
                                callable,
                                callable_span,
                                Ctx::Value(None),
                                ClosureMode::Generic,
                            ),
                        );
                    }
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
                    if let (Some(_), Some(written_type)) =
                        (&written, view.written_type())
                    {
                        self.destinations
                            .push((expr_span(&value), written_type.span()));
                    }
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

    /// The name, the callable, and its span, of a `let` that binds a generic
    /// anonymous callable to a bare name, which keeps it generic: one with
    /// no written type and no `else`, whose initializer is the callable
    /// alone.
    fn generic_let(
        &self,
        view: &LetView<'a>,
    ) -> Option<(String, Callable<'a>, ByteSpan)> {
        if view.written_type().is_some() || view.fallback().is_some() {
            return None;
        }
        let PatternForm::Path(path) = view.pattern()?.form() else {
            return None;
        };
        let segments = path.segments();
        let [name] = segments.as_slice() else {
            return None;
        };
        // A name of a module value is a constant pattern, not a binding.
        let binds = self.local_index(name.text()).is_some()
            || !matches!(
                self.program.lookup(self.module, name.text()),
                Some(Entity::Value(_) | Entity::Fn(_))
            );
        let value = view.value()?;
        let ExprForm::Postfix(postfix) = value.form() else {
            return None;
        };
        if !binds || !suffix_spans(postfix, self.text).is_empty() {
            return None;
        }
        let Primary::Callable(callable) = postfix.primary()? else {
            return None;
        };
        if callable.params().is_empty() {
            return None;
        }
        let span = primary_node(postfix).map_or(postfix.node().span(), CstNode::span);
        Some((name.text().to_owned(), callable, span))
    }

    /// Takes the value of the statement or fallback that `state` waits for,
    /// returning the fallback of a `let … else` to check next.
    fn arrive(&mut self, state: &mut BodyState<'a>, out: Out) -> Option<Task<'a>> {
        match std::mem::replace(&mut state.pending, Pending::Nothing) {
            Pending::Nothing => {}
            Pending::GenericLet { name } => {
                state.last_type = None;
                let generic = self.generic.take().map(Rc::new);
                if generic.is_none() {
                    state.failed = true;
                }
                // The name has no slot: each use is a closure of its own.
                self.locals.push(Local {
                    name,
                    slot: 0,
                    ty: None,
                    frame: self.frames.len(),
                    generic: generic.clone(),
                });
                state.pieces.push(generic.map(|_| Piece::Skip));
            }
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
                            let node = self.widen(
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
                if irrefutable == Some(false)
                    && !self.construction_pattern(
                        typed.lowered.as_ref(),
                        pattern_span,
                        node.as_ref(),
                    )
                {
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
                } else if !used
                    && let Some(actual) = &ty
                    && !matches!(actual, Type::Void | Type::Never)
                {
                    // An expression statement has type `Void` or `Never`
                    // ("Bodies and statements"), a plan whose `await` was
                    // forgotten included. Only the last statement of a
                    // branch gives a value: one whose value is used conforms
                    // to it, and one whose value is not is reported below.
                    self.report(
                        DiagnosticCode::TypeMismatch,
                        span,
                        format!(
                            "a statement has type `Void` or `Never`, found `{actual}`; \
                             bind a value to drop with `let _`"
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
                    // A fallback whose one statement has a value was reported
                    // at that statement, which is the whole fallback.
                    let reported = self.diagnostics.iter().any(|diagnostic| {
                        diagnostic.code() == DiagnosticCode::TypeMismatch
                            && diagnostic.primary_span() == state.span
                    });
                    if !reported {
                        self.report(
                            DiagnosticCode::TypeMismatch,
                            state.span,
                            "expected `Never`: the `else` of a `let … else` can run off \
                             its end, but every path of it must leave",
                        );
                    }
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
            match piece {
                Piece::Expr(node) => {
                    tail.push(node);
                    continue;
                }
                Piece::Skip => continue,
                _ => {}
            }
            let body = Box::new(self.close(std::mem::take(&mut tail), span));
            let (expr, let_span) = match piece {
                Piece::Expr(_) | Piece::Skip => continue,
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

/// How the written arguments of a call bind a callable with labelled
/// parameters: the parameter each binds, the number of positional ones, each
/// label and its type, the type each argument has, and the call's span.
struct Shape<'s> {
    binding: &'s [Option<usize>],
    positional: usize,
    named: &'s [(String, Option<Type>)],
    types: &'s [Option<Type>],
    span: ByteSpan,
}

/// Puts `node` at position `at` of `nodes`, filling any gap before it.
fn place(nodes: &mut Vec<Option<Node>>, at: usize, node: Option<Node>) {
    while nodes.len() < at {
        nodes.push(None);
    }
    match nodes.get_mut(at) {
        Some(slot) => *slot = node,
        None => nodes.push(node),
    }
}

/// Whether `node` is a call, after the `let`s that hoist its arguments.
fn ends_in_call(node: &Node) -> bool {
    let mut node = node;
    loop {
        match &node.expr {
            IrExpr::Call { .. } | IrExpr::TailCall { .. } => return true,
            IrExpr::Let { body, .. } => node = body,
            _ => return false,
        }
    }
}

/// Matches `pattern`, in which a [`Type::Binder`] below `assignment.len()`
/// is a type parameter of a callee, against the type `actual` of an
/// operand, recording what each parameter it meets must be. `Never` is
/// never inferred: an operand of type `Never` fixes nothing. Whether every
/// part matched.
fn unify(pattern: &Type, actual: &Type, assignment: &mut [Option<Type>]) -> bool {
    unify_at(pattern, actual, assignment, false)
}

/// [`unify`] against an expected type, which a written type supplies: a
/// `Never` it fixes a parameter to binds that parameter.
fn unify_expected(
    pattern: &Type,
    expected: &Type,
    assignment: &mut [Option<Type>],
) -> bool {
    unify_at(pattern, expected, assignment, true)
}

fn unify_at(
    pattern: &Type,
    actual: &Type,
    assignment: &mut [Option<Type>],
    binds_never: bool,
) -> bool {
    let mut pending = vec![(pattern, actual)];
    while let Some((pattern, actual)) = pending.pop() {
        match (pattern, actual) {
            (Type::Binder(at), _) if *at < assignment.len() => {
                if *actual == Type::Never && !binds_never {
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
            (
                Type::Interface(left, patterns, choices),
                Type::Interface(right, actuals, chosen),
            ) if left == right
                && patterns.len() == actuals.len()
                && choices.names().eq(chosen.names()) =>
            {
                pending.extend(patterns.iter().zip(actuals));
                pending.extend(
                    choices
                        .entries()
                        .iter()
                        .zip(chosen.entries())
                        .map(|((_, pattern), (_, actual))| (pattern, actual)),
                );
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
            // A row names no type parameter, and rows compare exactly.
            (Type::Proc(pattern), Type::Proc(actual))
                if pattern.parameters.len() == actual.parameters.len()
                    && pattern.labels.names().eq(actual.labels.names())
                    && pattern.effects == actual.effects =>
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
            (Type::Intent(pattern), Type::Intent(actual))
                if pattern.effects == actual.effects =>
            {
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

/// Records `span` as the source of each type parameter that `after` fixes
/// and `before` did not.
fn note_sources(
    before: &[Option<Type>],
    after: &[Option<Type>],
    sources: &mut [Option<ByteSpan>],
    span: ByteSpan,
) {
    for ((before, after), source) in before.iter().zip(after).zip(sources.iter_mut()) {
        if before.is_none() && after.is_some() && source.is_none() {
            *source = Some(span);
        }
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
