//! Profile dispatch and expectation checking for the internal corpus runner.

use std::collections::BTreeMap;
use std::fmt;

use vibra_diagnostics::Diagnostic;
use vibra_ir::Value;

use crate::corpus::{Case, Corpus};
use crate::manifest::{CaseExpectations, ExpectedExecution};
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
    pub audit_trace: Vec<String>,
}

/// What the WebAssembly backend did with an accepted executable case.
///
/// The case has one expectation (`docs/spec/07-diagnostics-and-conformance.md`,
/// "Differential execution"), so the Wasm backend reports the same kinds of
/// observation the interpreter does, and the runner compares them with it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WasmObservation {
    /// The program uses forms the backend does not lower yet, named by the
    /// emitter. The runner counts the case as not lowered and neither passes
    /// nor fails the backend on it.
    NotLowered {
        /// The names of the forms, empty when no Wasm execution handler took
        /// part in the case.
        forms: Vec<String>,
    },
    /// The module ran to completion.
    Completed(ExecutionObservation),
    /// The instance ended in a host event: the atom of its unlocated
    /// diagnostic, such as `@runtime.memory-exhausted`.
    HostEvent(String),
    /// The backend failed on a program it lowered: the module did not
    /// validate, the engine refused it, or the run stopped some way the
    /// interpreter did not. It fails the Wasm backend on the case.
    Failed {
        /// What went wrong.
        reason: String,
    },
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
    pub interpreter: Option<ExecutionObservation>,
    /// The host event that ended execution, instead of a result: the atom of
    /// the unlocated diagnostic, such as `@runtime.memory-exhausted`.
    pub host_event: Option<String>,
    /// What the WebAssembly backend did with the case. A handler that runs an
    /// executable case sets it for every program it runs. A rejected program
    /// is no executable case, so a handler leaves it unset.
    pub wasm: Option<WasmObservation>,
    /// Deterministic artifact hashes.
    pub artifact_hashes: Vec<String>,
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

    /// Profiles that have a handler registered, in stable order.
    #[must_use]
    pub fn registered_profiles(&self) -> Vec<ConformanceProfile> {
        self.handlers.keys().copied().collect()
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
    /// The program uses forms the backend does not lower yet. The case runs
    /// in the interpreter only, and the backend neither passes nor fails it.
    NotLowered {
        /// The names of the forms, empty when no Wasm execution handler took
        /// part in the case.
        forms: Vec<String>,
    },
}

/// The status of each backend on one executable case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendStatuses {
    /// The reference interpreter's status: the case's expectations compared
    /// with what the interpreter handler observed.
    pub interpreter: CaseStatus,
    /// The WebAssembly backend's status, or `None` when the case never
    /// reached a backend because its handler failed or was unavailable.
    pub wasm: Option<WasmStatus>,
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
                WasmStatus::NotLowered { .. } => counts.not_lowered += 1,
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
    /// Cases whose program uses forms the backend does not lower yet.
    pub not_lowered: usize,
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
        let reports = corpus
            .cases()
            .iter()
            .map(|case| self.run_case(case))
            .collect();
        RunReport::new(reports)
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
                CaseReport {
                    case_id,
                    required_profile: required,
                    provided_profile: Some(provided),
                    status: differential_status(&interpreter, &wasm),
                    backends: Some(BackendStatuses {
                        interpreter,
                        wasm: Some(wasm),
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
            observation.interpreter.as_ref(),
        )?;
        if let Some(expected_hashes) = &self.artifact_hashes
            && expected_hashes != &observation.artifact_hashes
        {
            return Err(format!(
                "artifact hash mismatch: expected {:?}, got {:?}",
                expected_hashes, observation.artifact_hashes
            ));
        }
        Ok(())
    }
}

/// The WebAssembly backend's status on an executable case, against the one
/// expectation the case has.
fn wasm_status(case: &Case, observation: &CaseObservation) -> WasmStatus {
    let expectations = &case.manifest().expectations;
    match &observation.wasm {
        // No Wasm execution handler took part, so nothing is lowered.
        None => WasmStatus::NotLowered { forms: Vec::new() },
        Some(WasmObservation::NotLowered { forms }) => WasmStatus::NotLowered {
            forms: forms.clone(),
        },
        Some(WasmObservation::Completed(actual)) => {
            if let Some(event) = &expectations.host_event {
                return WasmStatus::Failed {
                    reason: format!(
                        "host event mismatch: expected {event}, but the module completed"
                    ),
                };
            }
            compare_execution(
                case,
                "wasm",
                expectations.interpreter.as_ref(),
                Some(actual),
            )
            .map_or_else(
                |reason| WasmStatus::Failed { reason },
                |()| WasmStatus::Matched,
            )
        }
        Some(WasmObservation::Failed { reason }) => WasmStatus::Failed {
            reason: reason.clone(),
        },
        Some(WasmObservation::HostEvent(event)) => {
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
fn differential_status(interpreter: &CaseStatus, wasm: &WasmStatus) -> CaseStatus {
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
    if let WasmStatus::Failed { reason } = wasm {
        reasons.push(format!("wasm backend: {reason}"));
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

fn canonical_audit_snapshot(events: &[String]) -> String {
    let values = events
        .iter()
        .map(|event| Value::Str(event.clone()).canonical_vibon())
        .collect::<Vec<_>>();
    if values.is_empty() {
        "(record format: @audit-trace.v1 events: (array))\n".to_owned()
    } else {
        format!(
            "(record format: @audit-trace.v1 events: (array {}))\n",
            values.join(" ")
        )
    }
}
