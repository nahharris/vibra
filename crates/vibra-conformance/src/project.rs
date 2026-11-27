//! The `projectDecode` and `sourceGraph` operations under `staticV2`.
//!
//! `docs/spec/07-diagnostics-and-conformance.md`, "Case operations and
//! inputs": a `projectDecode` case has exactly one `project.vibon` and
//! observes the typed decode and its diagnostics, with no discovery; a
//! `sourceGraph` case has one confined tree with its `project.vibon` and
//! observes the `@sourceGraph.v1` snapshot.

use vibra_workspace::{
    PROJECT_FILE_NAME, SourceGraph, SourceSnapshot, decode_project, discover_project_at,
};

use crate::corpus::Case;
use crate::manifest::Verdict;
use crate::profile::{
    CaseObservation, ConformanceOperation, HandlerError, ProfileHandler,
};

/// The source identity of an embedded library module is its module atom,
/// such as `@std.io`, and a project module's is a path ending in `.vib`, so
/// the two never collide and a graph case names no embedded identity.
const EMBEDDED_SOURCE_IDS: &[&str] = &[];

fn refuted(diagnostics: Vec<vibra_diagnostics::Diagnostic>) -> CaseObservation {
    let mut observation = CaseObservation::new(Verdict::Refuted);
    observation.diagnostics = diagnostics;
    observation
}

/// The `projectDecode` handler: decodes the declared `project.vibon` alone.
#[derive(Clone, Copy, Debug, Default)]
pub struct ProjectDecodeHandler;

impl ProfileHandler for ProjectDecodeHandler {
    fn can_run(&self, case: &Case) -> bool {
        case.manifest().operation == ConformanceOperation::ProjectDecode
    }

    fn run(&self, case: &Case) -> Result<CaseObservation, HandlerError> {
        let inputs = &case.manifest().inputs;
        let (Some(project), None, [], None) = (
            &inputs.project,
            &inputs.source,
            inputs.data.as_slice(),
            &inputs.tree,
        ) else {
            return Err(HandlerError::new(
                "a projectDecode case declares exactly one project document",
            ));
        };
        let bytes = case
            .read_bytes(project)
            .map_err(|error| HandlerError::new(error.to_string()))?;
        Ok(match decode_project(PROJECT_FILE_NAME, &bytes) {
            Ok(_) => CaseObservation::new(Verdict::Accepted),
            Err(diagnostics) => refuted(diagnostics),
        })
    }
}

/// The `sourceGraph` handler: captures the declared tree as a project root,
/// searching no ancestor, and observes its graph.
#[derive(Clone, Copy, Debug, Default)]
pub struct SourceGraphHandler;

impl ProfileHandler for SourceGraphHandler {
    fn can_run(&self, case: &Case) -> bool {
        case.manifest().operation == ConformanceOperation::SourceGraph
    }

    fn run(&self, case: &Case) -> Result<CaseObservation, HandlerError> {
        let tree = case
            .tree_directory()
            .map_err(|error| HandlerError::new(error.to_string()))?
            .ok_or_else(|| HandlerError::new("a sourceGraph case declares one tree"))?;
        let graph = discover_project_at(&tree)
            .and_then(SourceSnapshot::capture)
            .and_then(|snapshot| SourceGraph::build(&snapshot, EMBEDDED_SOURCE_IDS));
        Ok(match graph {
            Ok(graph) => {
                let mut observation = CaseObservation::new(Verdict::Accepted);
                observation.graph = Some(graph.canonical_text());
                observation
            }
            Err(diagnostics) => refuted(diagnostics),
        })
    }
}
