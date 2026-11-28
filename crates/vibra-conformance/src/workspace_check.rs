//! The `workspaceCheck` operation under `staticV2`.
//!
//! `docs/spec/07-diagnostics-and-conformance.md`, "Case operations and
//! inputs": a `workspaceCheck` case has one confined tree with its
//! `project.vibon` and observes the diagnostics of the check of every local
//! target and the `@tests` unit, and the `@types.v2` document. A case whose
//! only errors are `@tool.unavailable` is `unsupported`; one with a bare
//! guard that is not established or a reached tool limit is `unknown`; any
//! other error refutes it. A project that is not discovered or captured is
//! judged by the same rule over the diagnostics that stopped it, so a project
//! file past the source nesting limit is `unknown`.

use vibra_workspace::check_snapshot;

use crate::corpus::Case;
use crate::interpret::check_verdict;
use crate::profile::{
    CaseObservation, ConformanceOperation, HandlerError, ProfileHandler,
};

/// The `workspaceCheck` handler: captures the declared tree as a project
/// root, searching no ancestor, and checks it.
#[derive(Clone, Copy, Debug, Default)]
pub struct WorkspaceCheckHandler;

impl ProfileHandler for WorkspaceCheckHandler {
    fn can_run(&self, case: &Case) -> bool {
        case.manifest().operation == ConformanceOperation::WorkspaceCheck
    }

    fn run(&self, case: &Case) -> Result<CaseObservation, HandlerError> {
        let snapshot = match case.snapshot()? {
            Ok(snapshot) => snapshot,
            Err(diagnostics) => {
                let mut observation = CaseObservation::new(check_verdict(&diagnostics));
                observation.diagnostics = diagnostics;
                return Ok(observation);
            }
        };
        let check = check_snapshot(&snapshot, None);
        let mut observation = CaseObservation::new(check_verdict(check.diagnostics()));
        observation.diagnostics = check.diagnostics().to_vec();
        observation.types = check.types_document().map(str::to_owned);
        Ok(observation)
    }
}
