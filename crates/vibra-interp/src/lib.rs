//! Deterministic execution for the checked M2 function and binding IR.
//!
//! No parser, resolver, type checker, filesystem, clock, random source, or
//! host provider is reachable from this crate.  The only executable input is
//! [`vibra_ir::CheckedProgram`], which is the controlled boundary produced by
//! `vibra-types`.
//!
//! Activations are held on the heap, so the depth of non-tail recursion is
//! bounded by the memory budget a runner supplies and not by the host stack
//! (see [`MemoryBudget`] and `docs/spec/06-runtime.md`, "Activations and
//! memory").

#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used
    )
)]

mod machine;
mod registry;
mod value;

use std::fmt;

use vibra_ir::{
    CheckedProgram, ClosedContract, FunctionSignature, ObservedValue, Pattern,
    SourceOrigin, Type, TypeId, Value,
};

use machine::{Halt, Machine};
use value::{RuntimeValue, TypeMap};

/// The memory an instance may hold, in bytes of live frames, continuations,
/// and values, as the interpreter counts them.
///
/// The runner supplies it: `docs/spec/06-runtime.md` defines no portable
/// limit, because the limit that produces `@runtime.memory-exhausted` belongs
/// to the embedding host. It applies to the whole run, and exceeding it is
/// [`RuntimeError::MemoryExhausted`]. The count is a model of the interpreter's
/// own storage, not a measurement of the process: frames and continuations
/// are counted exactly, and values are measured whenever enough has been
/// allocated since the last measurement to matter, so the budget is enforced
/// to within a bounded fraction of what remains of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MemoryBudget {
    bytes: usize,
}

impl MemoryBudget {
    /// The budget `run` and `run_test` use when a caller names none: large
    /// enough for recursion hundreds of thousands of activations deep, small enough that
    /// a program with no base case stops in seconds.
    pub const DEFAULT: Self = Self::new(256 * 1024 * 1024);

    /// A budget of `bytes`.
    #[must_use]
    pub const fn new(bytes: usize) -> Self {
        Self { bytes }
    }

    /// The budget in bytes.
    #[must_use]
    pub const fn bytes(self) -> usize {
        self.bytes
    }
}

impl Default for MemoryBudget {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// One successful reference-interpreter run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Execution {
    /// The entry's value when it is a primitive.
    value: Option<Value>,
    /// The canonical observation of the entry's value.
    canonical: String,
    audit_trace: Vec<String>,
    max_activation_depth: usize,
    tail_transfer_count: usize,
    peak_memory_bytes: usize,
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

    /// The most memory the run held at once, in the units of
    /// [`MemoryBudget`]: a budget of exactly this completes the run, and a
    /// smaller one ends it with [`RuntimeError::MemoryExhausted`] when the
    /// run is made of activations alone. This is host-side instrumentation
    /// and is not part of a Vibra result.
    #[must_use]
    pub const fn peak_memory_bytes(&self) -> usize {
        self.peak_memory_bytes
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
    /// The instance exhausted the memory budget its runner supplied. This is
    /// a host event, not a trap or a portable language result.
    MemoryExhausted {
        /// The budget that was exceeded.
        budget: MemoryBudget,
    },
    /// A value holding a function reached an observation: a test assertion's
    /// operand or the entry's result. The checker rejects a type that names a
    /// function there; this is one hidden behind `any` or an interface.
    UnobservableFunction {
        /// The observing call, when it is in source.
        origin: Option<SourceOrigin>,
    },
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
            Self::MemoryExhausted { budget } => write!(
                formatter,
                "the instance exhausted its memory budget of {} bytes",
                budget.bytes()
            ),
            Self::UnobservableFunction { .. } => formatter.write_str(
                "a value that holds a function has no canonical encoding to observe",
            ),
        }
    }
}

impl std::error::Error for RuntimeError {}

impl RuntimeError {
    /// Whether the host, rather than the checked program, stopped execution.
    #[must_use]
    pub const fn is_host_event(&self) -> bool {
        matches!(self, Self::MemoryExhausted { .. })
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
            Self::MemoryExhausted { .. } => None,
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

    /// The registered unlocated diagnostic for a host event, if this is one.
    #[must_use]
    pub fn host_diagnostic(&self) -> Option<vibra_diagnostics::Diagnostic> {
        matches!(self, Self::MemoryExhausted { .. }).then(|| {
            vibra_diagnostics::Diagnostic::new(
                vibra_diagnostics::DiagnosticCode::RuntimeMemoryExhausted,
                vibra_diagnostics::ByteSpan::empty_at(0),
                self.to_string(),
            )
        })
    }
}

/// The pure M2 reference interpreter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Interpreter;

impl Interpreter {
    /// Executes the checked program's validated entry function under
    /// [`MemoryBudget::DEFAULT`].
    pub fn run(program: &CheckedProgram) -> Result<Execution, RuntimeError> {
        Self::run_with_budget(program, MemoryBudget::DEFAULT)
    }

    /// Executes the checked program's validated entry function, ending in
    /// [`RuntimeError::MemoryExhausted`] if the instance outgrows `budget`.
    pub fn run_with_budget(
        program: &CheckedProgram,
        budget: MemoryBudget,
    ) -> Result<Execution, RuntimeError> {
        let function = program.entry();
        let invalid = || RuntimeError::InvalidBody {
            function: function.name().to_owned(),
        };
        if function.test_assertion().is_some() {
            return Err(invalid());
        }
        let mut machine = Machine::new(program, false, budget.bytes());
        let value = match machine.run_entry(program.entry_index(), entry_frame(program))
        {
            Ok(value) => value,
            Err(Halt::Memory) => return Err(RuntimeError::MemoryExhausted { budget }),
            Err(Halt::Invalid | Halt::Assertion) => return Err(invalid()),
        };
        let value_type = function.signature().result();
        if !admits_value(&value_type, &value) {
            return Err(invalid());
        }
        let Some(value) = observe(value) else {
            return Err(RuntimeError::UnobservableFunction { origin: None });
        };
        let canonical = value.canonical_observation(&value_type);
        Ok(Execution {
            value: value.as_primitive().cloned(),
            canonical,
            audit_trace: Vec::new(),
            max_activation_depth: machine.max_depth,
            tail_transfer_count: machine.tail_transfers,
            peak_memory_bytes: machine.peak_memory_bytes(),
        })
    }

    /// Executes one checked void test entry with the verified assertion path
    /// under [`MemoryBudget::DEFAULT`].
    ///
    /// Each call constructs fresh module-value and trace state. A false
    /// assertion is returned as test data and does not become a runtime error.
    pub fn run_test(program: &CheckedProgram) -> Result<TestExecution, RuntimeError> {
        Self::run_test_with_budget(program, MemoryBudget::DEFAULT)
    }

    /// Executes one checked void test entry, ending in
    /// [`RuntimeError::MemoryExhausted`] if the test outgrows `budget`.
    pub fn run_test_with_budget(
        program: &CheckedProgram,
        budget: MemoryBudget,
    ) -> Result<TestExecution, RuntimeError> {
        let function = program.entry();
        let invalid = || RuntimeError::InvalidBody {
            function: function.name().to_owned(),
        };
        if function.signature().result() != Type::Void {
            return Err(invalid());
        }
        let mut machine = Machine::new(program, true, budget.bytes());
        let outcome = machine.run_entry(program.entry_index(), entry_frame(program));
        if outcome == Err(Halt::Memory) {
            return Err(RuntimeError::MemoryExhausted { budget });
        }
        if let Some(origin) = machine.unobservable.take() {
            return Err(RuntimeError::UnobservableFunction {
                origin: Some(origin),
            });
        }
        if machine.assertion_failure.is_none()
            && !matches!(&outcome, Ok(RuntimeValue::Primitive(Value::Void)))
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

/// The entry activation: positional slots empty and labelled defaults bound.
fn entry_frame(program: &CheckedProgram) -> Vec<Option<RuntimeValue>> {
    let function = program.entry();
    let signature = function.signature();
    let mut slots = vec![None; function.slot_count()];
    for (offset, parameter) in signature.labelled().iter().enumerate() {
        if let Some(default) = parameter.default()
            && let Some(slot) = slots.get_mut(signature.parameters().len() + offset)
        {
            *slot = constant_value(program, default);
        }
    }
    slots
}

/// Convenience entry point for the reference interpreter.
pub fn run(program: &CheckedProgram) -> Result<Execution, RuntimeError> {
    Interpreter::run(program)
}

/// One activation's slots.
type Slots = [Option<RuntimeValue>];

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
        || match declared_value_type(value) {
            Some(value_type) => expected.admits(value_type),
            None => expected.admits(&runtime_type(value)),
        }
}

/// The type a compound value was built at, without copying it.
fn declared_value_type(value: &RuntimeValue) -> Option<&Type> {
    match value {
        RuntimeValue::Primitive(_) | RuntimeValue::Function(_) => None,
        RuntimeValue::Record { value_type, .. }
        | RuntimeValue::Enum { value_type, .. }
        | RuntimeValue::Wrapper { value_type, .. }
        | RuntimeValue::Tuple { value_type, .. }
        | RuntimeValue::Array { value_type, .. }
        | RuntimeValue::Dict { value_type, .. }
        | RuntimeValue::Union { value_type, .. } => Some(value_type),
    }
}

fn runtime_type(value: &RuntimeValue) -> Type {
    match value {
        RuntimeValue::Primitive(value) => value.ty(),
        RuntimeValue::Function(callable) => {
            Type::Function(Box::new(callable.signature().clone()))
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

/// Whether each operand fits the slot type at its position.
fn values_match_slots(values: &[RuntimeValue], slots: &[Type]) -> bool {
    values
        .iter()
        .zip(slots)
        .all(|(value, expected)| admits_value(expected, value))
}

/// The observable form of a runtime value; `None` when it is or contains a
/// function, which has no canonical encoding.
///
/// A value may be nested arbitrarily deep, so this walks it with an explicit
/// worklist: children are visited first and each node is built from its
/// finished children on the result stack.
fn observe(value: RuntimeValue) -> Option<ObservedValue> {
    enum Shape {
        Record {
            type_id: Option<TypeId>,
            names: Vec<String>,
        },
        Enum {
            type_id: Option<TypeId>,
            variant: String,
            payload: bool,
        },
        Wrapper(TypeId),
        Tuple {
            type_id: Option<TypeId>,
            count: usize,
        },
        Array(usize),
        Dict(usize),
        Union {
            type_id: Option<TypeId>,
            member: Type,
        },
    }
    enum Task {
        Visit(RuntimeValue),
        Build(Shape),
    }

    let mut tasks = vec![Task::Visit(value)];
    let mut done: Vec<ObservedValue> = Vec::new();
    while let Some(task) = tasks.pop() {
        match task {
            // Parts are shared, so the children are visited by handle.
            Task::Visit(value) => match &value {
                RuntimeValue::Primitive(primitive) => {
                    done.push(ObservedValue::Primitive(primitive.clone()));
                }
                RuntimeValue::Function(_) => return None,
                RuntimeValue::Record { value_type, fields } => {
                    tasks.push(Task::Build(Shape::Record {
                        type_id: declared_id(value_type),
                        names: fields.iter().map(|(name, _)| name.clone()).collect(),
                    }));
                    tasks.extend(
                        fields
                            .iter()
                            .rev()
                            .map(|(_, field)| Task::Visit(field.clone())),
                    );
                }
                RuntimeValue::Enum {
                    value_type,
                    variant,
                    payload,
                } => {
                    tasks.push(Task::Build(Shape::Enum {
                        type_id: declared_id(value_type),
                        variant: variant.clone(),
                        payload: payload.is_some(),
                    }));
                    if let Some(payload) = payload {
                        tasks.push(Task::Visit(RuntimeValue::clone(payload)));
                    }
                }
                RuntimeValue::Wrapper {
                    value_type,
                    value: inner,
                } => {
                    tasks.push(Task::Build(Shape::Wrapper(declared_id(value_type)?)));
                    tasks.push(Task::Visit(RuntimeValue::clone(inner)));
                }
                RuntimeValue::Tuple { value_type, values } => {
                    tasks.push(Task::Build(Shape::Tuple {
                        type_id: declared_id(value_type),
                        count: values.len(),
                    }));
                    tasks.extend(
                        values.iter().rev().map(|value| Task::Visit(value.clone())),
                    );
                }
                RuntimeValue::Array { values, .. } => {
                    tasks.push(Task::Build(Shape::Array(values.len())));
                    tasks.extend(
                        values.iter().rev().map(|value| Task::Visit(value.clone())),
                    );
                }
                RuntimeValue::Dict { entries, .. } => {
                    tasks.push(Task::Build(Shape::Dict(entries.len())));
                    for (key, entry) in entries.iter().rev() {
                        tasks.push(Task::Visit(entry.clone()));
                        tasks.push(Task::Visit(key.clone()));
                    }
                }
                RuntimeValue::Union {
                    value_type,
                    member_type,
                    value: inner,
                    ..
                } => {
                    tasks.push(Task::Build(Shape::Union {
                        type_id: declared_id(value_type),
                        member: Type::clone(member_type),
                    }));
                    tasks.push(Task::Visit(RuntimeValue::clone(inner)));
                }
            },
            Task::Build(shape) => {
                let count = match &shape {
                    Shape::Record { names, .. } => names.len(),
                    Shape::Enum { payload, .. } => usize::from(*payload),
                    Shape::Wrapper(_) | Shape::Union { .. } => 1,
                    Shape::Tuple { count, .. } => *count,
                    Shape::Array(count) => *count,
                    Shape::Dict(count) => count.saturating_mul(2),
                };
                let at = done.len().checked_sub(count)?;
                let mut items = done.split_off(at).into_iter();
                let built = match shape {
                    Shape::Record { type_id, names } => ObservedValue::Record {
                        type_id,
                        fields: names.into_iter().zip(items).collect(),
                    },
                    Shape::Enum {
                        type_id,
                        variant,
                        payload,
                    } => ObservedValue::Enum {
                        type_id,
                        variant,
                        payload: if payload {
                            Some(Box::new(items.next()?))
                        } else {
                            None
                        },
                    },
                    Shape::Wrapper(type_id) => ObservedValue::Wrapper {
                        type_id,
                        value: Box::new(items.next()?),
                    },
                    Shape::Tuple { type_id, .. } => ObservedValue::Tuple {
                        type_id,
                        values: items.collect(),
                    },
                    Shape::Array(_) => ObservedValue::Array(items.collect()),
                    Shape::Dict(_) => {
                        let mut entries = Vec::new();
                        while let (Some(key), Some(value)) =
                            (items.next(), items.next())
                        {
                            entries.push((key, value));
                        }
                        ObservedValue::Dict(entries)
                    }
                    Shape::Union { type_id, member } => ObservedValue::Union {
                        type_id,
                        member: Box::new(member),
                        value: Box::new(items.next()?),
                    },
                };
                done.push(built);
            }
        }
    }
    done.pop()
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
    let (element, values): (Type, Vec<RuntimeValue>) = match value {
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
        values: values.into(),
    })
}

/// The runtime value of a labelled default, which is a constant of any type.
/// It is built as the construction it came from builds it: a record in the
/// order of its type, a `void` payload as no payload.
pub(crate) fn constant_value(
    program: &CheckedProgram,
    constant: &vibra_ir::Constant,
) -> Option<RuntimeValue> {
    use vibra_ir::Constant;
    Some(match constant {
        Constant::Primitive(value) => RuntimeValue::Primitive(value.clone()),
        Constant::Tuple {
            value_type,
            components,
        } => RuntimeValue::Tuple {
            value_type: value_type.clone(),
            values: components
                .iter()
                .map(|component| constant_value(program, component))
                .collect::<Option<Vec<_>>>()?
                .into(),
        },
        Constant::Record { value_type, fields } => {
            let mut named = fields
                .iter()
                .map(|(name, field)| {
                    Some((name.clone(), constant_value(program, field)?))
                })
                .collect::<Option<Vec<_>>>()?;
            let order: Vec<&str> = match value_type {
                Type::Record(members) => {
                    members.iter().map(|(name, _)| name.as_str()).collect()
                }
                Type::Declared(id) | Type::Applied(id, _) => program
                    .types()
                    .iter()
                    .find(|definition| definition.id() == id)
                    .and_then(|definition| definition.record_fields())?
                    .iter()
                    .map(|(name, _)| name.as_str())
                    .collect(),
                _ => return None,
            };
            named.sort_by_key(|(name, _)| {
                order.iter().position(|member| member == name)
            });
            RuntimeValue::Record {
                value_type: value_type.clone(),
                fields: named.into(),
            }
        }
        Constant::Variant {
            value_type,
            variant,
            payload,
        } => RuntimeValue::Enum {
            value_type: value_type.clone(),
            variant: variant.clone(),
            payload: match payload {
                Some(payload) => present_payload(constant_value(program, payload)?),
                None => None,
            },
        },
        Constant::Wrap { value_type, value } => RuntimeValue::Wrapper {
            value_type: value_type.clone(),
            value: constant_value(program, value)?.into(),
        },
        Constant::Member {
            value_type,
            index,
            value,
        } => RuntimeValue::Union {
            value_type: value_type.clone(),
            member: *index,
            member_type: std::rc::Rc::new(value.value_type()),
            value: constant_value(program, value)?.into(),
        },
    })
}

/// The payload a variant stores for `value`: none for `void`, so a variant
/// whose payload slot is `void` has one shape however it was built
/// (`docs/spec/02-type-system.md`, "Nominal declarations").
pub(crate) fn present_payload(value: RuntimeValue) -> Option<value::Inner> {
    (value != RuntimeValue::Primitive(Value::Void)).then(|| value::Inner::new(value))
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
                    .zip(values.iter())
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

/// Stores every binder of a pattern that already matched `value`. A binder
/// takes a handle to its part of the value, which shares it.
fn bind_pattern(
    pattern: &Pattern,
    value: &RuntimeValue,
    slots: &mut Slots,
) -> Option<()> {
    match (pattern, value) {
        (Pattern::Wildcard | Pattern::Literal(_), _) => Some(()),
        (Pattern::Bind { slot, .. }, value) => {
            *slots.get_mut(*slot)? = Some(value.clone());
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
        ) => bind_pattern(pattern, payload, slots),
        (
            Pattern::Variant {
                payload: Some(pattern),
                ..
            },
            RuntimeValue::Enum { payload: None, .. },
        ) => bind_pattern(pattern, &RuntimeValue::Primitive(Value::Void), slots),
        (Pattern::Variant { payload: None, .. }, RuntimeValue::Enum { .. }) => Some(()),
        (Pattern::Record(expected), RuntimeValue::Record { fields, .. }) => {
            for (name, pattern) in expected {
                let (_, field) = fields.iter().find(|(field, _)| field == name)?;
                bind_pattern(pattern, field, slots)?;
            }
            Some(())
        }
        (Pattern::Tuple(expected), RuntimeValue::Tuple { values, .. })
        | (Pattern::Array(expected), RuntimeValue::Array { values, .. }) => {
            for (pattern, value) in expected.iter().zip(values.iter()) {
                bind_pattern(pattern, value, slots)?;
            }
            Some(())
        }
        (Pattern::Wrap(inner), RuntimeValue::Wrapper { value, .. }) => {
            bind_pattern(inner, value, slots)
        }
        (
            Pattern::Wrap(inner),
            value @ RuntimeValue::Primitive(Value::Str(_) | Value::Bytes(_)),
        ) => bind_pattern(inner, &items_of(value)?, slots),
        (Pattern::Member { pattern, .. }, RuntimeValue::Union { value, .. }) => {
            bind_pattern(pattern, value, slots)
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
                        values: rest.to_vec().into(),
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
                            values: vec![first_key.clone(), first_value.clone()].into(),
                        },
                        RuntimeValue::Dict {
                            value_type: value_type.clone(),
                            entries: rest.to_vec().into(),
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
                RuntimeValue::clone(payload),
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
            value::Inner::new(RuntimeValue::Tuple {
                value_type: arguments.first().cloned().unwrap_or(Type::Void),
                values: vec![item, remaining].into(),
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
fn lookup(collection: &RuntimeValue, key: &RuntimeValue) -> Option<RuntimeValue> {
    let index = || match key {
        RuntimeValue::Primitive(Value::U64(index)) => usize::try_from(*index).ok(),
        _ => None,
    };
    match collection {
        RuntimeValue::Array { values, .. } => values.get(index()?).cloned(),
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

    use super::{MemoryBudget, run};

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
            assert!(error.host_diagnostic().is_none());
        }
        let unobservable = RuntimeError::UnobservableFunction { origin: None };
        assert_eq!(
            unobservable.program_trap(),
            Some((DiagnosticCode::RuntimeUnobservableFunction, None))
        );
        let exhausted = RuntimeError::MemoryExhausted {
            budget: MemoryBudget::new(1),
        };
        assert_eq!(exhausted.program_trap(), None);
        assert!(exhausted.is_host_event());
        let diagnostic = exhausted.host_diagnostic().expect("host diagnostic");
        assert_eq!(diagnostic.code(), DiagnosticCode::RuntimeMemoryExhausted);
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

        let mut machine =
            super::Machine::new(&program, false, MemoryBudget::DEFAULT.bytes());
        let result = machine
            .run_entry(0, vec![None; program.entry().slot_count()])
            .expect("untaken branch must not execute");

        assert_eq!(result, super::RuntimeValue::Primitive(Value::I32(7)));
        assert_eq!(
            machine.globals[0],
            crate::machine::GlobalState::Uninitialized
        );
    }

    #[test]
    fn a_global_initializer_runs_once_as_its_own_activation() {
        let origin = SourceOrigin::new("test.vib", ByteSpan::new(0, 1));
        let global = CheckedGlobal::new(
            "value",
            Type::I32,
            Expr::literal(Value::I32(9), origin.clone()),
            origin.clone(),
        )
        .expect("valid checked global shape");
        // Reading the global twice runs its initializer once.
        let body = Expr::sequence(
            vec![
                Expr::global(0, Type::I32, origin.clone()),
                Expr::global(0, Type::I32, origin.clone()),
            ],
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
        let execution = run(&program).expect("execution");
        assert_eq!(execution.value(), Some(&Value::I32(9)));
        assert_eq!(
            execution.max_activation_depth(),
            1,
            "an initializer is not a counted activation"
        );
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
    fn non_tail_recursion_ends_in_memory_exhaustion_under_a_small_budget() {
        let program = non_tail_self_recursion();
        let budget = MemoryBudget::new(1024 * 1024);
        assert_eq!(
            super::Interpreter::run_with_budget(&program, budget),
            Err(super::RuntimeError::MemoryExhausted { budget })
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
    fn activations_use_no_host_stack() {
        // About thirty thousand live activations fit this budget, a depth the
        // host stack of this thread could not hold if each cost a Rust call.
        let handle = std::thread::Builder::new()
            .stack_size(192 * 1024)
            .spawn(|| {
                let program = non_tail_self_recursion();
                let budget = MemoryBudget::new(16 * 1024 * 1024);
                assert_eq!(
                    super::Interpreter::run_with_budget(&program, budget),
                    Err(super::RuntimeError::MemoryExhausted { budget })
                );
            })
            .expect("thread");
        handle.join().expect("no stack overflow");
    }

    #[test]
    fn a_budget_of_nothing_is_exhausted_by_the_entry_activation() {
        let program = non_tail_self_recursion();
        assert_eq!(
            super::Interpreter::run_with_budget(&program, MemoryBudget::new(0)),
            Err(super::RuntimeError::MemoryExhausted {
                budget: MemoryBudget::new(0)
            })
        );
    }
}
