//! Comparison of observations with expectations, and the run report.
//!
//! The runner reports, per profile, the counts of passed, failed, unknown,
//! and unavailable cases (`docs/spec/07-diagnostics-and-conformance.md`,
//! "Differential execution"). There is no "skipped" count: a case no handler
//! runs is unavailable, and only a passed case is a success.

use std::collections::BTreeMap;
use std::fmt;

use vibra_diagnostics::DiagnosticCode;
use vibra_syntax::DataValue;

use crate::corpus::{Case, Corpus};
use crate::manifest::{CaseExpectations, Verdict};
use crate::profile::{CaseObservation, ConformanceProfile};
use crate::standard::{DispatchResult, ProfileDispatcher};

/// The status of one case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CaseStatus {
    /// The observations matched every expectation.
    Passed,
    /// A handler ran and an observation did not match, or the handler failed.
    Failed {
        /// The first mismatch.
        reason: String,
    },
    /// A handler ran and established no result: it observed the verdict
    /// `unknown` where the case expects another.
    Unknown {
        /// What the handler did not establish.
        reason: String,
    },
    /// No registered handler runs the case.
    Unavailable {
        /// Why no handler was selected.
        reason: String,
    },
}

/// One case's line of a run report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaseReport {
    /// The case identity.
    pub case_id: String,
    /// The case's profile.
    pub required_profile: ConformanceProfile,
    /// The profile of the handler that ran it, when one was selected.
    pub provided_profile: Option<ConformanceProfile>,
    /// The status.
    pub status: CaseStatus,
}

/// The counts of one profile.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProfileCounts {
    /// Cases that passed.
    pub passed: usize,
    /// Cases that failed.
    pub failed: usize,
    /// Cases whose result was not established.
    pub unknown: usize,
    /// Cases no handler runs.
    pub unavailable: usize,
}

impl ProfileCounts {
    fn add(&mut self, status: &CaseStatus) {
        match status {
            CaseStatus::Passed => self.passed += 1,
            CaseStatus::Failed { .. } => self.failed += 1,
            CaseStatus::Unknown { .. } => self.unknown += 1,
            CaseStatus::Unavailable { .. } => self.unavailable += 1,
        }
    }
}

impl fmt::Display for ProfileCounts {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} passed, {} failed, {} unknown, {} unavailable",
            self.passed, self.failed, self.unknown, self.unavailable
        )
    }
}

/// The results of a corpus run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunReport {
    reports: Vec<CaseReport>,
}

impl RunReport {
    /// Every case's line, in corpus order.
    #[must_use]
    pub fn cases(&self) -> &[CaseReport] {
        &self.reports
    }

    /// The counts of every profile that has a case, in specification order.
    #[must_use]
    pub fn profile_counts(&self) -> Vec<(ConformanceProfile, ProfileCounts)> {
        let mut counts = BTreeMap::<ConformanceProfile, ProfileCounts>::new();
        for report in &self.reports {
            counts
                .entry(report.required_profile)
                .or_default()
                .add(&report.status);
        }
        ConformanceProfile::ALL
            .iter()
            .filter_map(|profile| counts.get(profile).map(|count| (*profile, *count)))
            .collect()
    }

    /// The counts over every case.
    #[must_use]
    pub fn total(&self) -> ProfileCounts {
        let mut total = ProfileCounts::default();
        for report in &self.reports {
            total.add(&report.status);
        }
        total
    }

    /// Whether every case passed.
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.reports
            .iter()
            .all(|report| report.status == CaseStatus::Passed)
    }
}

/// The backend-independent corpus runner.
#[derive(Debug)]
pub struct ConformanceRunner {
    dispatcher: ProfileDispatcher,
}

impl ConformanceRunner {
    /// A runner over `dispatcher`.
    #[must_use]
    pub const fn new(dispatcher: ProfileDispatcher) -> Self {
        Self { dispatcher }
    }

    /// Runs every case, on as many threads as the machine has, and reports
    /// them in corpus order.
    #[must_use]
    pub fn run(&self, corpus: &Corpus) -> RunReport {
        /// A worker's stack: readers and checkers recurse on the nesting of a
        /// source, and some cases nest deeply on purpose.
        const WORKER_STACK_BYTES: usize = 64 * 1024 * 1024;
        let cases = corpus.cases();
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
                // A thread that cannot start leaves its cases to the others,
                // or to the sequential pass below.
                let _ = std::thread::Builder::new()
                    .stack_size(WORKER_STACK_BYTES)
                    .spawn_scoped(scope, work);
            }
        });
        let reports = cases
            .iter()
            .zip(slots)
            .map(|(case, slot)| {
                slot.into_inner()
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| self.run_case(case))
            })
            .collect();
        RunReport { reports }
    }

    /// Runs one case.
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
                status: compare(case, &case.manifest().expectations, &observation),
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

    /// The dispatcher of this runner.
    #[must_use]
    pub const fn dispatcher(&self) -> &ProfileDispatcher {
        &self.dispatcher
    }
}

/// The status of one executed case.
fn compare(
    case: &Case,
    expected: &CaseExpectations,
    observation: &CaseObservation,
) -> CaseStatus {
    if observation.verdict == Verdict::Unknown && expected.verdict != Verdict::Unknown {
        return CaseStatus::Unknown {
            reason: format!(
                "the handler established no result; the case expects {}",
                expected.verdict
            ),
        };
    }
    match matches(case, expected, observation) {
        Ok(()) => CaseStatus::Passed,
        Err(reason) => CaseStatus::Failed { reason },
    }
}

fn matches(
    case: &Case,
    expected: &CaseExpectations,
    observation: &CaseObservation,
) -> Result<(), String> {
    if expected.verdict != observation.verdict {
        return Err(format!(
            "verdict mismatch: expected {}, got {}",
            expected.verdict, observation.verdict
        ));
    }
    if expected.host_event != observation.host_event {
        return Err(format!(
            "host event mismatch: expected {:?}, got {:?}",
            expected.host_event.map(DiagnosticCode::as_atom),
            observation.host_event.map(DiagnosticCode::as_atom)
        ));
    }
    if expected.diagnostics.len() != observation.diagnostics.len() {
        return Err(format!(
            "diagnostic count mismatch: expected {}, got {}",
            expected.diagnostics.len(),
            observation.diagnostics.len()
        ));
    }
    for (index, (expected, actual)) in expected
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
        if expected.source_id.is_some()
            && expected.source_id.as_deref() != actual.source_id()
        {
            return Err(format!(
                "diagnostic {index} source mismatch: expected {:?}, got {:?}",
                expected.source_id,
                actual.source_id()
            ));
        }
        if expected.primary_span != actual.primary_span() {
            return Err(format!(
                "diagnostic {index} span mismatch: expected {:?}, got {:?}",
                expected.primary_span,
                actual.primary_span()
            ));
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
                    "diagnostic {index} related span {related_index} mismatch: \
                     expected {:?}, got {:?}",
                    expected_related.span, actual_related.span
                ));
            }
            if expected_related.source_id.is_some()
                && expected_related.source_id != actual_related.source_id
            {
                return Err(format!(
                    "diagnostic {index} related span {related_index} source mismatch: \
                     expected {:?}, got {:?}",
                    expected_related.source_id, actual_related.source_id
                ));
            }
        }
        if !expected.notes.is_empty() && expected.notes != actual.notes() {
            return Err(format!("diagnostic {index} notes mismatch"));
        }
        if let Some(fixes) = &expected.fixes {
            if fixes.len() != actual.fixes().len() {
                return Err(format!(
                    "diagnostic {index} fix count mismatch: expected {}, got {}",
                    fixes.len(),
                    actual.fixes().len()
                ));
            }
            for (fix_index, (expected_fix, actual_fix)) in
                fixes.iter().zip(actual.fixes()).enumerate()
            {
                let fix_matches = expected_fix.safe == actual_fix.is_safe()
                    && expected_fix
                        .description
                        .as_deref()
                        .is_none_or(|description| {
                            description == actual_fix.description()
                        })
                    && expected_fix.expected_revision.as_deref().is_none_or(
                        |revision| revision == actual_fix.expected_revision().as_str(),
                    );
                if !fix_matches {
                    return Err(format!("diagnostic {index} fix {fix_index} mismatch"));
                }
            }
        }
    }
    let observed = [
        ("tree", observation.tree.as_deref()),
        ("formatted", observation.formatted.as_deref()),
        ("graph", observation.graph.as_deref()),
        ("resolved", observation.resolved.as_deref()),
        ("types", observation.types.as_deref()),
        ("result", observation.result.as_deref()),
        ("testRun", observation.test_run.as_deref()),
        ("targetRun", observation.target_run.as_deref()),
    ];
    for (key, path) in expected.snapshots() {
        let snapshot = case.read_bytes(path).map_err(|error| {
            format!("{key} snapshot `{path}` cannot be read: {error}")
        })?;
        let Some(actual) = observed
            .iter()
            .find(|(name, _)| *name == key)
            .and_then(|(_, actual)| *actual)
        else {
            return Err(format!("the handler produced no {key} observation"));
        };
        let same = match key {
            "formatted" => actual.as_bytes() == snapshot.as_slice(),
            "targetRun" => same_document(
                path,
                &without_failure_messages(&snapshot),
                &String::from_utf8_lossy(&without_failure_messages(actual.as_bytes())),
            )?,
            _ => same_document(path, &snapshot, actual)?,
        };
        if !same {
            return Err(format!("{key} snapshot mismatch (`{path}`)"));
        }
    }
    Ok(())
}

/// Whether a VIBON snapshot and an observation are one decoded value.
///
/// The formatted text is the one observation compared byte for byte: it is
/// the formatter's output, not a persistent observation. Every other snapshot
/// is a VIBON document of a format of "Persistent observations", compared as
/// its decoded value, so the layout of a snapshot file is free.
fn same_document(path: &str, snapshot: &[u8], actual: &str) -> Result<bool, String> {
    let expected = vibra_syntax::parse_data(Some(path), snapshot);
    let Some(expected) = expected.value() else {
        return Err(format!(
            "snapshot `{path}` is not a valid VIBON document: {:?}",
            expected
                .diagnostics()
                .iter()
                .map(|diagnostic| (diagnostic.code(), diagnostic.primary_span()))
                .collect::<Vec<_>>()
        ));
    };
    let observed = vibra_syntax::parse_data(None, actual.as_bytes());
    let Some(observed) = observed.value() else {
        return Err(format!(
            "the observation compared with `{path}` is not a valid VIBON document"
        ));
    };
    Ok(expected.decoded_eq(observed))
}

/// A `@targetRun.v2` document with the `message` of every `failure` record
/// written as `""`: a startup failure is compared by its code and its
/// fields, and its message is text for a person, as a diagnostic's is. A
/// document that does not decode is returned as it is.
fn without_failure_messages(document: &[u8]) -> Vec<u8> {
    let parsed = vibra_syntax::parse_data(None, document);
    let Some(root) = parsed.value() else {
        return document.to_vec();
    };
    let mut messages = Vec::new();
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        match node.value() {
            DataValue::Record(fields) => {
                for field in fields {
                    if field.name() == "failure"
                        && let DataValue::Record(failure) = field.value().value()
                    {
                        messages.extend(
                            failure
                                .iter()
                                .filter(|field| field.name() == "message")
                                .map(|field| field.value().span()),
                        );
                    }
                    pending.push(field.value());
                }
            }
            DataValue::List(items) | DataValue::Tuple(items) => pending.extend(items),
            DataValue::Dict(entries) => {
                pending.extend(
                    entries
                        .iter()
                        .flat_map(|entry| [entry.key(), entry.value()]),
                );
            }
            _ => {}
        }
    }
    messages.sort_by_key(|span| span.start());
    let mut written = Vec::with_capacity(document.len());
    let mut at = 0;
    for span in messages {
        written.extend_from_slice(document.get(at..span.start()).unwrap_or_default());
        written.extend_from_slice(b"\"\"");
        at = span.end();
    }
    written.extend_from_slice(document.get(at..).unwrap_or_default());
    written
}
