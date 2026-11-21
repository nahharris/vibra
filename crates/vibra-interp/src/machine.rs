//! The machine: an explicit loop over evaluation steps and a stack of
//! continuations, with every activation's frame on the heap
//! (`docs/spec/06-runtime.md`, "Evaluation", "Tail calls", "Activations and
//! memory").
//!
//! Nothing here recurses on the host stack in proportion to the depth of
//! Vibra calls or of Vibra values: an operand is evaluated by pushing a
//! continuation and returning to the loop.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use vibra_diagnostics::DiagnosticCode;
use vibra_ir::external::CompilerIntrinsic;
use vibra_ir::{
    Arguments, CallTarget, CheckedFunction, CheckedModuleSet, ClosureBody, Constant,
    Construction, Expr, FunctionBody, Guard, Library, MatchArm, NamedTypes, Node,
    Pattern, SourceOrigin, Type, TypeBody, TypeId, Value,
};

use crate::compare::{DictBuild, DictFind, Structural};
use crate::encode::{ObserveError, encoding, from_neutral};
use crate::frame::{Frame, TypeEnv};
use crate::ingress::{Check, Ingress, Then, checks_of, needs_checks, refused};
use crate::plan::Invocation;
use crate::types::{mentions_self, type_of};
use crate::value::{ClosureVal, FunctionVal, Held, Repr, Val, neutral_storage, same};
use crate::{FailureKind, Instrumentation, Reads, TestAssertionFailure, intrinsics};

/// Why the machine stopped before its work was done.
#[derive(Debug)]
pub(crate) enum Halt {
    /// The checked program is not one the checker accepts:
    /// `@runtime.invalidCheckedProgram`.
    Invalid(String),
    /// A value the host handed in is not a value of its type:
    /// `@runtime.invalidHostValue`.
    HostValue(String),
    /// The memory budget is exhausted.
    Memory,
    /// The reduction limit of an evaluation is reached.
    Reductions,
    /// A form of a later milestone, such as a host-bound member reached
    /// with no host context.
    Unavailable(String),
    /// An assertion did not hold, which ends a test body.
    Failed(Box<TestAssertionFailure>),
    /// The encoder reached a value that has no value encoding: a plan.
    NoEncoding,
}

pub(crate) fn invalid<T>(message: impl Into<String>) -> Result<T, Halt> {
    Err(Halt::Invalid(message.into()))
}

/// What the machine is running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    /// A program's entry.
    Run,
    /// A test body, where the assertion rows are defined.
    Test,
    /// One expression.
    Evaluate,
}

/// The state of a module value: lazy, and evaluated at most once.
#[derive(Debug)]
enum GlobalState<'a> {
    Unread,
    Evaluating,
    Ready(Val<'a>),
}

/// One move of the machine.
#[derive(Debug)]
pub(crate) enum Step<'a> {
    /// Evaluate a node.
    Eval(&'a Node),
    /// Evaluate an expression written at an origin: the root of an
    /// evaluation, which is not a node of the module set.
    Root(&'a Expr, &'a SourceOrigin),
    /// Hand a value to the top continuation.
    Ret(Val<'a>),
}

/// The operands a node evaluates, in written order, before it acts.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Work<'a> {
    Intrinsic {
        intrinsic: CompilerIntrinsic,
        type_arguments: &'a [Type],
        nodes: &'a [Node],
        origin: &'a SourceOrigin,
    },
    Construct {
        value_type: &'a Type,
        construction: &'a Construction,
    },
    Closure {
        body: &'a ClosureBody,
        nodes: &'a [Node],
    },
    Lookup {
        collection: &'a Node,
        key: &'a Node,
    },
    Call {
        target: &'a CallTarget,
        arguments: &'a Arguments,
        tail: bool,
        origin: &'a SourceOrigin,
    },
    /// `await plan`: the plan is its one operand.
    Await {
        plan: &'a Node,
        origin: &'a SourceOrigin,
    },
}

impl<'a> Work<'a> {
    /// The operand at `index`: for a call, the callee value first, then the
    /// positional arguments, then the labelled ones as written.
    fn node(self, index: usize) -> Option<&'a Node> {
        match self {
            Self::Intrinsic { nodes, .. } | Self::Closure { nodes, .. } => {
                nodes.get(index)
            }
            Self::Construct { construction, .. } => match construction {
                Construction::Tuple(nodes) | Construction::List(nodes) => {
                    nodes.get(index)
                }
                Construction::Record(fields) | Construction::Case { fields, .. } => {
                    fields.get(index).map(|(_, node)| node)
                }
                Construction::Wrapper(node) => (index == 0).then_some(node.as_ref()),
                Construction::Dict(entries) => {
                    entries.get(index / 2).map(|(key, value)| {
                        if index.is_multiple_of(2) { key } else { value }
                    })
                }
            },
            Self::Lookup { collection, key } => match index {
                0 => Some(collection),
                1 => Some(key),
                _ => None,
            },
            Self::Await { plan, .. } => (index == 0).then_some(plan),
            Self::Call {
                target, arguments, ..
            } => {
                let index = match target {
                    CallTarget::Value(callee) => match index.checked_sub(1) {
                        None => return Some(callee),
                        Some(index) => index,
                    },
                    _ => index,
                };
                arguments.positional.get(index).or_else(|| {
                    let labelled = index.checked_sub(arguments.positional.len())?;
                    arguments.labelled.get(labelled).map(|(_, node)| node)
                })
            }
        }
    }
}

/// The work left to do when a value arrives.
#[derive(Debug)]
pub(crate) enum Kont<'a> {
    /// The machine's work is done.
    End,
    /// The end of an activation: a pending `ensures … else` runs, then the
    /// frame is released.
    Leave,
    /// A module value's initializer finished.
    Global(usize),
    Sequence(&'a [Node]),
    Let {
        slot: Option<usize>,
        body: &'a Node,
    },
    LetElse {
        pattern: &'a Pattern,
        fallback: &'a Node,
        body: &'a Node,
    },
    Match(&'a [MatchArm]),
    If {
        then_branch: &'a Node,
        else_branch: Option<&'a Node>,
    },
    Field(&'a str),
    Return,
    Try,
    Widen(Type),
    Operands {
        work: Work<'a>,
        done: Vec<Val<'a>>,
    },
    Requires {
        function: &'a CheckedFunction,
        guard: &'a Guard,
    },
    ClosureRequires {
        body: &'a ClosureBody,
        guard: &'a Guard,
    },
    Ensures {
        candidate: Val<'a>,
        guard: &'a Guard,
    },
    Invariant {
        value: Val<'a>,
        consequence: &'a Constant,
    },
    Fold {
        function: Val<'a>,
        items: Rc<Vec<Val<'a>>>,
        next: usize,
    },
    DictBuild(Box<DictBuild<'a>>),
    DictFind(Box<DictFind<'a>>),
    Structural(Box<Structural<'a>>),
    /// The checks of a value coming in, the answer of the current one
    /// awaited.
    Ingress(Box<Ingress<'a>>),
    /// Widens each `Self` position of a result dispatched through an
    /// interface value back to that interface value.
    Rebox {
        result: Type,
        held_type: Type,
    },
    /// A restoration boundary: the invocation to restore when the awaited
    /// one returns. A non-tail `await` pushes one; a tail `await` none.
    Await {
        caller: Invocation,
    },
}

impl<'a> Kont<'a> {
    fn for_each_value(&self, visit: &mut impl FnMut(&Val<'a>)) {
        match self {
            Self::Operands { done, .. } => done.iter().for_each(visit),
            Self::Ensures { candidate, .. } => visit(candidate),
            Self::Invariant { value, .. } => visit(value),
            Self::Fold {
                function, items, ..
            } => {
                visit(function);
                items.iter().for_each(visit);
            }
            Self::DictBuild(build) => build.for_each_value(visit),
            Self::DictFind(find) => find.for_each_value(visit),
            Self::Structural(structural) => structural.for_each_value(visit),
            Self::Ingress(ingress) => ingress.for_each_value(visit),
            _ => {}
        }
    }
}

/// A callable resolved to what runs.
#[derive(Clone, Debug)]
pub(crate) enum Target<'a> {
    Function(usize),
    Closure(Rc<ClosureVal<'a>>),
    /// The structural `Ordered.compare` (`order`) or `Equatable.equal` of
    /// an atom, an atom singleton, or an anonymous tuple or record type.
    Structural {
        order: bool,
        ty: Type,
    },
}

/// A callable and the closed substitution it runs under.
#[derive(Clone, Debug)]
pub(crate) struct Resolved<'a> {
    pub(crate) target: Target<'a>,
    pub(crate) env: Rc<TypeEnv>,
}

type DispatchKey = (TypeId, String, Type, Vec<Type>, Vec<Type>);

/// The machine and everything one execution owns.
#[derive(Debug)]
pub(crate) struct Machine<'a> {
    pub(crate) set: &'a CheckedModuleSet,
    mode: Mode,
    pub(crate) frames: Vec<Frame<'a>>,
    pub(crate) konts: Vec<Kont<'a>>,
    globals: Vec<GlobalState<'a>>,
    dispatch: HashMap<DispatchKey, Resolved<'a>>,
    pub(crate) empty_env: Rc<TypeEnv>,
    origin: Option<&'a SourceOrigin>,
    reduction_limit: u64,
    budget: u64,
    frame_bytes: u64,
    live_bytes: u64,
    allocated_bytes: u64,
    /// The accounted footprint above which the live bytes are measured
    /// again: twice the last measurement, and at least
    /// [`MEASURE_FLOOR`] above it.
    next_measure: u64,
    /// The constants already admitted, by address: each is checked once.
    admitted: HashSet<usize>,
    pub(crate) stats: Instrumentation,
    /// What an evaluation read: the functions it entered and the module
    /// values it read. Kept only in [`Mode::Evaluate`].
    pub(crate) reads: Reads,
    /// The invocation running now. It lives here, not in a frame: a tail
    /// `await` replaces it, and a `Kont::Await` restores it.
    pub(crate) invocation: Invocation,
    /// The function whose every entry records the current invocation in
    /// `probed`, when one is set.
    pub(crate) probe: Option<usize>,
    /// The invocation current at each entry of `probe`, in order.
    pub(crate) probed: Vec<Invocation>,
}

/// The least growth of the accounted footprint, above the last
/// measurement, that measures the live bytes again.
const MEASURE_FLOOR: u64 = 4 * 1024;

const FRAME_BYTES: u64 = size_of::<Frame<'static>>() as u64;
const SLOT_BYTES: u64 = size_of::<Option<Val<'static>>>() as u64;
const KONT_BYTES: u64 = size_of::<Kont<'static>>() as u64;
const VAL_BYTES: u64 = size_of::<Val<'static>>() as u64;

fn as_u64(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(u64::MAX)
}

fn frame_cost(frame: &Frame<'_>) -> u64 {
    FRAME_BYTES.saturating_add(as_u64(frame.slots.len()).saturating_mul(SLOT_BYTES))
}

impl<'a> Machine<'a> {
    pub(crate) fn new(
        set: &'a CheckedModuleSet,
        mode: Mode,
        budget: u64,
        reduction_limit: u64,
    ) -> Self {
        Self {
            set,
            mode,
            frames: Vec::new(),
            konts: vec![Kont::End],
            globals: set.globals().iter().map(|_| GlobalState::Unread).collect(),
            dispatch: HashMap::new(),
            empty_env: Rc::new(TypeEnv::default()),
            origin: None,
            reduction_limit,
            budget,
            frame_bytes: 0,
            live_bytes: 0,
            allocated_bytes: 0,
            next_measure: 0,
            admitted: HashSet::new(),
            stats: Instrumentation {
                invocation_count: match mode {
                    Mode::Run | Mode::Test => Invocation::ROOT.ordinal,
                    Mode::Evaluate => 0,
                },
                ..Instrumentation::default()
            },
            reads: Reads::default(),
            invocation: Invocation::ROOT,
            probe: None,
            probed: Vec::new(),
        }
    }

    /// Where the node being evaluated was written.
    pub(crate) const fn origin(&self) -> Option<&'a SourceOrigin> {
        self.origin
    }

    /// Runs the machine from `step` until its work is done.
    pub(crate) fn execute(&mut self, mut step: Step<'a>) -> Result<Val<'a>, Halt> {
        loop {
            step = match step {
                Step::Eval(node) => {
                    self.stats.reduction_count =
                        self.stats.reduction_count.saturating_add(1);
                    if self.stats.reduction_count > self.reduction_limit {
                        return Err(Halt::Reductions);
                    }
                    self.origin = Some(&node.origin);
                    self.eval(&node.expr, &node.origin)?
                }
                Step::Root(expr, origin) => {
                    self.stats.reduction_count =
                        self.stats.reduction_count.saturating_add(1);
                    if self.stats.reduction_count > self.reduction_limit {
                        return Err(Halt::Reductions);
                    }
                    self.origin = Some(origin);
                    self.eval(expr, origin)?
                }
                Step::Ret(value) => match self.konts.pop() {
                    None => return invalid("a value with no continuation"),
                    Some(Kont::End) => return Ok(value),
                    Some(kont) => self.resume(kont, value)?,
                },
            };
            self.stats.max_continuation_depth =
                self.stats.max_continuation_depth.max(self.konts.len());
        }
    }

    pub(crate) fn frame(&self) -> Result<&Frame<'a>, Halt> {
        self.frames
            .last()
            .ok_or_else(|| Halt::Invalid("no activation".to_owned()))
    }

    fn frame_mut(&mut self) -> Result<&mut Frame<'a>, Halt> {
        self.frames
            .last_mut()
            .ok_or_else(|| Halt::Invalid("no activation".to_owned()))
    }

    fn env(&self) -> Result<Rc<TypeEnv>, Halt> {
        Ok(Rc::clone(&self.frame()?.env))
    }

    pub(crate) fn function(&self, index: usize) -> Result<&'a CheckedFunction, Halt> {
        let set = self.set;
        set.functions()
            .get(index)
            .ok_or_else(|| Halt::Invalid(format!("no function {index}")))
    }

    /// A constant as a runtime value.
    fn constant(&mut self, constant: &'a Constant) -> Result<Val<'a>, Halt> {
        self.ingress(constant.value(), constant.value_type(), false)
    }

    /// A neutral value as a runtime value of `ty`: its storage is reserved
    /// against the budget before any of it is built. A value that does not
    /// fit `ty` halts as an invalid host value when `host` holds.
    pub(crate) fn ingress(
        &mut self,
        value: &'a Value,
        ty: &Type,
        host: bool,
    ) -> Result<Val<'a>, Halt> {
        let cost = as_u64(neutral_storage(value));
        self.reserve(cost, &[])?;
        let converted =
            from_neutral(self.set, value, ty).map_err(|halt| match halt {
                Halt::Invalid(message) => refused(host, message),
                other => other,
            })?;
        self.allocated_bytes = self.allocated_bytes.saturating_add(cost);
        Ok(converted)
    }

    /// The checks `constant` must pass the first time the run meets it.
    fn constant_checks(
        &mut self,
        constant: &'a Constant,
        checks: &mut Vec<Check<'a>>,
    ) -> Result<(), Halt> {
        if !self.admitted.insert(std::ptr::from_ref(constant).addr())
            || !needs_checks(self.set, constant.value_type())
        {
            return Ok(());
        }
        let value = self.constant(constant)?;
        checks.extend(checks_of(self.set, &value, constant.value_type(), false)?);
        Ok(())
    }

    /// A literal: its constant, admitted the first time.
    fn literal(&mut self, constant: &'a Constant) -> Result<Step<'a>, Halt> {
        let value = self.constant(constant)?;
        if !self.admitted.insert(std::ptr::from_ref(constant).addr())
            || !needs_checks(self.set, constant.value_type())
        {
            return Ok(Step::Ret(value));
        }
        let checks = checks_of(self.set, &value, constant.value_type(), false)?;
        self.admit(checks, false, Then::Ret(value))
    }

    /// Runs `step` once the constants a call can reach without evaluating a
    /// node are admitted: the defaults of every function and the
    /// consequence of every invariant.
    pub(crate) fn prelude(&mut self, step: Step<'a>) -> Result<Step<'a>, Halt> {
        let set = self.set;
        let mut checks = Vec::new();
        for function in set.functions() {
            for (_, default) in &function.defaults {
                self.constant_checks(default, &mut checks)?;
            }
        }
        for definition in set.types().types().values() {
            if let Some(consequence) = definition
                .invariant()
                .and_then(|invariant| invariant.consequence.as_ref())
            {
                self.constant_checks(consequence, &mut checks)?;
            }
        }
        self.admit(checks, false, Then::Continue(step))
    }

    // ----- memory -----

    /// Accounts a newly built value, which no root holds yet, against the
    /// budget.
    pub(crate) fn allocated(&mut self, value: &Val<'a>) -> Result<(), Halt> {
        if let Some((_, bytes)) = value.storage() {
            self.allocated_bytes = self.allocated_bytes.saturating_add(as_u64(bytes));
            self.reserve(0, std::slice::from_ref(value))?;
        }
        Ok(())
    }

    /// Checks that `pending` more bytes fit the budget beside what is
    /// accounted. When they do not at first, or when the accounted
    /// footprint has passed the point of the next measurement, the live
    /// bytes are measured again from the roots and from `in_flight`, the
    /// values built that no root holds yet. Each measurement is at least
    /// twice the last one apart, so measuring costs a constant per byte
    /// allocated.
    fn reserve(&mut self, pending: u64, in_flight: &[Val<'a>]) -> Result<(), Halt> {
        let accounted = self.accounted().saturating_add(pending);
        if accounted <= self.budget && accounted <= self.next_measure {
            self.stats.peak_memory_bytes = self.stats.peak_memory_bytes.max(accounted);
            return Ok(());
        }
        self.live_bytes = self.measure(in_flight);
        self.allocated_bytes = 0;
        let live = self.accounted().saturating_add(pending);
        self.stats.peak_memory_bytes = self.stats.peak_memory_bytes.max(live);
        self.stats.peak_live_bytes = self.stats.peak_live_bytes.max(live);
        self.next_measure = live
            .saturating_mul(2)
            .max(live.saturating_add(MEASURE_FLOOR));
        if live > self.budget {
            return Err(Halt::Memory);
        }
        Ok(())
    }

    /// A frame of `slot_count` slots, its bytes reserved before it is built;
    /// `in_flight` are the values that will fill it.
    pub(crate) fn new_frame(
        &mut self,
        slot_count: usize,
        env: Rc<TypeEnv>,
        in_flight: &[Val<'a>],
    ) -> Result<Frame<'a>, Halt> {
        let cost = FRAME_BYTES
            .saturating_add(as_u64(slot_count).saturating_mul(SLOT_BYTES))
            .saturating_add(KONT_BYTES);
        self.reserve(cost, in_flight)?;
        Frame::try_new(slot_count, env).ok_or(Halt::Memory)
    }

    /// Sets `slot` of the current activation. An evaluation's root
    /// activation grows to hold it, its new slots reserved first.
    pub(crate) fn bind_slot(
        &mut self,
        slot: usize,
        value: Val<'a>,
    ) -> Result<(), Halt> {
        let count = self.frame()?.slots.len();
        if slot >= count {
            let wanted = slot.checked_add(1).ok_or(Halt::Memory)?;
            let extra = wanted.saturating_sub(count);
            let bytes = as_u64(extra).saturating_mul(SLOT_BYTES);
            self.reserve(bytes, std::slice::from_ref(&value))?;
            let frame = self.frame_mut()?;
            frame
                .slots
                .try_reserve_exact(extra)
                .map_err(|_| Halt::Memory)?;
            frame.slots.resize(wanted, None);
            self.frame_bytes = self.frame_bytes.saturating_add(bytes);
        }
        self.frame_mut()?.set(slot, value)
    }

    fn accounted(&self) -> u64 {
        self.frame_bytes
            .saturating_add(as_u64(self.konts.len()).saturating_mul(KONT_BYTES))
            .saturating_add(self.live_bytes)
            .saturating_add(self.allocated_bytes)
    }

    fn check_memory(&mut self) -> Result<(), Halt> {
        self.reserve(0, &[])
    }

    /// The bytes of every value reachable from the machine's roots and from
    /// `extra`, each shared part counted once.
    pub(crate) fn measure(&self, extra: &[Val<'a>]) -> u64 {
        let mut seen = HashSet::new();
        let mut pending: Vec<Val<'a>> = Vec::new();
        let mut total: u64 = 0;
        let mut root = |value: &Val<'a>, inline: bool, total: &mut u64| {
            if inline {
                *total = total.saturating_add(VAL_BYTES);
            }
            pending.push(value.clone());
        };
        for frame in &self.frames {
            for value in frame.slots.iter().flatten() {
                root(value, false, &mut total);
            }
            if let Some(closure) = &frame.closure {
                root(&Val(Repr::Closure(Rc::clone(closure))), false, &mut total);
            }
        }
        for kont in &self.konts {
            kont.for_each_value(&mut |value| root(value, true, &mut total));
        }
        for global in &self.globals {
            if let GlobalState::Ready(value) = global {
                root(value, true, &mut total);
            }
        }
        for value in extra {
            root(value, true, &mut total);
        }
        while let Some(value) = pending.pop() {
            let Some((address, bytes)) = value.storage() else {
                continue;
            };
            if !seen.insert(address) {
                continue;
            }
            total = total.saturating_add(as_u64(bytes));
            value.for_each_child(|child| pending.push(child.clone()));
        }
        total
    }

    fn push_frame(&mut self, frame: Frame<'a>, tail: bool) -> Result<(), Halt> {
        if tail {
            if let Some(old) = self.frames.pop() {
                self.frame_bytes = self.frame_bytes.saturating_sub(frame_cost(&old));
            }
            self.stats.tail_transfer_count =
                self.stats.tail_transfer_count.saturating_add(1);
        } else {
            self.konts.push(Kont::Leave);
        }
        self.frame_bytes = self.frame_bytes.saturating_add(frame_cost(&frame));
        self.frames.push(frame);
        self.stats.max_activation_depth =
            self.stats.max_activation_depth.max(self.frames.len());
        self.check_memory()
    }

    fn pop_frame(&mut self) -> Result<(), Halt> {
        let frame = self
            .frames
            .pop()
            .ok_or_else(|| Halt::Invalid("no activation to leave".to_owned()))?;
        self.frame_bytes = self.frame_bytes.saturating_sub(frame_cost(&frame));
        Ok(())
    }

    /// Starts an activation for an evaluation's root expression.
    pub(crate) fn enter_root(&mut self, frame: Frame<'a>) -> Result<(), Halt> {
        self.frame_bytes = self.frame_bytes.saturating_add(frame_cost(&frame));
        self.frames.push(frame);
        self.check_memory()
    }

    // ----- evaluation -----

    /// Reads `slot`; an evaluation's input is converted and admitted on its
    /// first read, at the type the read names.
    fn read_slot(&mut self, slot: usize, value_type: &Type) -> Result<Step<'a>, Halt> {
        let frame = self.frame()?;
        if let Some(Some(value)) = frame.slots.get(slot) {
            return Ok(Step::Ret(value.clone()));
        }
        let inputs = frame.inputs;
        let input = inputs.get(slot).ok_or_else(|| {
            Halt::Invalid(format!("slot {slot} is read before it is set"))
        })?;
        let ty = frame.env.apply(value_type);
        let value = self.ingress(input, &ty, true)?;
        let checks = checks_of(self.set, &value, &ty, true)?;
        self.admit(checks, true, Then::Bind(slot, value))
    }

    #[allow(clippy::too_many_lines)]
    fn eval(
        &mut self,
        expr: &'a Expr,
        origin: &'a SourceOrigin,
    ) -> Result<Step<'a>, Halt> {
        Ok(match expr {
            Expr::Literal(constant) => self.literal(constant)?,
            Expr::Intrinsic {
                intrinsic,
                type_arguments,
                arguments,
            } => self.operands(
                Work::Intrinsic {
                    intrinsic: *intrinsic,
                    type_arguments,
                    nodes: arguments,
                    origin,
                },
                Vec::new(),
            )?,
            Expr::Sequence(statements) => self.sequence(statements)?,
            Expr::Variable { slot, value_type } => self.read_slot(*slot, value_type)?,
            Expr::Global(index) => self.global(*index)?,
            Expr::Function {
                function,
                type_arguments,
            } => {
                let type_arguments = self.frame()?.env.apply_all(type_arguments);
                let value = Val(Repr::Function(Rc::new(FunctionVal {
                    function: *function,
                    type_arguments,
                })));
                self.allocated(&value)?;
                Step::Ret(value)
            }
            Expr::Closure { closure, captures } => self.operands(
                Work::Closure {
                    body: closure,
                    nodes: captures,
                },
                Vec::new(),
            )?,
            Expr::Captured { slot, .. } => {
                let value = self
                    .frame()?
                    .closure
                    .as_ref()
                    .and_then(|closure| closure.captures.get(*slot))
                    .cloned()
                    .ok_or_else(|| Halt::Invalid(format!("no capture {slot}")))?;
                Step::Ret(value)
            }
            Expr::Let { slot, value, body } => {
                self.konts.push(Kont::Let { slot: *slot, body });
                Step::Eval(value)
            }
            Expr::LetElse {
                pattern,
                value,
                fallback,
                body,
            } => {
                self.konts.push(Kont::LetElse {
                    pattern,
                    fallback,
                    body,
                });
                Step::Eval(value)
            }
            Expr::Match { subject, arms, .. } => {
                self.konts.push(Kont::Match(arms));
                Step::Eval(subject)
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.konts.push(Kont::If {
                    then_branch,
                    else_branch: else_branch.as_deref(),
                });
                Step::Eval(condition)
            }
            Expr::Construct {
                value_type,
                construction,
            } => self.operands(
                Work::Construct {
                    value_type,
                    construction,
                },
                Vec::new(),
            )?,
            Expr::Field { record, field } => {
                self.konts.push(Kont::Field(field));
                Step::Eval(record)
            }
            Expr::Lookup { collection, key } => {
                self.operands(Work::Lookup { collection, key }, Vec::new())?
            }
            Expr::Call { target, arguments } | Expr::TailCall { target, arguments } => {
                self.operands(
                    Work::Call {
                        target,
                        arguments,
                        tail: matches!(expr, Expr::TailCall { .. }),
                        origin,
                    },
                    Vec::new(),
                )?
            }
            Expr::Return(value) => {
                self.konts.push(Kont::Return);
                Step::Eval(value)
            }
            Expr::Try(value) => {
                self.konts.push(Kont::Try);
                Step::Eval(value)
            }
            Expr::WidenToInterface { value, .. } => {
                let held_type = self.frame()?.env.apply(&type_of(self.set, value)?);
                self.konts.push(Kont::Widen(held_type));
                Step::Eval(value)
            }
            Expr::WidenAtom(value) => Step::Eval(value),
            Expr::Default(_) => {
                return invalid("a default outside a labelled argument");
            }
            Expr::Await { plan } => self.start_await(plan, origin)?,
        })
    }

    fn sequence(&mut self, statements: &'a [Node]) -> Result<Step<'a>, Halt> {
        match statements.split_first() {
            None => invalid("an empty sequence"),
            Some((last, [])) => Ok(Step::Eval(last)),
            Some((first, rest)) => {
                self.konts.push(Kont::Sequence(rest));
                Ok(Step::Eval(first))
            }
        }
    }

    fn global(&mut self, index: usize) -> Result<Step<'a>, Halt> {
        if matches!(self.mode, Mode::Evaluate) {
            self.reads.module_values.insert(index);
        }
        let set = self.set;
        let global = set
            .globals()
            .get(index)
            .ok_or_else(|| Halt::Invalid(format!("no module value {index}")))?;
        let state = self
            .globals
            .get_mut(index)
            .ok_or_else(|| Halt::Invalid(format!("no module value {index}")))?;
        match state {
            GlobalState::Ready(value) => Ok(Step::Ret(value.clone())),
            GlobalState::Evaluating => invalid(format!(
                "the module value `{}` is read while it is being evaluated",
                global.name
            )),
            GlobalState::Unread => {
                *state = GlobalState::Evaluating;
                self.stats.module_value_initializations =
                    self.stats.module_value_initializations.saturating_add(1);
                self.konts.push(Kont::Global(index));
                let env = Rc::clone(&self.empty_env);
                let frame = self.new_frame(global.slot_count, env, &[])?;
                self.frame_bytes = self.frame_bytes.saturating_add(frame_cost(&frame));
                self.frames.push(frame);
                self.check_memory()?;
                Ok(Step::Eval(&global.initializer))
            }
        }
    }

    /// Evaluates the operands of `work` from the first not yet `done`.
    pub(crate) fn operands(
        &mut self,
        work: Work<'a>,
        mut done: Vec<Val<'a>>,
    ) -> Result<Step<'a>, Halt> {
        while let Some(node) = work.node(done.len()) {
            if matches!(node.expr, Expr::Default(_)) {
                // Resolved from the callee when the call is made.
                done.push(Val::void());
                continue;
            }
            self.konts.push(Kont::Operands { work, done });
            return Ok(Step::Eval(node));
        }
        self.finish(work, done)
    }

    fn finish(&mut self, work: Work<'a>, done: Vec<Val<'a>>) -> Result<Step<'a>, Halt> {
        match work {
            Work::Intrinsic {
                intrinsic,
                type_arguments,
                origin,
                ..
            } => {
                let type_arguments = self.frame()?.env.apply_all(type_arguments);
                self.intrinsic(intrinsic, &type_arguments, done, origin)
            }
            Work::Closure { body, .. } => {
                let env = self.env()?;
                let value = Val(Repr::Closure(Rc::new(ClosureVal {
                    body,
                    captures: done,
                    env,
                })));
                self.allocated(&value)?;
                let mut checks = Vec::new();
                for (_, default) in &body.defaults {
                    self.constant_checks(default, &mut checks)?;
                }
                self.admit(checks, false, Then::Ret(value))
            }
            Work::Lookup { collection, .. } => {
                let mut done = done.into_iter();
                let (Some(collection_value), Some(key)) = (done.next(), done.next())
                else {
                    return invalid("a lookup without its operands");
                };
                if let Repr::Dict(dict) = &collection_value.0 {
                    let key_type = dict.key.clone();
                    let _ = collection;
                    return self.dict_find(Rc::clone(dict), key, &key_type);
                }
                let found =
                    intrinsics::lookup(&collection_value, &key).ok_or_else(|| {
                        Halt::Invalid("a lookup on a value that has none".to_owned())
                    })?;
                self.allocated(&found)?;
                Ok(Step::Ret(found))
            }
            Work::Construct {
                value_type,
                construction,
            } => self.construct(value_type, construction, done),
            Work::Call {
                target,
                arguments,
                tail,
                origin,
            } => self.call(target, arguments, tail, origin, done),
            Work::Await { origin, .. } => self.await_plan(single(done)?, origin),
        }
    }

    // ----- construction -----

    fn construct(
        &mut self,
        value_type: &'a Type,
        construction: &'a Construction,
        done: Vec<Val<'a>>,
    ) -> Result<Step<'a>, Halt> {
        let set = self.set;
        let value_type = self.frame()?.env.apply(value_type);
        let library = set.library();
        let role = match &value_type {
            Type::Declared(id, _) => library.library_of(id),
            _ => None,
        };
        // The operand an invariant's predicate takes: the representation.
        let mut representation = None;
        let value = match (construction, role) {
            (Construction::Dict(_), Some(Library::Dict)) => {
                let key = dict_key(&value_type)?;
                let mut entries = Vec::with_capacity(done.len() / 2);
                let mut done = done.into_iter();
                while let (Some(key), Some(value)) = (done.next(), done.next()) {
                    entries.push((key, value));
                }
                return self.dict_build(key, entries);
            }
            (Construction::Wrapper(_), Some(Library::Str)) => {
                intrinsics::apply(CompilerIntrinsic::StrFromChars, &[single(done)?])?
            }
            (Construction::Wrapper(_), Some(Library::Bytes)) => {
                intrinsics::apply(CompilerIntrinsic::BytesFromList, &[single(done)?])?
            }
            (Construction::Wrapper(_), _) => {
                let inner = single(done)?;
                representation = Some(inner.clone());
                Val(Repr::Wrapper(Rc::new(inner)))
            }
            (Construction::Tuple(_), _) => Val::tuple(done),
            (Construction::List(_), _) => Val::list(done),
            (Construction::Record(fields), _) => Val(Repr::Record(Rc::new(
                fields
                    .iter()
                    .map(|(name, _)| name.as_str())
                    .zip(done)
                    .collect(),
            ))),
            (Construction::Case { case, .. }, Some(Library::Bool)) => {
                Val::boolean(case == "true")
            }
            (Construction::Case { case, .. }, Some(Library::Option)) => {
                match case.as_str() {
                    "none" => Val::none(),
                    _ => Val::some(single(done)?),
                }
            }
            (Construction::Case { case, fields }, _) => Val::case(
                case,
                fields
                    .iter()
                    .map(|(name, _)| name.as_str())
                    .zip(done)
                    .collect(),
            ),
            (Construction::Dict(_), _) => {
                return invalid("a dict literal of a non-dict type");
            }
        };
        self.allocated(&value)?;
        if let Type::Declared(id, arguments) = &value_type
            && let Some(invariant) =
                set.types().definition(id).and_then(|d| d.invariant())
            && let Some(consequence) = &invariant.consequence
        {
            self.konts.push(Kont::Invariant {
                value: value.clone(),
                consequence,
            });
            let resolved = Resolved {
                target: Target::Function(invariant.predicate),
                env: Rc::new(TypeEnv::of(arguments.clone())),
            };
            let origin = self.origin;
            let operand = representation.unwrap_or_else(|| value.clone());
            return self.invoke(resolved, vec![operand], false, origin);
        }
        Ok(Step::Ret(value))
    }

    // ----- calls -----

    fn call(
        &mut self,
        target: &'a CallTarget,
        arguments: &'a Arguments,
        tail: bool,
        origin: &'a SourceOrigin,
        done: Vec<Val<'a>>,
    ) -> Result<Step<'a>, Halt> {
        let mut values = done.into_iter();
        let callee = match target {
            CallTarget::Value(_) => Some(values.next().ok_or_else(|| {
                Halt::Invalid("a call without its callee".to_owned())
            })?),
            _ => None,
        };
        let positional = arguments.positional.len();
        let mut args: Vec<Val<'a>> = values.by_ref().take(positional).collect();
        let written: Vec<Val<'a>> = values.collect();
        let mut rebox = None;
        let resolved = match (target, callee) {
            (
                CallTarget::Direct {
                    function,
                    type_arguments,
                },
                _,
            ) => {
                let type_arguments = self.frame()?.env.apply_all(type_arguments);
                Resolved {
                    target: Target::Function(*function),
                    env: self.env_of(type_arguments),
                }
            }
            (CallTarget::Value(_), Some(callee)) => self.resolve_value(&callee)?,
            (CallTarget::Contract(call), _) => {
                let env = self.env()?;
                let receiver = env.apply(&call.receiver);
                let interface_arguments = env.apply_all(&call.arguments);
                let member_arguments = env.apply_all(&call.member_arguments);
                if receiver.is_interface_value() {
                    let (held_type, result) = self.unbox(
                        &call.interface,
                        &call.member,
                        &mut args,
                        &interface_arguments,
                        &member_arguments,
                        &env.apply_all_named(&call.choices)?,
                    )?;
                    // Reboxing is work after the return, so only a result
                    // that holds `Self` keeps the call out of tail position.
                    if mentions_self(&result) {
                        rebox = Some((result, held_type.clone()));
                    }
                    self.dispatch(
                        &call.interface,
                        &call.member,
                        &held_type,
                        &interface_arguments,
                        &member_arguments,
                    )?
                } else {
                    self.dispatch(
                        &call.interface,
                        &call.member,
                        &receiver,
                        &interface_arguments,
                        &member_arguments,
                    )?
                }
            }
            (CallTarget::Value(_), None) => {
                return invalid("a call without its callee");
            }
        };
        if !written.is_empty() {
            let mut order: Vec<usize> = (0..written.len()).collect();
            order.sort_by(|left, right| {
                let name = |index: &usize| {
                    arguments
                        .labelled
                        .get(*index)
                        .map_or("", |(label, _)| label.as_str())
                };
                name(left).cmp(name(right))
            });
            let mut labelled: Vec<Option<Val<'a>>> =
                written.into_iter().map(Some).collect();
            for index in order {
                let (label, node) = arguments.labelled.get(index).ok_or_else(|| {
                    Halt::Invalid("a labelled argument out of range".to_owned())
                })?;
                let value = if matches!(node.expr, Expr::Default(_)) {
                    self.default_of(&resolved.target, label)?
                } else {
                    labelled
                        .get_mut(index)
                        .and_then(Option::take)
                        .ok_or_else(|| {
                            Halt::Invalid("a labelled argument used twice".to_owned())
                        })?
                };
                args.push(value);
            }
        }
        if self.builds_plan(&resolved.target)? {
            return self.build_plan(resolved, args, rebox);
        }
        let tail = tail && rebox.is_none() && self.prepare_tail();
        if let Some((result, held_type)) = rebox {
            self.konts.push(Kont::Rebox { result, held_type });
        }
        self.invoke(resolved, args, tail, Some(origin))
    }

    pub(crate) fn env_of(&self, binders: Vec<Type>) -> Rc<TypeEnv> {
        if binders.is_empty() {
            Rc::clone(&self.empty_env)
        } else {
            Rc::new(TypeEnv::of(binders))
        }
    }

    /// The callable a function value or a closure runs.
    pub(crate) fn resolve_value(&self, callee: &Val<'a>) -> Result<Resolved<'a>, Halt> {
        match &callee.0 {
            Repr::Function(function) => Ok(Resolved {
                target: Target::Function(function.function),
                env: self.env_of(function.type_arguments.clone()),
            }),
            Repr::Closure(closure) => Ok(Resolved {
                target: Target::Closure(Rc::clone(closure)),
                env: Rc::clone(&closure.env),
            }),
            _ => invalid("a call of a value that is not callable"),
        }
    }

    /// The value of the default of `label` that the resolved callee, a
    /// function or a closure value, carries.
    fn default_of(
        &mut self,
        target: &Target<'a>,
        label: &str,
    ) -> Result<Val<'a>, Halt> {
        let defaults = match target {
            Target::Function(index) => self.function(*index)?.defaults.as_slice(),
            Target::Closure(closure) => closure.body.defaults.as_slice(),
            _ => {
                return invalid(format!(
                    "a default for `{label}` of a callee without defaults"
                ));
            }
        };
        let constant = defaults
            .iter()
            .find_map(|(name, constant)| (name == label).then_some(constant))
            .ok_or_else(|| {
                Halt::Invalid(format!("the callee has no default for `{label}`"))
            })?;
        self.constant(constant)
    }

    /// Whether the call being made may replace the current activation: no
    /// continuation remains but the activation's end, and no `ensures …
    /// else` is pending. A `return` around the call leaves nothing either.
    pub(crate) fn prepare_tail(&mut self) -> bool {
        if matches!(self.konts.last(), Some(Kont::Return)) {
            let boundary = self.konts.iter().rposition(|kont| {
                matches!(kont, Kont::Leave | Kont::End | Kont::Global(_))
            });
            if let Some(at) = boundary
                && matches!(self.konts.get(at), Some(Kont::Leave))
            {
                self.konts.truncate(at.saturating_add(1));
            }
        }
        matches!(self.konts.last(), Some(Kont::Leave))
            && self.frames.last().is_some_and(|frame| frame.exit.is_none())
    }

    /// Runs a resolved callable on `args`, in its slot order.
    pub(crate) fn invoke(
        &mut self,
        resolved: Resolved<'a>,
        args: Vec<Val<'a>>,
        tail: bool,
        origin: Option<&'a SourceOrigin>,
    ) -> Result<Step<'a>, Halt> {
        let Resolved { target, env } = resolved;
        match target {
            Target::Structural { order, ty } => self.structural(order, ty, args),
            Target::Closure(closure) => {
                let body: &'a ClosureBody = closure.body;
                let mut frame = self.new_frame(body.slot_count, env, &args)?;
                for (slot, value) in args.into_iter().enumerate() {
                    frame.set(slot, value)?;
                }
                frame.procedure = matches!(body.value_type, Type::Proc(_));
                frame.closure = Some(closure);
                self.push_frame(frame, tail)?;
                if let Some(guard) = &body.entry_guard {
                    self.konts.push(Kont::ClosureRequires { body, guard });
                    return Ok(Step::Eval(&guard.predicate));
                }
                self.start_closure(body)
            }
            Target::Function(index) => {
                if matches!(self.mode, Mode::Evaluate) {
                    self.reads.functions.insert(index);
                }
                if self.probe == Some(index) {
                    self.probed.push(self.invocation);
                }
                let function = self.function(index)?;
                if let FunctionBody::Intrinsic {
                    intrinsic,
                    type_arguments,
                } = &function.body
                    && function.entry_guard.is_none()
                    && function.exit_guard.is_none()
                {
                    let type_arguments = env.apply_all(type_arguments);
                    let origin = origin.unwrap_or(&function.origin);
                    return self.intrinsic(*intrinsic, &type_arguments, args, origin);
                }
                let mut frame = self.new_frame(function.slot_count, env, &args)?;
                for (slot, value) in args.into_iter().enumerate() {
                    frame.set(slot, value)?;
                }
                frame.procedure = self.runs_in_procedure(&Target::Function(index))?;
                self.push_frame(frame, tail)?;
                if let Some(guard) = &function.entry_guard {
                    self.konts.push(Kont::Requires { function, guard });
                    return Ok(Step::Eval(&guard.predicate));
                }
                self.start_body(function)
            }
        }
    }

    /// Runs a closure's body in its activation, arming its `ensures`.
    fn start_closure(&mut self, body: &'a ClosureBody) -> Result<Step<'a>, Halt> {
        self.frame_mut()?.exit = body.exit_guard.as_ref();
        Ok(Step::Eval(&body.body))
    }

    /// Runs a function's body in its activation, arming its `ensures`.
    fn start_body(&mut self, function: &'a CheckedFunction) -> Result<Step<'a>, Halt> {
        let frame = self.frame_mut()?;
        frame.exit = function.exit_guard.as_ref();
        match &function.body {
            FunctionBody::Expr(body) => Ok(Step::Eval(body)),
            FunctionBody::Intrinsic {
                intrinsic,
                type_arguments,
            } => {
                let count = intrinsic.row().parameters.len();
                let args = frame
                    .slots
                    .iter()
                    .take(count)
                    .map(|slot| {
                        slot.clone().ok_or_else(|| {
                            Halt::Invalid("an unset parameter".to_owned())
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let type_arguments = frame.env.apply_all(type_arguments);
                self.intrinsic(*intrinsic, &type_arguments, args, &function.origin)
            }
            FunctionBody::Host(binding) => Err(Halt::Unavailable(format!(
                "the host-bound member `{}` runs only with a host context",
                binding.symbol()
            ))),
        }
    }

    // ----- dispatch -----

    /// The implementation of `member` of `interface` that a closed receiver
    /// selects, or the interface's default member under that
    /// implementation.
    pub(crate) fn dispatch(
        &mut self,
        interface: &TypeId,
        member: &str,
        receiver: &Type,
        arguments: &[Type],
        member_arguments: &[Type],
    ) -> Result<Resolved<'a>, Halt> {
        let key: DispatchKey = (
            interface.clone(),
            member.to_owned(),
            receiver.clone(),
            arguments.to_vec(),
            member_arguments.to_vec(),
        );
        if let Some(resolved) = self.dispatch.get(&key) {
            return Ok(resolved.clone());
        }
        let resolved =
            self.select(interface, member, receiver, arguments, member_arguments)?;
        self.dispatch.insert(key, resolved.clone());
        Ok(resolved)
    }

    fn select(
        &self,
        interface: &TypeId,
        member: &str,
        receiver: &Type,
        arguments: &[Type],
        member_arguments: &[Type],
    ) -> Result<Resolved<'a>, Halt> {
        let set = self.set;
        let definition = set.interfaces().get(interface).ok_or_else(|| {
            Halt::Invalid(format!("no interface `{}`", interface.path()))
        })?;
        let declared = definition.member(member).ok_or_else(|| {
            Halt::Invalid(format!("`{}` has no member `{member}`", interface.path()))
        })?;
        let default = |choices: NamedTypes| -> Result<Resolved<'a>, Halt> {
            let function = declared.default.ok_or_else(|| {
                Halt::Invalid(format!("no implementation of `{member}` for {receiver}"))
            })?;
            Ok(Resolved {
                target: Target::Function(function),
                env: Rc::new(TypeEnv {
                    binders: arguments
                        .iter()
                        .chain(member_arguments)
                        .cloned()
                        .collect(),
                    self_type: Some(receiver.clone()),
                    associated: Some(choices),
                }),
            })
        };
        let selected = vibra_ir::select_implementation(
            set.implementations(),
            receiver,
            interface,
            arguments,
            |_, _, _| None,
        )
        .map_err(|error| Halt::Invalid(error.to_string()))?;
        if let Some((implementation, binders)) = selected {
            return match implementation.member(member) {
                Some(function) => Ok(Resolved {
                    target: Target::Function(function),
                    env: self.env_of(
                        binders
                            .into_iter()
                            .chain(member_arguments.iter().cloned())
                            .collect(),
                    ),
                }),
                None => {
                    default(
                        NamedTypes::new(implementation.choices().entries().iter().map(
                            |(name, ty)| (name.clone(), ty.instantiate(&binders)),
                        ))
                        .map_err(|error| Halt::Invalid(error.to_string()))?,
                    )
                }
            };
        }
        let library = set.library();
        let structural_member = match library.library_of(interface) {
            Some(Library::Ordered) => Some(("compare", true)),
            Some(Library::Equatable) => Some(("equal", false)),
            _ => None,
        };
        if let Some((name, order)) = structural_member
            && is_structural(receiver)
        {
            if member == name {
                return Ok(Resolved {
                    target: Target::Structural {
                        order,
                        ty: receiver.clone(),
                    },
                    env: Rc::clone(&self.empty_env),
                });
            }
            return default(NamedTypes::empty());
        }
        invalid(format!(
            "no implementation of `{}` for {receiver}",
            interface.path()
        ))
    }

    /// Unboxes the receiver of a call through an interface value, giving
    /// the held type and the member's result type with `Self` left open.
    fn unbox(
        &self,
        interface: &TypeId,
        member: &str,
        args: &mut [Val<'a>],
        arguments: &[Type],
        member_arguments: &[Type],
        choices: &NamedTypes,
    ) -> Result<(Type, Type), Halt> {
        let declared = self
            .set
            .interfaces()
            .get(interface)
            .and_then(|definition| definition.member(member))
            .ok_or_else(|| Halt::Invalid(format!("no member `{member}`")))?;
        let slot = declared
            .signature
            .receiver()
            .and_then(|slot| usize::try_from(slot).ok())
            .ok_or_else(|| Halt::Invalid(format!("`{member}` has no receiver slot")))?;
        let receiver = args
            .get_mut(slot)
            .ok_or_else(|| Halt::Invalid("no receiver argument".to_owned()))?;
        let Repr::Interface(held) = &receiver.0 else {
            return invalid("a receiver of an interface value type that is not one");
        };
        let held = Rc::clone(held);
        *receiver = held.value.clone();
        let binders = arguments
            .iter()
            .chain(member_arguments)
            .cloned()
            .collect::<Vec<_>>();
        let signature =
            declared
                .signature
                .type_info()
                .substitute(&vibra_ir::Substitution {
                    binders: &binders,
                    self_type: None,
                    associated: Some(choices),
                });
        // A `proc` member builds a plan, and the plan's result is reboxed
        // when it is awaited.
        let result = match &signature {
            Type::Fn(signature) => signature.result.clone(),
            Type::Proc(signature) => signature.result.clone(),
            _ => return invalid(format!("`{member}` is not a callable")),
        };
        Ok((held.held_type.clone(), result))
    }

    /// `value`, with each `Self` position of `ty` widened to an interface
    /// value holding `held_type`. The value is walked with a work list, so
    /// no host stack grows with its depth or its type's.
    fn rebox(
        &self,
        value: Val<'a>,
        ty: &Type,
        held_type: &Type,
    ) -> Result<Val<'a>, Halt> {
        let mut tasks = vec![Rebox::Enter(value, ty.clone())];
        let mut built: Vec<Val<'a>> = Vec::new();
        while let Some(task) = tasks.pop() {
            match task {
                Rebox::Enter(value, ty) => {
                    self.rebox_enter(value, &ty, held_type, &mut tasks, &mut built)?;
                }
                Rebox::Build(shape, count) => {
                    let children = built.split_off(built.len().saturating_sub(count));
                    built.push(shape.build(children)?);
                }
            }
        }
        built
            .pop()
            .ok_or_else(|| Halt::Invalid("a rebox with no result".to_owned()))
    }

    /// Takes apart one value of `ty`: a value with no `Self` position is
    /// done; otherwise its rebuilding and each of its components is queued.
    fn rebox_enter(
        &self,
        value: Val<'a>,
        ty: &Type,
        held_type: &Type,
        tasks: &mut Vec<Rebox<'a>>,
        built: &mut Vec<Val<'a>>,
    ) -> Result<(), Halt> {
        if *ty == Type::SelfType {
            built.push(Val(Repr::Interface(Rc::new(Held {
                held_type: held_type.clone(),
                value,
            }))));
            return Ok(());
        }
        if !mentions_self(ty) {
            built.push(value);
            return Ok(());
        }
        let mut queue = |shape: RebuildShape<'a>, parts: Vec<(Val<'a>, Type)>| {
            tasks.push(Rebox::Build(shape, parts.len()));
            tasks.extend(
                parts
                    .into_iter()
                    .rev()
                    .map(|(value, ty)| Rebox::Enter(value, ty)),
            );
        };
        let field_of = |fields: &dyn Fn(&str) -> Option<Type>, name: &str| {
            fields(name).ok_or_else(|| Halt::Invalid(format!("no field `{name}`")))
        };
        match (ty, &value.0) {
            (Type::Tuple(types), Repr::Tuple(values)) => queue(
                RebuildShape::Tuple,
                values.iter().cloned().zip(types.iter().cloned()).collect(),
            ),
            (Type::Record(fields), Repr::Record(values)) => {
                let lookup = |name: &str| fields.get(name).cloned();
                let parts = values
                    .iter()
                    .map(|(name, value)| Ok((value.clone(), field_of(&lookup, name)?)))
                    .collect::<Result<Vec<_>, Halt>>()?;
                queue(
                    RebuildShape::Record(
                        values.iter().map(|(name, _)| *name).collect(),
                    ),
                    parts,
                );
            }
            (Type::List(element), Repr::List(values)) => queue(
                RebuildShape::List,
                values
                    .iter()
                    .map(|value| (value.clone(), element.as_ref().clone()))
                    .collect(),
            ),
            (Type::Declared(id, arguments), _) => {
                match (
                    self.set.library().library_of(id),
                    &value.0,
                    arguments.as_slice(),
                ) {
                    (Some(Library::Option), Repr::Opt(None), _) => built.push(value),
                    (Some(Library::Option), Repr::Opt(Some(inner)), [item]) => queue(
                        RebuildShape::Some,
                        vec![(Val::clone(inner), item.clone())],
                    ),
                    (Some(Library::Dict), Repr::Dict(dict), [key, item]) => queue(
                        RebuildShape::Dict(dict.key.clone()),
                        dict.entries
                            .iter()
                            .flat_map(|(k, v)| {
                                [(k.clone(), key.clone()), (v.clone(), item.clone())]
                            })
                            .collect(),
                    ),
                    _ => self.rebox_declared(value, id, arguments, tasks, built)?,
                }
            }
            _ => {
                return invalid(format!(
                    "a `Self` position in {ty} that cannot be reboxed"
                ));
            }
        }
        Ok(())
    }

    fn rebox_declared(
        &self,
        value: Val<'a>,
        id: &TypeId,
        arguments: &[Type],
        tasks: &mut Vec<Rebox<'a>>,
        built: &mut Vec<Val<'a>>,
    ) -> Result<(), Halt> {
        let definition = self
            .set
            .types()
            .definition(id)
            .ok_or_else(|| Halt::Invalid(format!("no type `{}`", id.path())))?;
        let mut queue = |shape: RebuildShape<'a>, parts: Vec<(Val<'a>, Type)>| {
            tasks.push(Rebox::Build(shape, parts.len()));
            tasks.extend(
                parts
                    .into_iter()
                    .rev()
                    .map(|(value, ty)| Rebox::Enter(value, ty)),
            );
        };
        let fields_of = |declared: &[(String, Type)], values: &[(&'a str, Val<'a>)]| {
            values
                .iter()
                .map(|(name, value)| {
                    let ty = declared
                        .iter()
                        .find_map(|(field, ty)| (field == name).then_some(ty))
                        .ok_or_else(|| Halt::Invalid(format!("no field `{name}`")))?;
                    Ok((value.clone(), ty.instantiate(arguments)))
                })
                .collect::<Result<Vec<_>, Halt>>()
        };
        match (definition.body(), &value.0) {
            (TypeBody::Record(fields), Repr::Record(values)) => queue(
                RebuildShape::Record(values.iter().map(|(name, _)| *name).collect()),
                fields_of(fields, values)?,
            ),
            (TypeBody::Tuple(types), Repr::Tuple(values)) => queue(
                RebuildShape::Tuple,
                values
                    .iter()
                    .cloned()
                    .zip(types.iter().map(|ty| ty.instantiate(arguments)))
                    .collect(),
            ),
            (TypeBody::Wrapper(representation), Repr::Wrapper(inner)) => queue(
                RebuildShape::Wrapper,
                vec![(Val::clone(inner), representation.instantiate(arguments))],
            ),
            (TypeBody::Variant(_), Repr::Case(case)) => {
                let declared = definition
                    .case(case.case)
                    .ok_or_else(|| Halt::Invalid(format!("no case `{}`", case.case)))?;
                queue(
                    RebuildShape::Case(
                        case.case,
                        case.fields.iter().map(|(name, _)| *name).collect(),
                    ),
                    fields_of(&declared.fields, &case.fields)?,
                );
            }
            (TypeBody::Variant(_), Repr::Tag(_)) => built.push(value),
            _ => return invalid("a reboxed value that does not fit its type"),
        }
        Ok(())
    }

    // ----- intrinsics -----

    fn intrinsic(
        &mut self,
        intrinsic: CompilerIntrinsic,
        type_arguments: &[Type],
        args: Vec<Val<'a>>,
        origin: &'a SourceOrigin,
    ) -> Result<Step<'a>, Halt> {
        match intrinsic {
            CompilerIntrinsic::ListFold => {
                let [list, initial, function]: [Val<'a>; 3] =
                    args.try_into().map_err(|_| {
                        Halt::Invalid("`list.fold` takes three operands".to_owned())
                    })?;
                let items = Rc::clone(list.as_list().ok_or_else(|| {
                    Halt::Invalid("`list.fold` of a non-list".to_owned())
                })?);
                self.fold(function, items, 0, initial)
            }
            CompilerIntrinsic::DictFromList => {
                let [key, _] = type_arguments else {
                    return invalid("`dict.fromList` takes two type arguments");
                };
                let entries = pairs(&single(args)?)?;
                self.dict_build(key.clone(), entries)
            }
            CompilerIntrinsic::AssertTrue | CompilerIntrinsic::AssertFalse => {
                self.assertion_allowed(intrinsic, origin)?;
                let holds = single(args)?.as_bool().ok_or_else(|| {
                    Halt::Invalid("an assertion of a non-Bool".to_owned())
                })?;
                if holds == (intrinsic == CompilerIntrinsic::AssertTrue) {
                    Ok(Step::Ret(Val::void()))
                } else {
                    Err(failure(
                        FailureKind::Assertion,
                        intrinsic,
                        None,
                        None,
                        origin,
                    ))
                }
            }
            CompilerIntrinsic::AssertEqual => {
                self.assertion_allowed(intrinsic, origin)?;
                let [ty] = type_arguments else {
                    return invalid("`assert.equal` takes one type argument");
                };
                let [expected, actual] = args.as_slice() else {
                    return invalid("`assert.equal` takes two operands");
                };
                let observe = |value: &Val<'a>| match encoding(self.set, value, ty) {
                    Ok(text) => Ok(Some(text)),
                    Err(ObserveError::Unsupported) => Ok(None),
                    Err(ObserveError::Halt(halt)) => Err(halt),
                };
                match (observe(expected)?, observe(actual)?) {
                    (Some(expected), Some(actual)) if expected == actual => {
                        Ok(Step::Ret(Val::void()))
                    }
                    (Some(expected), Some(actual)) => Err(failure(
                        FailureKind::Assertion,
                        intrinsic,
                        Some(expected),
                        Some(actual),
                        origin,
                    )),
                    (expected, actual) => Err(failure(
                        FailureKind::Observation {
                            code: DiagnosticCode::ToolUnsupportedValue,
                            type_info: ty.clone(),
                        },
                        intrinsic,
                        expected,
                        actual,
                        origin,
                    )),
                }
            }
            _ => {
                let value = intrinsics::apply(intrinsic, &args)?;
                self.allocated(&value)?;
                Ok(Step::Ret(value))
            }
        }
    }

    fn assertion_allowed(
        &mut self,
        intrinsic: CompilerIntrinsic,
        origin: &'a SourceOrigin,
    ) -> Result<(), Halt> {
        if self.mode == Mode::Test {
            Ok(())
        } else {
            self.origin = Some(origin);
            invalid(format!("`{}` outside a test", intrinsic.symbol()))
        }
    }

    fn fold(
        &mut self,
        function: Val<'a>,
        items: Rc<Vec<Val<'a>>>,
        next: usize,
        accumulator: Val<'a>,
    ) -> Result<Step<'a>, Halt> {
        let Some(item) = items.get(next).cloned() else {
            return Ok(Step::Ret(accumulator));
        };
        let resolved = self.resolve_value(&function)?;
        self.konts.push(Kont::Fold {
            function,
            items,
            next: next.saturating_add(1),
        });
        let origin = self.origin;
        self.invoke(resolved, vec![accumulator, item], false, origin)
    }

    // ----- continuations -----

    #[allow(clippy::too_many_lines)]
    fn resume(&mut self, kont: Kont<'a>, value: Val<'a>) -> Result<Step<'a>, Halt> {
        Ok(match kont {
            Kont::End => return invalid("the end resumed"),
            Kont::Leave => {
                let frame = self.frame_mut()?;
                if let Some(exit) = frame.exit.take() {
                    frame.set(exit.candidate, value.clone())?;
                    self.konts.push(Kont::Leave);
                    self.konts.push(Kont::Ensures {
                        candidate: value,
                        guard: &exit.guard,
                    });
                    return Ok(Step::Eval(&exit.guard.predicate));
                }
                self.pop_frame()?;
                Step::Ret(value)
            }
            Kont::Global(index) => {
                self.pop_frame()?;
                let state = self
                    .globals
                    .get_mut(index)
                    .ok_or_else(|| Halt::Invalid(format!("no module value {index}")))?;
                *state = GlobalState::Ready(value.clone());
                Step::Ret(value)
            }
            Kont::Sequence(rest) => {
                drop(value);
                self.sequence(rest)?
            }
            Kont::Let { slot, body } => {
                if let Some(slot) = slot {
                    self.bind_slot(slot, value)?;
                }
                Step::Eval(body)
            }
            Kont::LetElse {
                pattern,
                fallback,
                body,
            } => {
                if self.bind_pattern(pattern, &value)? {
                    Step::Eval(body)
                } else {
                    Step::Eval(fallback)
                }
            }
            Kont::Match(arms) => {
                for arm in arms {
                    if self.bind_pattern(&arm.pattern, &value)? {
                        return Ok(Step::Eval(&arm.body));
                    }
                }
                return invalid("no arm of a match fits its subject");
            }
            Kont::If {
                then_branch,
                else_branch,
            } => match (truth(&value)?, else_branch) {
                (true, _) => Step::Eval(then_branch),
                (false, Some(else_branch)) => Step::Eval(else_branch),
                (false, None) => Step::Ret(Val::void()),
            },
            Kont::Field(field) => match &value.0 {
                Repr::Record(fields) => Step::Ret(
                    fields
                        .iter()
                        .find_map(|(name, value)| {
                            (*name == field).then(|| value.clone())
                        })
                        .ok_or_else(|| Halt::Invalid(format!("no field `{field}`")))?,
                ),
                _ => return invalid(format!("a read of `{field}` from a non-record")),
            },
            Kont::Return => self.leave_with(value)?,
            Kont::Try => match value.case_name() {
                Some("some" | "ok") => {
                    Step::Ret(value.case_field("value").ok_or_else(|| {
                        Halt::Invalid("a carrier without its value".to_owned())
                    })?)
                }
                Some("none" | "err") => self.leave_with(value)?,
                _ => return invalid("a try on a value that is not a carrier"),
            },
            Kont::Widen(held_type) => {
                let value = Val(Repr::Interface(Rc::new(Held { held_type, value })));
                self.allocated(&value)?;
                Step::Ret(value)
            }
            Kont::Operands { work, mut done } => {
                done.push(value);
                self.operands(work, done)?
            }
            Kont::Requires { function, guard } => {
                if truth(&value)? {
                    self.start_body(function)?
                } else {
                    Step::Eval(&guard.consequence)
                }
            }
            Kont::ClosureRequires { body, guard } => {
                if truth(&value)? {
                    self.start_closure(body)?
                } else {
                    Step::Eval(&guard.consequence)
                }
            }
            Kont::Ensures { candidate, guard } => {
                if truth(&value)? {
                    Step::Ret(candidate)
                } else {
                    Step::Eval(&guard.consequence)
                }
            }
            Kont::Invariant {
                value: built,
                consequence,
            } => {
                let result = if truth(&value)? {
                    Val::ok(built)
                } else {
                    Val::err(self.constant(consequence)?)
                };
                self.allocated(&result)?;
                Step::Ret(result)
            }
            Kont::Fold {
                function,
                items,
                next,
            } => self.fold(function, items, next, value)?,
            Kont::DictBuild(build) => self.resume_dict_build(*build, &value)?,
            Kont::DictFind(find) => self.resume_dict_find(*find, &value)?,
            Kont::Structural(structural) => {
                self.resume_structural(*structural, &value)?
            }
            Kont::Ingress(ingress) => self.resume_ingress(*ingress, &value)?,
            Kont::Rebox { result, held_type } => {
                let value = self.rebox(value, &result, &held_type)?;
                self.allocated(&value)?;
                Step::Ret(value)
            }
            Kont::Await { caller } => {
                self.invocation = caller;
                Step::Ret(value)
            }
        })
    }

    /// Leaves the current activation with `value`, skipping the work left
    /// in it.
    fn leave_with(&mut self, value: Val<'a>) -> Result<Step<'a>, Halt> {
        loop {
            match self.konts.last() {
                Some(Kont::Leave) => break,
                Some(Kont::End | Kont::Global(_)) | None => {
                    return invalid("a return outside a callable");
                }
                Some(_) => {
                    self.konts.pop();
                }
            }
        }
        Ok(Step::Ret(value))
    }

    /// Matches `value` against `pattern`, binding its variables.
    fn bind_pattern(
        &mut self,
        pattern: &'a Pattern,
        value: &Val<'a>,
    ) -> Result<bool, Halt> {
        let mut pending: Vec<(&'a Pattern, Val<'a>)> = vec![(pattern, value.clone())];
        while let Some((pattern, value)) = pending.pop() {
            let fits = match pattern {
                Pattern::Discard => true,
                Pattern::Bind(slot) => {
                    self.bind_slot(*slot, value)?;
                    true
                }
                Pattern::Literal(constant) => same(&self.constant(constant)?, &value),
                Pattern::Tuple(patterns) => match &value.0 {
                    Repr::Tuple(values) if values.len() == patterns.len() => {
                        pending.extend(patterns.iter().zip(values.iter().cloned()));
                        true
                    }
                    _ => false,
                },
                Pattern::List(patterns) => match &value.0 {
                    Repr::List(values) if values.len() == patterns.len() => {
                        pending.extend(patterns.iter().zip(values.iter().cloned()));
                        true
                    }
                    Repr::List(_) => false,
                    _ => return invalid("a list pattern on a non-list"),
                },
                Pattern::Record(fields) => match &value.0 {
                    Repr::Record(values) => {
                        for (name, pattern) in fields {
                            let field = values
                                .iter()
                                .find_map(|(field, value)| {
                                    (*field == name).then(|| value.clone())
                                })
                                .ok_or_else(|| {
                                    Halt::Invalid(format!("no field `{name}`"))
                                })?;
                            pending.push((pattern, field));
                        }
                        true
                    }
                    _ => return invalid("a record pattern on a non-record"),
                },
                Pattern::Wrapper(inner) => {
                    let unwrapped = match &value.0 {
                        Repr::Wrapper(inner) => inner.as_ref().clone(),
                        Repr::Str(_) => {
                            intrinsics::apply(CompilerIntrinsic::StrToChars, &[value])?
                        }
                        Repr::Bytes(_) => {
                            intrinsics::apply(CompilerIntrinsic::BytesToList, &[value])?
                        }
                        Repr::Dict(_) => {
                            intrinsics::apply(CompilerIntrinsic::DictEntries, &[value])?
                        }
                        _ => return invalid("a wrapper pattern on a non-wrapper"),
                    };
                    self.allocated(&unwrapped)?;
                    pending.push((inner, unwrapped));
                    true
                }
                Pattern::Case { case, fields } => {
                    if value.case_name() == Some(case.as_str()) {
                        for (name, pattern) in fields {
                            let field = value.case_field(name).ok_or_else(|| {
                                Halt::Invalid(format!("no field `{name}`"))
                            })?;
                            pending.push((pattern, field));
                        }
                        true
                    } else {
                        false
                    }
                }
            };
            if !fits {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

impl TypeEnv {
    /// Each choice of `choices` at this activation's type arguments.
    fn apply_all_named(&self, choices: &NamedTypes) -> Result<NamedTypes, Halt> {
        NamedTypes::new(
            choices
                .entries()
                .iter()
                .map(|(name, ty)| (name.clone(), self.apply(ty))),
        )
        .map_err(|error| Halt::Invalid(error.to_string()))
    }
}

fn single<'a>(values: Vec<Val<'a>>) -> Result<Val<'a>, Halt> {
    let [value]: [Val<'a>; 1] = values
        .try_into()
        .map_err(|_| Halt::Invalid("an operation of one operand".to_owned()))?;
    Ok(value)
}

/// The entries of `list`, a `List({K V})`, as key and value pairs in order.
fn pairs<'a>(list: &Val<'a>) -> Result<Vec<(Val<'a>, Val<'a>)>, Halt> {
    list.as_list()
        .ok_or_else(|| Halt::Invalid("a dict built from a non-list".to_owned()))?
        .iter()
        .map(|entry| match &entry.0 {
            Repr::Tuple(pair) => match pair.as_slice() {
                [key, value] => Ok((key.clone(), value.clone())),
                _ => invalid("a dict entry that is not a pair"),
            },
            _ => invalid("a dict entry that is not a pair"),
        })
        .collect()
}

fn truth(value: &Val<'_>) -> Result<bool, Halt> {
    value
        .as_bool()
        .ok_or_else(|| Halt::Invalid("a condition that is not a Bool".to_owned()))
}

fn dict_key(value_type: &Type) -> Result<Type, Halt> {
    match value_type {
        Type::Declared(_, arguments) => match arguments.as_slice() {
            [key, _] => Ok(key.clone()),
            _ => invalid("a dict type without its key"),
        },
        _ => invalid("a dict of a non-dict type"),
    }
}

/// Whether `ty` conforms to `Equatable` and `Ordered` with no declaration.
pub(crate) const fn is_structural(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Atom | Type::AtomSingleton(_) | Type::Tuple(_) | Type::Record(_)
    )
}

fn failure(
    kind: FailureKind,
    intrinsic: CompilerIntrinsic,
    expected: Option<String>,
    actual: Option<String>,
    origin: &SourceOrigin,
) -> Halt {
    Halt::Failed(Box::new(TestAssertionFailure {
        kind,
        assertion: intrinsic.symbol(),
        expected,
        actual,
        origin: origin.clone(),
    }))
}

/// One step of a rebox: take a value of a type apart, or put a value back
/// together from its last reboxed components.
enum Rebox<'a> {
    Enter(Val<'a>, Type),
    Build(RebuildShape<'a>, usize),
}

/// What a reboxed value is rebuilt as, around its reboxed components.
enum RebuildShape<'a> {
    Tuple,
    Record(Vec<&'a str>),
    List,
    Some,
    Dict(Type),
    Wrapper,
    Case(&'a str, Vec<&'a str>),
}

impl<'a> RebuildShape<'a> {
    fn build(self, components: Vec<Val<'a>>) -> Result<Val<'a>, Halt> {
        let missing = || Halt::Invalid("a rebox lost a component".to_owned());
        Ok(match self {
            Self::Tuple => Val::tuple(components),
            Self::Record(names) => Val(Repr::Record(Rc::new(
                names.into_iter().zip(components).collect(),
            ))),
            Self::List => Val::list(components),
            Self::Some => Val::some(components.into_iter().next().ok_or_else(missing)?),
            Self::Dict(key) => {
                let mut components = components.into_iter();
                let mut entries = Vec::new();
                while let Some(entry_key) = components.next() {
                    entries.push((entry_key, components.next().ok_or_else(missing)?));
                }
                Val(Repr::Dict(Rc::new(crate::value::DictVal { key, entries })))
            }
            Self::Wrapper => Val(Repr::Wrapper(Rc::new(
                components.into_iter().next().ok_or_else(missing)?,
            ))),
            Self::Case(case, names) => {
                Val::case(case, names.into_iter().zip(components).collect())
            }
        })
    }
}
