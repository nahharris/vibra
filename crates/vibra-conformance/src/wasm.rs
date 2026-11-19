//! The WebAssembly executor column.
//!
//! `docs/spec/07-diagnostics-and-conformance.md`, "Differential execution":
//! an executable case accepted by the reference interpreter is also run on
//! the backend, and the two executions are compared on their neutral
//! endings: the canonical value, the trap code and origin, the memory host
//! event, and each test's outcome, its failed assertion with the operands'
//! canonical encodings and the call's origin. The program is the checked IR
//! the case's handler built, emitted by `vibra-wasm` and run by
//! `vibra-wasm-run`, and the reference is `vibra-interp` on the same IR.
//!
//! The transition profile is fail-closed (`docs/spec/06-runtime.md`, "Wasm
//! transition"): a program with a node the emitter does not lower is
//! unavailable on this executor, never approximated. A module the runner
//! refuses, or a run that breaks the protocol, is the toolchain's defect and
//! ends as the trap `@runtime.invalidCheckedProgram` with no origin.

use std::fmt;
use std::sync::Arc;

use vibra_interp::{FailureKind, MemoryBudget, RuntimeError};
use vibra_ir::{CheckedModuleSet, CheckedProgram, SourceOrigin, Trap, Type};
use vibra_wasm::{EmitError, OriginTable};
use vibra_wasm_run::{MemoryLimit, Observed, Outcome, Runner, TestRun};

use crate::runner::CaseStatus;

/// The memory both executors run a program under, in bytes.
const MEMORY_BYTES: usize = 256 * 1024 * 1024;

/// The checked IR an executable case's handler built.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Executable {
    /// An `interpret` or `workspaceRun` case: the program and its entry.
    Program(CheckedProgram),
    /// A `workspaceTest` case: the set and its tests, in canonical discovery
    /// order, each a function with no parameter.
    Tests {
        /// The checked module set.
        set: Arc<CheckedModuleSet>,
        /// The function index of each test.
        tests: Vec<usize>,
    },
}

/// How one execution ended, with nothing an executor owns in it.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Ending {
    /// The entry returned this canonical `@value.v2` document.
    Value(String),
    /// A registered trap, by code atom, and where it was written.
    Trap {
        code: String,
        origin: Option<SourceOrigin>,
    },
    /// The host event `@runtime.memoryExhausted`.
    MemoryExhausted,
    /// The test body ran to its end.
    Passed,
    /// An assertion did not hold.
    Failed {
        assertion: String,
        expected: Option<String>,
        actual: Option<String>,
        origin: Option<SourceOrigin>,
    },
}

impl fmt::Display for Ending {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let place = |origin: &Option<SourceOrigin>| {
            origin.as_ref().map_or_else(
                || "no origin".to_owned(),
                |origin| {
                    format!(
                        "{} at {}..{}",
                        origin.source_id(),
                        origin.span().start(),
                        origin.span().end()
                    )
                },
            )
        };
        match self {
            Self::Value(value) => write!(formatter, "the value {value}"),
            Self::Trap { code, origin } => {
                write!(formatter, "the trap {code} ({})", place(origin))
            }
            Self::MemoryExhausted => formatter.write_str("@runtime.memoryExhausted"),
            Self::Passed => formatter.write_str("a passed test"),
            Self::Failed {
                assertion,
                expected,
                actual,
                origin,
            } => write!(
                formatter,
                "a failed {assertion} (expected {}, actual {}, {})",
                expected.as_deref().unwrap_or("none"),
                actual.as_deref().unwrap_or("none"),
                place(origin)
            ),
        }
    }
}

/// The defect ending: what a refused module or a broken protocol reports.
fn defect() -> Ending {
    trap(Trap::InvalidCheckedProgram, None)
}

fn trap(trap: Trap, origin: Option<SourceOrigin>) -> Ending {
    Ending::Trap {
        code: trap.code().as_atom().to_owned(),
        origin,
    }
}

/// Why the interpreter's ending is not compared: it established none.
fn unobserved(error: &RuntimeError) -> CaseStatus {
    CaseStatus::Unavailable {
        reason: format!(
            "wasm executor: the interpreter established no ending ({})",
            error.code().as_atom()
        ),
    }
}

/// The reference interpreter's ending, or the status when it has none.
fn reference_ending(
    result: Result<String, RuntimeError>,
) -> Result<Ending, CaseStatus> {
    match result {
        Ok(canonical) => Ok(Ending::Value(canonical)),
        Err(RuntimeError::Trap {
            trap: which,
            origin,
            ..
        }) => Ok(trap(which, origin)),
        Err(RuntimeError::MemoryExhausted { .. }) => Ok(Ending::MemoryExhausted),
        Err(error) => Err(unobserved(&error)),
    }
}

/// The backend's ending of a stop that is not a completion.
fn stopped(outcome: Outcome, origins: &OriginTable) -> Ending {
    match outcome {
        Outcome::Trapped { code, origin } => trap(
            code.trap(),
            origin.and_then(|ordinal| origins.origin(ordinal)).cloned(),
        ),
        Outcome::MemoryExhausted => Ending::MemoryExhausted,
        Outcome::Completed { .. }
        | Outcome::AssertionFailed { .. }
        | Outcome::Defect { .. } => defect(),
    }
}

/// The result type of a program's entry, `None` for a malformed program.
fn result_type(program: &CheckedProgram) -> Option<Type> {
    let function = program.module_set().functions().get(program.entry())?;
    match function.signature.type_info() {
        Type::Fn(signature) => Some(signature.result.clone()),
        _ => None,
    }
}

/// The executor: the engine, and a fault hook for the harness's own tests.
#[derive(Debug)]
pub struct WasmExecutor {
    runner: Runner,
    flip: bool,
}

impl WasmExecutor {
    /// The executor's name on the command line and in the report.
    pub const NAME: &'static str = "wasm";

    /// An executor over a fresh engine.
    ///
    /// # Errors
    ///
    /// The reason when the engine cannot be configured on this machine: the
    /// executor is then missing.
    pub fn new() -> Result<Self, String> {
        Runner::new(MemoryLimit::new(MEMORY_BYTES))
            .map(|runner| Self {
                runner,
                flip: false,
            })
            .map_err(|error| error.to_string())
    }

    /// The same executor with a fault injected: the backend's first ending
    /// is replaced by one no execution produces. It is a hook for the tests
    /// of the harness, which check that a mismatch fails the case, and it is
    /// never a way to run a corpus.
    #[must_use]
    pub fn with_flipped_result(mut self) -> Self {
        self.flip = true;
        self
    }

    /// The status of `executable` on this executor: passed when every ending
    /// equals the interpreter's, failed at the first difference, and
    /// unavailable when the emitter does not lower the program or the
    /// interpreter established no ending.
    #[must_use]
    pub fn check(&self, executable: &Executable) -> CaseStatus {
        let compared = match executable {
            Executable::Program(program) => self.program(program),
            Executable::Tests { set, tests } => self.tests(set, tests),
        };
        let (reference, mut backend) = match compared {
            Ok(endings) => endings,
            Err(status) => return status,
        };
        if self.flip
            && let Some(first) = backend.first_mut()
        {
            *first = Ending::Value("flipped".to_owned());
        }
        if reference.len() != backend.len() {
            return CaseStatus::Failed {
                reason: format!(
                    "wasm executor: {} endings, the interpreter {}",
                    backend.len(),
                    reference.len()
                ),
            };
        }
        for (position, (expected, actual)) in reference.iter().zip(&backend).enumerate()
        {
            if expected != actual {
                let what = match executable {
                    Executable::Program(_) => "the entry".to_owned(),
                    Executable::Tests { .. } => format!("test {position}"),
                };
                return CaseStatus::Failed {
                    reason: format!(
                        "wasm executor: {what} ended with {actual}; the interpreter \
                         with {expected}"
                    ),
                };
            }
        }
        CaseStatus::Passed
    }

    /// The interpreter's and the backend's ending of a program.
    fn program(
        &self,
        program: &CheckedProgram,
    ) -> Result<(Vec<Ending>, Vec<Ending>), CaseStatus> {
        let module = emitted(vibra_wasm::emit(program))?;
        let reference = reference_ending(
            vibra_interp::run(program, budget()).map(|execution| execution.canonical),
        )?;
        let Some(result) = result_type(program) else {
            return Err(malformed("the entry has no result type"));
        };
        let types = program.module_set().types();
        let backend = match self.runner.run_observed(module.bytes(), &result, types) {
            Ok(Observed::Completed { canonical, .. }) => Ending::Value(canonical),
            Ok(Observed::Stopped(outcome)) => stopped(outcome, module.origins()),
            Err(_) => defect(),
        };
        Ok((vec![reference], vec![backend]))
    }

    /// The interpreter's and the backend's ending of each test.
    fn tests(
        &self,
        set: &Arc<CheckedModuleSet>,
        tests: &[usize],
    ) -> Result<(Vec<Ending>, Vec<Ending>), CaseStatus> {
        let module = emitted(vibra_wasm::emit_tests(set, tests))?;
        let mut reference = Vec::with_capacity(tests.len());
        let mut backend = Vec::with_capacity(tests.len());
        for (position, test) in tests.iter().enumerate() {
            let Ok(program) = CheckedProgram::for_entry(Arc::clone(set), *test) else {
                return Err(malformed("a test is not an entry"));
            };
            reference.push(match vibra_interp::run_test(&program, budget()) {
                Ok(execution) => match execution.failure {
                    None => Ending::Passed,
                    Some(failure) if failure.kind == FailureKind::Assertion => {
                        Ending::Failed {
                            assertion: failure.assertion,
                            expected: failure.expected,
                            actual: failure.actual,
                            origin: Some(failure.origin),
                        }
                    }
                    Some(_) => {
                        return Err(CaseStatus::Unavailable {
                            reason: "wasm executor: the interpreter could not observe \
                                     an operand"
                                .to_owned(),
                        });
                    }
                },
                Err(error) => reference_ending(Err(error))?,
            });
            let Ok(index) = u32::try_from(position) else {
                return Err(malformed("more tests than a module holds"));
            };
            let origins = module.origins();
            let run =
                self.runner
                    .run_test(module.bytes(), index, set.types(), &|ordinal| {
                        origins.operand_type(ordinal).cloned()
                    });
            backend.push(match run {
                Ok(TestRun::Passed { .. }) => Ending::Passed,
                Ok(TestRun::AssertionFailed {
                    failure,
                    origin,
                    expected,
                    actual,
                }) => Ending::Failed {
                    assertion: failure.symbol().to_owned(),
                    expected,
                    actual,
                    origin: origin.and_then(|ordinal| origins.origin(ordinal)).cloned(),
                },
                Ok(TestRun::Trapped { code, origin }) => {
                    stopped(Outcome::Trapped { code, origin }, origins)
                }
                Ok(TestRun::MemoryExhausted) => Ending::MemoryExhausted,
                Ok(TestRun::Defect { .. }) | Err(_) => defect(),
            });
        }
        Ok((reference, backend))
    }
}

/// The interpreter's budget, the same bytes as the backend's limit.
fn budget() -> MemoryBudget {
    u64::try_from(MEMORY_BYTES).map_or(MemoryBudget::DEFAULT, MemoryBudget::new)
}

/// The module, or the status of a program the emitter did not lower.
fn emitted(
    module: Result<vibra_wasm::EmittedModule, EmitError>,
) -> Result<vibra_wasm::EmittedModule, CaseStatus> {
    module.map_err(|error| match error {
        EmitError::Unsupported { node, .. } => CaseStatus::Unavailable {
            reason: format!(
                "wasm executor: the transition profile does not run `{node}`"
            ),
        },
        other => CaseStatus::Failed {
            reason: format!("wasm executor: {other}"),
        },
    })
}

fn malformed(what: &str) -> CaseStatus {
    CaseStatus::Failed {
        reason: format!("wasm executor: {what}"),
    }
}
