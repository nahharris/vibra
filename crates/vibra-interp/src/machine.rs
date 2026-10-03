//! The reference interpreter's activation machine.
//!
//! The runtime chapter bounds the depth of non-tail recursion only by memory
//! (`docs/spec/06-runtime.md`, "Activations and memory"), so no Rust call is
//! made for a language activation. The machine holds activations on the heap:
//!
//! - a **frame** is one activation's slots, captures, and type arguments;
//! - a **continuation** is what remains to be done with a value an expression
//!   produces, such as the operands still to evaluate or the body that
//!   follows a `let`; and
//! - the machine is a loop over the two stacks that either evaluates an
//!   expression or hands a value to the continuation on top.
//!
//! A non-tail call pushes a frame and an end-of-activation continuation, and
//! the callee's return pops them. A call in tail position replaces the frame
//! the activation already holds. A single function body's own nesting is
//! bounded by its source, so the continuations it pushes need no more
//! structure than that.
//!
//! Memory is accounted in bytes of frames and continuations exactly, and in
//! bytes of values by measurement: a value does not count when it is built,
//! because it may be released before the next step, but a walk over every
//! live value runs whenever enough has been allocated since the last one to
//! matter. Exceeding the runner's budget, or failing to grow either stack,
//! is [`Halt::Memory`].

use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};
use std::mem::size_of;
use std::rc::Rc;
use std::sync::Arc;

mod compare;

use vibra_ir::external::CompilerIntrinsic;
use vibra_ir::{
    CallTarget, CheckedProgram, ClosedContract, Expr, FunctionSignature, MatchArm,
    SourceOrigin, TestAssertion, Type, TypeId, Value,
};

use crate::value::{Callable, Elements, Inner, LambdaId, RuntimeValue, TypeMap};
use crate::{
    TestAssertionFailure, activation_slots, admits_value, bind_pattern, bind_type,
    closed_next, lookup, mentions_param, observe, pattern_matches, present_payload,
    registry, representation_of, runtime_type, slots_match_signature,
    values_match_slots,
};

/// Why the machine stopped before the entry returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Halt {
    /// The checked IR violated an invariant, which the checker rules out.
    Invalid,
    /// A test assertion was false; the failure is recorded in the machine.
    Assertion,
    /// The budget was exhausted or a stack could not grow: the host event.
    Memory,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum GlobalState {
    Uninitialized,
    Evaluating,
    Ready(RuntimeValue),
}

/// What a call checks about a callee's signature, worked out once rather than
/// at every call.
struct SignatureInfo {
    signature: Arc<FunctionSignature>,
    /// The type of every argument slot, in slot order.
    slots: Vec<Type>,
    result: Type,
    /// Whether any slot or the result names a generic parameter.
    generic: bool,
}

impl SignatureInfo {
    fn new(signature: Arc<FunctionSignature>) -> Self {
        let slots = signature.slot_types();
        let result = signature.result();
        let generic = slots.iter().any(mentions_param) || mentions_param(&result);
        Self {
            signature,
            slots,
            result,
            generic,
        }
    }
}

/// One activation. Slots are write-once and names never shadow, so a `let`
/// writes its slot in place.
struct Frame {
    slots: Vec<Option<RuntimeValue>>,
    captures: Elements,
    /// The type arguments of this activation.
    types: Arc<TypeMap>,
    /// The module value whose initializer this frame runs, if it runs one.
    initializer: Option<usize>,
}

/// What to do with the value the machine is about to produce.
enum Kont<'a> {
    /// The end of an activation: its frame is released and the value goes to
    /// whatever called it.
    End { counted: bool },
    /// A module value's initializer finished.
    Global { node: &'a Expr, index: usize },
    /// A leading expression of a sequence finished; the rest follows.
    Sequence { rest: &'a [Expr] },
    /// One operand of `node` finished; the rest follow, then `node` itself.
    Operands {
        node: &'a Expr,
        next: usize,
        values: Vec<RuntimeValue>,
    },
    /// The one operand of `node` finished.
    Unary { node: &'a Expr },
    /// The value of a `let` finished.
    Let { slot: Option<usize>, body: &'a Expr },
    /// The condition of an `if` finished.
    If {
        then_branch: &'a Expr,
        else_branch: &'a Expr,
    },
    /// The scrutinee of a `match` finished.
    Match { arms: &'a [MatchArm] },
    /// One step of `array.fold` returned.
    Fold(Box<FoldState>),
    /// A key comparison that called a user's `compare` returned.
    Compare(Box<compare::CompareKont>),
}

/// `array.fold` calls its step once for each element in order, and each call
/// is an activation like any other, so the fold is a continuation.
struct FoldState {
    items: Elements,
    next: usize,
    step: Callable,
    result: Type,
}

enum Step<'a> {
    Eval(&'a Expr),
    Ret(RuntimeValue),
}

const KONT_BYTES: usize = size_of::<Kont<'static>>();
const FRAME_BYTES: usize = size_of::<Frame>();
const SLOT_BYTES: usize = size_of::<Option<RuntimeValue>>();

/// The least allocation between two measurements of the live values.
const MIN_WALK_BYTES: usize = 64 * 1024;

pub(crate) struct Machine<'a> {
    program: &'a CheckedProgram,
    pub(crate) globals: Vec<GlobalState>,
    frames: Vec<Frame>,
    konts: Vec<Kont<'a>>,
    /// Closure bodies, indexed by [`LambdaId`], and the id of each by the
    /// address of its body in the checked program.
    lambdas: Vec<&'a Expr>,
    lambda_ids: BTreeMap<usize, usize>,
    /// The signature of each module function, shared once it is first used.
    signatures: RefCell<Vec<Option<Rc<SignatureInfo>>>>,
    /// The captures and type arguments of an activation that has none, shared
    /// so that calling a function allocates neither.
    no_captures: Elements,
    no_types: Arc<TypeMap>,
    current_depth: usize,
    pub(crate) max_depth: usize,
    pub(crate) tail_transfers: usize,
    test_mode: bool,
    limit: usize,
    /// Bytes of frames and continuations now live, counted exactly.
    stack_bytes: usize,
    /// Bytes of values at the last measurement.
    live_bytes: usize,
    /// Bytes of values built since the last measurement.
    allocated: usize,
    /// Allocation that triggers the next measurement.
    walk_after: usize,
    peak: usize,
    pub(crate) assertion_failure: Option<TestAssertionFailure>,
    /// The assertion whose operand held a function, which stops the test.
    pub(crate) unobservable: Option<SourceOrigin>,
}

impl<'a> Machine<'a> {
    pub(crate) fn new(
        program: &'a CheckedProgram,
        test_mode: bool,
        budget_bytes: usize,
    ) -> Self {
        Self {
            program,
            globals: vec![GlobalState::Uninitialized; program.globals().len()],
            frames: Vec::new(),
            konts: Vec::new(),
            lambdas: Vec::new(),
            lambda_ids: BTreeMap::new(),
            signatures: RefCell::new(vec![None; program.functions().len()]),
            no_captures: Elements::new(Vec::new()),
            no_types: Arc::default(),
            current_depth: 0,
            max_depth: 0,
            tail_transfers: 0,
            test_mode,
            limit: budget_bytes,
            stack_bytes: 0,
            live_bytes: 0,
            allocated: 0,
            walk_after: (budget_bytes / 2).max(MIN_WALK_BYTES),
            peak: 0,
            assertion_failure: None,
            unobservable: None,
        }
    }

    /// The most memory the run held at once, in the machine's own accounting:
    /// bytes of frames and continuations, plus the values at the last
    /// measurement.
    pub(crate) const fn peak_memory_bytes(&self) -> usize {
        self.peak
    }

    // -----------------------------------------------------------------
    // Entry.
    // -----------------------------------------------------------------

    /// Runs the function at `index` as the outermost activation.
    pub(crate) fn run_entry(
        &mut self,
        index: usize,
        slots: Vec<Option<RuntimeValue>>,
    ) -> Result<RuntimeValue, Halt> {
        let program = self.program;
        let function = program.functions().get(index).ok_or(Halt::Invalid)?;
        if slots.len() != function.slot_count()
            || !slots_match_signature(&slots, function.signature())
        {
            return Err(Halt::Invalid);
        }
        self.enter(slots, self.no_captures.clone(), Arc::clone(&self.no_types))?;
        self.execute(Step::Eval(function.body()), 0)
    }

    /// Runs the machine until the continuations above `base` are spent and a
    /// value reaches the continuation at `base`.
    fn execute(&mut self, start: Step<'a>, base: usize) -> Result<RuntimeValue, Halt> {
        let mut step = start;
        loop {
            step = match step {
                Step::Eval(expression) => self.eval(expression)?,
                Step::Ret(value) => {
                    if self.konts.len() <= base {
                        return Ok(value);
                    }
                    let Some(kont) = self.konts.pop() else {
                        return Err(Halt::Invalid);
                    };
                    self.stack_bytes = self.stack_bytes.saturating_sub(KONT_BYTES);
                    self.resume(kont, value)?
                }
            };
        }
    }
    // -----------------------------------------------------------------
    // Frames, continuations, and the memory budget.
    // -----------------------------------------------------------------

    fn frame(&self) -> Result<&Frame, Halt> {
        self.frames.last().ok_or(Halt::Invalid)
    }

    fn frame_mut(&mut self) -> Result<&mut Frame, Halt> {
        self.frames.last_mut().ok_or(Halt::Invalid)
    }

    /// The running activation's type arguments.
    fn current_types(&self) -> Arc<TypeMap> {
        self.frames
            .last()
            .map(|frame| Arc::clone(&frame.types))
            .unwrap_or_default()
    }

    /// `value` at the running activation's type arguments.
    fn concrete(&self, value: &Type) -> Type {
        match self.frames.last() {
            Some(frame) if !frame.types.is_empty() => value.substitute(&frame.types),
            _ => value.clone(),
        }
    }

    fn push(&mut self, kont: Kont<'a>) -> Result<(), Halt> {
        self.stack_bytes = self.stack_bytes.saturating_add(KONT_BYTES);
        self.check_budget()?;
        self.konts.try_reserve(1).map_err(|_| Halt::Memory)?;
        self.konts.push(kont);
        Ok(())
    }

    /// Takes the continuations off the stack down to the end of the running
    /// activation, which stays, and releases what they hold.
    fn unwind(&mut self) -> Result<(), Halt> {
        while !matches!(self.konts.last(), Some(Kont::End { .. })) {
            if self.konts.pop().is_none() {
                return Err(Halt::Invalid);
            }
            self.stack_bytes = self.stack_bytes.saturating_sub(KONT_BYTES);
        }
        Ok(())
    }

    /// Leaves the running activation with `value`, whatever expressions were
    /// still pending in it.
    fn exit(&mut self, value: RuntimeValue) -> Result<Step<'a>, Halt> {
        self.unwind()?;
        Ok(Step::Ret(value))
    }

    fn frame_bytes(slot_count: usize) -> usize {
        FRAME_BYTES.saturating_add(slot_count.saturating_mul(SLOT_BYTES))
    }

    /// Starts an activation: its frame and the continuation that ends it.
    fn enter(
        &mut self,
        slots: Vec<Option<RuntimeValue>>,
        captures: Elements,
        types: Arc<TypeMap>,
    ) -> Result<(), Halt> {
        self.push_frame(
            Frame {
                slots,
                captures,
                types,
                initializer: None,
            },
            true,
        )
    }

    fn push_frame(&mut self, frame: Frame, counted: bool) -> Result<(), Halt> {
        self.stack_bytes = self
            .stack_bytes
            .saturating_add(Self::frame_bytes(frame.slots.len()));
        self.frames.try_reserve(1).map_err(|_| Halt::Memory)?;
        self.frames.push(frame);
        if counted {
            self.current_depth = self.current_depth.saturating_add(1);
            self.max_depth = self.max_depth.max(self.current_depth);
        }
        self.push(Kont::End { counted })
    }

    fn leave_frame(&mut self, counted: bool) {
        if let Some(frame) = self.frames.pop() {
            self.stack_bytes = self
                .stack_bytes
                .saturating_sub(Self::frame_bytes(frame.slots.len()));
        }
        if counted {
            self.current_depth = self.current_depth.saturating_sub(1);
        }
    }

    /// Exhaustion is decided on the exact stack bytes and the values as last
    /// measured; a measurement is taken only when that sum could exceed the
    /// budget, so a program well inside it never pays for one.
    fn check_budget(&mut self) -> Result<(), Halt> {
        let committed = self.stack_bytes.saturating_add(self.live_bytes);
        if committed > self.peak {
            self.peak = committed;
        }
        if committed.saturating_add(self.allocated) > self.limit {
            self.measure(None)?;
        }
        Ok(())
    }

    /// Measures every value the machine holds, then decides whether the
    /// budget is exhausted. `extra` is a value in flight.
    fn measure(&mut self, extra: Option<&RuntimeValue>) -> Result<(), Halt> {
        // Parts of values are shared, so one walk counts each part once
        // however many bindings and continuations hold it.
        let mut seen = HashSet::new();
        let mut live = 0usize;
        if let Some(value) = extra {
            value.live_bytes(&mut seen, &mut live);
        }
        for frame in &self.frames {
            for slot in frame.slots.iter().flatten() {
                slot.live_bytes(&mut seen, &mut live);
            }
            for capture in frame.captures.iter() {
                capture.live_bytes(&mut seen, &mut live);
            }
        }
        for kont in &self.konts {
            match kont {
                Kont::Operands { values, .. } => {
                    live = live.saturating_add(
                        values.len().saturating_mul(size_of::<RuntimeValue>()),
                    );
                    for value in values {
                        value.live_bytes(&mut seen, &mut live);
                    }
                }
                Kont::Fold(state) => {
                    let items = RuntimeValue::Array {
                        value_type: Type::Void,
                        values: state.items.clone(),
                    };
                    items.live_bytes(&mut seen, &mut live);
                    for capture in state.step.captures().iter() {
                        capture.live_bytes(&mut seen, &mut live);
                    }
                }
                Kont::Compare(state) => state.live_bytes(&mut seen, &mut live),
                _ => {}
            }
        }
        for global in &self.globals {
            if let GlobalState::Ready(value) = global {
                value.live_bytes(&mut seen, &mut live);
            }
        }
        self.live_bytes = live;
        self.allocated = 0;
        let committed = self.stack_bytes.saturating_add(live);
        self.peak = self.peak.max(committed);
        if committed > self.limit {
            return Err(Halt::Memory);
        }
        self.walk_after = ((self.limit - committed) / 2).max(MIN_WALK_BYTES);
        Ok(())
    }

    /// Accounts for a value an operation has just built, and passes it on.
    fn fresh(&mut self, value: RuntimeValue) -> Result<Step<'a>, Halt> {
        let bytes = value.built_bytes();
        if bytes != 0 {
            self.allocated = self.allocated.saturating_add(bytes);
            if self.allocated >= self.walk_after {
                self.measure(Some(&value))?;
            }
        }
        Ok(Step::Ret(value))
    }

    // -----------------------------------------------------------------
    // Evaluation.
    // -----------------------------------------------------------------

    /// The value of an expression that needs no continuation: a literal or a
    /// read of a binding or a function name.
    fn leaf(&mut self, expression: &'a Expr) -> Result<Option<RuntimeValue>, Halt> {
        Ok(Some(match expression {
            Expr::Literal { value, .. } => RuntimeValue::Primitive(value.clone()),
            Expr::Variable {
                slot, value_type, ..
            } => self
                .frame()?
                .slots
                .get(*slot)
                .and_then(Option::as_ref)
                .filter(|value| admits_value(value_type, value))
                .ok_or(Halt::Invalid)?
                .clone(),
            Expr::Captured {
                slot, value_type, ..
            } => self
                .frame()?
                .captures
                .get(*slot)
                .filter(|value| admits_value(value_type, value))
                .ok_or(Halt::Invalid)?
                .clone(),
            Expr::Function {
                function,
                signature,
                ..
            } => self.function_value(*function, signature)?,
            _ => return Ok(None),
        }))
    }

    /// A module function named as a value, instantiated by the signature
    /// written at the site.
    fn function_value(
        &self,
        function: usize,
        signature: &FunctionSignature,
    ) -> Result<RuntimeValue, Halt> {
        let mut callable = self.named_callable(function).ok_or(Halt::Invalid)?;
        let mut bound = TypeMap::new();
        bind_type(
            &Type::Function(Box::new(callable.signature().clone())),
            &self.concrete(&Type::Function(Box::new(signature.clone()))),
            &mut bound,
        );
        *callable.types_mut() = Arc::new(bound);
        Ok(RuntimeValue::Function(callable))
    }

    /// Evaluates `child` and then continues with `kont`, without pushing the
    /// continuation when the child is a leaf.
    fn then(&mut self, child: &'a Expr, kont: Kont<'a>) -> Result<Step<'a>, Halt> {
        if let Some(value) = self.leaf(child)? {
            return self.resume(kont, value);
        }
        self.push(kont)?;
        Ok(Step::Eval(child))
    }

    fn eval(&mut self, expression: &'a Expr) -> Result<Step<'a>, Halt> {
        if let Some(value) = self.leaf(expression)? {
            return Ok(Step::Ret(value));
        }
        match expression {
            Expr::Global { index, .. } => self.global(expression, *index),
            Expr::Default { .. } => Err(Halt::Invalid),
            Expr::Sequence { expressions, .. } => self.sequence(expressions),
            Expr::Let {
                slot, value, body, ..
            } => self.then(value, Kont::Let { slot: *slot, body }),
            Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => self.then(
                condition,
                Kont::If {
                    then_branch,
                    else_branch,
                },
            ),
            Expr::Match {
                scrutinee, arms, ..
            } => self.then(scrutinee, Kont::Match { arms }),
            Expr::Variant {
                value_type,
                variant,
                payload,
                ..
            } => {
                // `bool` is represented directly.
                if *value_type == Type::Bool {
                    return Ok(Step::Ret(RuntimeValue::Primitive(Value::Bool(
                        variant == "true",
                    ))));
                }
                match payload {
                    Some(payload) => {
                        self.then(payload, Kont::Unary { node: expression })
                    }
                    None => self.variant(expression, None),
                }
            }
            Expr::Wrap { value, .. }
            | Expr::Widen { value, .. }
            | Expr::Try { value, .. }
            | Expr::Return { value, .. } => {
                self.then(value, Kont::Unary { node: expression })
            }
            Expr::Project { record, .. } => {
                self.then(record, Kont::Unary { node: expression })
            }
            Expr::TupleProject { tuple, .. } => {
                self.then(tuple, Kont::Unary { node: expression })
            }
            Expr::External { .. }
            | Expr::Tuple { .. }
            | Expr::Array { .. }
            | Expr::Record { .. }
            | Expr::Closure { .. }
            | Expr::Dict { .. }
            | Expr::Lookup { .. }
            | Expr::Call { .. } => {
                let count = operand_count(expression);
                self.operands(expression, 0, Vec::with_capacity(count))
            }
            Expr::Literal { .. }
            | Expr::Variable { .. }
            | Expr::Captured { .. }
            | Expr::Function { .. } => Err(Halt::Invalid),
        }
    }

    /// Hands the value an expression produced to the continuation that was
    /// waiting for it.
    fn resume(
        &mut self,
        kont: Kont<'a>,
        value: RuntimeValue,
    ) -> Result<Step<'a>, Halt> {
        match kont {
            Kont::End { counted } => {
                self.leave_frame(counted);
                Ok(Step::Ret(value))
            }
            Kont::Global { node, index } => {
                let Expr::Global { value_type, .. } = node else {
                    return Err(Halt::Invalid);
                };
                *self.globals.get_mut(index).ok_or(Halt::Invalid)? =
                    GlobalState::Ready(value.clone());
                if !admits_value(value_type, &value) {
                    return Err(Halt::Invalid);
                }
                Ok(Step::Ret(value))
            }
            Kont::Sequence { rest } => {
                drop(value);
                self.sequence(rest)
            }
            Kont::Operands {
                node,
                next,
                mut values,
            } => {
                values.push(value);
                self.operands(node, next, values)
            }
            Kont::Unary { node } => self.unary(node, value),
            Kont::Let { slot, body } => {
                if let Some(slot) = slot {
                    *self.frame_mut()?.slots.get_mut(slot).ok_or(Halt::Invalid)? =
                        Some(value);
                }
                Ok(Step::Eval(body))
            }
            Kont::If {
                then_branch,
                else_branch,
            } => match value {
                RuntimeValue::Primitive(Value::Bool(true)) => {
                    Ok(Step::Eval(then_branch))
                }
                RuntimeValue::Primitive(Value::Bool(false)) => {
                    Ok(Step::Eval(else_branch))
                }
                _ => Err(Halt::Invalid),
            },
            Kont::Match { arms } => {
                for arm in arms {
                    if pattern_matches(&arm.pattern, &value) {
                        let frame = self.frame_mut()?;
                        bind_pattern(&arm.pattern, &value, &mut frame.slots)
                            .ok_or(Halt::Invalid)?;
                        return Ok(Step::Eval(&arm.body));
                    }
                }
                Err(Halt::Invalid)
            }
            Kont::Fold(state) => self.fold_next(state, value),
            Kont::Compare(state) => self.resume_compare(state, value),
        }
    }

    /// A sequence: the leading expressions for their effects on the frame,
    /// then the last as the value, in tail position.
    fn sequence(&mut self, expressions: &'a [Expr]) -> Result<Step<'a>, Halt> {
        match expressions {
            [] => Ok(Step::Ret(RuntimeValue::Primitive(Value::Void))),
            [last] => Ok(Step::Eval(last)),
            [first, rest @ ..] => {
                self.push(Kont::Sequence { rest })?;
                Ok(Step::Eval(first))
            }
        }
    }

    /// Evaluates the operands of `node` from `next` on, left to right, and
    /// then `node` itself.
    fn operands(
        &mut self,
        node: &'a Expr,
        next: usize,
        mut values: Vec<RuntimeValue>,
    ) -> Result<Step<'a>, Halt> {
        let count = operand_count(node);
        let mut next = next;
        while next < count {
            let operand = operand_at(node, next).ok_or(Halt::Invalid)?;
            if matches!(operand, Expr::Default { .. }) {
                let default = self.default_argument(node, next, &values)?;
                values.push(default);
            } else if let Some(value) = self.leaf(operand)? {
                values.push(value);
            } else {
                self.push(Kont::Operands {
                    node,
                    next: next + 1,
                    values,
                })?;
                return Ok(Step::Eval(operand));
            }
            next += 1;
        }
        self.finish(node, values)
    }

    /// The value of an omitted labelled argument: the default its callee's
    /// signature declares, resolved at run time from the evaluated callee.
    fn default_argument(
        &self,
        node: &'a Expr,
        operand: usize,
        values: &[RuntimeValue],
    ) -> Result<RuntimeValue, Halt> {
        let Expr::Call {
            target, arguments, ..
        } = node
        else {
            return Err(Halt::Invalid);
        };
        let program = self.program;
        let (signature, argument_index) = match target {
            CallTarget::Direct(function) => (
                program
                    .functions()
                    .get(*function)
                    .ok_or(Halt::Invalid)?
                    .signature(),
                operand,
            ),
            CallTarget::Indirect { .. } => match values.first() {
                Some(RuntimeValue::Function(callable)) => (
                    callable.signature(),
                    operand.checked_sub(1).ok_or(Halt::Invalid)?,
                ),
                _ => return Err(Halt::Invalid),
            },
            CallTarget::Contract { .. } => return Err(Halt::Invalid),
        };
        let argument = arguments.get(argument_index).ok_or(Halt::Invalid)?;
        let positional = signature.parameters().len();
        let parameter = signature
            .labelled()
            .get(
                argument_index
                    .checked_sub(positional)
                    .ok_or(Halt::Invalid)?,
            )
            .ok_or(Halt::Invalid)?;
        let default = parameter.default().ok_or(Halt::Invalid)?.clone();
        let atom_default =
            matches!(default, Value::Atom(_)) && argument.result_type() == Type::Atom;
        if !atom_default && !default.ty().same_shape(&argument.result_type()) {
            return Err(Halt::Invalid);
        }
        Ok(RuntimeValue::Primitive(default))
    }

    /// Builds the value of `node` once its operands are evaluated.
    fn finish(
        &mut self,
        node: &'a Expr,
        values: Vec<RuntimeValue>,
    ) -> Result<Step<'a>, Halt> {
        match node {
            Expr::External {
                intrinsic, result, ..
            } => {
                let result = self.concrete(result);
                self.external(*intrinsic, values, &result)
            }
            Expr::Tuple { value_type, .. } => {
                let value = RuntimeValue::Tuple {
                    value_type: self.concrete(value_type),
                    values: values.into(),
                };
                self.fresh(value)
            }
            Expr::Array { value_type, .. } => {
                let value = RuntimeValue::Array {
                    value_type: self.concrete(value_type),
                    values: values.into(),
                };
                self.fresh(value)
            }
            Expr::Record {
                value_type, fields, ..
            } => {
                let value_type = self.concrete(value_type);
                self.record(&value_type, fields, values)
            }
            Expr::Closure { .. } => self.closure(node, values),
            Expr::Dict { .. } => self.dict(node, values),
            Expr::Lookup { .. } => self.lookup_entry(node, values),
            Expr::Call { .. } => self.call(node, values),
            _ => Err(Halt::Invalid),
        }
    }

    /// Builds the value of a node with one operand, now evaluated.
    fn unary(&mut self, node: &'a Expr, value: RuntimeValue) -> Result<Step<'a>, Halt> {
        match node {
            Expr::Variant { .. } => self.variant(node, Some(value)),
            Expr::Wrap { value_type, .. } => {
                // `str` and `bytes` are represented directly over their items.
                if let Some(represented) = representation_of(value_type, &value) {
                    return self.fresh(represented);
                }
                let wrapper = RuntimeValue::Wrapper {
                    value_type: self.concrete(value_type),
                    value: value.into(),
                };
                self.fresh(wrapper)
            }
            Expr::Widen {
                value_type,
                value: operand,
                member,
                ..
            } => {
                let member_type = self.concrete(&operand.result_type());
                match member {
                    // Atom and interface widening are erased: a value keeps
                    // its own type, which selects its implementations.
                    None => Ok(Step::Ret(value)),
                    Some(member) => {
                        let union = RuntimeValue::Union {
                            value_type: self.concrete(value_type),
                            member: *member,
                            member_type: Rc::new(member_type),
                            value: value.into(),
                        };
                        self.fresh(union)
                    }
                }
            }
            Expr::Try { exit_type, .. } => {
                // `try`: the payload of `some` or `ok`, or an early exit that
                // rebuilds `none` or `err` at the enclosing result type and
                // leaves the innermost function or `lambda`.
                let exit_type = self.concrete(exit_type);
                let RuntimeValue::Enum {
                    variant, payload, ..
                } = value
                else {
                    return Err(Halt::Invalid);
                };
                match variant.as_str() {
                    // A nullary success carries the `void` value.
                    "some" | "ok" => Ok(Step::Ret(payload.map_or(
                        RuntimeValue::Primitive(Value::Void),
                        Inner::into_value,
                    ))),
                    "none" | "err" => {
                        let early = RuntimeValue::Enum {
                            value_type: exit_type,
                            variant,
                            payload,
                        };
                        self.exit(early)
                    }
                    _ => Err(Halt::Invalid),
                }
            }
            Expr::Return { .. } => self.exit(value),
            Expr::Project { field, .. } => {
                let RuntimeValue::Record { fields, .. } = &value else {
                    return Err(Halt::Invalid);
                };
                fields
                    .iter()
                    .find(|(name, _)| name == field)
                    .map(|(_, field)| Step::Ret(field.clone()))
                    .ok_or(Halt::Invalid)
            }
            Expr::TupleProject { index, .. } => {
                let RuntimeValue::Tuple { values, .. } = &value else {
                    return Err(Halt::Invalid);
                };
                values
                    .get(*index)
                    .map(|component| Step::Ret(component.clone()))
                    .ok_or(Halt::Invalid)
            }
            _ => Err(Halt::Invalid),
        }
    }

    /// An enum variant with its payload, when it has one.
    fn variant(
        &mut self,
        node: &'a Expr,
        payload: Option<RuntimeValue>,
    ) -> Result<Step<'a>, Halt> {
        let Expr::Variant {
            value_type,
            variant,
            ..
        } = node
        else {
            return Err(Halt::Invalid);
        };
        // A `void` payload is no payload: a generic slot instantiated to
        // `void` builds the same nullary value a written one does.
        let value = RuntimeValue::Enum {
            value_type: self.concrete(value_type),
            variant: variant.clone(),
            payload: payload.and_then(present_payload),
        };
        self.fresh(value)
    }

    /// A record, with its fields evaluated in their checked order and stored
    /// in the order of the record type.
    fn record(
        &mut self,
        value_type: &Type,
        fields: &'a [(String, Expr)],
        values: Vec<RuntimeValue>,
    ) -> Result<Step<'a>, Halt> {
        let mut named: Vec<(String, RuntimeValue)> = fields
            .iter()
            .map(|(name, _)| name.clone())
            .zip(values)
            .collect();
        let order: Vec<&str> = match value_type {
            Type::Record(members) => {
                members.iter().map(|(name, _)| name.as_str()).collect()
            }
            Type::Declared(id) | Type::Applied(id, _) => self
                .program
                .types()
                .iter()
                .find(|definition| definition.id() == id)
                .and_then(|definition| definition.record_fields())
                .ok_or(Halt::Invalid)?
                .iter()
                .map(|(name, _)| name.as_str())
                .collect(),
            _ => return Err(Halt::Invalid),
        };
        named.sort_by_key(|(name, _)| order.iter().position(|member| member == name));
        let record = RuntimeValue::Record {
            value_type: value_type.clone(),
            fields: named.into(),
        };
        self.fresh(record)
    }

    /// A `lambda` over the values its captures evaluated to.
    fn closure(
        &mut self,
        node: &'a Expr,
        environment: Vec<RuntimeValue>,
    ) -> Result<Step<'a>, Halt> {
        let Expr::Closure {
            signature,
            body,
            slot_count,
            ..
        } = node
        else {
            return Err(Halt::Invalid);
        };
        // The closure runs at the type arguments of the activation that made
        // it; its own generic parameters are fixed by each call.
        let types = self.current_types();
        let signature = if types.is_empty() {
            signature.clone()
        } else {
            signature.substitute(&types)
        };
        let body = self.intern(body);
        let closure = RuntimeValue::Function(Callable::Lambda {
            signature: Arc::new(signature),
            slot_count: *slot_count,
            body,
            captures: environment.into(),
            types,
        });
        self.fresh(closure)
    }

    /// The id of a closure body, registered the first time it is seen.
    fn intern(&mut self, body: &'a Arc<Expr>) -> LambdaId {
        let address = Arc::as_ptr(body).addr();
        if let Some(id) = self.lambda_ids.get(&address) {
            return LambdaId(*id);
        }
        let id = self.lambdas.len();
        self.lambdas.push(&**body);
        self.lambda_ids.insert(address, id);
        LambdaId(id)
    }

    fn lambda_body(&self, id: LambdaId) -> Result<&'a Expr, Halt> {
        self.lambdas.get(id.0).copied().ok_or(Halt::Invalid)
    }

    // -----------------------------------------------------------------
    // Module values.
    // -----------------------------------------------------------------

    /// A module value: read once initialized, otherwise its initializer runs
    /// as an activation of its own with no type arguments.
    fn global(&mut self, node: &'a Expr, index: usize) -> Result<Step<'a>, Halt> {
        let Expr::Global { value_type, .. } = node else {
            return Err(Halt::Invalid);
        };
        let ready = match self.globals.get(index).ok_or(Halt::Invalid)? {
            GlobalState::Ready(value) => Some(value.clone()),
            GlobalState::Evaluating => return Err(Halt::Invalid),
            GlobalState::Uninitialized => None,
        };
        if let Some(value) = ready {
            if !admits_value(value_type, &value) {
                return Err(Halt::Invalid);
            }
            return Ok(Step::Ret(value));
        }
        *self.globals.get_mut(index).ok_or(Halt::Invalid)? = GlobalState::Evaluating;
        let program = self.program;
        let global = program.globals().get(index).ok_or(Halt::Invalid)?;
        self.push(Kont::Global { node, index })?;
        self.push_frame(
            Frame {
                slots: vec![None; global.slot_count()],
                captures: self.no_captures.clone(),
                types: Arc::clone(&self.no_types),
                initializer: Some(index),
            },
            false,
        )?;
        Ok(Step::Eval(global.initializer()))
    }

    // -----------------------------------------------------------------
    // Collections and compiler intrinsics.
    // -----------------------------------------------------------------

    /// A `dict`: its entries inserted in canonical key order, where a repeated
    /// key keeps its first position and takes the later value.
    fn dict(
        &mut self,
        node: &'a Expr,
        values: Vec<RuntimeValue>,
    ) -> Result<Step<'a>, Halt> {
        let Expr::Dict {
            value_type,
            key_order,
            ..
        } = node
        else {
            return Err(Halt::Invalid);
        };
        let mut pairs = Vec::with_capacity(values.len() / 2);
        let mut operands = values.into_iter();
        while let (Some(key), Some(value)) = (operands.next(), operands.next()) {
            pairs.push((key, value));
        }
        let value_type = self.concrete(value_type);
        self.start_build(pairs, key_order.clone(), value_type)
    }

    /// A lookup, which never traps and answers with the standard `option`.
    fn lookup_entry(
        &mut self,
        node: &'a Expr,
        values: Vec<RuntimeValue>,
    ) -> Result<Step<'a>, Halt> {
        let Expr::Lookup {
            value_type,
            key_order,
            ..
        } = node
        else {
            return Err(Halt::Invalid);
        };
        let mut operands = values.into_iter();
        let (Some(collection), Some(key), None) =
            (operands.next(), operands.next(), operands.next())
        else {
            return Err(Halt::Invalid);
        };
        if let RuntimeValue::Dict { entries, .. } = &collection {
            let value_type = self.concrete(value_type);
            return self.start_lookup(
                entries.clone(),
                key,
                key_order.clone(),
                value_type,
            );
        }
        let found = lookup(&collection, &key);
        let entry = RuntimeValue::Enum {
            value_type: self.concrete(value_type),
            variant: if found.is_some() { "some" } else { "none" }.to_owned(),
            payload: found.and_then(present_payload),
        };
        self.fresh(entry)
    }

    /// The `ordered` interface of `@std.core`, when the program holds an
    /// implementation of it.
    fn ordered_interface(&self) -> Option<TypeId> {
        self.program.functions().iter().find_map(|function| {
            function
                .implements()
                .filter(|implements| {
                    implements.member == "compare"
                        && implements.interface.path() == "std.core.ordered"
                })
                .map(|implements| implements.interface.clone())
        })
    }

    /// The `compare` implementation of the `ordered` interface `interface`
    /// whose receiver is the declared type of `value`.
    fn key_compare_function(
        &self,
        interface: &TypeId,
        value: &RuntimeValue,
    ) -> Option<usize> {
        let value_type = match value {
            RuntimeValue::Record { value_type, .. }
            | RuntimeValue::Enum { value_type, .. }
            | RuntimeValue::Wrapper { value_type, .. }
            | RuntimeValue::Tuple { value_type, .. }
            | RuntimeValue::Union { value_type, .. } => value_type,
            _ => return None,
        };
        if !matches!(value_type, Type::Declared(_) | Type::Applied(_, _)) {
            return None;
        }
        self.program.functions().iter().position(|function| {
            function.implements().is_some_and(|implements| {
                implements.interface == *interface
                    && implements.member == "compare"
                    && !matches!(implements.receiver, Type::Param(_))
                    && admits_value(&implements.receiver, value)
            })
        })
    }

    /// The callable of module function `function`, whose signature is shared
    /// by every callable of it.
    fn named_callable(&self, function: usize) -> Option<Callable> {
        Some(Callable::Named {
            index: function,
            signature: Arc::clone(&self.info(function)?.signature),
            captures: self.no_captures.clone(),
            types: Arc::clone(&self.no_types),
        })
    }

    /// What a call checks of module function `function`'s signature, worked
    /// out the first time it is called.
    fn info(&self, function: usize) -> Option<Rc<SignatureInfo>> {
        let mut signatures = self.signatures.borrow_mut();
        let slot = signatures.get_mut(function)?;
        if slot.is_none() {
            let signature =
                Arc::new(self.program.functions().get(function)?.signature().clone());
            *slot = Some(Rc::new(SignatureInfo::new(signature)));
        }
        slot.clone()
    }

    /// What a call checks of `callable`'s signature.
    fn call_info(&self, callable: &Callable) -> Option<Rc<SignatureInfo>> {
        match callable {
            Callable::Named { index, .. } => self.info(*index),
            Callable::Lambda { signature, .. } => {
                Some(Rc::new(SignatureInfo::new(Arc::clone(signature))))
            }
        }
    }

    /// The implementation member `function` for `receiver`, at the type
    /// arguments the receiver's own type fixes.
    fn implementation_callable(
        &self,
        function: usize,
        receiver: &RuntimeValue,
    ) -> Option<Callable> {
        let mut callable = self.named_callable(function)?;
        if let Some(implements) = self.program.functions().get(function)?.implements() {
            let mut bound = TypeMap::new();
            bind_type(&implements.receiver, &runtime_type(receiver), &mut bound);
            *callable.types_mut() = Arc::new(bound);
        }
        Some(callable)
    }

    /// A call of a compiler intrinsic on evaluated operands.
    fn external(
        &mut self,
        intrinsic: CompilerIntrinsic,
        values: Vec<RuntimeValue>,
        result: &Type,
    ) -> Result<Step<'a>, Halt> {
        match intrinsic {
            // The packed variadic tail is already the built collection.
            CompilerIntrinsic::ArrayOf | CompilerIntrinsic::DictOf => {
                let mut values = values;
                return match (values.pop(), values.is_empty()) {
                    (Some(tail), true) => Ok(Step::Ret(tail)),
                    _ => Err(Halt::Invalid),
                };
            }
            CompilerIntrinsic::ArrayFold => return self.fold_start(values, result),
            _ => {}
        }
        let value = match (intrinsic, values.as_slice()) {
            (
                CompilerIntrinsic::TextConcat,
                [
                    RuntimeValue::Primitive(Value::Str(left)),
                    RuntimeValue::Primitive(Value::Str(right)),
                ],
            ) => RuntimeValue::Primitive(Value::Str(format!("{left}{right}"))),
            (
                CompilerIntrinsic::TextLength,
                [RuntimeValue::Primitive(Value::Str(value))],
            ) => RuntimeValue::Primitive(Value::U64(value.chars().count() as u64)),
            // A dict keeps its entries in key order already.
            (
                CompilerIntrinsic::DictEntries,
                [
                    RuntimeValue::Dict {
                        value_type: Type::Dict(key, value),
                        entries,
                    },
                ],
            ) => {
                let entry_type =
                    Type::Tuple(vec![key.as_ref().clone(), value.as_ref().clone()]);
                RuntimeValue::Array {
                    value_type: Type::Array(Box::new(entry_type.clone())),
                    values: entries
                        .iter()
                        .map(|(key, value)| RuntimeValue::Tuple {
                            value_type: entry_type.clone(),
                            values: vec![key.clone(), value.clone()].into(),
                        })
                        .collect::<Vec<_>>()
                        .into(),
                }
            }
            (CompilerIntrinsic::ArrayLength, [RuntimeValue::Array { values, .. }]) => {
                RuntimeValue::Primitive(Value::U64(values.len() as u64))
            }
            (
                CompilerIntrinsic::ArrayAppend,
                [RuntimeValue::Array { value_type, values }, element],
            ) => {
                let mut values = Vec::clone(values);
                values.push(element.clone());
                RuntimeValue::Array {
                    value_type: value_type.clone(),
                    values: values.into(),
                }
            }
            (
                CompilerIntrinsic::ArrayConcat,
                [
                    RuntimeValue::Array { value_type, values },
                    RuntimeValue::Array { values: right, .. },
                ],
            ) => {
                let mut values = Vec::clone(values);
                values.extend(right.iter().cloned());
                RuntimeValue::Array {
                    value_type: value_type.clone(),
                    values: values.into(),
                }
            }
            (
                CompilerIntrinsic::ArraySlice,
                [
                    RuntimeValue::Array { value_type, values },
                    RuntimeValue::Primitive(Value::U64(start)),
                    RuntimeValue::Primitive(Value::U64(end)),
                ],
            ) => {
                let range = usize::try_from(*start)
                    .ok()
                    .zip(usize::try_from(*end).ok())
                    .filter(|(start, end)| start <= end && *end <= values.len());
                let slice = range.and_then(|(start, end)| values.get(start..end));
                RuntimeValue::Enum {
                    // The checked result is the type playing `@option`.
                    value_type: result.clone(),
                    variant: if slice.is_some() { "some" } else { "none" }.to_owned(),
                    payload: slice.map(|slice| {
                        Inner::new(RuntimeValue::Array {
                            value_type: value_type.clone(),
                            values: slice.to_vec().into(),
                        })
                    }),
                }
            }
            _ => registry::apply(intrinsic, &values, result).ok_or(Halt::Invalid)?,
        };
        self.fresh(value)
    }

    /// `array.fold`: the first step call, with the rest left to a
    /// continuation.
    fn fold_start(
        &mut self,
        values: Vec<RuntimeValue>,
        result: &Type,
    ) -> Result<Step<'a>, Halt> {
        let mut operands = values.into_iter();
        let (Some(array), Some(initial), Some(step), None) = (
            operands.next(),
            operands.next(),
            operands.next(),
            operands.next(),
        ) else {
            return Err(Halt::Invalid);
        };
        let RuntimeValue::Array { values: items, .. } = &array else {
            return Err(Halt::Invalid);
        };
        let step = step.into_callable().ok_or(Halt::Invalid)?;
        let state = Box::new(FoldState {
            items: items.clone(),
            next: 0,
            step,
            result: result.clone(),
        });
        self.fold_next(state, initial)
    }

    /// Calls the step of a fold with the accumulator so far and the next
    /// element, or finishes with the accumulator when none remain.
    fn fold_next(
        &mut self,
        mut state: Box<FoldState>,
        accumulator: RuntimeValue,
    ) -> Result<Step<'a>, Halt> {
        let Some(element) = state.items.get(state.next).cloned() else {
            return Ok(Step::Ret(accumulator));
        };
        state.next += 1;
        let callable = state.step.clone();
        let result = state.result.clone();
        self.push(Kont::Fold(state))?;
        self.invoke(callable, vec![accumulator, element], &result)
    }

    // -----------------------------------------------------------------
    // Calls.
    // -----------------------------------------------------------------

    /// Extends `callable`'s type arguments with the ones this call fixes: its
    /// signature against the instantiated types of the operands and result.
    /// A parameter typed as an interface value takes the type its operand
    /// holds, which is what the callee's own contract calls dispatch on.
    fn bind_call(
        &self,
        callable: &mut Callable,
        arguments: &[Expr],
        values: &[RuntimeValue],
        result: &Type,
    ) {
        let Some(info) = self.call_info(callable) else {
            return;
        };
        if !info.generic {
            return;
        }
        let mut bound = TypeMap::clone(callable.types_mut());
        for (index, pattern) in info.slots.iter().enumerate() {
            let Some(argument) = arguments.get(index) else {
                continue;
            };
            let written = self.concrete(&argument.result_type());
            let actual = match (pattern, &written, values.get(index)) {
                (Type::Param(_), Type::Interface(_, _) | Type::Any, Some(value)) => {
                    runtime_type(value)
                }
                _ => written,
            };
            bind_type(pattern, &actual, &mut bound);
        }
        bind_type(&info.result, &self.concrete(result), &mut bound);
        *callable.types_mut() = Arc::new(bound);
    }

    /// A call whose operands are evaluated.
    fn call(
        &mut self,
        node: &'a Expr,
        values: Vec<RuntimeValue>,
    ) -> Result<Step<'a>, Halt> {
        let Expr::Call {
            target,
            arguments,
            result,
            tail,
            origin,
        } = node
        else {
            return Err(Halt::Invalid);
        };
        // A contract call selects its implementation from the receiver's
        // runtime type, after every operand is evaluated.
        if let CallTarget::Contract { .. } = target {
            return self.contract_call(target, arguments, result, *tail, values);
        }
        let mut values = values;
        let mut callable = match target {
            CallTarget::Direct(function) => {
                self.named_callable(*function).ok_or(Halt::Invalid)?
            }
            CallTarget::Indirect { .. } => {
                if values.is_empty() {
                    return Err(Halt::Invalid);
                }
                let callee = values.remove(0);
                callee.into_callable().ok_or(Halt::Invalid)?
            }
            CallTarget::Contract { .. } => return Err(Halt::Invalid),
        };
        self.bind_call(&mut callable, arguments, &values, result);
        if let Callable::Named { index, .. } = &callable
            && let Some(assertion) = self
                .program
                .functions()
                .get(*index)
                .and_then(|function| function.test_assertion())
        {
            if !self.test_mode {
                return Err(Halt::Invalid);
            }
            let value = self
                .invoke_test_assertion(assertion, values, origin.clone())
                .ok_or(Halt::Invalid)?;
            return if self.assertion_failure.is_some() {
                Err(Halt::Assertion)
            } else {
                Ok(Step::Ret(value))
            };
        }
        self.dispatch(callable, values, result, *tail)
    }

    /// A call of a contract member, dispatched from the runtime type of its
    /// receiver.
    fn contract_call(
        &mut self,
        target: &'a CallTarget,
        arguments: &'a [Expr],
        result: &Type,
        tail: bool,
        values: Vec<RuntimeValue>,
    ) -> Result<Step<'a>, Halt> {
        let CallTarget::Contract {
            interface,
            member,
            receiver,
            arguments: interface_arguments,
            member_types,
            destination,
            closed,
            ..
        } = target
        else {
            return Err(Halt::Invalid);
        };
        // The implementation is the one whose receiver and interface
        // arguments both match, at one binding of its own parameters: a
        // receiver may implement a generic interface more than once.
        // A member selected by its destination dispatches on that type, at
        // this activation's type arguments.
        let receiver_type = match destination {
            Some(destination) => self.concrete(destination),
            None => runtime_type(values.get(*receiver).ok_or(Halt::Invalid)?),
        };
        let interface_arguments = interface_arguments
            .iter()
            .map(|argument| self.concrete(argument))
            .collect::<Vec<_>>();
        // A written member for the receiver's type wins over the interface's
        // default, whose receiver is the open `self`.
        let candidates = self
            .program
            .functions()
            .iter()
            .enumerate()
            .filter_map(|(index, function)| {
                let implements = function.implements()?;
                if implements.interface != *interface || implements.member != *member {
                    return None;
                }
                let mut bound = TypeMap::new();
                let matches =
                    bind_type(&implements.receiver, &receiver_type, &mut bound)
                        && (interface_arguments.is_empty()
                            || (implements.arguments.len()
                                == interface_arguments.len()
                                && implements
                                    .arguments
                                    .iter()
                                    .zip(&interface_arguments)
                                    .all(|(pattern, actual)| {
                                        bind_type(pattern, actual, &mut bound)
                                    })));
                matches.then_some((
                    index,
                    matches!(implements.receiver, Type::Param(_)),
                    bound,
                ))
            })
            .collect::<Vec<_>>();
        let selected = candidates
            .iter()
            .find(|(_, default, _)| !default)
            .or_else(|| candidates.first())
            .cloned();
        let Some((index, _, bound)) = selected else {
            let closed = (*closed).ok_or(Halt::Invalid)?;
            if closed == ClosedContract::IterNext {
                let [iterator] = values.as_slice() else {
                    return Err(Halt::Invalid);
                };
                let next = closed_next(iterator, &self.concrete(result))
                    .ok_or(Halt::Invalid)?;
                return self.fresh(next);
            }
            let [left, right] = values.as_slice() else {
                return Err(Halt::Invalid);
            };
            // A structure holding a user key orders through its `compare`,
            // and is equal exactly when that says so: the closed `equal` has
            // no other answer for a key it cannot see into.
            let key_order = match closed {
                ClosedContract::KeyCompare => Some(interface.clone()),
                _ => self.ordered_interface(),
            };
            let result = self.concrete(result);
            return self.start_closed(
                left.clone(),
                right.clone(),
                key_order,
                closed,
                result,
            );
        };
        let mut callable = self.named_callable(index).ok_or(Halt::Invalid)?;
        let mut bound = bound;
        // The member's own type arguments at this call, which the selected
        // function names by its own generic parameters, at this activation's
        // type arguments.
        if let Some(implements) = self
            .program
            .functions()
            .get(index)
            .ok_or(Halt::Invalid)?
            .implements()
        {
            for (name, written) in implements.member_generics.iter().zip(member_types) {
                if let Some(name) = name {
                    bound.insert(name.clone(), self.concrete(written));
                }
            }
        }
        *callable.types_mut() = Arc::new(bound);
        // Anything the operands and the result still fix.
        self.bind_call(&mut callable, arguments, &values, result);
        self.dispatch(callable, values, result, tail)
    }

    /// Runs a call whose callable and operands are resolved: a call in tail
    /// position replaces the activation that makes it, and any other call
    /// enters a new activation.
    fn dispatch(
        &mut self,
        callable: Callable,
        values: Vec<RuntimeValue>,
        result: &Type,
        tail: bool,
    ) -> Result<Step<'a>, Halt> {
        let info = self.call_info(&callable).ok_or(Halt::Invalid)?;
        if !tail {
            return self.enter_callee(callable, &info, values, result);
        }
        if info.slots.len() != values.len()
            || !values_match_slots(&values, &info.slots)
            || !result.admits(&info.result)
        {
            return Err(Halt::Invalid);
        }
        self.transfer(callable, values, result)
    }

    /// Replaces the running activation with the callee: every callable that
    /// creates a language activation reuses the current one. A
    /// compiler-intrinsic wrapper creates none, so it is invoked as an
    /// ordinary call.
    fn transfer(
        &mut self,
        callable: Callable,
        values: Vec<RuntimeValue>,
        result: &Type,
    ) -> Result<Step<'a>, Halt> {
        match callable {
            // A named callable carries the signature of its own function, and
            // the caller has checked the operands against it.
            Callable::Named {
                index,
                signature,
                captures,
                types,
            } => {
                let program = self.program;
                let function = program.functions().get(index).ok_or(Halt::Invalid)?;
                if function.is_external_wrapper() {
                    return self.invoke(
                        Callable::Named {
                            index,
                            signature,
                            captures,
                            types,
                        },
                        values,
                        result,
                    );
                }
                let slots = activation_slots(values, function.slot_count());
                self.reuse(slots, captures, types)?;
                Ok(Step::Eval(function.body()))
            }
            Callable::Lambda {
                slot_count,
                body,
                captures,
                types,
                ..
            } => {
                let slots = activation_slots(values, slot_count);
                self.reuse(slots, captures, types)?;
                Ok(Step::Eval(self.lambda_body(body)?))
            }
        }
    }

    /// Makes the callee's slots, captures, and type arguments those of the
    /// running activation, after discarding what the caller still had to do.
    fn reuse(
        &mut self,
        slots: Vec<Option<RuntimeValue>>,
        captures: Elements,
        types: Arc<TypeMap>,
    ) -> Result<(), Halt> {
        self.unwind()?;
        let frame = self.frames.last_mut().ok_or(Halt::Invalid)?;
        // A module value's initializer is not a function body, so it has no
        // activation to hand over.
        if frame.initializer.is_some() {
            return Err(Halt::Invalid);
        }
        let before = Self::frame_bytes(frame.slots.len());
        let after = Self::frame_bytes(slots.len());
        frame.slots = slots;
        frame.captures = captures;
        frame.types = types;
        self.stack_bytes = self
            .stack_bytes
            .saturating_sub(before)
            .saturating_add(after);
        self.tail_transfers = self.tail_transfers.saturating_add(1);
        self.check_budget()
    }

    /// Enters a new activation of `callable`.
    fn invoke(
        &mut self,
        callable: Callable,
        values: Vec<RuntimeValue>,
        result: &Type,
    ) -> Result<Step<'a>, Halt> {
        let info = self.call_info(&callable).ok_or(Halt::Invalid)?;
        self.enter_callee(callable, &info, values, result)
    }

    /// Enters a new activation of `callable`, whose signature is `info`.
    fn enter_callee(
        &mut self,
        callable: Callable,
        info: &SignatureInfo,
        values: Vec<RuntimeValue>,
        result: &Type,
    ) -> Result<Step<'a>, Halt> {
        if info.slots.len() != values.len()
            || !values_match_slots(&values, &info.slots)
            || !result.admits(&info.result)
        {
            return Err(Halt::Invalid);
        }
        match callable {
            Callable::Named {
                index,
                captures,
                types,
                ..
            } => {
                let program = self.program;
                let function = program.functions().get(index).ok_or(Halt::Invalid)?;
                let slots = activation_slots(values, function.slot_count());
                self.enter(slots, captures, types)?;
                Ok(Step::Eval(function.body()))
            }
            Callable::Lambda {
                slot_count,
                body,
                captures,
                types,
                ..
            } => {
                let slots = activation_slots(values, slot_count);
                self.enter(slots, captures, types)?;
                Ok(Step::Eval(self.lambda_body(body)?))
            }
        }
    }

    fn invoke_test_assertion(
        &mut self,
        assertion: TestAssertion,
        values: Vec<RuntimeValue>,
        origin: SourceOrigin,
    ) -> Option<RuntimeValue> {
        // Every operand is compared and reported by its canonical encoding.
        // The checker rejects an operand whose type names a function; one
        // hidden behind `any` or an interface stops the test here.
        let Some(encodings) = values
            .into_iter()
            .map(|value| observe(value).map(|value| value.canonical_vibon()))
            .collect::<Option<Vec<_>>>()
        else {
            self.unobservable = Some(origin);
            return None;
        };
        let (passed, expected, actual) = match (assertion, encodings.as_slice()) {
            (TestAssertion::True | TestAssertion::False, [actual]) => {
                let expected =
                    Value::Bool(assertion == TestAssertion::True).canonical_vibon();
                (*actual == expected, expected, actual.clone())
            }
            (TestAssertion::Equal, [expected, actual]) => {
                (expected == actual, expected.clone(), actual.clone())
            }
            _ => return None,
        };
        if !passed {
            self.assertion_failure = Some(TestAssertionFailure {
                assertion: assertion.symbol(),
                expected,
                actual,
                origin,
            });
        }
        Some(RuntimeValue::Primitive(Value::Void))
    }
}

/// How many operands `node` evaluates before it is built.
fn operand_count(node: &Expr) -> usize {
    match node {
        Expr::External { arguments, .. } => arguments.len(),
        Expr::Tuple { components, .. } => components.len(),
        Expr::Array { elements, .. } => elements.len(),
        Expr::Record { fields, .. } => fields.len(),
        Expr::Closure { captures, .. } => captures.len(),
        // Each entry is a key and then a value.
        Expr::Dict { entries, .. } => entries.len().saturating_mul(2),
        Expr::Lookup { .. } => 2,
        // An indirect call evaluates its callee first, once.
        Expr::Call {
            target, arguments, ..
        } => arguments
            .len()
            .saturating_add(usize::from(matches!(target, CallTarget::Indirect { .. }))),
        _ => 0,
    }
}

/// The operand of `node` at `index`, in evaluation order.
fn operand_at(node: &Expr, index: usize) -> Option<&Expr> {
    match node {
        Expr::External { arguments, .. } => arguments.get(index),
        Expr::Tuple { components, .. } => components.get(index),
        Expr::Array { elements, .. } => elements.get(index),
        Expr::Record { fields, .. } => fields.get(index).map(|(_, field)| field),
        Expr::Closure { captures, .. } => captures.get(index),
        Expr::Dict { entries, .. } => {
            let (key, value) = entries.get(index / 2)?;
            Some(if index.is_multiple_of(2) { key } else { value })
        }
        Expr::Lookup {
            collection, key, ..
        } => match index {
            0 => Some(collection),
            1 => Some(key),
            _ => None,
        },
        Expr::Call {
            target, arguments, ..
        } => match target {
            CallTarget::Indirect { callee, .. } => match index {
                0 => Some(callee),
                _ => arguments.get(index - 1),
            },
            _ => arguments.get(index),
        },
        _ => None,
    }
}
