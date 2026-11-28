//! The `workspaceRun` operation under `interpreterV2`.
//!
//! `docs/spec/07-diagnostics-and-conformance.md`, "Case operations and
//! inputs": a `workspaceRun` case has one confined tree with its
//! `project.vibon` and observes one `@targetRun.v2` document: startup, the
//! entry's result, and the ordered audit trace, in the case's `hostWorld`
//! or the default world. The manifest names no target, so the run selects
//! the tree's one binary target. The run is always under a fake world:
//! this handler never launches on the process.
//!
//! The verdict is the status of the run's `@command.*` result
//! (`docs/spec/05-tooling.md`, the result table): `ok` is accepted,
//! `diagnostics` is refuted, `unavailable` is unsupported, and every other
//! result is unknown. A run that exhausts its memory ends with the host
//! event `@runtime.memoryExhausted`, which the case expects with
//! `expect.hostEvent`.

use vibra_interp::MemoryBudget;
use vibra_workspace::run::{CommandResult, Launch, run_target};

use vibra_diagnostics::DiagnosticCode;

use crate::corpus::Case;
use crate::manifest::Verdict;
use crate::profile::{
    CaseObservation, ConformanceOperation, HandlerError, ProfileHandler,
};

/// The `workspaceRun` handler: captures the declared tree as a project root,
/// searching no ancestor, and runs its one binary target in a fresh copy of
/// the case's `hostWorld`, or of the default world when it declares none.
#[derive(Clone, Copy, Debug, Default)]
pub struct WorkspaceRunHandler;

impl ProfileHandler for WorkspaceRunHandler {
    fn can_run(&self, case: &Case) -> bool {
        case.manifest().operation == ConformanceOperation::WorkspaceRun
    }

    fn run(&self, case: &Case) -> Result<CaseObservation, HandlerError> {
        let snapshot = match case.snapshot()? {
            Ok(snapshot) => snapshot,
            Err(diagnostics) => return Ok(CaseObservation::refuted(diagnostics)),
        };
        let world = case.host_world()?;
        let outcome = run_target(
            &snapshot,
            None,
            Launch::World(&world),
            MemoryBudget::DEFAULT,
        );
        let mut observation = CaseObservation::new(verdict(outcome.result));
        observation.target_run = Some(outcome.document());
        if outcome.limits_reached.iter().any(|limit| limit == "memory") {
            observation.host_event = Some(DiagnosticCode::RuntimeMemoryExhausted);
        }
        observation.diagnostics = outcome.diagnostics;
        Ok(observation)
    }
}

/// The status of a run's result.
const fn verdict(result: CommandResult) -> Verdict {
    match result {
        CommandResult::Ok => Verdict::Accepted,
        CommandResult::Diagnostics => Verdict::Refuted,
        CommandResult::Unavailable => Verdict::Unsupported,
        CommandResult::InvalidInput
        | CommandResult::OperationalFailure
        | CommandResult::Trap
        | CommandResult::ProgramFailed => Verdict::Unknown,
    }
}
