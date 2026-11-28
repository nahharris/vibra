//! The `workspaceTest` operation under `interpreterV2`.
//!
//! `docs/spec/07-diagnostics-and-conformance.md`, "Case operations and
//! inputs": a `workspaceTest` case has one confined tree with its
//! `project.vibon` and observes one suite result, the `@testRun.v2`
//! document. The verdict is the status of the suite's `@command.*` result:
//! `ok` is accepted, `diagnostics` and `testFailed` are refuted,
//! `unavailable` is unsupported, and `operationalFailure` and `trap` are
//! unknown. A test that exhausts its memory budget ends with the host event
//! `@runtime.memoryExhausted`, which the case expects with
//! `expect.hostEvent`; its item is `@test.exhausted` in the observation. A
//! test whose `TestWorld.directory` fixture conflicts is `@test.invalid`,
//! and its `@test.conflictingFixture` joins the observed diagnostics.

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_interp::MemoryBudget;
use vibra_workspace::{SuiteResult, TestOutcome, run_tests_in};

use crate::corpus::Case;
use crate::manifest::Verdict;
use crate::profile::{
    CaseObservation, ConformanceOperation, HandlerError, ProfileHandler,
};

/// The `workspaceTest` handler: captures the declared tree as a project
/// root, searching no ancestor, and runs every test in a fresh copy of the
/// case's `hostWorld`, or of the default world when it declares none.
#[derive(Clone, Copy, Debug, Default)]
pub struct WorkspaceTestHandler;

impl ProfileHandler for WorkspaceTestHandler {
    fn can_run(&self, case: &Case) -> bool {
        case.manifest().operation == ConformanceOperation::WorkspaceTest
    }

    fn run(&self, case: &Case) -> Result<CaseObservation, HandlerError> {
        let snapshot = match case.snapshot()? {
            Ok(snapshot) => snapshot,
            Err(diagnostics) => return Ok(CaseObservation::refuted(diagnostics)),
        };
        let run =
            run_tests_in(&snapshot, None, &case.host_world()?, MemoryBudget::DEFAULT)?;
        let mut observation = CaseObservation::new(verdict(run.result()));
        observation.diagnostics = run.diagnostics().to_vec();
        observation
            .diagnostics
            .extend(run.items().iter().filter_map(|item| match item.outcome() {
                TestOutcome::Invalid(Some(diagnostic)) => Some(diagnostic.clone()),
                _ => None,
            }));
        observation.test_run = Some(run.document());
        if run
            .items()
            .iter()
            .any(|item| item.outcome() == &TestOutcome::Exhausted)
        {
            observation.host_event = Some(DiagnosticCode::RuntimeMemoryExhausted);
            observation.diagnostics.push(Diagnostic::new(
                DiagnosticCode::RuntimeMemoryExhausted,
                ByteSpan::empty_at(0),
                "a test exhausted its memory budget",
            ));
        }
        Ok(observation)
    }
}

/// The status of a suite result.
const fn verdict(result: SuiteResult) -> Verdict {
    match result {
        SuiteResult::Ok => Verdict::Accepted,
        SuiteResult::Diagnostics | SuiteResult::TestFailed => Verdict::Refuted,
        SuiteResult::Unavailable => Verdict::Unsupported,
        SuiteResult::OperationalFailure | SuiteResult::Trap => Verdict::Unknown,
    }
}
