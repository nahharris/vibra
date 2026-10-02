//! Tooling-v1 `@index.v1` conformance adapter.

use vibra_workspace::WorkspaceSnapshot;

use crate::corpus::Case;
use crate::manifest::ConformanceOperation;
use crate::runner::{CaseObservation, HandlerError, ProfileHandler};

/// Renders the `@index.v1` document of one confined workspace
/// (`docs/spec/05-tooling.md`, "Index records").
///
/// The handler acquires the declared tree once and indexes that immutable
/// snapshot with the embedded standard library. The observation is accepted
/// when the workspace check is; the document is rendered either way, since an
/// unavailable or recovered declaration keeps its record.
#[derive(Clone, Copy, Debug, Default)]
pub struct ToolingV1IndexHandler;

impl ProfileHandler for ToolingV1IndexHandler {
    fn can_run(&self, case: &Case) -> bool {
        case.manifest().operation == ConformanceOperation::Index
    }

    fn run(&self, case: &Case) -> Result<CaseObservation, HandlerError> {
        case.tree_files()
            .map_err(|error| HandlerError::new(error.to_string()))?;
        let tree = case
            .tree_path()
            .map_err(|error| HandlerError::new(error.to_string()))?;
        let workspace = match WorkspaceSnapshot::load_confined(tree) {
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
        let stdlib = vibra_types::load_stdlib()
            .map_err(|error| HandlerError::new(error.to_string()))?;
        let checked = vibra_workspace::semantic::check_all_with_bootstrap(
            &workspace,
            Some(&stdlib),
        );
        let document = vibra_workspace::index::index(&workspace, Some(&stdlib))
            .map_err(|error| HandlerError::new(error.to_string()))?;
        Ok(CaseObservation {
            accepted: checked.status()
                == vibra_workspace::semantic::CheckStatus::Accepted,
            diagnostics: checked.diagnostics().to_vec(),
            index: Some(document.canonical_vibon()),
            ..CaseObservation::default()
        })
    }
}
