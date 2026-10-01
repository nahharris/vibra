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

use std::fmt;
use std::sync::Arc;

use vibra_ir::{
    CallTarget, CheckedProgram, ClosedContract, Expr, FunctionSignature, MatchArm,
    ObservedValue, Pattern, SourceOrigin, TestAssertion, Type, TypeId, Value,
};

/// One successful reference-interpreter run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Execution {
    value: ObservedValue,
    value_type: Type,
    audit_trace: Vec<String>,
    max_activation_depth: usize,
    tail_transfer_count: usize,
}

impl Execution {
    /// The value returned by the selected entry function.
    #[must_use]
    pub const fn value(&self) -> &ObservedValue {
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
        self.value.canonical_observation(&self.value_type)
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
        let Some(value) = value else {
            return Err(invalid());
        };
        let value_type = function.signature().result();
        if !admits_value(&value_type, &value) {
            return Err(invalid());
        }
        let Some(value) = observe(value) else {
            return Err(invalid());
        };
        Ok(Execution {
            value,
            value_type,
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
    Map {
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
    /// The value a failing `try` returns from the innermost function or
    /// `lambda`: evaluation unwinds to that boundary, which takes it.
    pending_exit: Option<RuntimeValue>,
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
            pending_exit: None,
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
                None => break self.pending_exit.take(),
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
        result: &Type,
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
                if !signature.admits(actual_signature)
                    || actual_signature.fixed_parameter_count() != values.len()
                    || !values_match_signature(&values, actual_signature)
                    || !result.admits(&actual_signature.result())
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
                result,
                ..
            } => self.evaluate_external(*intrinsic, arguments, result, slots, captures),
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
            Expr::Function { function, .. } => self
                .named_callable(*function)
                .map(|callable| Evaluation::Value(RuntimeValue::Function(callable))),
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
            } => self.evaluate_record(value_type, fields, slots, captures),
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
                let payload = match payload {
                    Some(payload) => {
                        Some(Box::new(self.evaluate_value(payload, slots, captures)?))
                    }
                    None => None,
                };
                Some(Evaluation::Value(RuntimeValue::Enum {
                    value_type: value_type.clone(),
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
                    value_type: value_type.clone(),
                    value: Box::new(value),
                }))
            }
            Expr::Widen {
                value_type,
                value,
                member,
                ..
            } => {
                let member_type = value.result_type();
                let value = self.evaluate_value(value, slots, captures)?;
                Some(Evaluation::Value(match member {
                    // Atom widening is erased.
                    None => value,
                    Some(member) => RuntimeValue::Union {
                        value_type: value_type.clone(),
                        member: *member,
                        member_type,
                        value: Box::new(value),
                    },
                }))
            }
            Expr::Try {
                value, exit_type, ..
            } => self.evaluate_try(value, exit_type, slots, captures),
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
            | Expr::Map { .. }
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

    /// Evaluates tuple, array, and map construction, tuple projection, and
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
                value_type: value_type.clone(),
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
                value_type: value_type.clone(),
                values: self.evaluate_all(elements, slots, captures)?,
            },
            Expr::Map {
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
                RuntimeValue::Map {
                    value_type: value_type.clone(),
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
                    RuntimeValue::Map { entries, .. } => self
                        .search_entries(&entries, &key, key_order.as_ref())?
                        .ok()
                        .and_then(|position| entries.into_iter().nth(position))
                        .map(|(_, value)| value),
                    collection => lookup(collection, &key),
                };
                RuntimeValue::Enum {
                    value_type: value_type.clone(),
                    variant: if found.is_some() { "some" } else { "none" }.to_owned(),
                    payload: found.map(Box::new),
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

    /// Orders two keys of one map (`docs/spec/02-type-system.md`, "Nominal
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
            let callable = self.named_callable(function)?;
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

    /// Inserts one entry into a map's sorted entries; a repeated key keeps
    /// its first position and takes the later value.
    fn insert_entry(
        &mut self,
        entries: &mut Vec<(RuntimeValue, RuntimeValue)>,
        key: RuntimeValue,
        value: RuntimeValue,
        key_order: Option<&TypeId>,
    ) -> Option<()> {
        match self.search_entries(entries, &key, key_order)? {
            Ok(position) => {
                if let Some(entry) = entries.get_mut(position) {
                    entry.1 = value;
                }
            }
            Err(position) => entries.insert(position, (key, value)),
        }
        Some(())
    }

    /// Binary search over sorted map entries, with a comparison that may run
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
        })
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
            (CompilerIntrinsic::ArrayOf | CompilerIntrinsic::MapOf, [tail]) => {
                tail.clone()
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
            "some" | "ok" => payload.map(|payload| Evaluation::Value(*payload)),
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
            closed,
            ..
        } = target
        {
            let values = self.evaluate_all(arguments, slots, captures)?;
            let receiver = values.get(*receiver)?;
            // A written member for the receiver's type wins over the
            // interface's default, whose receiver is the open `self`.
            let candidates = self
                .program
                .functions()
                .iter()
                .enumerate()
                .filter_map(|(index, function)| {
                    let implements = function.implements()?;
                    (implements.interface == *interface
                        && implements.member == *member
                        && admits_value(&implements.receiver, receiver))
                    .then_some((index, matches!(implements.receiver, Type::Param(_))))
                })
                .collect::<Vec<_>>();
            let Some((index, _)) = candidates
                .iter()
                .find(|(_, default)| !default)
                .or_else(|| candidates.first())
                .copied()
            else {
                let closed = (*closed)?;
                let [left, right] = values.as_slice() else {
                    return None;
                };
                // A structure holding a user key orders through its `compare`.
                let key_order =
                    (closed == ClosedContract::KeyCompare).then_some(interface);
                let order = self.compare_keys(left, right, key_order)?;
                return Some(Evaluation::Value(closed_contract(closed, order, result)));
            };
            let callable = self.named_callable(index)?;
            return self
                .invoke_callable(callable, values, result)
                .map(Evaluation::Value);
        }
        let callable = match target {
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
                || !result.admits(&callable_signature.result())
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
        // Every operand is compared and reported by its canonical encoding;
        // the checker rejects a function operand.
        let encodings = values
            .into_iter()
            .map(|value| observe(value).map(|value| value.canonical_vibon()))
            .collect::<Option<Vec<_>>>()?;
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
            } => {
                let function = self.program.functions().get(index)?;
                if !signature.admits(function.signature()) {
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
                match evaluation {
                    Some(Evaluation::Value(value)) => Some(value),
                    Some(
                        Evaluation::TestAssertionFailed
                        | Evaluation::TailTransfer { .. },
                    ) => None,
                    None => self.pending_exit.take(),
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

/// Whether a slot of type `expected` may hold `value`. An atom value's own
/// type is its singleton, which the erased `atom` type also holds.
fn admits_value(expected: &Type, value: &RuntimeValue) -> bool {
    matches!(
        (expected, value),
        (Type::Atom, RuntimeValue::Primitive(Value::Atom(_)))
    ) || expected.admits(&runtime_type(value))
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
        | RuntimeValue::Map { value_type, .. }
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
        RuntimeValue::Map { entries, .. } => ObservedValue::Map(
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
                    (Some(_), None) => false,
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
/// variant then payload. Keys of one map share one type, so values of
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
        ClosedContract::KeyEqual => RuntimeValue::Primitive(Value::Bool(order.is_eq())),
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
        assert_eq!(result.value(), &vibra_ir::Value::I32(42));
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

        assert_eq!(result.value(), &vibra_ir::Value::I32(2));
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
            FunctionSignature::new(Vec::new(), Type::U64),
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
