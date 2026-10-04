//! The Wasmtime embedding that validates and runs v1 Vibra modules.
//!
//! This is the only crate of the workspace that depends on the engine. The
//! emitter (`vibra-wasm`) writes modules and never runs one, and it does not
//! depend on this crate. The toolchain-owned native code of
//! `vibra_native_v1`, when it exists, is supplied here, behind the module
//! boundary of the `natives` module.
//!
//! # What a run is
//!
//! [`Runner::run_entry`] validates a module under exactly the v1 feature
//! baseline, instantiates it under the runner's memory limit, calls
//! `vibra_v1_entry`, and interprets what the module recorded by the protocol of
//! `docs/spec/06-runtime.md`, "WebAssembly boundary": a call that returns has
//! completed; a call that stops did so because generated code recorded a
//! status first, and a stop with no record is a defect of the toolchain.
//!
//! # The engine
//!
//! Cranelift is the only compiler. NaN canonicalization is on as a defence in
//! depth (it is unobservable, because the specification canonicalizes NaN where
//! it is observed). Every optional WebAssembly proposal is off, fuel and epoch
//! interruption are unused, and a resource limiter applies the runner's memory
//! limit to every memory. The engine's own call stack is not part of the
//! language: activations of a later step live in the module's arena.

mod instance;
mod natives;
mod observe;
mod validate;

use std::fmt;

use vibra_ir::boundary::{
    FAILURE_ACTUAL_EXPORT, FAILURE_EXPECTED_EXPORT, FAILURE_EXPORT, Failure,
    LIVE_SIZE_EXPORT, ORIGIN_EXPORT, RESULT_EXPORT, STATUS_EXPORT, Status,
    TRAP_CODE_EXPORT, TrapCode,
};
use wasmtime::{
    Config, Engine, Instance as EngineInstance, Linker, Module, ResourceLimiter, Store,
    Strategy,
};

pub use instance::{Instance, Started, ValueId};
pub use observe::{LiveSizes, Observed};
pub use validate::{ModuleSummary, ValidationError, baseline_features, validate};

/// The most memory an instance may hold, in bytes of linear memory.
///
/// `docs/spec/07-diagnostics-and-conformance.md`, "Differential execution": a
/// runner applies one finite limit to every instance. Growth past it fails,
/// and a run that fails to grow ends with [`Outcome::MemoryExhausted`], the host
/// event `@runtime.memory-exhausted`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MemoryLimit {
    bytes: usize,
}

impl MemoryLimit {
    /// A limit of `bytes`.
    #[must_use]
    pub const fn new(bytes: usize) -> Self {
        Self { bytes }
    }

    /// The limit in bytes.
    #[must_use]
    pub const fn bytes(self) -> usize {
        self.bytes
    }
}

/// The state the store carries for the host functions and the limiter.
pub(crate) struct Host {
    pub(crate) limiter: Limiter,
}

/// Applies the memory limit and remembers that it refused a request.
pub(crate) struct Limiter {
    pub(crate) limit: MemoryLimit,
    pub(crate) refused: bool,
}

impl ResourceLimiter for Limiter {
    fn memory_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        if desired > self.limit.bytes {
            self.refused = true;
            return Ok(false);
        }
        Ok(true)
    }

    fn table_growing(
        &mut self,
        _current: usize,
        _desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        Ok(true)
    }
}

/// How a call into a module ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The call returned. `result` is what `vibra_v1_result` reports, which
    /// [`Instance::observe`] reads by the entry's result type.
    Completed {
        /// The entry's result slot.
        result: ResultSlot,
        /// The live arena size in bytes that `vibra_v1_live_size` reports
        /// after the call.
        live_size: u64,
    },
    /// The module recorded a registered trap before it stopped.
    Trapped {
        /// The trap's code.
        code: TrapCode,
        /// The origin ordinal into the origin table, `None` for no origin.
        origin: Option<u32>,
    },
    /// The instance exhausted its memory: the host event
    /// `@runtime.memory-exhausted`, which is not a trap.
    MemoryExhausted,
    /// The module recorded a failed assertion before it stopped.
    AssertionFailed {
        /// The assertion that failed.
        failure: Failure,
        /// The origin ordinal of the assertion call.
        origin: Option<u32>,
        /// The expected and actual operands of a failed `assert.equal`, as
        /// the bits of a scalar or as value IDs.
        operands: Option<(u64, u64)>,
    },
    /// The call stopped, or returned, in a way the protocol does not allow:
    /// the toolchain is at fault. It is reported as the trap
    /// `@runtime.invalid-checked-program` with no origin, and no conformance
    /// case expects it.
    Defect {
        /// What the runner saw.
        cause: String,
    },
}

/// The 64-bit slot `vibra_v1_result` reports after a completed entry.
///
/// It is `0` for a `void` result, the bits of a scalar result, or the value ID
/// of an arena result, and only the entry's result type says which. The slot
/// does not show its number: an ID never leaves the runner in a public result,
/// and [`Instance::observe`] turns a slot into the value it stands for.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ResultSlot(u64);

impl ResultSlot {
    /// A slot holding `bits`, for a module whose entry reports a scalar.
    #[must_use]
    pub const fn from_bits(bits: u64) -> Self {
        Self(bits)
    }
}

impl fmt::Debug for ResultSlot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ResultSlot(..)")
    }
}

impl Outcome {
    /// The trap a defect is reported as.
    #[must_use]
    pub const fn defect_trap() -> TrapCode {
        TrapCode::InvalidCheckedProgram
    }
}

/// Why a module could not be run at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunnerError {
    /// The engine could not be configured.
    Engine(String),
    /// The module is not a v1 module.
    Invalid(ValidationError),
    /// The engine rejected a module that validated, which is a defect.
    Compile(String),
    /// The module imports a native the runner does not supply.
    UnsuppliedImport {
        /// The import's module.
        module: String,
        /// The import's name.
        name: String,
    },
    /// The module lacks an export the protocol needs, or exports it with
    /// another type.
    Export {
        /// The export's name.
        name: &'static str,
        /// What the engine reported.
        reason: String,
    },
    /// Instantiation failed for a reason other than the memory limit.
    Instantiate(String),
}

impl fmt::Display for RunnerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Engine(message) => {
                write!(formatter, "engine configuration: {message}")
            }
            Self::Invalid(error) => error.fmt(formatter),
            Self::Compile(message) => {
                write!(formatter, "engine rejected a valid module: {message}")
            }
            Self::UnsuppliedImport { module, name } => write!(
                formatter,
                "the runner supplies no import `{module}`.`{name}`"
            ),
            Self::Export { name, reason } => {
                write!(formatter, "export `{name}` is unusable: {reason}")
            }
            Self::Instantiate(message) => {
                write!(formatter, "instantiation failed: {message}")
            }
        }
    }
}

impl std::error::Error for RunnerError {}

impl From<ValidationError> for RunnerError {
    fn from(error: ValidationError) -> Self {
        Self::Invalid(error)
    }
}

/// Validates and runs v1 modules under one memory limit.
pub struct Runner {
    engine: Engine,
    pub(crate) limit: MemoryLimit,
}

impl fmt::Debug for Runner {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Runner")
            .field("limit", &self.limit)
            .finish_non_exhaustive()
    }
}

impl Runner {
    /// Creates a runner whose every instance runs under `limit`.
    ///
    /// # Errors
    ///
    /// [`RunnerError::Engine`] when the engine rejects the configuration.
    pub fn new(limit: MemoryLimit) -> Result<Self, RunnerError> {
        Self::build(limit, None)
    }

    /// Creates a runner like [`Runner::new`] whose engine allows a module no
    /// more than `stack_bytes` of its own call stack. A test uses a small one
    /// to show that a routine does not recurse in proportion to its input: the
    /// language's activations live in the arena, so no language depth ever
    /// reaches this stack.
    ///
    /// # Errors
    ///
    /// [`RunnerError::Engine`] when the engine rejects the configuration.
    pub fn with_wasm_stack(
        limit: MemoryLimit,
        stack_bytes: usize,
    ) -> Result<Self, RunnerError> {
        Self::build(limit, Some(stack_bytes))
    }

    fn build(
        limit: MemoryLimit,
        stack_bytes: Option<usize>,
    ) -> Result<Self, RunnerError> {
        let engine = Engine::new(&engine_config(stack_bytes))
            .map_err(|error| RunnerError::Engine(format!("{error:#}")))?;
        Ok(Self { engine, limit })
    }

    /// The limit applied to every instance.
    #[must_use]
    pub const fn limit(&self) -> MemoryLimit {
        self.limit
    }

    /// Validates `bytes`, instantiates the module, calls `vibra_v1_entry`, and
    /// reads what the module recorded.
    ///
    /// # Errors
    ///
    /// A [`RunnerError`] when the module is not a v1 module, imports a native
    /// the runner does not supply, or lacks an export the protocol needs. A
    /// run that exhausts memory is the [`Outcome::MemoryExhausted`] host event,
    /// including when the module's own initial memory exceeds the limit.
    pub fn run_entry(&self, bytes: &[u8]) -> Result<Outcome, RunnerError> {
        match self.start(bytes)? {
            Started::MemoryExhausted => Ok(Outcome::MemoryExhausted),
            Started::Ready(mut instance) => instance.call_entry(),
        }
    }

    /// Validates `bytes` and instantiates the module under the runner's limit,
    /// without running it. The instance stays usable after any stop: the host
    /// reads values and the live size through its accessors.
    ///
    /// # Errors
    ///
    /// The same [`RunnerError`]s as [`Runner::run_entry`], before the entry
    /// runs. A module whose own memory exceeds the limit is
    /// [`Started::MemoryExhausted`].
    pub fn start(&self, bytes: &[u8]) -> Result<Started, RunnerError> {
        validate(bytes)?;
        let module = Module::new(&self.engine, bytes)
            .map_err(|error| RunnerError::Compile(format!("{error:#}")))?;
        let mut linker = Linker::new(&self.engine);
        natives::define(&mut linker)
            .map_err(|error| RunnerError::Engine(format!("{error:#}")))?;
        let mut store = Store::new(
            &self.engine,
            Host {
                limiter: Limiter {
                    limit: self.limit,
                    refused: false,
                },
            },
        );
        store.limiter(|host| &mut host.limiter);
        for import in module.imports() {
            if linker.get_by_import(&mut store, &import).is_none() {
                return Err(RunnerError::UnsuppliedImport {
                    module: import.module().to_owned(),
                    name: import.name().to_owned(),
                });
            }
        }
        let instance = match linker.instantiate(&mut store, &module) {
            Ok(instance) => instance,
            // The module's own memory does not fit the limit.
            Err(_) if store.data().limiter.refused => {
                return Ok(Started::MemoryExhausted);
            }
            Err(error) => return Err(RunnerError::Instantiate(format!("{error:#}"))),
        };
        Ok(Started::Ready(Instance::new(store, instance)))
    }
}

/// The engine configuration of a v1 runner.
///
/// Wasmtime is built with Cranelift and no optional engine feature, so the
/// threads, garbage-collection, exception, function-reference, and reference-type
/// proposals do not exist in this engine. The rest are turned off here, and
/// [`validate`] refuses a module that uses any of them before it reaches the
/// engine.
fn engine_config(stack_bytes: Option<usize>) -> Config {
    let mut config = Config::new();
    if let Some(stack_bytes) = stack_bytes {
        config.max_wasm_stack(stack_bytes);
    }
    config
        .strategy(Strategy::Cranelift)
        .cranelift_nan_canonicalization(true)
        .consume_fuel(false)
        .epoch_interruption(false)
        .wasm_multi_value(true)
        .wasm_bulk_memory(true)
        .wasm_simd(false)
        .wasm_relaxed_simd(false)
        .wasm_tail_call(false)
        .wasm_memory64(false)
        .wasm_multi_memory(false)
        .wasm_extended_const(false)
        .wasm_custom_page_sizes(false)
        .wasm_wide_arithmetic(false);
    config
}

pub(crate) fn export_error(name: &'static str, error: &wasmtime::Error) -> RunnerError {
    RunnerError::Export {
        name,
        reason: format!("{error:#}"),
    }
}

pub(crate) fn call_i32(
    store: &mut Store<Host>,
    instance: &EngineInstance,
    name: &'static str,
) -> Result<i32, RunnerError> {
    let function = instance
        .get_typed_func::<(), i32>(&mut *store, name)
        .map_err(|error| export_error(name, &error))?;
    function
        .call(&mut *store, ())
        .map_err(|error| export_error(name, &error))
}

pub(crate) fn call_i64(
    store: &mut Store<Host>,
    instance: &EngineInstance,
    name: &'static str,
) -> Result<i64, RunnerError> {
    let function = instance
        .get_typed_func::<(), i64>(&mut *store, name)
        .map_err(|error| export_error(name, &error))?;
    function
        .call(&mut *store, ())
        .map_err(|error| export_error(name, &error))
}

/// Reads an origin ordinal: `0` is no origin.
fn origin(
    store: &mut Store<Host>,
    instance: &EngineInstance,
) -> Result<Option<u32>, RunnerError> {
    let ordinal = call_i32(store, instance, ORIGIN_EXPORT)?;
    // The accessor returns an `i32` that is an unsigned ordinal.
    Ok(u32::try_from(ordinal).ok().filter(|ordinal| *ordinal != 0))
}

/// Interprets what a call recorded. `stop` is the engine's report when the
/// call did not return.
pub(crate) fn interpret(
    store: &mut Store<Host>,
    instance: &EngineInstance,
    stop: Option<String>,
) -> Result<Outcome, RunnerError> {
    // A refused growth is the host event, whatever the module did next.
    if store.data().limiter.refused {
        return Ok(Outcome::MemoryExhausted);
    }
    let recorded = call_i32(store, instance, STATUS_EXPORT)?;
    let Some(status) = Status::from_code(recorded) else {
        return Ok(Outcome::Defect {
            cause: format!("`{STATUS_EXPORT}` returned the code {recorded}"),
        });
    };
    match (stop, status) {
        (None, Status::Nothing) => {
            let result = call_i64(store, instance, RESULT_EXPORT)?;
            let live_size = call_i64(store, instance, LIVE_SIZE_EXPORT)?;
            // An ID and a size are unsigned 64-bit values carried in an `i64`.
            Ok(Outcome::Completed {
                result: ResultSlot(u64::from_ne_bytes(result.to_ne_bytes())),
                live_size: u64::from_ne_bytes(live_size.to_ne_bytes()),
            })
        }
        (None, recorded) => Ok(Outcome::Defect {
            cause: format!("the call returned but recorded the status {recorded:?}"),
        }),
        (Some(report), Status::Nothing) => Ok(Outcome::Defect {
            cause: format!("the call stopped with no recorded status: {report}"),
        }),
        (Some(_), Status::MemoryExhausted) => Ok(Outcome::MemoryExhausted),
        (Some(_), Status::Trap) => {
            let code = call_i32(store, instance, TRAP_CODE_EXPORT)?;
            let Some(code) = TrapCode::from_code(code) else {
                return Ok(Outcome::Defect {
                    cause: format!("`{TRAP_CODE_EXPORT}` returned the code {code}"),
                });
            };
            Ok(Outcome::Trapped {
                code,
                origin: origin(store, instance)?,
            })
        }
        (Some(_), Status::FailedAssertion) => {
            let code = call_i32(store, instance, FAILURE_EXPORT)?;
            let Some(failure) = Failure::from_code(code) else {
                return Ok(Outcome::Defect {
                    cause: format!("`{FAILURE_EXPORT}` returned the code {code}"),
                });
            };
            let operands = if failure == Failure::Equal {
                let expected = call_i64(store, instance, FAILURE_EXPECTED_EXPORT)?;
                let actual = call_i64(store, instance, FAILURE_ACTUAL_EXPORT)?;
                Some((
                    u64::from_ne_bytes(expected.to_ne_bytes()),
                    u64::from_ne_bytes(actual.to_ne_bytes()),
                ))
            } else {
                None
            };
            Ok(Outcome::AssertionFailed {
                failure,
                origin: origin(store, instance)?,
                operands,
            })
        }
    }
}
