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

use std::fmt;
use std::sync::Arc;

use vibra_ir::{
    CheckedProgram, Expr, FunctionSignature, PrimitiveType, SourceOrigin,
    TestAssertion, Value,
};

/// One successful reference-interpreter run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Execution {
    value: Value,
    audit_trace: Vec<String>,
    max_activation_depth: usize,
    tail_transfer_count: usize,
}

impl Execution {
    /// The value returned by the selected entry function.
    #[must_use]
    pub const fn value(&self) -> &Value {
        &self.value
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
        self.value.canonical_observation()
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
    expected: Value,
    actual: Value,
    origin: SourceOrigin,
}

impl TestAssertionFailure {
    /// Canonical assertion member identity.
    #[must_use]
    pub fn assertion(&self) -> &str {
        &self.assertion
    }

    /// Expected assertion operand or required boolean.
    #[must_use]
    pub const fn expected(&self) -> &Value {
        &self.expected
    }

    /// Actual assertion operand.
    #[must_use]
    pub const fn actual(&self) -> &Value {
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
/// stack. Tail transfers within a recursive group reuse their activation and
/// never approach it.
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
        );
        machine.check_host_budget()?;
        let Some(RuntimeValue::Primitive(value)) = value else {
            return Err(invalid());
        };
        if !value.ty().same_shape(&function.signature().result()) {
            return Err(invalid());
        }
        Ok(Execution {
            value,
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
        if function.signature().result() != PrimitiveType::Void {
            return Err(invalid());
        }
        let mut machine = Machine::new(program, true);
        let value = machine.evaluate_function(
            program.entry_index(),
            entry_frame(program),
            Vec::new(),
        );
        machine.check_host_budget()?;
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
enum GlobalState {
    Uninitialized,
    Evaluating,
    Ready(RuntimeValue),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum RuntimeValue {
    Primitive(Value),
    Function(Callable),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Evaluation {
    Value(RuntimeValue),
    TestAssertionFailed,
    TailTransfer {
        callable: Callable,
        values: Vec<RuntimeValue>,
        result: PrimitiveType,
    },
}

enum TailTransferAction {
    Reuse {
        index: usize,
        slots: Vec<Option<RuntimeValue>>,
        captures: Vec<RuntimeValue>,
    },
    Invoke {
        callable: Callable,
        values: Vec<RuntimeValue>,
    },
    Invalid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Callable {
    Named {
        index: usize,
        signature: FunctionSignature,
        captures: Vec<RuntimeValue>,
    },
    Lambda {
        signature: FunctionSignature,
        /// Activation slots, validated by checked IR to cover the body.
        slot_count: usize,
        /// Shared with the checked program; creating a closure never copies it.
        body: Arc<Expr>,
        captures: Vec<RuntimeValue>,
    },
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
        }
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
    ) -> Option<RuntimeValue> {
        self.enter_activation()?;
        let mut index = index;
        let mut slots = slots;
        let mut captures = captures;
        let result = loop {
            // Copy the program reference before borrowing a function body so
            // the mutable machine borrow used by evaluation remains disjoint.
            let program = self.program;
            let Some(function) = program.functions().get(index) else {
                break None;
            };
            if slots.len() != function.slot_count()
                || !slots_match_signature(&slots, function.signature())
            {
                break None;
            }
            let evaluation = self.evaluate(function.body(), &mut slots, &captures);
            match evaluation {
                Some(Evaluation::Value(value)) => break Some(value),
                Some(Evaluation::TestAssertionFailed) => break None,
                Some(Evaluation::TailTransfer {
                    callable,
                    values,
                    result,
                }) => {
                    match self.tail_transfer_action(index, callable, values, &result) {
                        TailTransferAction::Reuse {
                            index: next_index,
                            slots: next_slots,
                            captures: next_captures,
                        } => {
                            self.tail_transfers = self.tail_transfers.saturating_add(1);
                            index = next_index;
                            slots = next_slots;
                            captures = next_captures;
                        }
                        TailTransferAction::Invoke { callable, values } => {
                            break self.invoke_callable(callable, values, &result);
                        }
                        TailTransferAction::Invalid => break None,
                    }
                }
                None => break None,
            }
        };
        self.leave_activation();
        result
    }

    fn tail_transfer_action(
        &self,
        current_index: usize,
        callable: Callable,
        values: Vec<RuntimeValue>,
        result: &PrimitiveType,
    ) -> TailTransferAction {
        match callable {
            Callable::Named {
                index,
                signature,
                captures,
            } => {
                let Some(function) = self.program.functions().get(index) else {
                    return TailTransferAction::Invalid;
                };
                let actual_signature = function.signature();
                if !signature.same_shape(actual_signature)
                    || actual_signature.fixed_parameter_count() != values.len()
                    || !values_match_signature(&values, actual_signature)
                    || !actual_signature.result().same_shape(result)
                {
                    return TailTransferAction::Invalid;
                }
                if self.program.in_recursive_group(current_index, index) {
                    TailTransferAction::Reuse {
                        index,
                        slots: activation_slots(values, function.slot_count()),
                        captures,
                    }
                } else {
                    TailTransferAction::Invoke {
                        callable: Callable::Named {
                            index,
                            signature,
                            captures,
                        },
                        values,
                    }
                }
            }
            callable => TailTransferAction::Invoke { callable, values },
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
                ..
            } => self.evaluate_external(*intrinsic, arguments, slots, captures),
            Expr::Default { .. } => None,
            Expr::Sequence { expressions, .. } => {
                self.evaluate_sequence(expressions, slots, captures)
            }
            Expr::Variable {
                slot, value_type, ..
            } => slots
                .get(*slot)
                .and_then(Option::as_ref)
                .filter(|value| runtime_type(value).same_shape(value_type))
                .cloned()
                .map(Evaluation::Value),
            Expr::Global {
                index, value_type, ..
            } => self
                .evaluate_global(*index)
                .filter(|value| runtime_type(value).same_shape(value_type))
                .map(Evaluation::Value),
            Expr::Function { function, .. } => self
                .named_callable(*function)
                .map(|callable| Evaluation::Value(RuntimeValue::Function(callable))),
            Expr::Captured {
                slot, value_type, ..
            } => captures
                .get(*slot)
                .filter(|value| runtime_type(value).same_shape(value_type))
                .cloned()
                .map(Evaluation::Value),
            Expr::Closure { .. } => self.evaluate_closure(expression, slots, captures),
            Expr::Let {
                slot, value, body, ..
            } => self.evaluate_let(*slot, value, body, slots, captures),
            Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => self.evaluate_if(condition, then_branch, else_branch, slots, captures),
            Expr::Call {
                function,
                arguments,
                result,
                callee,
                tail,
                origin,
                ..
            } => self.evaluate_call(
                *function,
                callee.as_deref(),
                arguments,
                result,
                *tail,
                origin,
                slots,
                captures,
            ),
        }
    }

    fn named_callable(&self, function: usize) -> Option<Callable> {
        Some(Callable::Named {
            index: function,
            signature: self.program.functions().get(function)?.signature().clone(),
            captures: Vec::new(),
        })
    }

    #[inline(never)]
    fn evaluate_external(
        &mut self,
        intrinsic: vibra_ir::external::CompilerIntrinsic,
        arguments: &[Expr],
        slots: &mut Frame,
        captures: &[RuntimeValue],
    ) -> Option<Evaluation> {
        let mut primitives = Vec::with_capacity(arguments.len());
        for argument in arguments {
            let RuntimeValue::Primitive(value) =
                self.evaluate_value(argument, slots, captures)?
            else {
                return None;
            };
            primitives.push(value);
        }
        let value = match intrinsic {
            vibra_ir::external::CompilerIntrinsic::TextConcat => {
                let [Value::Str(left), Value::Str(right)] = primitives.as_slice()
                else {
                    return None;
                };
                Value::Str(format!("{left}{right}"))
            }
            vibra_ir::external::CompilerIntrinsic::TextLength => {
                let [Value::Str(value)] = primitives.as_slice() else {
                    return None;
                };
                Value::U64(value.chars().count() as u64)
            }
        };
        Some(Evaluation::Value(RuntimeValue::Primitive(value)))
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
        Some(Evaluation::Value(RuntimeValue::Function(
            Callable::Lambda {
                signature: signature.clone(),
                slot_count: *slot_count,
                body: Arc::clone(body),
                captures: environment,
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
        function: usize,
        callee: Option<&Expr>,
        arguments: &[Expr],
        result: &PrimitiveType,
        tail: bool,
        origin: &SourceOrigin,
        slots: &mut Frame,
        captures: &[RuntimeValue],
    ) -> Option<Evaluation> {
        let callable = if let Some(callee) = callee {
            let RuntimeValue::Function(callable) =
                self.evaluate_value(callee, slots, captures)?
            else {
                return None;
            };
            callable
        } else {
            self.named_callable(function)?
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
                if !default.ty().same_shape(&argument.result_type()) {
                    return None;
                }
                values.push(RuntimeValue::Primitive(default));
            } else {
                values.push(self.evaluate_value(argument, slots, captures)?);
            }
        }
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
        if tail {
            let callable_signature = match &callable {
                Callable::Named { signature, .. }
                | Callable::Lambda { signature, .. } => signature,
            };
            if callable_signature.fixed_parameter_count() != values.len()
                || !values_match_signature(&values, callable_signature)
                || !callable_signature.result().same_shape(result)
            {
                return None;
            }
            return Some(Evaluation::TailTransfer {
                callable,
                values,
                result: result.clone(),
            });
        }
        self.invoke_callable(callable, values, result)
            .map(Evaluation::Value)
    }

    fn invoke_test_assertion(
        &mut self,
        assertion: TestAssertion,
        values: Vec<RuntimeValue>,
        origin: SourceOrigin,
    ) -> Option<RuntimeValue> {
        let values = values
            .into_iter()
            .map(|value| match value {
                RuntimeValue::Primitive(value) => Some(value),
                RuntimeValue::Function(_) => None,
            })
            .collect::<Option<Vec<_>>>()?;
        let (passed, expected, actual) = match (assertion, values.as_slice()) {
            (TestAssertion::True, [Value::Bool(actual)]) => {
                (*actual, Value::Bool(true), Value::Bool(*actual))
            }
            (TestAssertion::False, [Value::Bool(actual)]) => {
                (!*actual, Value::Bool(false), Value::Bool(*actual))
            }
            (assertion, [left, right])
                if !matches!(assertion, TestAssertion::True | TestAssertion::False) =>
            {
                (left == right, left.clone(), right.clone())
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
        result: &PrimitiveType,
    ) -> Option<RuntimeValue> {
        let signature = match &callable {
            Callable::Named { signature, .. } | Callable::Lambda { signature, .. } => {
                signature
            }
        };
        if signature.fixed_parameter_count() != values.len()
            || !values_match_signature(&values, signature)
            || !signature.result().same_shape(result)
        {
            return None;
        }
        match callable {
            Callable::Named {
                index,
                signature,
                captures,
            } => {
                let function = self.program.functions().get(index)?;
                if !function.signature().same_shape(&signature) {
                    return None;
                }
                let slots = activation_slots(values, function.slot_count());
                self.evaluate_function(index, slots, captures)
            }
            Callable::Lambda {
                slot_count,
                body,
                captures,
                ..
            } => {
                let mut slots = activation_slots(values, slot_count);
                self.enter_activation()?;
                let evaluation = self.evaluate(&body, &mut slots, &captures);
                self.leave_activation();
                match evaluation? {
                    Evaluation::Value(value) => Some(value),
                    Evaluation::TestAssertionFailed
                    | Evaluation::TailTransfer { .. } => None,
                }
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
        let value = self.evaluate_value(global.initializer(), &mut slots, &[]);
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

fn runtime_type(value: &RuntimeValue) -> PrimitiveType {
    match value {
        RuntimeValue::Primitive(value) => value.ty(),
        RuntimeValue::Function(Callable::Named { signature, .. }) => {
            PrimitiveType::Function(Box::new(signature.clone()))
        }
        RuntimeValue::Function(Callable::Lambda { signature, .. }) => {
            PrimitiveType::Function(Box::new(signature.clone()))
        }
    }
}

fn slots_match_signature(
    slots: &[Option<RuntimeValue>],
    signature: &FunctionSignature,
) -> bool {
    let expected = signature
        .parameters()
        .iter()
        .cloned()
        .chain(
            signature
                .labelled()
                .iter()
                .map(|parameter| parameter.value_type()),
        )
        .collect::<Vec<_>>();
    slots
        .iter()
        .take(expected.len())
        .zip(expected)
        .all(|(value, expected)| {
            value
                .as_ref()
                .is_some_and(|value| runtime_type(value).same_shape(&expected))
        })
}

fn values_match_signature(
    values: &[RuntimeValue],
    signature: &FunctionSignature,
) -> bool {
    let expected = signature.parameters().iter().cloned().chain(
        signature
            .labelled()
            .iter()
            .map(|parameter| parameter.value_type()),
    );
    values
        .iter()
        .zip(expected)
        .all(|(value, expected)| runtime_type(value).same_shape(&expected))
}

#[cfg(test)]
mod tests {
    use vibra_diagnostics::ByteSpan;
    use vibra_ir::{
        CheckedFunction, CheckedGlobal, Expr, FunctionSignature, PrimitiveType,
        SourceOrigin, Value,
    };

    use super::run;

    #[test]
    fn a_checked_literal_has_no_audit_events() {
        let origin = SourceOrigin::new("test.vib", ByteSpan::new(0, 1));
        let function = CheckedFunction::new(
            "answer",
            FunctionSignature::new(Vec::new(), PrimitiveType::I32),
            vibra_ir::Expr::literal(vibra_ir::Value::I32(42), origin.clone()),
            origin,
        )
        .expect("valid checked function");
        let program = vibra_ir::CheckedProgram::try_new(vec![function], 0)
            .expect("valid checked program");
        let result = run(&program).expect("execution");
        assert_eq!(result.value(), &vibra_ir::Value::I32(42));
        assert!(result.audit_trace().is_empty());
    }

    #[test]
    fn execution_uses_the_checked_entry_index() {
        let first_origin = SourceOrigin::new("test.vib", ByteSpan::new(0, 1));
        let first = CheckedFunction::new(
            "first",
            FunctionSignature::new(Vec::new(), PrimitiveType::I32),
            vibra_ir::Expr::literal(vibra_ir::Value::I32(1), first_origin.clone()),
            first_origin,
        )
        .expect("valid first function");
        let entry_origin = SourceOrigin::new("test.vib", ByteSpan::new(2, 3));
        let entry = CheckedFunction::new(
            "entry",
            FunctionSignature::new(Vec::new(), PrimitiveType::I32),
            vibra_ir::Expr::literal(vibra_ir::Value::I32(2), entry_origin.clone()),
            entry_origin,
        )
        .expect("valid entry function");
        let program = vibra_ir::CheckedProgram::try_new(vec![first, entry], 1)
            .expect("valid checked program");

        let result = run(&program).expect("execution");

        assert_eq!(result.value(), &vibra_ir::Value::I32(2));
    }

    #[test]
    fn an_untaken_branch_does_not_force_a_global_initializer() {
        let origin = SourceOrigin::new("test.vib", ByteSpan::new(0, 1));
        let global = CheckedGlobal::new(
            "value",
            PrimitiveType::I32,
            Expr::literal(Value::I32(9), origin.clone()),
            origin.clone(),
        )
        .expect("valid checked global shape");
        let body = Expr::if_expression(
            Expr::literal(Value::Bool(true), origin.clone()),
            Expr::literal(Value::I32(7), origin.clone()),
            Expr::global(0, PrimitiveType::I32, origin.clone()),
            origin.clone(),
        );
        let function = CheckedFunction::new(
            "answer",
            FunctionSignature::new(Vec::new(), PrimitiveType::I32),
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
            .evaluate_function(0, vec![None; program.entry().slot_count()], Vec::new())
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
            FunctionSignature::new(Vec::new(), PrimitiveType::U64),
            length,
            origin,
        )
        .expect("valid intrinsic function");
        let program = vibra_ir::CheckedProgram::try_new(vec![function], 0)
            .expect("valid intrinsic program");
        let result = run(&program).expect("execution");
        let repeated = run(&program).expect("repeated execution");
        assert_eq!(result.value(), &Value::U64(3));
        assert!(result.audit_trace().is_empty());
        assert_eq!(result, repeated);
    }

    /// `(defn f () i32 (let v (f) v))`: every activation stays live.
    fn non_tail_self_recursion() -> vibra_ir::CheckedProgram {
        let origin = SourceOrigin::new("deep.vib", ByteSpan::new(0, 1));
        let body = Expr::let_binding(
            Some(0),
            Expr::call(0, Vec::new(), PrimitiveType::I32, origin.clone()),
            Expr::variable(0, PrimitiveType::I32, origin.clone()),
            origin.clone(),
        );
        let function = CheckedFunction::with_slots(
            "f",
            FunctionSignature::new(Vec::new(), PrimitiveType::I32),
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
