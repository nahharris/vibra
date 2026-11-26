//! The reference interpreter of Vibra.
//!
//! It runs a [`vibra_ir::CheckedProgram`] or evaluates one checked expression
//! over a [`vibra_ir::CheckedModuleSet`], with the semantics of `docs/spec/06-runtime.md`:
//! strict written-order evaluation, frames that live on the heap, proper
//! tail calls for every callee, lazy module values, callable contracts,
//! dispatch by the receiver's runtime type, the pure rows of the compiler
//! intrinsic registry, cold plans and `await` with their invocation
//! ordinals, and a memory budget whose exhaustion is a host event.
//!
//! The interpreter consumes no host stack in proportion to the depth of
//! Vibra calls or awaits, and it reads nothing from its surroundings: no
//! file, no environment variable, no clock, and no standard stream.
//!
//! Host-bound members, the narrowing of grants, and the test-world rows run
//! through a [`vibra_ir::authority::HostContext`] the caller hands in, which
//! issues and checks every grant: a sealed grant is a token of that context,
//! never encoded and never compared. With no context, a host-bound member
//! that is reached is reported as [`RuntimeError::Unavailable`].

mod compare;
mod encode;
mod evaluate;
mod frame;
mod host;
mod ingress;
mod intrinsics;
mod machine;
mod plan;
mod types;
mod value;

pub use evaluate::{
    evaluate, evaluate_reading, run, run_entry, run_test, run_test_probing,
};
pub use host::EntryArgument;
pub use plan::Invocation;

use std::collections::BTreeSet;

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

/// A published limit profile: its name, the limits of one compile-time
/// evaluation under it, and the derivations the proof of one obligation
/// may attempt.
///
/// `docs/spec/05-tooling.md`, "Limits".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LimitProfile {
    /// The profile's name, as a report names it.
    pub name: &'static str,
    /// The limits of one compile-time evaluation.
    pub compile_time: Limits,
    /// The most derivations of integer facts the discharge of one
    /// obligation may attempt. A derivation is one application of one
    /// integer fact to one premise or one pair of premises, or one
    /// comparison of two constants, whether or not its conclusion is new.
    pub proof_derivations: u64,
    /// The most levels a token of a source or data document may be nested,
    /// as `docs/spec/05-tooling.md`, "Limits", counts them. A command
    /// measures every document it reads against it before any later stage.
    pub source_nesting: usize,
}

impl LimitProfile {
    /// `m1Default`: 10,000,000 reductions for one compile-time evaluation,
    /// with the default memory budget, 100,000 attempted derivations for
    /// the proof of one obligation, and a source nesting of 1,024 levels.
    pub const M1_DEFAULT: Self = Self {
        name: "m1Default",
        compile_time: Limits {
            reductions: 10_000_000,
            memory: MemoryBudget::DEFAULT,
        },
        proof_derivations: 100_000,
        source_nesting: 1_024,
    };
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

/// What one [`evaluate_reading`] read of its module set, by index: the
/// functions whose bodies it entered and the module values it read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reads {
    /// The functions entered, by their index in the module set.
    pub functions: BTreeSet<usize>,
    /// The module values read, by their index in the module set.
    pub module_values: BTreeSet<usize>,
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
    /// How many module-value initializers started: each module value read
    /// at all is initialized once per execution.
    pub module_value_initializations: u64,
    /// The largest footprint accounted against the budget: frames,
    /// continuations, the values reachable at the last measurement, and
    /// every value allocated since.
    pub peak_memory_bytes: u64,
    /// The bytes still reachable when the execution ended: module values
    /// and the result.
    pub live_memory_bytes: u64,
    /// How many invocations started: the run of the entry or of the test
    /// body, and each `await`. An evaluation starts none.
    pub invocation_count: u64,
    /// The greatest length the continuation stack reached.
    pub max_continuation_depth: usize,
    /// The greatest live footprint measured: frames, continuations, the
    /// values reachable from them and from the module values, and the
    /// storage being reserved. The live bytes are measured whenever the
    /// accounted footprint passes twice the last measurement, or the
    /// budget.
    pub peak_live_bytes: u64,
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
    /// The result holds a function behind an interface value, or a plan,
    /// which has no observation: the observation error
    /// `@tool.unsupportedValue`.
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
    /// A `TestWorld.directory` fixture holds a path as a file and a
    /// directory, or holds the empty path: `@test.conflictingFixture`, which
    /// ends the test `@test.invalid`. It is not a trap.
    ConflictingFixture {
        /// The path, by its names; empty for the empty path.
        names: Vec<String>,
        /// The source origin of the `await` of the call, when known.
        origin: Option<SourceOrigin>,
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
            Self::ConflictingFixture { .. } => DiagnosticCode::TestConflictingFixture,
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
                    "a value of {value_type} holds a function or a plan and has no observation"
                ),
            ),
            Self::Unavailable { message } => {
                Diagnostic::new(self.code(), at_start, message.clone())
            }
            Self::ConflictingFixture { names, origin } => {
                let message = if names.is_empty() {
                    "the fixture of `TestWorld.directory` holds the empty path"
                        .to_owned()
                } else {
                    format!(
                        "the fixture of `TestWorld.directory` holds the path [{}] as a file and a directory",
                        names
                            .iter()
                            .map(|name| format!("{name:?}"))
                            .collect::<Vec<_>>()
                            .join(" ")
                    )
                };
                match origin {
                    Some(origin) => {
                        Diagnostic::new(self.code(), origin.span(), message)
                            .with_source_id(origin.source_id())
                    }
                    None => Diagnostic::new(self.code(), at_start, message),
                }
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
