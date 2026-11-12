//! Profile dispatch and expectation checking for the internal corpus runner.

use std::collections::BTreeMap;
use std::fmt;

use vibra_diagnostics::Diagnostic;

use crate::corpus::{Case, Corpus};
use crate::manifest::{CaseExpectations, ExpectedExecution};
use crate::native::{NativeCounts, NativeObservation, NativeStatus};
use crate::profile::ConformanceProfile;

/// The one finite memory limit, in bytes, the runner applies to every instance
/// of either backend (`docs/spec/07-diagnostics-and-conformance.md`,
/// "Differential execution"). It is a runner setting and not case data, so a
/// case that exhausts memory ends. A case states only whether it does, never
/// where.
///
/// The hundred-thousand-deep case needs about 38 MiB in the interpreter's
/// accounting, so this leaves it a margin, and it is small enough that recursion
/// with no base case reaches it in seconds. The Wasm backend applies it to the
/// instance's linear memory.
pub const INSTANCE_MEMORY_LIMIT_BYTES: usize = 64 * 1024 * 1024;

/// The reference interpreter's budget under [`INSTANCE_MEMORY_LIMIT_BYTES`].
pub(crate) const fn interpreter_budget() -> vibra_interp::MemoryBudget {
    vibra_interp::MemoryBudget::new(INSTANCE_MEMORY_LIMIT_BYTES)
}

/// The Wasm runner's limit under [`INSTANCE_MEMORY_LIMIT_BYTES`].
pub(crate) const fn wasm_memory_limit() -> vibra_wasm_run::MemoryLimit {
    vibra_wasm_run::MemoryLimit::new(INSTANCE_MEMORY_LIMIT_BYTES)
}

/// A backend's result and its ordered audit trace.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExecutionObservation {
    /// The serialized result, when execution produced one.
    pub result: Option<String>,
    /// Audit events in execution order.
    pub audit_trace: Vec<vibra_host::AuditEvent>,
}

/// What the WebAssembly backend did with an accepted executable case.
///
/// The case has one expectation (`docs/spec/07-diagnostics-and-conformance.md`,
/// "Differential execution"), so the Wasm backend reports the same kinds of
/// observation the interpreter does, and the runner compares them with it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WasmObservation {
    /// The module ran to completion.
    Completed(ExecutionObservation),
    /// The instance ended in a host event: the atom of its unlocated
    /// diagnostic, such as `@runtime.memory-exhausted`.
    HostEvent(String),
    /// The entry's run ended in a trap: the atom of its code, such as
    /// `@runtime.unobservable-function`. The case expects that code as its one
    /// runtime error diagnostic.
    Trapped(String),
    /// The backend failed on the program: the emitter produced no module (a
    /// toolchain defect, since a checked program has every form lowered), the
    /// module did not validate, the engine refused it, or the run stopped some
    /// way the interpreter did not. It fails the Wasm backend on the case.
    Failed {
        /// What went wrong.
        reason: String,
    },
}

/// What one backend observed of an executable case: its outcome, and whether
/// the entry returned `err` when the run completed. The command outcome follows
/// from the result, so a case records no field for it; the harness holds the
/// backends to the same one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendObservation<T> {
    /// What the backend did with the case.
    pub outcome: T,
    /// Whether the entry returned `err`, for a run that completed and whose
    /// handler reports it.
    pub program_failed: Option<bool>,
}

impl<T> BackendObservation<T> {
    /// An observation that does not say whether the entry returned `err`.
    #[must_use]
    pub const fn new(outcome: T) -> Self {
        Self {
            outcome,
            program_failed: None,
        }
    }
}

/// One structural query result returned by a profile handler.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QueryObservation {
    /// The case-relative input document that was queried.
    pub input: String,
    /// The queried UTF-8 byte offset.
    pub offset: usize,
    /// The canonical serialized schema result.
    pub result: String,
}

/// The backend-neutral facts a profile handler returns for one case.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CaseObservation {
    /// Whether checking accepted the case.
    pub accepted: bool,
    /// Diagnostics in emission order.
    pub diagnostics: Vec<Diagnostic>,
    /// Canonical formatted source, if the handler provides it.
    pub formatted: Option<String>,
    /// Canonical source-graph snapshot, if the handler provides it.
    pub graph: Option<String>,
    /// Resolved-identity output, if the handler provides it.
    pub resolved: Option<String>,
    /// Type output, if the handler provides it.
    pub types: Option<String>,
    /// Effect output, if the handler provides it.
    pub effects: Option<String>,
    /// The `@index.v1` document, if the handler provides it.
    pub index: Option<String>,
    /// Structural source-position query observations.
    pub queries: Vec<QueryObservation>,
    /// Reference-interpreter observation.
    pub interpreter: Option<BackendObservation<ExecutionObservation>>,
    /// The host event that ended execution, instead of a result: the atom of
    /// the unlocated diagnostic, such as `@runtime.memory-exhausted`.
    pub host_event: Option<String>,
    /// What the WebAssembly backend did with the case. A handler that runs an
    /// executable case sets it for every program it runs. A rejected program
    /// is no executable case, so a handler leaves it unset.
    pub wasm: Option<BackendObservation<WasmObservation>>,
    /// What the reference stepper did with the case, set wherever `wasm` is.
    pub native: Option<NativeObservation>,
}

impl CaseObservation {
    /// Starts an observation with the acceptance result.
    #[must_use]
    pub fn new(accepted: bool) -> Self {
        Self {
            accepted,
            ..Self::default()
        }
    }
}

/// A backend failure while executing a conformance case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandlerError {
    message: String,
}

impl HandlerError {
    /// Creates a backend failure with a human-readable explanation.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// The failure explanation.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for HandlerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for HandlerError {}

/// The interface a future reader, static, interpreter, tooling, or Wasm
/// implementation uses to plug into the internal runner.
pub trait ProfileHandler: Send + Sync {
    /// Whether this handler owns the declared shape of `case`.
    ///
    /// A profile can have several composable handlers. The dispatcher asks
    /// this predicate before selecting one, so a later static slice can add a
    /// handler without replacing the project handler or claiming unrelated
    /// cases.
    fn can_run(&self, _case: &Case) -> bool {
        true
    }

    /// Executes one case and returns backend-neutral observations.
    fn run(&self, case: &Case) -> Result<CaseObservation, HandlerError>;
}

/// Selects the closest registered profile capable of running each case.
#[derive(Default)]
pub struct ProfileDispatcher {
    handlers: BTreeMap<ConformanceProfile, Vec<Box<dyn ProfileHandler>>>,
}

type HandlerCandidate<'a> = (ConformanceProfile, &'a dyn ProfileHandler);
type HandlerSelection<'a> =
    Result<Option<HandlerCandidate<'a>>, (ConformanceProfile, String)>;

impl fmt::Debug for ProfileDispatcher {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProfileDispatcher")
            .field("profiles", &self.handlers.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl ProfileDispatcher {
    /// Creates an empty dispatcher.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers or replaces the handler for `profile`.
    pub fn register<H>(&mut self, profile: ConformanceProfile, handler: H)
    where
        H: ProfileHandler + 'static,
    {
        self.handlers.insert(profile, vec![Box::new(handler)]);
    }

    /// Builder form of [`Self::register`].
    #[must_use]
    pub fn with_handler<H>(mut self, profile: ConformanceProfile, handler: H) -> Self
    where
        H: ProfileHandler + 'static,
    {
        self.register(profile, handler);
        self
    }

    /// Adds a handler to a profile without replacing existing capability
    /// handlers. Selection remains deterministic by profile depth and
    /// registration order.
    pub fn register_additional<H>(&mut self, profile: ConformanceProfile, handler: H)
    where
        H: ProfileHandler + 'static,
    {
        self.handlers
            .entry(profile)
            .or_default()
            .push(Box::new(handler));
    }

    /// Builder form of [`Self::register_additional`].
    #[must_use]
    pub fn with_additional_handler<H>(
        mut self,
        profile: ConformanceProfile,
        handler: H,
    ) -> Self
    where
        H: ProfileHandler + 'static,
    {
        self.register_additional(profile, handler);
        self
    }

    /// Dispatches one case to the closest capable handler.
    #[must_use]
    pub fn dispatch(&self, case: &Case) -> DispatchResult {
        let required = case.manifest().profile;
        let selection = match self.best_handler(required, case) {
            Ok(selection) => selection,
            Err((provided, reason)) => {
                return DispatchResult::Failed {
                    required,
                    provided,
                    error: HandlerError::new(reason),
                };
            }
        };
        let Some((provided, handler)) = selection else {
            return DispatchResult::Unavailable {
                required,
                reason: format!("no handler provides {required}"),
            };
        };

        match handler.run(case) {
            Ok(observation) => DispatchResult::Executed {
                required,
                provided,
                observation: Box::new(observation),
            },
            Err(error) => DispatchResult::Failed {
                required,
                provided,
                error,
            },
        }
    }

    fn best_handler(
        &self,
        required: ConformanceProfile,
        case: &Case,
    ) -> HandlerSelection<'_> {
        let candidates = self
            .handlers
            .iter()
            .filter(|(profile, _)| profile.supports(required))
            .flat_map(|(profile, handlers)| {
                handlers
                    .iter()
                    .filter(|handler| handler.can_run(case))
                    .map(move |handler| (*profile, handler.as_ref()))
            })
            .collect::<Vec<_>>();
        let Some(best_key) = candidates
            .iter()
            .map(|(profile, _)| {
                (profile.depth().saturating_sub(required.depth()), *profile)
            })
            .min()
        else {
            return Ok(None);
        };
        let mut matching = candidates.into_iter().filter(|(profile, _)| {
            (profile.depth().saturating_sub(required.depth()), *profile) == best_key
        });
        let Some(first) = matching.next() else {
            return Ok(None);
        };
        if matching.next().is_some() {
            return Err((
                first.0,
                format!(
                    "multiple handlers claim operation `{}` at profile {}",
                    case.manifest().operation(),
                    first.0
                ),
            ));
        }
        Ok(Some(first))
    }
}

/// The result of dispatching one case before expectation comparison.
#[derive(Debug)]
pub enum DispatchResult {
    /// A handler returned observations.
    Executed {
        /// The case's requested profile.
        required: ConformanceProfile,
        /// The handler profile that supplied the observations.
        provided: ConformanceProfile,
        /// Backend-neutral observations.
        observation: Box<CaseObservation>,
    },
    /// No registered handler provides the requested capability.
    Unavailable {
        /// The case's requested profile.
        required: ConformanceProfile,
        /// Why no handler was selected.
        reason: String,
    },
    /// A selected handler failed while executing the case.
    Failed {
        /// The case's requested profile.
        required: ConformanceProfile,
        /// The handler profile that failed.
        provided: ConformanceProfile,
        /// The backend failure.
        error: HandlerError,
    },
}

/// The status of one case in a run report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CaseStatus {
    /// The handler's observations matched the manifest.
    Passed,
    /// The handler ran but the observations did not match.
    Failed {
        /// A stable, actionable mismatch explanation.
        reason: String,
    },
    /// The configured implementation does not provide this case's profile.
    Unavailable {
        /// Why execution was not attempted.
        reason: String,
    },
}

/// The WebAssembly backend's status on one executable case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WasmStatus {
    /// The backend ran the case and reproduced its one expectation.
    Matched,
    /// The backend ran the case and disagreed with the expectation.
    Failed {
        /// A stable, actionable mismatch explanation.
        reason: String,
    },
}

/// The status of each backend on one executable case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendStatuses {
    /// The reference interpreter's status: the case's expectations compared
    /// with what the interpreter handler observed.
    pub interpreter: CaseStatus,
    /// The WebAssembly backend's status, or `None` when no Wasm execution
    /// handler took part in the case, or the case never reached a backend
    /// because its handler failed or was unavailable.
    pub wasm: Option<WasmStatus>,
    /// The reference stepper's status, under the same conditions as `wasm`.
    pub native: Option<NativeStatus>,
}

/// One case's result in a run report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaseReport {
    /// Stable case identifier.
    pub case_id: String,
    /// Requested profile.
    pub required_profile: ConformanceProfile,
    /// Profile that supplied execution, when one was selected.
    pub provided_profile: Option<ConformanceProfile>,
    /// Final status. An executable case passes only when both backends do.
    pub status: CaseStatus,
    /// Each backend's status, for an executable case; `None` for any other.
    pub backends: Option<BackendStatuses>,
}

/// Results for an entire corpus run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunReport {
    reports: Vec<CaseReport>,
}

impl RunReport {
    fn new(reports: Vec<CaseReport>) -> Self {
        Self { reports }
    }

    /// Per-case results in corpus order.
    #[must_use]
    pub fn cases(&self) -> &[CaseReport] {
        &self.reports
    }

    /// Number of passed cases.
    #[must_use]
    pub fn passed(&self) -> usize {
        self.reports
            .iter()
            .filter(|report| report.status == CaseStatus::Passed)
            .count()
    }

    /// Number of failed cases, including backend failures and mismatches.
    #[must_use]
    pub fn failed(&self) -> usize {
        self.reports
            .iter()
            .filter(|report| matches!(report.status, CaseStatus::Failed { .. }))
            .count()
    }

    /// Number of cases for which no capable handler was available.
    #[must_use]
    pub fn unavailable(&self) -> usize {
        self.reports
            .iter()
            .filter(|report| matches!(report.status, CaseStatus::Unavailable { .. }))
            .count()
    }

    /// Whether every case executed and matched its expectations.
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.failed() == 0 && self.unavailable() == 0
    }

    /// The reference interpreter's counts over the executable cases.
    #[must_use]
    pub fn interpreter_counts(&self) -> InterpreterCounts {
        let mut counts = InterpreterCounts::default();
        for backends in self
            .reports
            .iter()
            .filter_map(|report| report.backends.as_ref())
        {
            match backends.interpreter {
                CaseStatus::Passed => counts.passed += 1,
                CaseStatus::Failed { .. } => counts.failed += 1,
                CaseStatus::Unavailable { .. } => counts.unavailable += 1,
            }
        }
        counts
    }

    /// The reference stepper's status on every executable case, by case id. A
    /// case that never reached the stepper, because its handler failed or was
    /// unavailable, counts as a failure of it.
    #[must_use]
    pub fn native_statuses(&self) -> Vec<(String, NativeStatus)> {
        self.reports
            .iter()
            .filter_map(|report| {
                let backends = report.backends.as_ref()?;
                let status =
                    backends
                        .native
                        .clone()
                        .unwrap_or_else(|| NativeStatus::Failed {
                            reason: "the case never reached the stepper".to_owned(),
                        });
                Some((report.case_id.clone(), status))
            })
            .collect()
    }

    /// The reference stepper's counts over the executable cases.
    #[must_use]
    pub fn native_counts(&self) -> NativeCounts {
        let mut counts = NativeCounts::default();
        for (_, status) in self.native_statuses() {
            match status {
                NativeStatus::Matched => counts.matched += 1,
                NativeStatus::Pending(_) => counts.pending += 1,
                NativeStatus::Failed { .. } => counts.failed += 1,
            }
        }
        counts
    }

    /// The WebAssembly backend's counts over the executable cases.
    #[must_use]
    pub fn wasm_counts(&self) -> WasmCounts {
        let mut counts = WasmCounts::default();
        for status in self
            .reports
            .iter()
            .filter_map(|report| report.backends.as_ref())
            .filter_map(|backends| backends.wasm.as_ref())
        {
            match status {
                WasmStatus::Matched => counts.matched += 1,
                WasmStatus::Failed { .. } => counts.failed += 1,
            }
        }
        counts
    }
}

/// The reference interpreter's counts over the executable cases.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InterpreterCounts {
    /// Cases whose interpreter observations matched the expectation.
    pub passed: usize,
    /// Cases on which the interpreter did not match, or its handler failed.
    pub failed: usize,
    /// Cases no registered handler could run.
    pub unavailable: usize,
}

/// The WebAssembly backend's counts over the executable cases.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WasmCounts {
    /// Cases the backend reproduced.
    pub matched: usize,
    /// Cases on which the backend disagreed with the expectation.
    pub failed: usize,
}

/// The internal backend-independent conformance runner.
#[derive(Debug)]
pub struct ConformanceRunner {
    dispatcher: ProfileDispatcher,
}

impl ConformanceRunner {
    /// Creates a runner using the supplied profile dispatcher.
    #[must_use]
    pub fn new(dispatcher: ProfileDispatcher) -> Self {
        Self { dispatcher }
    }

    /// Runs all cases in deterministic corpus order.
    #[must_use]
    pub fn run(&self, corpus: &Corpus) -> RunReport {
        let cases = corpus.cases().iter().collect::<Vec<_>>();
        RunReport::new(self.run_cases(&cases))
    }

    /// Runs `cases` on as many threads as the machine has, and reports them in
    /// the order given. Cases share nothing but the engine, which is built to be
    /// shared, so a report does not depend on how the cases were spread over
    /// threads. Executing every case in both backends is the cost of a
    /// conformance run, and it is spread over the cores rather than repeated.
    #[must_use]
    pub fn run_cases(&self, cases: &[&Case]) -> Vec<CaseReport> {
        /// A worker's stack: the checker and the reader recurse on the nesting
        /// of a source, and some cases nest deeply on purpose.
        const WORKER_STACK_BYTES: usize = 64 * 1024 * 1024;
        let workers = std::thread::available_parallelism()
            .map_or(1, std::num::NonZero::get)
            .min(cases.len());
        let next = std::sync::atomic::AtomicUsize::new(0);
        let slots = cases
            .iter()
            .map(|_| std::sync::Mutex::new(None))
            .collect::<Vec<_>>();
        std::thread::scope(|scope| {
            for _ in 0..workers {
                let work = || {
                    loop {
                        let position =
                            next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let (Some(case), Some(slot)) =
                            (cases.get(position), slots.get(position))
                        else {
                            break;
                        };
                        let report = self.run_case(case);
                        if let Ok(mut slot) = slot.lock() {
                            *slot = Some(report);
                        }
                    }
                };
                // A thread that cannot start leaves its cases to the others, or
                // to the sequential pass below.
                let _ = std::thread::Builder::new()
                    .stack_size(WORKER_STACK_BYTES)
                    .spawn_scoped(scope, work);
            }
        });
        cases
            .iter()
            .zip(slots)
            .map(|(case, slot)| {
                slot.into_inner()
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| self.run_case(case))
            })
            .collect()
    }

    /// Runs one loaded case.
    #[must_use]
    pub fn run_case(&self, case: &Case) -> CaseReport {
        let case_id = case.manifest().id.clone();
        let executable = case.manifest().is_executable();
        match self.dispatcher.dispatch(case) {
            DispatchResult::Executed {
                required,
                provided,
                observation,
            } => {
                let interpreter = case
                    .manifest()
                    .expectations
                    .matches(case, &observation)
                    .map_or_else(
                        |reason| CaseStatus::Failed { reason },
                        |()| CaseStatus::Passed,
                    );
                if !executable {
                    return CaseReport {
                        case_id,
                        required_profile: required,
                        provided_profile: Some(provided),
                        status: interpreter,
                        backends: None,
                    };
                }
                let wasm = wasm_status(case, &observation);
                let native = native_status(case, &observation);
                CaseReport {
                    case_id,
                    required_profile: required,
                    provided_profile: Some(provided),
                    status: differential_status(
                        &interpreter,
                        wasm.as_ref(),
                        native.as_ref(),
                    ),
                    backends: Some(BackendStatuses {
                        interpreter,
                        wasm,
                        native,
                    }),
                }
            }
            DispatchResult::Unavailable { required, reason } => {
                let status = CaseStatus::Unavailable { reason };
                CaseReport {
                    case_id,
                    required_profile: required,
                    provided_profile: None,
                    backends: executable.then(|| BackendStatuses {
                        interpreter: status.clone(),
                        wasm: None,
                        native: None,
                    }),
                    status,
                }
            }
            DispatchResult::Failed {
                required,
                provided,
                error,
            } => {
                let status = CaseStatus::Failed {
                    reason: format!("handler failed: {error}"),
                };
                CaseReport {
                    case_id,
                    required_profile: required,
                    provided_profile: Some(provided),
                    backends: executable.then(|| BackendStatuses {
                        interpreter: status.clone(),
                        wasm: None,
                        native: None,
                    }),
                    status,
                }
            }
        }
    }

    /// The dispatch table used by this runner.
    #[must_use]
    pub fn dispatcher(&self) -> &ProfileDispatcher {
        &self.dispatcher
    }
}

impl CaseExpectations {
    fn matches(
        &self,
        case: &Case,
        observation: &CaseObservation,
    ) -> Result<(), String> {
        if self.accepted != observation.accepted {
            return Err(format!(
                "acceptance mismatch: expected {}, got {}",
                self.accepted, observation.accepted
            ));
        }
        if self.diagnostics.len() != observation.diagnostics.len() {
            return Err(format!(
                "diagnostic count mismatch: expected {}, got {}",
                self.diagnostics.len(),
                observation.diagnostics.len()
            ));
        }
        for (index, (expected, actual)) in self
            .diagnostics
            .iter()
            .zip(&observation.diagnostics)
            .enumerate()
        {
            if expected.code != actual.code() {
                return Err(format!(
                    "diagnostic {index} code mismatch: expected {}, got {}",
                    expected.code,
                    actual.code()
                ));
            }
            if expected.level != actual.level() {
                return Err(format!(
                    "diagnostic {index} level mismatch: expected {}, got {}",
                    expected.level,
                    actual.level()
                ));
            }
            if expected.source_id.as_deref() != actual.source_id() {
                return Err(format!(
                    "diagnostic {index} source mismatch: expected {:?}, got {:?}",
                    expected.source_id,
                    actual.source_id()
                ));
            }
            if expected.primary_span != actual.primary_span() {
                return Err(format!(
                    "diagnostic {index} primary span mismatch: expected {:?}, got {:?}",
                    expected.primary_span,
                    actual.primary_span()
                ));
            }
            if let Some(message) = &expected.message
                && actual.message() != message
            {
                return Err(format!("diagnostic {index} message mismatch"));
            }
            if expected.related.len() != actual.related().len() {
                return Err(format!(
                    "diagnostic {index} related-span count mismatch: expected {}, got {}",
                    expected.related.len(),
                    actual.related().len()
                ));
            }
            for (related_index, (expected_related, actual_related)) in
                expected.related.iter().zip(actual.related()).enumerate()
            {
                if expected_related.span != actual_related.span {
                    return Err(format!(
                        "diagnostic {index} related span {related_index} mismatch"
                    ));
                }
                if expected_related.source_id.as_deref()
                    != actual_related.source_id.as_deref()
                {
                    return Err(format!(
                        "diagnostic {index} related span {related_index} source mismatch"
                    ));
                }
                if let Some(message) = &expected_related.message
                    && actual_related.message != *message
                {
                    return Err(format!(
                        "diagnostic {index} related message {related_index} mismatch"
                    ));
                }
            }
            if !expected.notes.is_empty() && expected.notes != actual.notes() {
                return Err(format!("diagnostic {index} notes mismatch"));
            }
            if let Some(expected_fixes) = &expected.fixes {
                if expected_fixes.len() != actual.fixes().len() {
                    return Err(format!(
                        "diagnostic {index} fix count mismatch: expected {}, got {}",
                        expected_fixes.len(),
                        actual.fixes().len()
                    ));
                }
                for (fix_index, (expected_fix, actual_fix)) in
                    expected_fixes.iter().zip(actual.fixes()).enumerate()
                {
                    if expected_fix.safe != actual_fix.is_safe() {
                        return Err(format!(
                            "diagnostic {index} fix {fix_index} safety mismatch"
                        ));
                    }
                    if let Some(description) = &expected_fix.description
                        && actual_fix.description() != description
                    {
                        return Err(format!(
                            "diagnostic {index} fix {fix_index} description mismatch"
                        ));
                    }
                    if let Some(revision) = &expected_fix.revision
                        && actual_fix.expected_revision().as_str() != revision
                    {
                        return Err(format!(
                            "diagnostic {index} fix {fix_index} revision mismatch"
                        ));
                    }
                }
            }
        }

        compare_snapshot(
            case,
            "formatted",
            self.formatted.as_deref(),
            observation.formatted.as_deref(),
        )?;
        compare_snapshot(
            case,
            "graph",
            self.graph.as_deref(),
            observation.graph.as_deref(),
        )?;
        compare_snapshot(
            case,
            "resolved",
            self.resolved.as_deref(),
            observation.resolved.as_deref(),
        )?;
        compare_snapshot(
            case,
            "types",
            self.types.as_deref(),
            observation.types.as_deref(),
        )?;
        compare_snapshot(
            case,
            "effects",
            self.effects.as_deref(),
            observation.effects.as_deref(),
        )?;
        compare_snapshot(
            case,
            "index",
            self.index.as_deref(),
            observation.index.as_deref(),
        )?;
        if self.queries.len() != observation.queries.len() {
            return Err(format!(
                "query observation count mismatch: expected {}, got {}",
                self.queries.len(),
                observation.queries.len()
            ));
        }
        for (index, (expected, actual)) in
            self.queries.iter().zip(&observation.queries).enumerate()
        {
            if expected.input != actual.input || expected.offset != actual.offset {
                return Err(format!(
                    "query {index} subject mismatch: expected {} at {}, got {} at {}",
                    expected.input, expected.offset, actual.input, actual.offset
                ));
            }
            let snapshot = case.read_file(&expected.snapshot).map_err(|error| {
                format!(
                    "query snapshot `{}` cannot be read: {error}",
                    expected.snapshot
                )
            })?;
            if actual.result != snapshot {
                return Err(format!(
                    "query snapshot mismatch (`{}`)",
                    expected.snapshot
                ));
            }
        }
        if self.host_event != observation.host_event {
            return Err(format!(
                "host event mismatch: expected {:?}, got {:?}",
                self.host_event, observation.host_event
            ));
        }
        compare_execution(
            case,
            "interpreter",
            self.interpreter.as_ref(),
            observation
                .interpreter
                .as_ref()
                .map(|backend| &backend.outcome),
        )?;
        Ok(())
    }
}

/// The WebAssembly backend's status on an executable case, against the one
/// expectation the case has, or `None` when no Wasm execution handler took part
/// in the case, so that only the interpreter judges it.
fn wasm_status(case: &Case, observation: &CaseObservation) -> Option<WasmStatus> {
    let wasm = observation.wasm.as_ref()?;
    Some(executor_status(
        case,
        observation,
        wasm,
        "wasm",
        "the module",
    ))
}

/// The stepper's status: each of its runs, held to the comparison the Wasm
/// backend is held to.
fn native_status(case: &Case, observation: &CaseObservation) -> Option<NativeStatus> {
    Some(match observation.native.as_ref()? {
        NativeObservation::Pending { package, .. } => {
            NativeStatus::Pending(package.clone())
        }
        NativeObservation::Ran(runs) => {
            for run in runs {
                if let WasmStatus::Failed { reason } = executor_status(
                    case,
                    observation,
                    &run.observation,
                    "stepper",
                    "the stepper",
                ) {
                    let quota = if run.quota == u64::MAX {
                        "unbounded".to_owned()
                    } else {
                        format!("quota {}", run.quota)
                    };
                    return Some(NativeStatus::Failed {
                        reason: format!("{reason} ({quota})"),
                    });
                }
            }
            NativeStatus::Matched
        }
    })
}

/// An executor that reports in the terms of [`WasmObservation`], held to the
/// case's one expectation. `name` is how a comparison names it and `subject`
/// how a reason refers to it.
fn executor_status(
    case: &Case,
    observation: &CaseObservation,
    wasm: &BackendObservation<WasmObservation>,
    name: &str,
    subject: &str,
) -> WasmStatus {
    let expectations = &case.manifest().expectations;
    match &wasm.outcome {
        WasmObservation::Completed(actual) => {
            if let Some(event) = &expectations.host_event {
                return WasmStatus::Failed {
                    reason: format!(
                        "host event mismatch: expected {event}, but {subject} completed"
                    ),
                };
            }
            // A case with no result snapshot and a runtime error expects the
            // run to trap, so a module that completed disagrees.
            if let Some(trap) = expectations.diagnostics.iter().find(|diagnostic| {
                expectations.interpreter.is_none()
                    && diagnostic.code.domain() == vibra_diagnostics::Domain::Runtime
            }) {
                return WasmStatus::Failed {
                    reason: format!(
                        "trap mismatch: expected {}, but {subject} completed",
                        trap.code.as_atom()
                    ),
                };
            }
            if let (Some(interpreter), Some(wasm)) = (
                observation
                    .interpreter
                    .as_ref()
                    .and_then(|backend| backend.program_failed),
                wasm.program_failed,
            ) && interpreter != wasm
            {
                return WasmStatus::Failed {
                    reason: format!(
                        "command outcome mismatch: the interpreter's entry {} err, {subject}'s {}",
                        if interpreter {
                            "returned"
                        } else {
                            "did not return"
                        },
                        if wasm { "did" } else { "did not" }
                    ),
                };
            }
            compare_execution(
                case,
                name,
                expectations.interpreter.as_ref(),
                Some(actual),
            )
            .map_or_else(
                |reason| WasmStatus::Failed { reason },
                |()| WasmStatus::Matched,
            )
        }
        WasmObservation::Failed { reason } => WasmStatus::Failed {
            reason: reason.clone(),
        },
        WasmObservation::Trapped(code) => {
            if expectations
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.as_atom() == code)
            {
                WasmStatus::Matched
            } else {
                WasmStatus::Failed {
                    reason: format!(
                        "trap mismatch: {subject} trapped with {code}, which the case does not expect"
                    ),
                }
            }
        }
        WasmObservation::HostEvent(event) => {
            if expectations.host_event.as_deref() == Some(event.as_str()) {
                WasmStatus::Matched
            } else {
                WasmStatus::Failed {
                    reason: format!(
                        "host event mismatch: expected {:?}, got {event:?}",
                        expectations.host_event
                    ),
                }
            }
        }
    }
}

/// The status of an executable case: it passes only when both backends do,
/// and a failure names the backend that disagreed.
fn differential_status(
    interpreter: &CaseStatus,
    wasm: Option<&WasmStatus>,
    native: Option<&NativeStatus>,
) -> CaseStatus {
    let mut reasons = Vec::new();
    match interpreter {
        CaseStatus::Passed => {}
        CaseStatus::Failed { reason } => {
            reasons.push(format!("interpreter backend: {reason}"));
        }
        CaseStatus::Unavailable { reason } => {
            return CaseStatus::Unavailable {
                reason: reason.clone(),
            };
        }
    }
    if let Some(WasmStatus::Failed { reason }) = wasm {
        reasons.push(format!("wasm backend: {reason}"));
    }
    if let Some(NativeStatus::Failed { reason }) = native {
        reasons.push(format!("native backend: stepper: {reason}"));
    }
    if reasons.is_empty() {
        CaseStatus::Passed
    } else {
        CaseStatus::Failed {
            reason: reasons.join("; "),
        }
    }
}

fn compare_snapshot(
    case: &Case,
    name: &str,
    expected_path: Option<&str>,
    actual: Option<&str>,
) -> Result<(), String> {
    let Some(expected_path) = expected_path else {
        return Ok(());
    };
    let expected = case.read_file(expected_path).map_err(|error| {
        format!("{name} snapshot `{expected_path}` cannot be read: {error}")
    })?;
    if actual != Some(expected.as_str()) {
        return Err(format!("{name} snapshot mismatch (`{expected_path}`)"));
    }
    Ok(())
}

fn compare_execution(
    case: &Case,
    name: &str,
    expected: Option<&ExpectedExecution>,
    actual: Option<&ExecutionObservation>,
) -> Result<(), String> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let Some(actual) = actual else {
        return Err(format!("{name} execution observation is unavailable"));
    };
    if let Some(result_path) = &expected.result {
        let result = case.read_file(result_path).map_err(|error| {
            format!("{name} result snapshot `{result_path}` cannot be read: {error}")
        })?;
        if actual.result.as_deref() != Some(result.as_str()) {
            return Err(format!("{name} result snapshot mismatch (`{result_path}`)"));
        }
    }
    if let Some(audit_path) = &expected.audit_trace {
        let audit = case.read_file(audit_path).map_err(|error| {
            format!("{name} audit snapshot `{audit_path}` cannot be read: {error}")
        })?;
        let actual_audit = canonical_audit_snapshot(&actual.audit_trace);
        if actual_audit != audit {
            return Err(format!(
                "{name} audit-trace snapshot mismatch (`{audit_path}`)"
            ));
        }
    }
    Ok(())
}

fn canonical_audit_snapshot(events: &[vibra_host::AuditEvent]) -> String {
    format!("{}\n", vibra_host::trace_to_vibon(events))
}
