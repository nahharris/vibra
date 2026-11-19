//! The reference interpreter of Vibra.
//!
//! It runs a [`CheckedProgram`] or evaluates one checked expression over a
//! [`CheckedModuleSet`], with the semantics of `docs/spec/06-runtime.md`:
//! strict written-order evaluation, frames that live on the heap, proper
//! tail calls for every callee, lazy module values, callable contracts,
//! dispatch by the receiver's runtime type, the pure rows of the compiler
//! intrinsic registry, and a memory budget whose exhaustion is a host event.
//!
//! The interpreter consumes no host stack in proportion to the depth of
//! Vibra calls, and it reads nothing from its surroundings: no file, no
//! environment variable, no clock, and no standard stream.
//!
//! Host operations, grants, plans, `await`, and native bindings are not
//! here: applying a `proc` is reported as [`RuntimeError::Unavailable`].

mod compare;
mod encode;
mod evaluate;
mod frame;
mod intrinsics;
mod machine;
mod types;
mod value;

pub use evaluate::{evaluate, run, run_test};

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::{ObservedValue, SourceOrigin, Trap, Type, Value};

/// The memory an execution may account for its frames, continuations, and
/// values. Its limit belongs to the host; source cannot observe it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MemoryBudget {
    /// The limit, in bytes.
    pub bytes: u64,
}

impl MemoryBudget {
    /// The budget a host uses when it configures none: 256 MiB.
    pub const DEFAULT: Self = Self::new(256 * 1024 * 1024);

    /// A budget of `bytes`.
    #[must_use]
    pub const fn new(bytes: u64) -> Self {
        Self { bytes }
    }
}

impl Default for MemoryBudget {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// The limits of one [`evaluate`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// The most reductions the evaluation may perform. A reduction is one
    /// evaluation of one checked node.
    pub reductions: u64,
    /// The memory the evaluation may account for.
    pub memory: MemoryBudget,
}

/// The limit an [`evaluate`] reached.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Limit {
    /// [`Limits::reductions`].
    Reductions,
    /// [`Limits::memory`].
    Memory,
}

/// The outcome of [`evaluate`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Evaluation {
    /// The expression's value, as the neutral value of its static type.
    Value(Value),
    /// The evaluation stopped at a limit before it finished.
    LimitReached(Limit),
    /// The evaluation trapped.
    Defect(Trap),
}

/// What an execution measured. None of it is observable by the program.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Instrumentation {
    /// The most activations alive at once, the entry's included.
    pub max_activation_depth: usize,
    /// How many tail calls replaced their caller's activation.
    pub tail_transfer_count: u64,
    /// How many checked nodes were evaluated.
    pub reduction_count: u64,
    /// The largest footprint accounted against the budget: frames,
    /// continuations, the values reachable at the last measurement, and
    /// every value allocated since.
    pub peak_memory_bytes: u64,
    /// The bytes still reachable when the execution ended: module values
    /// and the result.
    pub live_memory_bytes: u64,
}

/// A finished [`run`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Execution {
    /// The entry's result, at the entry's declared result type.
    pub value: ObservedValue,
    /// The canonical bytes of the result's complete `@value.v2`
    /// observation.
    pub canonical: String,
    /// What the run measured.
    pub instrumentation: Instrumentation,
}

/// How an assertion failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FailureKind {
    /// The assertion did not hold.
    Assertion,
    /// An operand of `assert.equal` could not be observed.
    Observation {
        /// The observation error: `@tool.unsupportedValue`.
        code: DiagnosticCode,
        /// The operand's static type.
        type_info: Type,
    },
}

/// The structured failure of an assertion row, which stops the test body.
/// It is not a trap, a `Result`, or a host event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestAssertionFailure {
    /// Whether the assertion failed or could not observe an operand.
    pub kind: FailureKind,
    /// The row's wire symbol, such as `assert.equal`.
    pub assertion: String,
    /// The canonical value encoding of `assert.equal`'s expected operand.
    pub expected: Option<String>,
    /// The canonical value encoding of `assert.equal`'s actual operand.
    pub actual: Option<String>,
    /// Where the failing call was written.
    pub origin: SourceOrigin,
}

/// A finished [`run_test`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestExecution {
    /// The first assertion that did not hold, which ended the body; `None`
    /// when the body ran to its end.
    pub failure: Option<TestAssertionFailure>,
    /// What the run measured.
    pub instrumentation: Instrumentation,
}

/// Why a [`run`] or a [`run_test`] did not finish.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeError {
    /// The program trapped.
    Trap {
        /// The trap.
        trap: Trap,
        /// Where the trapping node was written, when one was.
        origin: Option<SourceOrigin>,
        /// What the interpreter found, for a person.
        message: String,
    },
    /// The execution exceeded its memory budget: the host event
    /// `@runtime.memoryExhausted`, with no source location.
    MemoryExhausted {
        /// The budget that was exceeded.
        budget: MemoryBudget,
    },
    /// The result holds a function behind an interface value, which has no
    /// observation: the observation error `@tool.unsupportedValue`.
    UnsupportedValue {
        /// The static type of the value that was observed.
        value_type: Type,
    },
    /// The program reached a form this toolchain does not yet implement:
    /// `@tool.unavailable`.
    Unavailable {
        /// The form, for a person.
        message: String,
    },
}

impl RuntimeError {
    /// Whether this is a host event rather than an outcome of the program.
    #[must_use]
    pub const fn is_host_event(&self) -> bool {
        matches!(self, Self::MemoryExhausted { .. })
    }

    /// The registered code.
    #[must_use]
    pub const fn code(&self) -> DiagnosticCode {
        match self {
            Self::Trap { trap, .. } => trap.code(),
            Self::MemoryExhausted { .. } => DiagnosticCode::RuntimeMemoryExhausted,
            Self::UnsupportedValue { .. } => DiagnosticCode::ToolUnsupportedValue,
            Self::Unavailable { .. } => DiagnosticCode::ToolUnavailable,
        }
    }

    /// The trap, when this is one.
    #[must_use]
    pub const fn trap(&self) -> Option<Trap> {
        match self {
            Self::Trap { trap, .. } => Some(*trap),
            _ => None,
        }
    }

    /// The diagnostic a tool reports. A trap keeps its origin when it is
    /// known; memory exhaustion has the span `0..0` and no source identity.
    #[must_use]
    pub fn diagnostic(&self) -> Diagnostic {
        let at_start = ByteSpan::empty_at(0);
        match self {
            Self::Trap {
                origin: Some(origin),
                message,
                ..
            } => Diagnostic::new(self.code(), origin.span(), message.clone())
                .with_source_id(origin.source_id()),
            Self::Trap {
                origin: None,
                message,
                ..
            } => Diagnostic::new(self.code(), at_start, message.clone()),
            Self::MemoryExhausted { budget } => Diagnostic::new(
                self.code(),
                at_start,
                format!("the memory budget of {} bytes is exhausted", budget.bytes),
            ),
            Self::UnsupportedValue { value_type } => Diagnostic::new(
                self.code(),
                at_start,
                format!(
                    "a value of {value_type} holds a function and has no observation"
                ),
            ),
            Self::Unavailable { message } => {
                Diagnostic::new(self.code(), at_start, message.clone())
            }
        }
    }
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let diagnostic = self.diagnostic();
        write!(f, "{}: {}", self.code().as_atom(), diagnostic.message())
    }
}

impl std::error::Error for RuntimeError {}
