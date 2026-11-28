//! The `resolve` operation under `staticV2`.
//!
//! `docs/spec/07-diagnostics-and-conformance.md`, "Case operations and
//! inputs": a `resolve` case has one confined tree with its `project.vibon`
//! and observes the `@resolved.v2` snapshot and the diagnostics of
//! resolution. A case with an error is refuted; warnings leave it accepted.

use vibra_resolve::resolve;
use vibra_workspace::resolve_input;

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
        let snapshot = match case.snapshot()? {
            Ok(snapshot) => snapshot,
            Err(diagnostics) => return Ok(CaseObservation::refuted(diagnostics)),
        };
        let input = resolve_input(&snapshot)?;
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
