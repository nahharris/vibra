//! Static-v1 source-graph conformance adapter.

use vibra_workspace::WorkspaceSnapshot;

use crate::corpus::Case;
use crate::manifest::ConformanceOperation;
use crate::runner::{CaseObservation, HandlerError, ProfileHandler};

/// Acquires declared tree inputs and builds the confined Step 3 graph.
#[derive(Clone, Copy, Debug, Default)]
pub struct StaticV1SourceGraphHandler;

impl ProfileHandler for StaticV1SourceGraphHandler {
    fn can_run(&self, case: &Case) -> bool {
        case.manifest().operation == ConformanceOperation::SourceGraph
    }

    fn run(&self, case: &Case) -> Result<CaseObservation, HandlerError> {
        // The corpus boundary acquires every declared input before the
        // workspace engine is allowed to build the graph. This keeps source,
        // data, and tree identities explicit and proves confined loading.
        case.input_documents()
            .map_err(|error| HandlerError::new(error.to_string()))?;
        case.tree_files()
            .map_err(|error| HandlerError::new(error.to_string()))?;

        let project = case.manifest().inputs.project.as_deref().ok_or_else(|| {
            HandlerError::new("source-graph case needs a project input")
        })?;
        let project_path = case
            .file(project)
            .map_err(|error| HandlerError::new(error.to_string()))?;
        let workspace = match WorkspaceSnapshot::load(project_path) {
            Ok(workspace) => workspace,
            Err(error) => {
                let diagnostics = error.diagnostics().to_vec();
                if diagnostics.is_empty() {
                    return Err(HandlerError::new(error.to_string()));
                }
                return Ok(CaseObservation {
                    accepted: false,
                    diagnostics,
                    ..CaseObservation::default()
                });
            }
        };
        let graph = workspace.source_graph();
        let diagnostics = graph.diagnostics().to_vec();
        Ok(CaseObservation {
            accepted: diagnostics.is_empty(),
            diagnostics,
            ..CaseObservation::default()
        })
    }
}
