//! The `workspaceCheck` operation under `staticV2`.
//!
//! `docs/spec/07-diagnostics-and-conformance.md`, "Case operations and
//! inputs": a `workspaceCheck` case has one confined tree with its
//! `project.vibon` and observes the diagnostics of the check of every local
//! target and the `@tests` unit, and the `@types.v2` document. A case whose
//! only errors are `@tool.unavailable` is `unsupported`; any other error
//! refutes it.

use vibra_workspace::{SourceSnapshot, check_snapshot, discover_project_at};

use crate::corpus::Case;
use crate::interpret::check_verdict;
use crate::manifest::Verdict;
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
        let tree = case
            .tree_directory()
            .map_err(|error| HandlerError::new(error.to_string()))?
            .ok_or_else(|| {
                HandlerError::new("a workspaceCheck case declares one tree")
            })?;
        let snapshot =
            match discover_project_at(&tree).and_then(SourceSnapshot::capture) {
                Ok(snapshot) => snapshot,
                Err(diagnostics) => {
                    let mut observation = CaseObservation::new(Verdict::Refuted);
                    observation.diagnostics = diagnostics;
                    return Ok(observation);
                }
            };
        let check = check_snapshot(&snapshot, None);
        let mut observation = CaseObservation::new(check_verdict(
            check.has_errors(),
            check.only_unavailable(),
        ));
        observation.diagnostics = check.diagnostics().to_vec();
        observation.types = check.types_document().map(str::to_owned);
        Ok(observation)
    }
}
