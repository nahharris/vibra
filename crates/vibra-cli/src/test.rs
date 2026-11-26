//! `vibra test [TEST]`.
//!
//! `docs/spec/05-tooling.md`, "Tests and selection", "Versioned envelope and
//! results", and "Command payloads", are the authority; the work is the
//! workspace engine's [`run_tests`].
//!
//! 1. A selector is read before anything else: one that is not well formed
//!    is invalid input, with no snapshot and no diagnostics.
//! 2. Project, source-graph, and required parse errors come first, with zero
//!    counts and no items.
//! 3. A well-formed selector that names no module or no test is invalid
//!    input.
//! 4. Every selected test is checked, then each runs under the published
//!    memory budget, [`MEMORY_BUDGET`]. The suite result is the engine's:
//!    static errors make every item `@test.invalid`, and otherwise the
//!    precedence is trap, operational exhaustion, unavailable, test failed,
//!    ok.
//! 5. A diagnostic appears once in the envelope, sorted by source, start,
//!    end, and registry order, and its code is attributed to every item it
//!    affects: every error to every item of an invalid selection, and to an
//!    item the diagnostic of its own ending (its `@tool.unavailable`, its
//!    `@test.conflictingFixture`, its trap, its exhaustion, or its
//!    observation error).
//! 6. Memory exhaustion is the host event `@runtime.memoryExhausted` on its
//!    item: the item is `@test.exhausted`, the limit `memory` is reached,
//!    and the observation is incomplete. It is not a trap.

use serde_json::Value as Json;
use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode, LineIndex};
use vibra_schema::{
    AuditEvent, CommandOutcome, ObservationError, ObservationErrorCode, Payload,
    SpanDocument, Status, TestFailure, TestFailureKind, TestItem as ItemRecord,
    TestPayload, TestResult, TrapRecord,
};
use vibra_workspace::{
    AuditTrace, Discovery, FailureKind, MemoryBudget, PROJECT_FILE_NAME, SourceOrigin,
    SuiteResult, TestAssertionFailure, TestOutcome, TestSelector, WorkspaceCapture,
    observation_type, run_tests,
};

use crate::args::Invocation;
use crate::refusal;
use crate::render::{Response, reach, source_text, status_for};

/// The memory budget each test runs under: the memory limit of the published
/// limit profile.
pub const MEMORY_BUDGET: MemoryBudget = MemoryBudget::DEFAULT;

/// The name `limits.reached` gives the memory limit.
pub const MEMORY_LIMIT: &str = "memory";

/// The response to a well-formed `test` line.
#[must_use]
pub fn run(invocation: &Invocation) -> Response {
    let format = invocation.globals.format;
    let name = invocation.line.name();
    let selector = match invocation.operands.first() {
        None => None,
        Some(operand) => match operand.to_str().map(TestSelector::parse) {
            Some(Ok(selector)) => Some(selector),
            Some(Err(invalid)) => return refusal(format, name, &invalid),
            None => {
                return refusal(
                    format,
                    name,
                    &format!("`{}` is not a test selector", operand.to_string_lossy()),
                );
            }
        },
    };
    let capture =
        WorkspaceCapture::capture(&invocation.globals.workspace, Discovery::Search);
    let mut response =
        Response::new(format, name.clone(), CommandOutcome::Ok, Payload::Empty {});
    response.workspace_revision = capture.revision().map(str::to_owned);
    if let Some(project) = capture.project() {
        response.sources.insert(
            PROJECT_FILE_NAME.to_owned(),
            String::from_utf8_lossy(project.bytes()).into_owned(),
        );
    }
    let Some(snapshot) = capture.snapshot() else {
        finish(
            &mut response,
            SuiteResult::Diagnostics,
            Vec::new(),
            Vec::new(),
            capture.diagnostics(),
        );
        reach(&mut response, capture.limits_reached());
        return response;
    };
    for module in snapshot.modules() {
        response.sources.insert(
            module.source_id().to_owned(),
            String::from_utf8_lossy(module.bytes()).into_owned(),
        );
    }
    let run = match run_tests(snapshot, selector.as_ref(), MEMORY_BUDGET) {
        Ok(run) => run,
        Err(invalid) => return refusal(format, name, &invalid),
    };
    let mut items = run
        .items()
        .iter()
        .map(|item| (item.selector().to_string(), item.outcome().clone()))
        .collect::<Vec<_>>();
    let traces = run
        .items()
        .iter()
        .map(|item| audit_events(item.trace(), &response))
        .collect::<Vec<_>>();
    let mut result = run.result();
    inject(&mut items, &mut result);
    finish(&mut response, result, items, traces, run.diagnostics());
    reach(&mut response, capture.limits_reached());
    response
}

/// The command result of a suite result.
const fn command_outcome(result: SuiteResult) -> CommandOutcome {
    match result {
        SuiteResult::Ok => CommandOutcome::Ok,
        SuiteResult::Diagnostics => CommandOutcome::Diagnostics,
        SuiteResult::TestFailed => CommandOutcome::TestFailed,
        SuiteResult::OperationalFailure => CommandOutcome::OperationalFailure,
        SuiteResult::Unavailable => CommandOutcome::Unavailable,
        SuiteResult::Trap => CommandOutcome::Trap,
    }
}

/// The `@test.*` result of an outcome.
const fn test_result(outcome: &TestOutcome) -> TestResult {
    match outcome {
        TestOutcome::Passed => TestResult::Passed,
        TestOutcome::AssertionFailed(_) => TestResult::AssertionFailed,
        TestOutcome::Trap { .. } => TestResult::Trap,
        TestOutcome::Invalid(_) => TestResult::Invalid,
        TestOutcome::Unavailable(_) => TestResult::Unavailable,
        TestOutcome::Exhausted => TestResult::Exhausted,
    }
}

/// The status an item reports: the status of the command result its ending
/// corresponds to.
const fn item_status(outcome: &TestOutcome) -> Status {
    status_for(match outcome {
        TestOutcome::Passed => CommandOutcome::Ok,
        TestOutcome::AssertionFailed(_) => CommandOutcome::TestFailed,
        TestOutcome::Trap { .. } => CommandOutcome::Trap,
        TestOutcome::Invalid(_) => CommandOutcome::Diagnostics,
        TestOutcome::Unavailable(_) => CommandOutcome::Unavailable,
        TestOutcome::Exhausted => CommandOutcome::OperationalFailure,
    })
}

/// The diagnostic of how one test ended, when its ending has one.
fn ending_diagnostic(outcome: &TestOutcome) -> Option<Diagnostic> {
    match outcome {
        TestOutcome::Trap { trap, origin } => {
            let message = "a defect trap ended the test";
            Some(match origin {
                Some(origin) => Diagnostic::new(trap.code(), origin.span(), message)
                    .with_source_id(origin.source_id()),
                None => Diagnostic::new(trap.code(), ByteSpan::empty_at(0), message),
            })
        }
        TestOutcome::Exhausted => Some(Diagnostic::new(
            DiagnosticCode::RuntimeMemoryExhausted,
            ByteSpan::empty_at(0),
            format!(
                "the memory budget of {} bytes is exhausted",
                MEMORY_BUDGET.bytes
            ),
        )),
        TestOutcome::Unavailable(diagnostic)
        | TestOutcome::Invalid(Some(diagnostic)) => Some(diagnostic.clone()),
        TestOutcome::AssertionFailed(failure) => match &failure.kind {
            FailureKind::Observation { code, .. } => Some(
                Diagnostic::new(
                    *code,
                    failure.origin.span(),
                    format!("an operand of `{}` cannot be observed", failure.assertion),
                )
                .with_source_id(failure.origin.source_id()),
            ),
            FailureKind::Assertion => None,
        },
        TestOutcome::Passed | TestOutcome::Invalid(None) => None,
    }
}

/// The order of the envelope: source, primary start and end, registry order.
fn sort_key(diagnostic: &Diagnostic) -> (Option<&str>, usize, usize, DiagnosticCode) {
    let span = diagnostic.primary_span();
    (
        diagnostic.source_id(),
        span.start(),
        span.end(),
        diagnostic.code(),
    )
}

fn count(items: usize) -> u64 {
    u64::try_from(items).unwrap_or(u64::MAX)
}

/// Fills `response` with the suite result `result`, the payload of `items`,
/// and every diagnostic once.
fn finish(
    response: &mut Response,
    result: SuiteResult,
    items: Vec<(String, TestOutcome)>,
    traces: Vec<Vec<AuditEvent>>,
    diagnostics: &[Diagnostic],
) {
    let mut envelope = diagnostics.to_vec();
    let mut attributed = Vec::with_capacity(items.len());
    for (_, outcome) in &items {
        let ending = ending_diagnostic(outcome);
        if let Some(diagnostic) = &ending
            && !envelope.contains(diagnostic)
        {
            envelope.push(diagnostic.clone());
        }
        attributed.push(match outcome {
            TestOutcome::Invalid(None) => diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.is_error())
                .cloned()
                .collect(),
            _ => ending.into_iter().collect::<Vec<_>>(),
        });
    }
    envelope.sort_by(|left, right| sort_key(left).cmp(&sort_key(right)));
    let exhausted = items
        .iter()
        .any(|(_, outcome)| matches!(outcome, TestOutcome::Exhausted));
    let tests = items
        .into_iter()
        .zip(attributed)
        .zip(traces)
        .map(|(((name, outcome), attributed), audit_trace)| ItemRecord {
            name,
            result: test_result(&outcome),
            failure: match &outcome {
                TestOutcome::AssertionFailed(failure) => {
                    Some(failure_record(failure, response))
                }
                _ => None,
            },
            trap: match &outcome {
                TestOutcome::Trap { trap, origin } => Some(TrapRecord {
                    trap_code: trap.code().as_atom().to_owned(),
                    origin: origin
                        .as_ref()
                        .map(|origin| span_document(origin, response)),
                }),
                _ => None,
            },
            audit_trace,
            diagnostics: envelope
                .iter()
                .filter(|diagnostic| attributed.contains(diagnostic))
                .map(|diagnostic| diagnostic.code().as_atom().to_owned())
                .collect(),
            status: item_status(&outcome),
        })
        .collect::<Vec<_>>();
    let selected = tests.len();
    let passed = tests
        .iter()
        .filter(|item| item.result == TestResult::Passed)
        .count();
    response.result = command_outcome(result);
    response.status = status_for(response.result);
    if exhausted {
        response.limits.reached.push(MEMORY_LIMIT.to_owned());
        response.limits.complete = false;
    }
    response.diagnostics = envelope;
    response.payload = Payload::Test(TestPayload {
        selected: count(selected),
        passed: count(passed),
        failed: count(selected - passed),
        tests,
    });
}

/// The `SpanDocument` of a source origin, against the captured text of its
/// source.
fn span_document(origin: &SourceOrigin, response: &Response) -> SpanDocument {
    let source_id = origin.source_id();
    let text = source_text(response, source_id);
    SpanDocument::render(origin.span(), &LineIndex::new(text), Some(source_id))
}

/// The JSON projection of the events of `trace`, a known origin as its
/// `SpanDocument`. A test's trace holds only `@host` events, which every
/// projection covers.
fn audit_events(trace: &AuditTrace, response: &Response) -> Vec<AuditEvent> {
    let span = |origin: &SourceOrigin| {
        serde_json::to_value(span_document(origin, response)).unwrap_or(Json::Null)
    };
    trace
        .events
        .iter()
        .filter_map(|event| event.json(&span))
        .map(AuditEvent)
        .collect()
}
/// The `TestItem.failure` of a failed assertion.
fn failure_record(failure: &TestAssertionFailure, response: &Response) -> TestFailure {
    let observation = matches!(failure.kind, FailureKind::Observation { .. });
    TestFailure {
        kind: if observation {
            TestFailureKind::Observation
        } else {
            TestFailureKind::Assertion
        },
        assertion: Some(failure.assertion.clone()),
        expected: failure.expected.clone(),
        actual: failure.actual.clone(),
        primary_span: span_document(&failure.origin, response),
        observation_error: observation.then(|| ObservationError {
            code: ObservationErrorCode::UnsupportedValue,
            value_type: observation_type(failure),
            origin: None,
        }),
    }
}

#[cfg(test)]
thread_local! {
    /// The fault-injection hook of the trap test: the first selected test of
    /// a checked selection ends in `@runtime.invalidCheckedProgram`.
    static TRAP_FIRST: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The calling thread's test hook, to set on the thread a command runs on
/// ([`crate::run`] runs every command on its own thread).
#[cfg(test)]
pub(crate) fn carry_hook() -> impl FnOnce() + Send {
    let trap_first = TRAP_FIRST.get();
    move || TRAP_FIRST.set(trap_first)
}

/// Applies the test hook to a run's items and suite result.
#[cfg(test)]
fn inject(items: &mut [(String, TestOutcome)], result: &mut SuiteResult) {
    if !TRAP_FIRST.get() || *result == SuiteResult::Diagnostics {
        return;
    }
    if let Some((_, first)) = items.first_mut() {
        *first = TestOutcome::Trap {
            trap: vibra_workspace::Trap::InvalidCheckedProgram,
            origin: None,
        };
        *result = SuiteResult::by_precedence(items.iter().map(|(_, outcome)| outcome));
    }
}

/// Outside the crate's own tests no fault is injected.
#[cfg(not(test))]
fn inject(_items: &mut [(String, TestOutcome)], _result: &mut SuiteResult) {}

#[cfg(test)]
mod tests;
