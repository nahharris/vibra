//! Profile dispatch and expectation checking for the internal corpus runner.

use std::collections::BTreeMap;
use std::fmt;

use vibra_diagnostics::Diagnostic;

use crate::corpus::{Case, Corpus};
use crate::manifest::{CaseExpectations, ExpectedExecution};
use crate::profile::ConformanceProfile;

/// A reference-interpreter or Wasm result and its ordered audit trace.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExecutionObservation {
    /// The serialized result, when execution produced one.
    pub result: Option<String>,
    /// Audit events in execution order.
    pub audit_trace: Vec<String>,
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
    /// Resolved-identity output, if the handler provides it.
    pub resolved: Option<String>,
    /// Type output, if the handler provides it.
    pub types: Option<String>,
    /// Effect output, if the handler provides it.
    pub effects: Option<String>,
    /// Reference-interpreter observation.
    pub interpreter: Option<ExecutionObservation>,
    /// Wasm observation.
    pub wasm: Option<ExecutionObservation>,
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
    /// Executes one case and returns backend-neutral observations.
    fn run(&self, case: &Case) -> Result<CaseObservation, HandlerError>;
}

/// Selects the closest registered profile capable of running each case.
#[derive(Default)]
pub struct ProfileDispatcher {
    handlers: BTreeMap<ConformanceProfile, Box<dyn ProfileHandler>>,
}

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
        self.handlers.insert(profile, Box::new(handler));
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

    /// Profiles that have a handler registered, in stable order.
    #[must_use]
    pub fn registered_profiles(&self) -> Vec<ConformanceProfile> {
        self.handlers.keys().copied().collect()
    }

    /// Dispatches one case to the closest capable handler.
    #[must_use]
    pub fn dispatch(&self, case: &Case) -> DispatchResult {
        let required = case.manifest().profile;
        let Some((provided, handler)) = self.best_handler(required) else {
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
    ) -> Option<(ConformanceProfile, &dyn ProfileHandler)> {
        self.handlers
            .iter()
            .filter(|(profile, _)| profile.supports(required))
            .min_by_key(|(profile, _)| {
                (profile.depth().saturating_sub(required.depth()), **profile)
            })
            .map(|(profile, handler)| (*profile, handler.as_ref()))
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

/// One case's result in a run report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaseReport {
    /// Stable case identifier.
    pub case_id: String,
    /// Requested profile.
    pub required_profile: ConformanceProfile,
    /// Profile that supplied execution, when one was selected.
    pub provided_profile: Option<ConformanceProfile>,
    /// Final status.
    pub status: CaseStatus,
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
        match self.dispatcher.dispatch(case) {
            DispatchResult::Executed {
                required,
                provided,
                observation,
            } => CaseReport {
                case_id,
                required_profile: required,
                provided_profile: Some(provided),
                status: case
                    .manifest()
                    .expectations
                    .matches(case, &observation)
                    .map_or_else(
                        |reason| CaseStatus::Failed { reason },
                        |_| CaseStatus::Passed,
                    ),
            },
            DispatchResult::Unavailable { required, reason } => CaseReport {
                case_id,
                required_profile: required,
                provided_profile: None,
                status: CaseStatus::Unavailable { reason },
            },
            DispatchResult::Failed {
                required,
                provided,
                error,
            } => CaseReport {
                case_id,
                required_profile: required,
                provided_profile: Some(provided),
                status: CaseStatus::Failed {
                    reason: format!("handler failed: {error}"),
                },
            },
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
        compare_execution(
            case,
            "interpreter",
            self.interpreter.as_ref(),
            observation.interpreter.as_ref(),
        )?;
        compare_execution(case, "wasm", self.wasm.as_ref(), observation.wasm.as_ref())?;
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
        let actual_audit = actual.audit_trace.join("\n");
        if actual_audit != audit {
            return Err(format!(
                "{name} audit-trace snapshot mismatch (`{audit_path}`)"
            ));
        }
    }
    Ok(())
}
