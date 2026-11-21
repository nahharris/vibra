//! The `resolve` operation under `staticV2`.
//!
//! `docs/spec/07-diagnostics-and-conformance.md`, "Case operations and
//! inputs": a `resolve` case has one confined tree with its `project.vibon`
//! and observes the `@resolved.v2` snapshot and the diagnostics of
//! resolution. A case with an error is refuted; warnings leave it accepted.

use vibra_resolve::resolve;
use vibra_workspace::{SourceSnapshot, discover_project_at, resolve_input};

use crate::corpus::Case;
use crate::manifest::Verdict;
use crate::profile::{
    CaseObservation, ConformanceOperation, HandlerError, ProfileHandler,
};

/// The `resolve` handler: captures the declared tree as a project root,
/// searching no ancestor, and resolves every module.
#[derive(Clone, Copy, Debug, Default)]
pub struct ResolveHandler;

impl ProfileHandler for ResolveHandler {
    fn can_run(&self, case: &Case) -> bool {
        case.manifest().operation == ConformanceOperation::Resolve
    }

    fn run(&self, case: &Case) -> Result<CaseObservation, HandlerError> {
        let tree = case
            .tree_directory()
            .map_err(|error| HandlerError::new(error.to_string()))?
            .ok_or_else(|| HandlerError::new("a resolve case declares one tree"))?;
        let snapshot =
            match discover_project_at(&tree).and_then(SourceSnapshot::capture) {
                Ok(snapshot) => snapshot,
                Err(diagnostics) => {
                    let mut observation = CaseObservation::new(Verdict::Refuted);
                    observation.diagnostics = diagnostics;
                    return Ok(observation);
                }
            };
        let input = resolve_input(&snapshot)
            .map_err(|error| HandlerError::new(error.to_string()))?;
        let resolution = resolve(&input);
        let verdict = if resolution.has_errors() {
            Verdict::Refuted
        } else {
            Verdict::Accepted
        };
        let mut observation = CaseObservation::new(verdict);
        observation.resolved = Some(resolution.canonical_text());
        observation.diagnostics = resolution.diagnostics().to_vec();
        Ok(observation)
    }
}
