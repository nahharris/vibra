//! Deterministic execution for the checked M2 function and binding IR.
//!
//! No parser, resolver, type checker, filesystem, clock, random source, or
//! host provider is reachable from this crate.  The only executable input is
//! [`vibra_ir::CheckedProgram`], which is the controlled boundary produced by
//! `vibra-types`.

#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used
    )
)]

mod registry;

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use vibra_ir::{
    CallTarget, CheckedProgram, ClosedContract, Expr, FunctionSignature, MatchArm,
    ObservedValue, Pattern, SourceOrigin, TestAssertion, Type, TypeId, Value,
};

/// One successful reference-interpreter run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Execution {
    /// The entry's value when it is a primitive. A compound value stays on
    /// the interpreter thread, where it is encoded and released: both walk
    /// its whole depth.
    value: Option<Value>,
    /// The canonical observation, encoded on the interpreter thread: the
    /// encoder recurses through the value, which may be deeper than a
    /// caller's stack allows.
    canonical: String,
    audit_trace: Vec<String>,
    max_activation_depth: usize,
    tail_transfer_count: usize,
}

impl Execution {
    /// The value returned by the selected entry function, when it is a
    /// primitive; [`Self::canonical_result`] observes every value.
    #[must_use]
    pub const fn value(&self) -> Option<&Value> {
        self.value.as_ref()
    }

    /// Ordered audit events.  Pure M2 literals always return an empty trace.
    #[must_use]
    pub fn audit_trace(&self) -> &[String] {
        &self.audit_trace
    }

    /// Maximum number of active language frames observed during execution.
    /// This is host-side instrumentation and is not part of a Vibra result.
    #[must_use]
    pub const fn max_activation_depth(&self) -> usize {
        self.max_activation_depth
    }

    /// Number of explicit tail transfers performed during execution.
    /// This is host-side instrumentation and is not part of a Vibra result.
    #[must_use]
    pub const fn tail_transfer_count(&self) -> usize {
        self.tail_transfer_count
    }

    /// Canonical typed value observation.
    #[must_use]
    pub fn canonical_result(&self) -> String {
        self.canonical.clone()
    }
}

/// One completed test body and its optional non-exception assertion failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestExecution {
    assertion_failure: Option<TestAssertionFailure>,
    audit_trace: Vec<String>,
    max_activation_depth: usize,
    tail_transfer_count: usize,
}

impl TestExecution {
    /// Structured failure from the first false assertion, if any.
    #[must_use]
    pub const fn assertion_failure(&self) -> Option<&TestAssertionFailure> {
        self.assertion_failure.as_ref()
    }

    /// Ordered audit events. Pure M2 test execution has an empty trace.
    #[must_use]
    pub fn audit_trace(&self) -> &[String] {
        &self.audit_trace
    }

    /// Maximum active language frames observed during the test.
    #[must_use]
    pub const fn max_activation_depth(&self) -> usize {
        self.max_activation_depth
    }

    /// Number of tail transfers during the test.
    #[must_use]
    pub const fn tail_transfer_count(&self) -> usize {
        self.tail_transfer_count
    }
}

/// Data from a false assertion, kept outside the language value and trap paths.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestAssertionFailure {
    assertion: String,
    expected: String,
    actual: String,
    origin: SourceOrigin,
}

impl TestAssertionFailure {
    /// Canonical assertion member identity.
    #[must_use]
    pub fn assertion(&self) -> &str {
        &self.assertion
    }

    /// The canonical encoding of the expected operand or required boolean.
    #[must_use]
    pub fn expected(&self) -> &str {
        &self.expected
    }

    /// The canonical encoding of the actual operand.
    #[must_use]
    pub fn actual(&self) -> &str {
        &self.actual
    }

    /// Source origin of the assertion call.
    #[must_use]
    pub const fn origin(&self) -> &SourceOrigin {
        &self.origin
    }
}

/// A failure at the checked-program execution boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeError {
    /// The checked program contained no executable entry.
    NoEntry,
    /// A checked IR invariant was violated before an expression ran.
    InvalidBody {
        /// The function that contained the invalid body.
        function: String,
    },
    /// Non-tail activations reached [`MAX_ACTIVATION_DEPTH`]. This is a host
    /// event, not a trap or a portable language result.
    HostStackExhausted {
        /// The activation bound that was reached.
        limit: usize,
    },
    /// A value holding a function reached an observation: a test assertion's
    /// operand or the entry's result. The checker rejects a type that names a
    /// function there; this is one hidden behind `any` or an interface.
    UnobservableFunction {
        /// The observing call, when it is in source.
        origin: Option<SourceOrigin>,
    },
    /// The host could not start the interpreter thread.
    HostThreadUnavailable(String),
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoEntry => {
                formatter.write_str("checked program has no entry function")
            }
            Self::InvalidBody { function } => {
                write!(formatter, "checked body for `{function}` is invalid")
            }
            Self::HostStackExhausted { limit } => write!(
                formatter,
                "non-tail activations exhausted the interpreter host budget of {limit}"
            ),
            Self::UnobservableFunction { .. } => formatter.write_str(
                "a value that holds a function has no canonical encoding to observe",
            ),
            Self::HostThreadUnavailable(error) => {
                write!(formatter, "cannot start the interpreter thread: {error}")
            }
        }
    }
}

impl std::error::Error for RuntimeError {}

impl RuntimeError {
    /// Whether the host, rather than the checked program, stopped execution.
    #[must_use]
    pub const fn is_host_event(&self) -> bool {
        matches!(
            self,
            Self::HostStackExhausted { .. } | Self::HostThreadUnavailable(_)
        )
    }

    /// The stable trap code and source origin, if this failure is a trap
    /// rather than a host event.
    #[must_use]
    pub const fn program_trap(
        &self,
    ) -> Option<(vibra_diagnostics::DiagnosticCode, Option<&SourceOrigin>)> {
        match self {
            Self::UnobservableFunction { origin } => Some((
                vibra_diagnostics::DiagnosticCode::RuntimeUnobservableFunction,
                origin.as_ref(),
            )),
            Self::NoEntry | Self::InvalidBody { .. } => Some((
                vibra_diagnostics::DiagnosticCode::RuntimeInvalidCheckedProgram,
                None,
            )),
            Self::HostStackExhausted { .. } | Self::HostThreadUnavailable(_) => None,
        }
    }

    /// The message of this failure's trap diagnostic.
    #[must_use]
    pub fn trap_message(&self) -> String {
        match self {
            Self::NoEntry | Self::InvalidBody { .. } => {
                "checked program violated M2 runtime invariants".to_owned()
            }
            _ => self.to_string(),
        }
    }

    /// The registered unlocated diagnostic for a host-budget stop, if this is
    /// one. Other boundary failures keep their existing reporting.
    #[must_use]
    pub fn host_diagnostic(&self) -> Option<vibra_diagnostics::Diagnostic> {
        matches!(self, Self::HostStackExhausted { .. }).then(|| {
            vibra_diagnostics::Diagnostic::new(
                vibra_diagnostics::DiagnosticCode::RuntimeHostStackExhausted,
                vibra_diagnostics::ByteSpan::empty_at(0),
                self.to_string(),
            )
        })
    }
}

/// Live language activations the reference interpreter admits at once.
///
/// V1 has no portable stack-depth limit (06-runtime); this is the reference
/// interpreter's host budget. Reaching it stops execution with
/// [`RuntimeError::HostStackExhausted`] instead of overflowing the host
/// stack. Calls in tail position reuse their activation and never approach it.
pub const MAX_ACTIVATION_DEPTH: usize = 4096;

/// Host stack reserved for the interpreter thread, independent of the
/// platform's main-thread stack. An unoptimized build uses roughly 14 KiB per
/// simple activation, so [`MAX_ACTIVATION_DEPTH`] fits with a wide margin.
const INTERPRETER_STACK_BYTES: usize = 256 * 1024 * 1024;

/// Stack kept free below the guard. Evaluation checks the guard at every
/// expression, and no path between two checks comes close to this.
const STACK_GUARD_BYTES: usize = 8 * 1024 * 1024;

/// The pure M2 reference interpreter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Interpreter;

impl Interpreter {
    /// Executes the checked program's validated entry function.
    pub fn run(program: &CheckedProgram) -> Result<Execution, RuntimeError> {
        on_interpreter_thread(|| Self::run_on_current_thread(program))
    }

    fn run_on_current_thread(
        program: &CheckedProgram,
    ) -> Result<Execution, RuntimeError> {
        let function = program.entry();
        let invalid = || RuntimeError::InvalidBody {
            function: function.name().to_owned(),
        };
        if function.test_assertion().is_some() {
            return Err(invalid());
        }
        let mut machine = Machine::new(program, false);
        let value = machine.evaluate_function(
            program.entry_index(),
            entry_frame(program),
            Vec::new(),
            Arc::default(),
        );
        machine.check_host_budget()?;
        let Some(value) = value else {
            return Err(invalid());
        };
        let value_type = function.signature().result();
        if !admits_value(&value_type, &value) {
            return Err(invalid());
        }
        let Some(value) = observe(value) else {
            return Err(RuntimeError::UnobservableFunction { origin: None });
        };
        let canonical = value.canonical_observation(&value_type);
        let value = match value {
            ObservedValue::Primitive(value) => Some(value),
            _ => None,
        };
        Ok(Execution {
            value,
            canonical,
            audit_trace: Vec::new(),
            max_activation_depth: machine.max_depth,
            tail_transfer_count: machine.tail_transfers,
        })
    }

    /// Executes one checked void test entry with the verified assertion path.
    ///
    /// Each call constructs fresh module-value and trace state. A false
    /// assertion is returned as test data and does not become a runtime error.
    pub fn run_test(program: &CheckedProgram) -> Result<TestExecution, RuntimeError> {
        on_interpreter_thread(|| Self::run_test_on_current_thread(program))
    }

    fn run_test_on_current_thread(
        program: &CheckedProgram,
    ) -> Result<TestExecution, RuntimeError> {
        let function = program.entry();
        let invalid = || RuntimeError::InvalidBody {
            function: function.name().to_owned(),
        };
        if function.signature().result() != Type::Void {
            return Err(invalid());
        }
        let mut machine = Machine::new(program, true);
        let value = machine.evaluate_function(
            program.entry_index(),
            entry_frame(program),
            Vec::new(),
            Arc::default(),
        );
        machine.check_host_budget()?;
        if let Some(origin) = machine.unobservable.take() {
            return Err(RuntimeError::UnobservableFunction {
                origin: Some(origin),
            });
        }
        if machine.assertion_failure.is_none()
            && value != Some(RuntimeValue::Primitive(Value::Void))
        {
            return Err(invalid());
        }
        Ok(TestExecution {
            assertion_failure: machine.assertion_failure,
            audit_trace: Vec::new(),
            max_activation_depth: machine.max_depth,
            tail_transfer_count: machine.tail_transfers,
        })
    }
}

/// Runs `body` on a thread with a fixed, known stack so the activation budget
/// does not depend on the embedding process's main-thread stack.
fn on_interpreter_thread<T: Send>(
    body: impl FnOnce() -> Result<T, RuntimeError> + Send,
) -> Result<T, RuntimeError> {
    std::thread::scope(|scope| {
        let handle = std::thread::Builder::new()
            .name("vibra-interp".to_owned())
            .stack_size(INTERPRETER_STACK_BYTES)
            .spawn_scoped(scope, body)
            .map_err(|error| RuntimeError::HostThreadUnavailable(error.to_string()))?;
        handle
            .join()
            .unwrap_or_else(|payload| std::panic::resume_unwind(payload))
    })
}

/// The entry activation: positional slots empty and labelled defaults bound.
fn entry_frame(program: &CheckedProgram) -> Vec<Option<RuntimeValue>> {
    let function = program.entry();
    let signature = function.signature();
    let mut slots = vec![None; function.slot_count()];
    for (offset, parameter) in signature.labelled().iter().enumerate() {
        if let Some(default) = parameter.default()
            && let Some(slot) = slots.get_mut(signature.parameters().len() + offset)
        {
            *slot = Some(RuntimeValue::Primitive(default.clone()));
        }
    }
    slots
}

/// Convenience entry point for the reference interpreter.
pub fn run(program: &CheckedProgram) -> Result<Execution, RuntimeError> {
    Interpreter::run(program)
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
enum GlobalState {
    Uninitialized,
    Evaluating,
    Ready(RuntimeValue),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum RuntimeValue {
    Primitive(Value),
    Function(Callable),
    /// A record; fields are in type order (declaration order when declared,
    /// canonical order when anonymous), whatever the evaluation order.
    Record {
        value_type: Type,
        fields: Vec<(String, RuntimeValue)>,
    },
    Enum {
        value_type: Type,
        variant: String,
        payload: Option<Box<RuntimeValue>>,
    },
    Wrapper {
        value_type: Type,
        value: Box<RuntimeValue>,
    },
    Tuple {
        value_type: Type,
        values: Vec<RuntimeValue>,
    },
    Array {
        value_type: Type,
        values: Vec<RuntimeValue>,
    },
    /// Entries in canonical key order, so no host hash order is reachable.
    Dict {
        value_type: Type,
        entries: Vec<(RuntimeValue, RuntimeValue)>,
    },
    /// A union value: its discriminant, the member type, and the member
    /// value.
    Union {
        value_type: Type,
        member: usize,
        member_type: Type,
        value: Box<RuntimeValue>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Evaluation {
    Value(RuntimeValue),
    TestAssertionFailed,
    TailTransfer {
        callable: Callable,
        values: Vec<RuntimeValue>,
        result: Type,
    },
}

/// The code an activation runs: a module function's body or a `lambda`'s.
enum Code {
    Function(usize),
    Lambda(Arc<Expr>),
}

enum TailTransferAction {
    Reuse {
        code: Code,
        slots: Vec<Option<RuntimeValue>>,
        captures: Vec<RuntimeValue>,
        types: Arc<TypeMap>,
    },
    Invoke {
        callable: Box<Callable>,
        values: Vec<RuntimeValue>,
    },
    Invalid,
}

/// The type arguments of one activation: each generic parameter the running
/// function names, at the type this call instantiated it to.
///
/// Generics are not erased at run time. A value built in generic code carries
/// its instantiated type, and a contract call selects its implementation from
/// instantiated types (`docs/spec/06-runtime.md`, "Generic instantiation").
type TypeMap = BTreeMap<String, Type>;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Callable {
    Named {
        index: usize,
        signature: FunctionSignature,
        captures: Vec<RuntimeValue>,
        /// The type arguments fixed so far: where the value was made, and
        /// then by the call that invokes it.
        types: Arc<TypeMap>,
    },
    Lambda {
        signature: FunctionSignature,
        /// Activation slots, validated by checked IR to cover the body.
        slot_count: usize,
        /// Shared with the checked program; creating a closure never copies it.
        body: Arc<Expr>,
        captures: Vec<RuntimeValue>,
        /// The type arguments of the activation that made the closure, and
        /// then its own from the call that invokes it.
        types: Arc<TypeMap>,
    },
}

impl Callable {
    fn signature(&self) -> &FunctionSignature {
        match self {
            Self::Named { signature, .. } | Self::Lambda { signature, .. } => signature,
        }
    }

    fn types_mut(&mut self) -> &mut Arc<TypeMap> {
        match self {
            Self::Named { types, .. } | Self::Lambda { types, .. } => types,
        }
    }
}

/// Whether `value` names a generic parameter anywhere.
fn mentions_param(value: &Type) -> bool {
    matches!(value, Type::Param(_)) || value.components().iter().any(mentions_param)
}

/// Binds the generic parameters of `pattern` so that it equals `actual`,
/// extending `bound`. A parameter already bound must agree. An unresolved
/// parameter left in `actual` matches anything, as nothing fixed it.
fn bind_type(pattern: &Type, actual: &Type, bound: &mut TypeMap) -> bool {
    if let Type::Param(name) = pattern {
        if matches!(actual, Type::Param(other) if other == name) {
            return true;
        }
        return match bound.get(name) {
            Some(existing) => {
                existing == actual || mentions_param(existing) || mentions_param(actual)
            }
            None => {
                bound.insert(name.clone(), actual.clone());
                // A quantified `lambda` parameter is `name#index@site` in the
                // signature and `name` in the body.
                if let Some((written, _)) = name.split_once('#') {
                    bound.insert(written.to_owned(), actual.clone());
                }
                true
            }
        };
    }
    if matches!(actual, Type::Param(_)) {
        return true;
    }
    let all = |patterns: &[Type], actuals: &[Type], bound: &mut TypeMap| {
        patterns.len() == actuals.len()
            && patterns
                .iter()
                .zip(actuals)
                .all(|(pattern, actual)| bind_type(pattern, actual, bound))
    };
    match (pattern, actual) {
        (Type::Applied(left, patterns), Type::Applied(right, actuals))
        | (Type::Interface(left, patterns), Type::Interface(right, actuals)) => {
            left == right && all(patterns, actuals, bound)
        }
        (Type::Tuple(patterns), Type::Tuple(actuals))
        | (Type::Union(patterns), Type::Union(actuals)) => {
            all(patterns, actuals, bound)
        }
        (Type::Array(pattern), Type::Array(actual)) => {
            bind_type(pattern, actual, bound)
        }
        (Type::Dict(key, value), Type::Dict(actual_key, actual_value)) => {
            bind_type(key, actual_key, bound) && bind_type(value, actual_value, bound)
        }
        (Type::Record(patterns), Type::Record(actuals))
        | (Type::Enum(patterns), Type::Enum(actuals)) => {
            patterns.len() == actuals.len()
                && patterns.iter().zip(actuals).all(
                    |((name, pattern), (actual_name, actual))| {
                        name == actual_name && bind_type(pattern, actual, bound)
                    },
                )
        }
        (Type::Function(pattern), Type::Function(actual)) => {
            let patterns = pattern.slot_types();
            let actuals = actual.slot_types();
            all(&patterns, &actuals, bound)
                && bind_type(&pattern.result(), &actual.result(), bound)
        }
        // An atom literal's own type is one atom of `atom`.
        (Type::Atom, Type::AtomSingleton(_)) => true,
        _ => pattern.admits(actual),
    }
}

/// One activation's slots. Slots are write-once and names never shadow, so
/// one frame is threaded through an activation by reference and a `let`
/// writes its slot in place.
type Frame = [Option<RuntimeValue>];

struct Machine<'a> {
    program: &'a CheckedProgram,
    globals: Vec<GlobalState>,
    current_depth: usize,
    max_depth: usize,
    tail_transfers: usize,
    test_mode: bool,
    host_budget_exhausted: bool,
    /// Address of a local in the frame that created the machine; stacks grow
    /// down on every supported host, so the distance from it is stack use.
    stack_base: usize,
    assertion_failure: Option<TestAssertionFailure>,
    /// The assertion whose operand held a function, which stops the test.
    unobservable: Option<SourceOrigin>,
    /// The value a failing `try` returns from the innermost function or
    /// `lambda`: evaluation unwinds to that boundary, which takes it.
    pending_exit: Option<RuntimeValue>,
    /// A tail transfer that a `return` operand produced, which the innermost
    /// activation performs as its result.
    pending_transfer: Option<Evaluation>,
    /// The type arguments of each live activation, innermost last.
    types: Vec<Arc<TypeMap>>,
}

impl<'a> Machine<'a> {
    fn new(program: &'a CheckedProgram, test_mode: bool) -> Self {
        Self {
            program,
            globals: vec![GlobalState::Uninitialized; program.globals().len()],
            current_depth: 0,
            max_depth: 0,
            tail_transfers: 0,
            test_mode,
            host_budget_exhausted: false,
            stack_base: stack_address(),
            assertion_failure: None,
            unobservable: None,
            pending_exit: None,
            pending_transfer: None,
            types: Vec::new(),
        }
    }

    /// The running activation's type arguments.
    fn current_types(&self) -> Arc<TypeMap> {
        self.types.last().cloned().unwrap_or_default()
    }

    /// `value` at the running activation's type arguments.
    fn concrete(&self, value: &Type) -> Type {
        match self.types.last() {
            Some(types) if !types.is_empty() => value.substitute(types),
            _ => value.clone(),
        }
    }

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
        let signature = callable.signature();
        let slots = signature.slot_types();
        let callee_result = signature.result();
        if !slots.iter().any(mentions_param) && !mentions_param(&callee_result) {
            return;
        }
        let mut bound = TypeMap::clone(callable.types_mut());
        for (index, pattern) in slots.iter().enumerate() {
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
        bind_type(&callee_result, &self.concrete(result), &mut bound);
        *callable.types_mut() = Arc::new(bound);
    }

    /// Records exhaustion when this thread's stack use nears its reservation.
    ///
    /// The activation bound is the deterministic limit; this guard only
    /// catches a single activation whose expression nesting alone is deep
    /// enough to exhaust the host stack.
    fn stack_exhausted(&mut self) -> bool {
        let used = self.stack_base.abs_diff(stack_address());
        if used > INTERPRETER_STACK_BYTES - STACK_GUARD_BYTES {
            self.host_budget_exhausted = true;
        }
        self.host_budget_exhausted
    }

    /// Enters an activation, or records exhaustion and refuses at the bound.
    fn enter_activation(&mut self) -> Option<()> {
        if self.current_depth >= MAX_ACTIVATION_DEPTH {
            self.host_budget_exhausted = true;
            return None;
        }
        self.current_depth += 1;
        self.max_depth = self.max_depth.max(self.current_depth);
        Some(())
    }

    fn leave_activation(&mut self) {
        self.current_depth = self.current_depth.saturating_sub(1);
    }

    fn check_host_budget(&self) -> Result<(), RuntimeError> {
        if self.host_budget_exhausted {
            Err(RuntimeError::HostStackExhausted {
                limit: MAX_ACTIVATION_DEPTH,
            })
        } else {
            Ok(())
        }
    }

    fn evaluate_function(
        &mut self,
        index: usize,
        slots: Vec<Option<RuntimeValue>>,
        captures: Vec<RuntimeValue>,
        types: Arc<TypeMap>,
    ) -> Option<RuntimeValue> {
        self.run_activation(Code::Function(index), slots, captures, types)
    }

    /// Runs one activation. A call in tail position returns here as a
    /// [`Evaluation::TailTransfer`], and the loop reuses this activation for
    /// the callee, whatever it is, instead of entering a new one.
    fn run_activation(
        &mut self,
        code: Code,
        slots: Vec<Option<RuntimeValue>>,
        captures: Vec<RuntimeValue>,
        types: Arc<TypeMap>,
    ) -> Option<RuntimeValue> {
        self.enter_activation()?;
        self.types.push(types);
        let mut code = code;
        let mut slots = slots;
        let mut captures = captures;
        let result = loop {
            let evaluation = match &code {
                Code::Function(index) => {
                    // Copy the program reference before borrowing a function
                    // body so the mutable machine borrow used by evaluation
                    // remains disjoint.
                    let program = self.program;
                    let Some(function) = program.functions().get(*index) else {
                        break None;
                    };
                    if slots.len() != function.slot_count()
                        || !slots_match_signature(&slots, function.signature())
                    {
                        break None;
                    }
                    self.evaluate(function.body(), &mut slots, &captures)
                }
                Code::Lambda(body) => self.evaluate(body, &mut slots, &captures),
            };
            // A `return` operand that is a tail transfer leaves it for the
            // activation to perform.
            let evaluation = evaluation.or_else(|| self.pending_transfer.take());
            match evaluation {
                Some(Evaluation::Value(value)) => break Some(value),
                Some(Evaluation::TestAssertionFailed) => break None,
                Some(Evaluation::TailTransfer {
                    callable,
                    values,
                    result,
                }) => match self.tail_transfer_action(callable, values, &result) {
                    TailTransferAction::Reuse {
                        code: next_code,
                        slots: next_slots,
                        captures: next_captures,
                        types: next_types,
                    } => {
                        self.tail_transfers = self.tail_transfers.saturating_add(1);
                        code = next_code;
                        slots = next_slots;
                        captures = next_captures;
                        // The reused activation runs at the callee's type
                        // arguments.
                        if let Some(current) = self.types.last_mut() {
                            *current = next_types;
                        }
                    }
                    TailTransferAction::Invoke { callable, values } => {
                        break self.invoke_callable(*callable, values, &result);
                    }
                    TailTransferAction::Invalid => break None,
                },
                None => break self.pending_exit.take(),
            }
        };
        self.types.pop();
        self.leave_activation();
        result
    }

    /// Decides how a call in tail position runs: every callable that creates
    /// a language activation reuses the current one. A compiler-intrinsic
    /// wrapper creates none, so it is invoked as an ordinary call.
    fn tail_transfer_action(
        &self,
        callable: Callable,
        values: Vec<RuntimeValue>,
        result: &Type,
    ) -> TailTransferAction {
        match callable {
            Callable::Named {
                index,
                signature,
                captures,
                types,
            } => {
                let Some(function) = self.program.functions().get(index) else {
                    return TailTransferAction::Invalid;
                };
                let actual_signature = function.signature();
                if !signature.admits(actual_signature)
                    || actual_signature.fixed_parameter_count() != values.len()
                    || !values_match_signature(&values, actual_signature)
                    || !result.admits(&actual_signature.result())
                {
                    return TailTransferAction::Invalid;
                }
                if function.is_external_wrapper() {
                    TailTransferAction::Invoke {
                        callable: Box::new(Callable::Named {
                            index,
                            signature,
                            captures,
                            types,
                        }),
                        values,
                    }
                } else {
                    TailTransferAction::Reuse {
                        code: Code::Function(index),
                        slots: activation_slots(values, function.slot_count()),
                        captures,
                        types,
                    }
                }
            }
            Callable::Lambda {
                slot_count,
                body,
                captures,
                types,
                ..
            } => TailTransferAction::Reuse {
                code: Code::Lambda(body),
                slots: activation_slots(values, slot_count),
                captures,
                types,
            },
        }
    }

    fn evaluate_value(
        &mut self,
        expression: &Expr,
        slots: &mut Frame,
        captures: &[RuntimeValue],
    ) -> Option<RuntimeValue> {
        match self.evaluate(expression, slots, captures)? {
            Evaluation::Value(value) => Some(value),
            Evaluation::TestAssertionFailed => None,
            // A tail transfer is only valid as the final result of its
            // enclosing activation, never as an immediate operand.
            Evaluation::TailTransfer { .. } => None,
        }
    }

    /// Dispatches one expression. Every arm with locals lives in its own
    /// non-inlined function: unoptimized builds give each arm separate stack
    /// slots, and this frame is paid once per nested expression.
    fn evaluate(
        &mut self,
        expression: &Expr,
        slots: &mut Frame,
        captures: &[RuntimeValue],
    ) -> Option<Evaluation> {
        if self.stack_exhausted() {
            return None;
        }
        match expression {
            Expr::Literal { value, .. } => {
                Some(Evaluation::Value(RuntimeValue::Primitive(value.clone())))
            }
            Expr::External {
                intrinsic,
                arguments,
                result,
                ..
            } => {
                let result = self.concrete(result);
                self.evaluate_external(*intrinsic, arguments, &result, slots, captures)
            }
            Expr::Default { .. } => None,
            Expr::Sequence { expressions, .. } => {
                self.evaluate_sequence(expressions, slots, captures)
            }
            Expr::Variable {
                slot, value_type, ..
            } => slots
                .get(*slot)
                .and_then(Option::as_ref)
                .filter(|value| admits_value(value_type, value))
                .cloned()
                .map(Evaluation::Value),
            Expr::Global {
                index, value_type, ..
            } => self
                .evaluate_global(*index)
                .filter(|value| admits_value(value_type, value))
                .map(Evaluation::Value),
            Expr::Function {
                function,
                signature,
                ..
            } => {
                // A generic function named as a value is instantiated by the
                // signature written at this site.
                let mut callable = self.named_callable(*function)?;
                let mut bound = TypeMap::new();
                bind_type(
                    &Type::Function(Box::new(callable.signature().clone())),
                    &self.concrete(&Type::Function(Box::new(signature.clone()))),
                    &mut bound,
                );
                *callable.types_mut() = Arc::new(bound);
                Some(Evaluation::Value(RuntimeValue::Function(callable)))
            }
            Expr::Captured {
                slot, value_type, ..
            } => captures
                .get(*slot)
                .filter(|value| admits_value(value_type, value))
                .cloned()
                .map(Evaluation::Value),
            Expr::Closure { .. } => self.evaluate_closure(expression, slots, captures),
            Expr::Record {
                value_type, fields, ..
            } => {
                let value_type = self.concrete(value_type);
                self.evaluate_record(&value_type, fields, slots, captures)
            }
            Expr::Variant {
                value_type,
                variant,
                payload,
                ..
            } => {
                // `bool` is represented directly.
                if *value_type == Type::Bool {
                    return Some(Evaluation::Value(RuntimeValue::Primitive(
                        Value::Bool(variant == "true"),
                    )));
                }
                // A `void` payload is no payload: a generic slot instantiated
                // to `void` builds the same nullary value a written one does.
                let payload = match payload {
                    Some(payload) => {
                        present_payload(self.evaluate_value(payload, slots, captures)?)
                    }
                    None => None,
                };
                Some(Evaluation::Value(RuntimeValue::Enum {
                    value_type: self.concrete(value_type),
                    variant: variant.clone(),
                    payload,
                }))
            }
            Expr::Wrap {
                value_type, value, ..
            } => {
                let value = self.evaluate_value(value, slots, captures)?;
                // `str` and `bytes` are represented directly over their items.
                if let Some(value) = representation_of(value_type, &value) {
                    return Some(Evaluation::Value(value));
                }
                Some(Evaluation::Value(RuntimeValue::Wrapper {
                    value_type: self.concrete(value_type),
                    value: Box::new(value),
                }))
            }
            Expr::Widen {
                value_type,
                value,
                member,
                ..
            } => {
                let member_type = self.concrete(&value.result_type());
                let value = self.evaluate_value(value, slots, captures)?;
                Some(Evaluation::Value(match member {
                    // Atom and interface widening are erased: a value keeps
                    // its own type, which selects its implementations.
                    None => value,
                    Some(member) => RuntimeValue::Union {
                        value_type: self.concrete(value_type),
                        member: *member,
                        member_type,
                        value: Box::new(value),
                    },
                }))
            }
            Expr::Try {
                value, exit_type, ..
            } => {
                let exit_type = self.concrete(exit_type);
                self.evaluate_try(value, &exit_type, slots, captures)
            }
            Expr::Return { value, .. } => {
                match self.evaluate(value, slots, captures)? {
                    Evaluation::Value(value) => {
                        self.pending_exit = Some(value);
                        None
                    }
                    Evaluation::TestAssertionFailed => {
                        Some(Evaluation::TestAssertionFailed)
                    }
                    transfer @ Evaluation::TailTransfer { .. } => {
                        self.pending_transfer = Some(transfer);
                        None
                    }
                }
            }
            Expr::Project { record, field, .. } => {
                let RuntimeValue::Record { fields, .. } =
                    self.evaluate_value(record, slots, captures)?
                else {
                    return None;
                };
                fields
                    .into_iter()
                    .find(|(name, _)| name == field)
                    .map(|(_, value)| Evaluation::Value(value))
            }
            Expr::Tuple { .. }
            | Expr::TupleProject { .. }
            | Expr::Array { .. }
            | Expr::Dict { .. }
            | Expr::Lookup { .. } => {
                self.evaluate_collection(expression, slots, captures)
            }
            Expr::Let {
                slot, value, body, ..
            } => self.evaluate_let(*slot, value, body, slots, captures),
            Expr::Match {
                scrutinee, arms, ..
            } => self.evaluate_match(scrutinee, arms, slots, captures),
            Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => self.evaluate_if(condition, then_branch, else_branch, slots, captures),
            Expr::Call {
                target,
                arguments,
                result,
                tail,
                origin,
            } => self.evaluate_call(
                target, arguments, result, *tail, origin, slots, captures,
            ),
        }
    }

    /// Evaluates tuple, array, and dict construction, tuple projection, and
    /// lookup. Operands evaluate from left to right; lookups never trap and
    /// answer with the standard `option`.
    #[inline(never)]
    fn evaluate_collection(
        &mut self,
        expression: &Expr,
        slots: &mut Frame,
        captures: &[RuntimeValue],
    ) -> Option<Evaluation> {
        let value = match expression {
            Expr::Tuple {
                value_type,
                components,
                ..
            } => RuntimeValue::Tuple {
                value_type: self.concrete(value_type),
                values: self.evaluate_all(components, slots, captures)?,
            },
            Expr::TupleProject { tuple, index, .. } => {
                let RuntimeValue::Tuple { values, .. } =
                    self.evaluate_value(tuple, slots, captures)?
                else {
                    return None;
                };
                values.into_iter().nth(*index)?
            }
            Expr::Array {
                value_type,
                elements,
                ..
            } => RuntimeValue::Array {
                value_type: self.concrete(value_type),
                values: self.evaluate_all(elements, slots, captures)?,
            },
            Expr::Dict {
                value_type,
                entries,
                key_order,
                ..
            } => {
                let mut ordered = Vec::with_capacity(entries.len());
                for (key, value) in entries {
                    let key = self.evaluate_value(key, slots, captures)?;
                    let value = self.evaluate_value(value, slots, captures)?;
                    self.insert_entry(&mut ordered, key, value, key_order.as_ref())?;
                }
                RuntimeValue::Dict {
                    value_type: self.concrete(value_type),
                    entries: ordered,
                }
            }
            Expr::Lookup {
                collection,
                key,
                value_type,
                key_order,
                ..
            } => {
                let collection = self.evaluate_value(collection, slots, captures)?;
                let key = self.evaluate_value(key, slots, captures)?;
                let found = match collection {
                    RuntimeValue::Dict { entries, .. } => self
                        .search_entries(&entries, &key, key_order.as_ref())?
                        .ok()
                        .and_then(|position| entries.into_iter().nth(position))
                        .map(|(_, value)| value),
                    collection => lookup(collection, &key),
                };
                RuntimeValue::Enum {
                    value_type: self.concrete(value_type),
                    variant: if found.is_some() { "some" } else { "none" }.to_owned(),
                    payload: found.and_then(present_payload),
                }
            }
            _ => return None,
        };
        Some(Evaluation::Value(value))
    }

    fn evaluate_all(
        &mut self,
        expressions: &[Expr],
        slots: &mut Frame,
        captures: &[RuntimeValue],
    ) -> Option<Vec<RuntimeValue>> {
        expressions
            .iter()
            .map(|expression| self.evaluate_value(expression, slots, captures))
            .collect()
    }

    /// Evaluates record fields in their checked evaluation order, then stores
    /// them in the order of the record type.
    #[inline(never)]
    fn evaluate_record(
        &mut self,
        value_type: &Type,
        fields: &[(String, Expr)],
        slots: &mut Frame,
        captures: &[RuntimeValue],
    ) -> Option<Evaluation> {
        let mut values = Vec::with_capacity(fields.len());
        for (name, field) in fields {
            values.push((name.clone(), self.evaluate_value(field, slots, captures)?));
        }
        let order: Vec<&str> = match value_type {
            Type::Record(members) => {
                members.iter().map(|(name, _)| name.as_str()).collect()
            }
            Type::Declared(id) | Type::Applied(id, _) => self
                .program
                .types()
                .iter()
                .find(|definition| definition.id() == id)?
                .record_fields()?
                .iter()
                .map(|(name, _)| name.as_str())
                .collect(),
            _ => return None,
        };
        values.sort_by_key(|(name, _)| order.iter().position(|member| member == name));
        Some(Evaluation::Value(RuntimeValue::Record {
            value_type: value_type.clone(),
            fields: values,
        }))
    }

    /// Orders two keys of one dict (`docs/spec/02-type-system.md`, "Nominal
    /// declarations"). A value of a declared type is ordered by its own
    /// `compare` of `key_order`, the `ordered` interface; everything else
    /// takes canonical key order, component-wise through structures.
    fn compare_keys(
        &mut self,
        left: &RuntimeValue,
        right: &RuntimeValue,
        key_order: Option<&TypeId>,
    ) -> Option<std::cmp::Ordering> {
        use std::cmp::Ordering;
        let Some(interface) = key_order else {
            return Some(key_order_canonical(left, right));
        };
        if let Some(function) = self.key_compare_function(interface, left) {
            let result = self.program.functions().get(function)?.signature().result();
            let callable = self.implementation_callable(function, left)?;
            let RuntimeValue::Enum { variant, .. } = self.invoke_callable(
                callable,
                vec![left.clone(), right.clone()],
                &result,
            )?
            else {
                return None;
            };
            return match variant.as_str() {
                "less" => Some(Ordering::Less),
                "equal" => Some(Ordering::Equal),
                "greater" => Some(Ordering::Greater),
                _ => None,
            };
        }
        match (left, right) {
            (
                RuntimeValue::Tuple { values: left, .. },
                RuntimeValue::Tuple { values: right, .. },
            ) => self.compare_sequences(left.iter(), right.iter(), key_order),
            (
                RuntimeValue::Record { fields: left, .. },
                RuntimeValue::Record { fields: right, .. },
            ) => self.compare_sequences(
                left.iter().map(|(_, value)| value),
                right.iter().map(|(_, value)| value),
                key_order,
            ),
            (
                RuntimeValue::Enum {
                    variant: left_variant,
                    payload: left_payload,
                    ..
                },
                RuntimeValue::Enum {
                    variant: right_variant,
                    payload: right_payload,
                    ..
                },
            ) => match left_variant.as_bytes().cmp(right_variant.as_bytes()) {
                Ordering::Equal => match (left_payload, right_payload) {
                    (Some(left), Some(right)) => {
                        self.compare_keys(left, right, key_order)
                    }
                    _ => Some(Ordering::Equal),
                },
                order => Some(order),
            },
            (
                RuntimeValue::Union {
                    member: left_member,
                    value: left,
                    ..
                },
                RuntimeValue::Union {
                    member: right_member,
                    value: right,
                    ..
                },
            ) => match left_member.cmp(right_member) {
                Ordering::Equal => self.compare_keys(left, right, key_order),
                order => Some(order),
            },
            _ => Some(key_order_canonical(left, right)),
        }
    }

    fn compare_sequences<'v>(
        &mut self,
        left: impl Iterator<Item = &'v RuntimeValue>,
        right: impl Iterator<Item = &'v RuntimeValue>,
        key_order: Option<&TypeId>,
    ) -> Option<std::cmp::Ordering> {
        use std::cmp::Ordering;
        let mut right = right;
        for left in left {
            let Some(right) = right.next() else {
                return Some(Ordering::Greater);
            };
            let order = self.compare_keys(left, right, key_order)?;
            if order.is_ne() {
                return Some(order);
            }
        }
        Some(if right.next().is_some() {
            Ordering::Less
        } else {
            Ordering::Equal
        })
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

    /// Inserts one entry into a dict's sorted entries; a repeated key keeps
    /// its first position and takes the later value.
    fn insert_entry(
        &mut self,
        entries: &mut Vec<(RuntimeValue, RuntimeValue)>,
        key: RuntimeValue,
        value: RuntimeValue,
        key_order: Option<&TypeId>,
    ) -> Option<()> {
        match self.search_entries(entries, &key, key_order)? {
            // The later pair replaces the earlier one: its key and its value.
            Ok(position) => {
                if let Some(entry) = entries.get_mut(position) {
                    *entry = (key, value);
                }
            }
            Err(position) => entries.insert(position, (key, value)),
        }
        Some(())
    }

    /// Binary search over sorted dict entries, with a comparison that may run
    /// Vibra code.
    fn search_entries(
        &mut self,
        entries: &[(RuntimeValue, RuntimeValue)],
        key: &RuntimeValue,
        key_order: Option<&TypeId>,
    ) -> Option<Result<usize, usize>> {
        let (mut low, mut high) = (0, entries.len());
        while low < high {
            let middle = low + (high - low) / 2;
            let (existing, _) = entries.get(middle)?;
            match self.compare_keys(existing, key, key_order)? {
                std::cmp::Ordering::Less => low = middle + 1,
                std::cmp::Ordering::Greater => high = middle,
                std::cmp::Ordering::Equal => return Some(Ok(middle)),
            }
        }
        Some(Err(low))
    }

    fn named_callable(&self, function: usize) -> Option<Callable> {
        Some(Callable::Named {
            index: function,
            signature: self.program.functions().get(function)?.signature().clone(),
            captures: Vec::new(),
            types: Arc::default(),
        })
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

    #[inline(never)]
    fn evaluate_external(
        &mut self,
        intrinsic: vibra_ir::external::CompilerIntrinsic,
        arguments: &[Expr],
        result: &Type,
        slots: &mut Frame,
        captures: &[RuntimeValue],
    ) -> Option<Evaluation> {
        use vibra_ir::external::CompilerIntrinsic;
        let values = self.evaluate_all(arguments, slots, captures)?;
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
            (
                CompilerIntrinsic::ArrayFold,
                [
                    RuntimeValue::Array { values, .. },
                    initial,
                    RuntimeValue::Function(step),
                ],
            ) => {
                let mut accumulator = initial.clone();
                for value in values {
                    accumulator = self.invoke_callable(
                        step.clone(),
                        vec![accumulator, value.clone()],
                        result,
                    )?;
                }
                accumulator
            }
            // The packed variadic tail is already the built collection.
            (CompilerIntrinsic::ArrayOf | CompilerIntrinsic::DictOf, [tail]) => {
                tail.clone()
            }
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
                            values: vec![key.clone(), value.clone()],
                        })
                        .collect(),
                }
            }
            (CompilerIntrinsic::ArrayLength, [RuntimeValue::Array { values, .. }]) => {
                RuntimeValue::Primitive(Value::U64(values.len() as u64))
            }
            (
                CompilerIntrinsic::ArrayAppend,
                [RuntimeValue::Array { value_type, values }, element],
            ) => {
                let mut values = values.clone();
                values.push(element.clone());
                RuntimeValue::Array {
                    value_type: value_type.clone(),
                    values,
                }
            }
            (
                CompilerIntrinsic::ArrayConcat,
                [
                    RuntimeValue::Array { value_type, values },
                    RuntimeValue::Array { values: right, .. },
                ],
            ) => {
                let mut values = values.clone();
                values.extend(right.iter().cloned());
                RuntimeValue::Array {
                    value_type: value_type.clone(),
                    values,
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
                        Box::new(RuntimeValue::Array {
                            value_type: value_type.clone(),
                            values: slice.to_vec(),
                        })
                    }),
                }
            }
            _ => registry::apply(intrinsic, &values, result)?,
        };
        Some(Evaluation::Value(value))
    }

    #[inline(never)]
    fn evaluate_sequence(
        &mut self,
        expressions: &[Expr],
        slots: &mut Frame,
        captures: &[RuntimeValue],
    ) -> Option<Evaluation> {
        let Some((last, leading)) = expressions.split_last() else {
            return Some(Evaluation::Value(RuntimeValue::Primitive(Value::Void)));
        };
        for expression in leading {
            self.evaluate_value(expression, slots, captures)?;
        }
        self.evaluate(last, slots, captures)
    }

    #[inline(never)]
    fn evaluate_closure(
        &mut self,
        closure: &Expr,
        slots: &mut Frame,
        captures: &[RuntimeValue],
    ) -> Option<Evaluation> {
        let Expr::Closure {
            signature,
            captures: capture_expressions,
            body,
            slot_count,
            ..
        } = closure
        else {
            return None;
        };
        let mut environment = Vec::with_capacity(capture_expressions.len());
        for capture in capture_expressions {
            environment.push(self.evaluate_value(capture, slots, captures)?);
        }
        // The closure runs at the type arguments of the activation that
        // made it; its own generic parameters are fixed by each call.
        let types = self.current_types();
        let signature = if types.is_empty() {
            signature.clone()
        } else {
            signature.substitute(&types)
        };
        Some(Evaluation::Value(RuntimeValue::Function(
            Callable::Lambda {
                signature,
                slot_count: *slot_count,
                body: Arc::clone(body),
                captures: environment,
                types,
            },
        )))
    }

    #[inline(never)]
    fn evaluate_let(
        &mut self,
        slot: Option<usize>,
        value: &Expr,
        body: &Expr,
        slots: &mut Frame,
        captures: &[RuntimeValue],
    ) -> Option<Evaluation> {
        let value = self.evaluate_value(value, slots, captures)?;
        if let Some(slot) = slot {
            *slots.get_mut(slot)? = Some(value);
        }
        self.evaluate(body, slots, captures)
    }

    /// Evaluates the subject once and runs the first arm whose pattern
    /// matches, with that arm's binders stored in their slots. The checker
    /// proved the arms exhaustive, so no arm matching is impossible IR.
    #[inline(never)]
    fn evaluate_match(
        &mut self,
        scrutinee: &Expr,
        arms: &[MatchArm],
        slots: &mut Frame,
        captures: &[RuntimeValue],
    ) -> Option<Evaluation> {
        let subject = self.evaluate_value(scrutinee, slots, captures)?;
        for arm in arms {
            if pattern_matches(&arm.pattern, &subject) {
                bind_pattern(&arm.pattern, subject, slots)?;
                return self.evaluate(&arm.body, slots, captures);
            }
        }
        None
    }

    /// `try`: the payload of `some` or `ok`, or an early exit that rebuilds
    /// `none` or `err` at the enclosing result type and unwinds to the
    /// innermost function or `lambda`.
    #[inline(never)]
    fn evaluate_try(
        &mut self,
        value: &Expr,
        exit_type: &Type,
        slots: &mut Frame,
        captures: &[RuntimeValue],
    ) -> Option<Evaluation> {
        let RuntimeValue::Enum {
            variant, payload, ..
        } = self.evaluate_value(value, slots, captures)?
        else {
            return None;
        };
        match variant.as_str() {
            // A nullary success carries the `void` value.
            "some" | "ok" => Some(Evaluation::Value(
                payload
                    .map_or(RuntimeValue::Primitive(Value::Void), |payload| *payload),
            )),
            "none" | "err" => {
                self.pending_exit = Some(RuntimeValue::Enum {
                    value_type: exit_type.clone(),
                    variant,
                    payload,
                });
                None
            }
            _ => None,
        }
    }

    #[inline(never)]
    fn evaluate_if(
        &mut self,
        condition: &Expr,
        then_branch: &Expr,
        else_branch: &Expr,
        slots: &mut Frame,
        captures: &[RuntimeValue],
    ) -> Option<Evaluation> {
        match self.evaluate_value(condition, slots, captures)? {
            RuntimeValue::Primitive(Value::Bool(true)) => {
                self.evaluate(then_branch, slots, captures)
            }
            RuntimeValue::Primitive(Value::Bool(false)) => {
                self.evaluate(else_branch, slots, captures)
            }
            _ => None,
        }
    }

    // Kept out of `evaluate` so that its frame, which every nested
    // expression pays for, stays small in unoptimized builds.
    #[inline(never)]
    #[allow(clippy::too_many_arguments)]
    fn evaluate_call(
        &mut self,
        target: &CallTarget,
        arguments: &[Expr],
        result: &Type,
        tail: bool,
        origin: &SourceOrigin,
        slots: &mut Frame,
        captures: &[RuntimeValue],
    ) -> Option<Evaluation> {
        // A contract call selects its implementation from the receiver's
        // runtime type, after every operand is evaluated.
        if let CallTarget::Contract {
            interface,
            member,
            receiver,
            arguments: interface_arguments,
            destination,
            closed,
            ..
        } = target
        {
            let values = self.evaluate_all(arguments, slots, captures)?;
            // The implementation is the one whose receiver and interface
            // arguments both match, at one binding of its own parameters: a
            // receiver may implement a generic interface more than once.
            // A member selected by its destination dispatches on that type,
            // at this activation's type arguments.
            let receiver_type = match destination {
                Some(destination) => self.concrete(destination),
                None => runtime_type(values.get(*receiver)?),
            };
            let interface_arguments = interface_arguments
                .iter()
                .map(|argument| self.concrete(argument))
                .collect::<Vec<_>>();
            // A written member for the receiver's type wins over the
            // interface's default, whose receiver is the open `self`.
            let candidates = self
                .program
                .functions()
                .iter()
                .enumerate()
                .filter_map(|(index, function)| {
                    let implements = function.implements()?;
                    if implements.interface != *interface
                        || implements.member != *member
                    {
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
                let closed = (*closed)?;
                if closed == ClosedContract::IterNext {
                    let [iterator] = values.as_slice() else {
                        return None;
                    };
                    return closed_next(iterator, &self.concrete(result))
                        .map(Evaluation::Value);
                }
                let [left, right] = values.as_slice() else {
                    return None;
                };
                // A structure holding a user key orders through its
                // `compare`, and is equal exactly when that says so: the
                // closed `equal` has no other answer for a key it cannot see
                // into.
                let key_order = match closed {
                    ClosedContract::KeyCompare => Some(interface.clone()),
                    _ => self.ordered_interface(),
                };
                let order = self.compare_keys(left, right, key_order.as_ref())?;
                return Some(Evaluation::Value(closed_contract(
                    closed,
                    order,
                    &self.concrete(result),
                )));
            };
            let mut callable = self.named_callable(index)?;
            *callable.types_mut() = Arc::new(bound);
            // The member's own generic parameters, from this call.
            self.bind_call(&mut callable, arguments, &values, result);
            return self.finish_call(callable, values, result, tail);
        }
        let mut callable = match target {
            CallTarget::Direct(function) => self.named_callable(*function)?,
            CallTarget::Indirect { callee, .. } => {
                let RuntimeValue::Function(callable) =
                    self.evaluate_value(callee, slots, captures)?
                else {
                    return None;
                };
                callable
            }
            CallTarget::Contract { .. } => return None,
        };
        let callable_signature = match &callable {
            Callable::Named { signature, .. } | Callable::Lambda { signature, .. } => {
                signature
            }
        };
        let positional = callable_signature.parameters().len();
        let mut values = Vec::with_capacity(arguments.len());
        for (argument_index, argument) in arguments.iter().enumerate() {
            if matches!(argument, Expr::Default { .. }) {
                let parameter = callable_signature
                    .labelled()
                    .get(argument_index.checked_sub(positional)?)?;
                let default = parameter.default()?.clone();
                let atom_default = matches!(default, Value::Atom(_))
                    && argument.result_type() == Type::Atom;
                if !atom_default && !default.ty().same_shape(&argument.result_type()) {
                    return None;
                }
                values.push(RuntimeValue::Primitive(default));
            } else {
                values.push(self.evaluate_value(argument, slots, captures)?);
            }
        }
        self.bind_call(&mut callable, arguments, &values, result);
        if let Callable::Named { index, .. } = &callable
            && let Some(assertion) = self
                .program
                .functions()
                .get(*index)
                .and_then(|function| function.test_assertion())
        {
            if !self.test_mode {
                return None;
            }
            let value =
                self.invoke_test_assertion(assertion, values, origin.clone())?;
            return Some(if self.assertion_failure.is_some() {
                Evaluation::TestAssertionFailed
            } else {
                Evaluation::Value(value)
            });
        }
        self.finish_call(callable, values, result, tail)
    }

    /// Runs a call whose callable and operands are resolved: a call in tail
    /// position is handed back to the activation loop, and any other call
    /// enters a new activation.
    fn finish_call(
        &mut self,
        callable: Callable,
        values: Vec<RuntimeValue>,
        result: &Type,
        tail: bool,
    ) -> Option<Evaluation> {
        if !tail {
            return self
                .invoke_callable(callable, values, result)
                .map(Evaluation::Value);
        }
        let callable_signature = callable.signature();
        if callable_signature.fixed_parameter_count() != values.len()
            || !values_match_signature(&values, callable_signature)
            || !result.admits(&callable_signature.result())
        {
            return None;
        }
        Some(Evaluation::TailTransfer {
            callable,
            values,
            result: result.clone(),
        })
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

    fn invoke_callable(
        &mut self,
        callable: Callable,
        values: Vec<RuntimeValue>,
        result: &Type,
    ) -> Option<RuntimeValue> {
        let signature = match &callable {
            Callable::Named { signature, .. } | Callable::Lambda { signature, .. } => {
                signature
            }
        };
        if signature.fixed_parameter_count() != values.len()
            || !values_match_signature(&values, signature)
            || !result.admits(&signature.result())
        {
            return None;
        }
        match callable {
            Callable::Named {
                index,
                signature,
                captures,
                types,
            } => {
                let function = self.program.functions().get(index)?;
                if !signature.admits(function.signature()) {
                    return None;
                }
                let slots = activation_slots(values, function.slot_count());
                self.evaluate_function(index, slots, captures, types)
            }
            Callable::Lambda {
                slot_count,
                body,
                captures,
                types,
                ..
            } => {
                let slots = activation_slots(values, slot_count);
                self.run_activation(Code::Lambda(body), slots, captures, types)
            }
        }
    }

    fn evaluate_global(&mut self, index: usize) -> Option<RuntimeValue> {
        match self.globals.get(index)? {
            GlobalState::Ready(value) => return Some(value.clone()),
            GlobalState::Evaluating => return None,
            GlobalState::Uninitialized => {}
        }
        *self.globals.get_mut(index)? = GlobalState::Evaluating;
        let program = self.program;
        let global = program.globals().get(index)?;
        let mut slots = vec![None; global.slot_count()];
        // An initializer is its own activation, with no type arguments.
        self.types.push(Arc::default());
        let value = self.evaluate_value(global.initializer(), &mut slots, &[]);
        self.types.pop();
        if let Some(value) = &value {
            *self.globals.get_mut(index)? = GlobalState::Ready(value.clone());
        }
        value
    }
}

/// The address of a local in the caller's frame.
#[inline(always)]
fn stack_address() -> usize {
    let marker = 0u8;
    std::ptr::from_ref(std::hint::black_box(&marker)).addr()
}

/// A fresh activation: fixed parameters bound, remaining slots empty.
fn activation_slots(
    values: Vec<RuntimeValue>,
    slot_count: usize,
) -> Vec<Option<RuntimeValue>> {
    values
        .into_iter()
        .map(Some)
        .chain(std::iter::repeat(None))
        .take(slot_count)
        .collect()
}

/// Whether a slot of type `expected` may hold `value`. An atom value's own
/// type is its singleton, which the erased `atom` type also holds.
fn admits_value(expected: &Type, value: &RuntimeValue) -> bool {
    // An interface value, including `any`, holds a value of whichever
    // concrete type was widened to it.
    matches!(expected, Type::Interface(_, _) | Type::Any)
        || matches!(
            (expected, value),
            (Type::Atom, RuntimeValue::Primitive(Value::Atom(_)))
        )
        || expected.admits(&runtime_type(value))
}

fn runtime_type(value: &RuntimeValue) -> Type {
    match value {
        RuntimeValue::Primitive(value) => value.ty(),
        RuntimeValue::Function(Callable::Named { signature, .. }) => {
            Type::Function(Box::new(signature.clone()))
        }
        RuntimeValue::Function(Callable::Lambda { signature, .. }) => {
            Type::Function(Box::new(signature.clone()))
        }
        RuntimeValue::Record { value_type, .. }
        | RuntimeValue::Enum { value_type, .. } => value_type.clone(),
        RuntimeValue::Wrapper { value_type, .. }
        | RuntimeValue::Tuple { value_type, .. }
        | RuntimeValue::Array { value_type, .. }
        | RuntimeValue::Dict { value_type, .. }
        | RuntimeValue::Union { value_type, .. } => value_type.clone(),
    }
}

fn slots_match_signature(
    slots: &[Option<RuntimeValue>],
    signature: &FunctionSignature,
) -> bool {
    let expected = signature.slot_types();
    slots
        .iter()
        .take(expected.len())
        .zip(expected)
        .all(|(value, expected)| {
            value
                .as_ref()
                .is_some_and(|value| admits_value(&expected, value))
        })
}

fn values_match_signature(
    values: &[RuntimeValue],
    signature: &FunctionSignature,
) -> bool {
    let expected = signature.slot_types();
    values
        .iter()
        .zip(expected)
        .all(|(value, expected)| admits_value(&expected, value))
}

/// The observable form of a runtime value; `None` when it is or contains a
/// function, which has no canonical encoding.
fn observe(value: RuntimeValue) -> Option<ObservedValue> {
    Some(match value {
        RuntimeValue::Primitive(value) => ObservedValue::Primitive(value),
        RuntimeValue::Function(_) => return None,
        RuntimeValue::Record { value_type, fields } => ObservedValue::Record {
            type_id: declared_id(&value_type),
            fields: fields
                .into_iter()
                .map(|(name, value)| Some((name, observe(value)?)))
                .collect::<Option<Vec<_>>>()?,
        },
        RuntimeValue::Enum {
            value_type,
            variant,
            payload,
        } => ObservedValue::Enum {
            type_id: declared_id(&value_type),
            variant,
            payload: match payload {
                Some(payload) => Some(Box::new(observe(*payload)?)),
                None => None,
            },
        },
        RuntimeValue::Wrapper { value_type, value } => ObservedValue::Wrapper {
            type_id: declared_id(&value_type)?,
            value: Box::new(observe(*value)?),
        },
        RuntimeValue::Tuple { value_type, values } => ObservedValue::Tuple {
            type_id: declared_id(&value_type),
            values: values
                .into_iter()
                .map(observe)
                .collect::<Option<Vec<_>>>()?,
        },
        RuntimeValue::Array { values, .. } => ObservedValue::Array(
            values
                .into_iter()
                .map(observe)
                .collect::<Option<Vec<_>>>()?,
        ),
        RuntimeValue::Dict { entries, .. } => ObservedValue::Dict(
            entries
                .into_iter()
                .map(|(key, value)| Some((observe(key)?, observe(value)?)))
                .collect::<Option<Vec<_>>>()?,
        ),
        RuntimeValue::Union {
            value_type,
            member_type,
            value,
            ..
        } => ObservedValue::Union {
            type_id: declared_id(&value_type),
            member: Box::new(member_type),
            value: Box::new(observe(*value)?),
        },
    })
}

/// The `str` or `bytes` value an array of scalars or bytes wraps into.
fn representation_of(value_type: &Type, value: &RuntimeValue) -> Option<RuntimeValue> {
    let RuntimeValue::Array { values, .. } = value else {
        return None;
    };
    let item = |value: &RuntimeValue| match value {
        RuntimeValue::Primitive(value) => Some(value.clone()),
        _ => None,
    };
    Some(RuntimeValue::Primitive(match value_type {
        Type::Str => Value::Str(
            values
                .iter()
                .map(|value| match item(value)? {
                    Value::Char(value) => Some(value),
                    _ => None,
                })
                .collect::<Option<String>>()?,
        ),
        Type::Bytes => Value::Bytes(
            values
                .iter()
                .map(|value| match item(value)? {
                    Value::U8(value) => Some(value),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>()?,
        ),
        _ => return None,
    }))
}

/// The array of scalars or bytes a `str` or `bytes` value is written over.
fn items_of(value: &RuntimeValue) -> Option<RuntimeValue> {
    let (element, values) = match value {
        RuntimeValue::Primitive(Value::Str(text)) => (
            Type::Char,
            text.chars()
                .map(|value| RuntimeValue::Primitive(Value::Char(value)))
                .collect(),
        ),
        RuntimeValue::Primitive(Value::Bytes(bytes)) => (
            Type::U8,
            bytes
                .iter()
                .map(|value| RuntimeValue::Primitive(Value::U8(*value)))
                .collect(),
        ),
        _ => return None,
    };
    Some(RuntimeValue::Array {
        value_type: Type::Array(Box::new(element)),
        values,
    })
}

/// Whether `value` matches `pattern`. Binders and discards match anything;
/// omitted record fields are never inspected.
/// The payload a variant stores for `value`: none for `void`, so a variant
/// whose payload slot is `void` has one shape however it was built
/// (`docs/spec/02-type-system.md`, "Nominal declarations").
pub(crate) fn present_payload(value: RuntimeValue) -> Option<Box<RuntimeValue>> {
    (value != RuntimeValue::Primitive(Value::Void)).then(|| Box::new(value))
}

fn pattern_matches(pattern: &Pattern, value: &RuntimeValue) -> bool {
    match (pattern, value) {
        (Pattern::Wildcard | Pattern::Bind { .. }, _) => true,
        (Pattern::Literal(expected), RuntimeValue::Primitive(actual)) => {
            expected == actual
        }
        (
            Pattern::Variant {
                variant: expected,
                payload: expected_payload,
            },
            RuntimeValue::Enum {
                variant, payload, ..
            },
        ) => {
            expected == variant
                && match (expected_payload, payload) {
                    (Some(pattern), Some(payload)) => pattern_matches(pattern, payload),
                    (None, _) => true,
                    // A nullary variant holds the `void` value.
                    (Some(pattern), None) => {
                        pattern_matches(pattern, &RuntimeValue::Primitive(Value::Void))
                    }
                }
        }
        (Pattern::Record(expected), RuntimeValue::Record { fields, .. }) => {
            expected.iter().all(|(name, pattern)| {
                fields
                    .iter()
                    .find(|(field, _)| field == name)
                    .is_some_and(|(_, value)| pattern_matches(pattern, value))
            })
        }
        (Pattern::Tuple(expected), RuntimeValue::Tuple { values, .. })
        | (Pattern::Array(expected), RuntimeValue::Array { values, .. }) => {
            expected.len() == values.len()
                && expected
                    .iter()
                    .zip(values)
                    .all(|(pattern, value)| pattern_matches(pattern, value))
        }
        (Pattern::Wrap(inner), RuntimeValue::Wrapper { value, .. }) => {
            pattern_matches(inner, value)
        }
        (
            Pattern::Wrap(inner),
            RuntimeValue::Primitive(Value::Str(_) | Value::Bytes(_)),
        ) => items_of(value).is_some_and(|items| pattern_matches(inner, &items)),
        (
            Pattern::Member { index, pattern, .. },
            RuntimeValue::Union { member, value, .. },
        ) => index == member && pattern_matches(pattern, value),
        _ => false,
    }
}

/// Stores every binder of a pattern that already matched `value`.
fn bind_pattern(
    pattern: &Pattern,
    value: RuntimeValue,
    slots: &mut Frame,
) -> Option<()> {
    match (pattern, value) {
        (Pattern::Wildcard | Pattern::Literal(_), _) => Some(()),
        (Pattern::Bind { slot, .. }, value) => {
            *slots.get_mut(*slot)? = Some(value);
            Some(())
        }
        (
            Pattern::Variant {
                payload: Some(pattern),
                ..
            },
            RuntimeValue::Enum {
                payload: Some(payload),
                ..
            },
        ) => bind_pattern(pattern, *payload, slots),
        (
            Pattern::Variant {
                payload: Some(pattern),
                ..
            },
            RuntimeValue::Enum { payload: None, .. },
        ) => bind_pattern(pattern, RuntimeValue::Primitive(Value::Void), slots),
        (Pattern::Variant { payload: None, .. }, RuntimeValue::Enum { .. }) => Some(()),
        (Pattern::Record(expected), RuntimeValue::Record { fields, .. }) => {
            let mut fields = fields;
            for (name, pattern) in expected {
                let index = fields.iter().position(|(field, _)| field == name)?;
                let (_, value) = fields.swap_remove(index);
                bind_pattern(pattern, value, slots)?;
            }
            Some(())
        }
        (Pattern::Tuple(expected), RuntimeValue::Tuple { values, .. })
        | (Pattern::Array(expected), RuntimeValue::Array { values, .. }) => {
            for (pattern, value) in expected.iter().zip(values) {
                bind_pattern(pattern, value, slots)?;
            }
            Some(())
        }
        (Pattern::Wrap(inner), RuntimeValue::Wrapper { value, .. }) => {
            bind_pattern(inner, *value, slots)
        }
        (
            Pattern::Wrap(inner),
            value @ RuntimeValue::Primitive(Value::Str(_) | Value::Bytes(_)),
        ) => bind_pattern(inner, items_of(&value)?, slots),
        (Pattern::Member { pattern, .. }, RuntimeValue::Union { value, .. }) => {
            bind_pattern(pattern, *value, slots)
        }
        _ => None,
    }
}

/// Canonical key order over admissible key values
/// (`docs/spec/02-type-system.md`, "Nominal declarations"): `false` before
/// `true`, numeric order for integers, scalar order for `char` and `str`,
/// byte order for `bytes` and for an atom's spelling, and component-wise
/// order for tuples, records in canonical field order, and enums by canonical
/// variant then payload. Keys of one dict share one type, so values of
/// different shapes never meet; they compare equal only to stay total.
fn key_order_canonical(
    left: &RuntimeValue,
    right: &RuntimeValue,
) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (left, right) {
        (RuntimeValue::Primitive(left), RuntimeValue::Primitive(right)) => {
            primitive_order(left, right)
        }
        (
            RuntimeValue::Tuple { values: left, .. },
            RuntimeValue::Tuple { values: right, .. },
        ) => sequence_order(left.iter(), right.iter()),
        (
            RuntimeValue::Record { fields: left, .. },
            RuntimeValue::Record { fields: right, .. },
        ) => sequence_order(
            left.iter().map(|(_, value)| value),
            right.iter().map(|(_, value)| value),
        ),
        (
            RuntimeValue::Enum {
                variant: left_variant,
                payload: left_payload,
                ..
            },
            RuntimeValue::Enum {
                variant: right_variant,
                payload: right_payload,
                ..
            },
        ) => left_variant
            .as_bytes()
            .cmp(right_variant.as_bytes())
            .then_with(|| match (left_payload, right_payload) {
                (Some(left), Some(right)) => key_order_canonical(left, right),
                _ => Ordering::Equal,
            }),
        (
            RuntimeValue::Union {
                member: left_member,
                value: left,
                ..
            },
            RuntimeValue::Union {
                member: right_member,
                value: right,
                ..
            },
        ) => left_member
            .cmp(right_member)
            .then_with(|| key_order_canonical(left, right)),
        _ => Ordering::Equal,
    }
}

/// A closed key type's `ordered.compare` or `equatable.equal` from the order
/// of its two operands.
fn closed_contract(
    closed: ClosedContract,
    order: std::cmp::Ordering,
    result: &Type,
) -> RuntimeValue {
    match closed {
        // `iter.next` takes one operand and is answered by `closed_next`.
        ClosedContract::KeyEqual | ClosedContract::IterNext => {
            RuntimeValue::Primitive(Value::Bool(order.is_eq()))
        }
        ClosedContract::KeyCompare => RuntimeValue::Enum {
            value_type: result.clone(),
            variant: match order {
                std::cmp::Ordering::Less => "less",
                std::cmp::Ordering::Equal => "equal",
                std::cmp::Ordering::Greater => "greater",
            }
            .to_owned(),
            payload: None,
        },
    }
}

/// `iter.next` of a builtin constructor value
/// (`docs/spec/02-type-system.md`, "Closed builtin conformance"): the first
/// item with the iterator that remains, or `none` when it is exhausted. An
/// array yields in index order, a dict one entry in key order, a `str` one
/// Unicode scalar, and an `option` its payload once.
fn closed_next(iterator: &RuntimeValue, result: &Type) -> Option<RuntimeValue> {
    let step = match iterator {
        RuntimeValue::Array { value_type, values } => {
            values.split_first().map(|(first, rest)| {
                (
                    first.clone(),
                    RuntimeValue::Array {
                        value_type: value_type.clone(),
                        values: rest.to_vec(),
                    },
                )
            })
        }
        RuntimeValue::Dict {
            value_type,
            entries,
        } => {
            let Type::Dict(key, value) = value_type else {
                return None;
            };
            entries
                .split_first()
                .map(|((first_key, first_value), rest)| {
                    (
                        RuntimeValue::Tuple {
                            value_type: Type::Tuple(vec![
                                key.as_ref().clone(),
                                value.as_ref().clone(),
                            ]),
                            values: vec![first_key.clone(), first_value.clone()],
                        },
                        RuntimeValue::Dict {
                            value_type: value_type.clone(),
                            entries: rest.to_vec(),
                        },
                    )
                })
        }
        RuntimeValue::Primitive(Value::Str(text)) => {
            let mut scalars = text.chars();
            scalars.next().map(|first| {
                (
                    RuntimeValue::Primitive(Value::Char(first)),
                    RuntimeValue::Primitive(Value::Str(scalars.as_str().to_owned())),
                )
            })
        }
        RuntimeValue::Enum {
            value_type,
            payload,
            ..
        } => payload.as_ref().map(|payload| {
            (
                payload.as_ref().clone(),
                RuntimeValue::Enum {
                    value_type: value_type.clone(),
                    variant: "none".to_owned(),
                    payload: None,
                },
            )
        }),
        _ => return None,
    };
    let Type::Applied(_, arguments) = result else {
        return None;
    };
    Some(RuntimeValue::Enum {
        value_type: result.clone(),
        variant: if step.is_some() { "some" } else { "none" }.to_owned(),
        payload: step.map(|(item, remaining)| {
            Box::new(RuntimeValue::Tuple {
                value_type: arguments.first().cloned().unwrap_or(Type::Void),
                values: vec![item, remaining],
            })
        }),
    })
}

fn sequence_order<'a>(
    left: impl Iterator<Item = &'a RuntimeValue>,
    right: impl Iterator<Item = &'a RuntimeValue>,
) -> std::cmp::Ordering {
    let mut right = right;
    for left in left {
        let Some(right) = right.next() else {
            return std::cmp::Ordering::Greater;
        };
        let order = key_order_canonical(left, right);
        if order.is_ne() {
            return order;
        }
    }
    if right.next().is_some() {
        std::cmp::Ordering::Less
    } else {
        std::cmp::Ordering::Equal
    }
}

fn primitive_order(left: &Value, right: &Value) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (left, right) {
        (Value::Bool(left), Value::Bool(right)) => left.cmp(right),
        (Value::Char(left), Value::Char(right)) => left.cmp(right),
        // UTF-8 byte order is scalar-value order.
        (Value::Str(left), Value::Str(right)) => left.as_bytes().cmp(right.as_bytes()),
        (Value::Bytes(left), Value::Bytes(right)) => left.cmp(right),
        (Value::Atom(left), Value::Atom(right)) => {
            left.as_bytes().cmp(right.as_bytes())
        }
        (Value::I8(left), Value::I8(right)) => left.cmp(right),
        (Value::I16(left), Value::I16(right)) => left.cmp(right),
        (Value::I32(left), Value::I32(right)) => left.cmp(right),
        (Value::I64(left), Value::I64(right)) => left.cmp(right),
        (Value::U8(left), Value::U8(right)) => left.cmp(right),
        (Value::U16(left), Value::U16(right)) => left.cmp(right),
        (Value::U32(left), Value::U32(right)) => left.cmp(right),
        (Value::U64(left), Value::U64(right)) => left.cmp(right),
        _ => Ordering::Equal,
    }
}

/// The element at `key` in an array, `str`, or `bytes` value. String
/// indices count Unicode scalars; byte indices count bytes.
fn lookup(collection: RuntimeValue, key: &RuntimeValue) -> Option<RuntimeValue> {
    let index = || match key {
        RuntimeValue::Primitive(Value::U64(index)) => usize::try_from(*index).ok(),
        _ => None,
    };
    match collection {
        RuntimeValue::Array { values, .. } => values.into_iter().nth(index()?),
        RuntimeValue::Primitive(Value::Str(text)) => text
            .chars()
            .nth(index()?)
            .map(|scalar| RuntimeValue::Primitive(Value::Char(scalar))),
        RuntimeValue::Primitive(Value::Bytes(bytes)) => bytes
            .get(index()?)
            .map(|byte| RuntimeValue::Primitive(Value::U8(*byte))),
        _ => None,
    }
}

fn declared_id(value_type: &Type) -> Option<TypeId> {
    match value_type {
        Type::Declared(id) | Type::Applied(id, _) => Some(id.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use vibra_diagnostics::ByteSpan;
    use vibra_ir::{
        CheckedFunction, CheckedGlobal, Expr, FunctionSignature, SourceOrigin, Type,
        Value,
    };

    use super::run;

    #[test]
    fn only_a_host_event_is_not_a_trap() {
        use super::RuntimeError;
        use vibra_diagnostics::DiagnosticCode;

        let invalid = RuntimeError::InvalidBody {
            function: "main".to_owned(),
        };
        for error in [RuntimeError::NoEntry, invalid] {
            assert_eq!(
                error.program_trap(),
                Some((DiagnosticCode::RuntimeInvalidCheckedProgram, None))
            );
            assert!(!error.is_host_event());
        }
        let unobservable = RuntimeError::UnobservableFunction { origin: None };
        assert_eq!(
            unobservable.program_trap(),
            Some((DiagnosticCode::RuntimeUnobservableFunction, None))
        );
        for error in [
            RuntimeError::HostStackExhausted { limit: 1 },
            RuntimeError::HostThreadUnavailable("none".to_owned()),
        ] {
            assert_eq!(error.program_trap(), None);
            assert!(error.is_host_event());
        }
    }

    #[test]
    fn a_checked_literal_has_no_audit_events() {
        let origin = SourceOrigin::new("test.vib", ByteSpan::new(0, 1));
        let function = CheckedFunction::new(
            "answer",
            FunctionSignature::new(Vec::new(), Type::I32),
            vibra_ir::Expr::literal(vibra_ir::Value::I32(42), origin.clone()),
            origin,
        )
        .expect("valid checked function");
        let program = vibra_ir::CheckedProgram::try_new(vec![function], 0)
            .expect("valid checked program");
        let result = run(&program).expect("execution");
        assert_eq!(result.value(), Some(&vibra_ir::Value::I32(42)));
        assert!(result.audit_trace().is_empty());
    }

    #[test]
    fn execution_uses_the_checked_entry_index() {
        let first_origin = SourceOrigin::new("test.vib", ByteSpan::new(0, 1));
        let first = CheckedFunction::new(
            "first",
            FunctionSignature::new(Vec::new(), Type::I32),
            vibra_ir::Expr::literal(vibra_ir::Value::I32(1), first_origin.clone()),
            first_origin,
        )
        .expect("valid first function");
        let entry_origin = SourceOrigin::new("test.vib", ByteSpan::new(2, 3));
        let entry = CheckedFunction::new(
            "entry",
            FunctionSignature::new(Vec::new(), Type::I32),
            vibra_ir::Expr::literal(vibra_ir::Value::I32(2), entry_origin.clone()),
            entry_origin,
        )
        .expect("valid entry function");
        let program = vibra_ir::CheckedProgram::try_new(vec![first, entry], 1)
            .expect("valid checked program");

        let result = run(&program).expect("execution");

        assert_eq!(result.value(), Some(&vibra_ir::Value::I32(2)));
    }

    #[test]
    fn an_untaken_branch_does_not_force_a_global_initializer() {
        let origin = SourceOrigin::new("test.vib", ByteSpan::new(0, 1));
        let global = CheckedGlobal::new(
            "value",
            Type::I32,
            Expr::literal(Value::I32(9), origin.clone()),
            origin.clone(),
        )
        .expect("valid checked global shape");
        let body = Expr::if_expression(
            Expr::literal(Value::Bool(true), origin.clone()),
            Expr::literal(Value::I32(7), origin.clone()),
            Expr::global(0, Type::I32, origin.clone()),
            origin.clone(),
        );
        let function = CheckedFunction::new(
            "answer",
            FunctionSignature::new(Vec::new(), Type::I32),
            body,
            origin,
        )
        .expect("valid checked function");
        let program = vibra_ir::CheckedProgram::try_new_with_globals(
            vec![global],
            vec![function],
            0,
        )
        .expect("valid checked program");

        let mut machine = super::Machine::new(&program, false);
        let result = machine
            .evaluate_function(
                0,
                vec![None; program.entry().slot_count()],
                Vec::new(),
                std::sync::Arc::default(),
            )
            .expect("untaken branch must not execute");

        assert_eq!(result, super::RuntimeValue::Primitive(Value::I32(7)));
        assert_eq!(machine.globals[0], super::GlobalState::Uninitialized);
    }

    #[test]
    fn compiler_text_intrinsics_execute_without_audit_events() {
        let origin = SourceOrigin::new("external.vib", ByteSpan::new(0, 1));
        let concat = Expr::external(
            vibra_ir::external::CompilerIntrinsic::TextConcat,
            vec![
                Expr::literal(Value::Str("A😀".to_owned()), origin.clone()),
                Expr::literal(Value::Str("Ω".to_owned()), origin.clone()),
            ],
            origin.clone(),
        );
        let length = Expr::external(
            vibra_ir::external::CompilerIntrinsic::TextLength,
            vec![concat],
            origin.clone(),
        );
        let function = CheckedFunction::new(
            "answer",
            FunctionSignature::new(Vec::new(), Type::U64),
            length,
            origin,
        )
        .expect("valid intrinsic function");
        let program = vibra_ir::CheckedProgram::try_new(vec![function], 0)
            .expect("valid intrinsic program");
        let result = run(&program).expect("execution");
        let repeated = run(&program).expect("repeated execution");
        assert_eq!(result.value(), Some(&Value::U64(3)));
        assert!(result.audit_trace().is_empty());
        assert_eq!(result, repeated);
    }

    /// `(defn f () i32 (let v (f)) v)`: every activation stays live.
    fn non_tail_self_recursion() -> vibra_ir::CheckedProgram {
        let origin = SourceOrigin::new("deep.vib", ByteSpan::new(0, 1));
        let body = Expr::let_binding(
            Some(0),
            Expr::call(0, Vec::new(), Type::I32, origin.clone()),
            Expr::variable(0, Type::I32, origin.clone()),
            origin.clone(),
        );
        let function = CheckedFunction::with_slots(
            "f",
            FunctionSignature::new(Vec::new(), Type::I32),
            body,
            origin,
            1,
        )
        .expect("valid recursive function");
        vibra_ir::CheckedProgram::try_new(vec![function], 0).expect("valid program")
    }

    #[test]
    fn non_tail_recursion_stops_at_the_host_activation_bound() {
        let program = non_tail_self_recursion();
        assert_eq!(
            run(&program),
            Err(super::RuntimeError::HostStackExhausted {
                limit: super::MAX_ACTIVATION_DEPTH
            })
        );
        assert_eq!(
            super::Interpreter::run_test(&program).map(|_| ()),
            Err(super::RuntimeError::InvalidBody {
                function: "f".to_owned()
            }),
            "a non-void test entry is rejected before execution"
        );
    }

    #[test]
    fn the_activation_bound_counts_live_activations_only() {
        let program = non_tail_self_recursion();
        let mut machine = super::Machine::new(&program, false);
        for _ in 0..super::MAX_ACTIVATION_DEPTH {
            assert!(machine.enter_activation().is_some());
        }
        assert!(machine.enter_activation().is_none());
        assert!(machine.check_host_budget().is_err());

        let mut machine = super::Machine::new(&program, false);
        for _ in 0..super::MAX_ACTIVATION_DEPTH * 2 {
            assert!(machine.enter_activation().is_some());
            machine.leave_activation();
        }
        assert_eq!(machine.max_depth, 1);
        assert!(machine.check_host_budget().is_ok());
    }
}
