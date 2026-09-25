//! Snapshot-backed formatter conformance adapter.

use std::path::{Path, PathBuf};

use vibra_workspace::WorkspaceSnapshot;
use vibra_workspace::format_plan::plan_format;

use crate::corpus::Case;
use crate::manifest::ConformanceOperation;
use crate::runner::{CaseObservation, HandlerError, ProfileHandler};

/// Formats one source input from its confined workspace snapshot.
#[derive(Clone, Copy, Debug, Default)]
pub struct ToolingV1FormatHandler;

impl ProfileHandler for ToolingV1FormatHandler {
    fn can_run(&self, case: &Case) -> bool {
        case.manifest().operation == ConformanceOperation::Format
    }

    fn run(&self, case: &Case) -> Result<CaseObservation, HandlerError> {
        let manifest = case.manifest();
        let tree = manifest.inputs.tree.as_deref().ok_or_else(|| {
            HandlerError::new("format case has no confined tree input")
        })?;
        let source = manifest
            .inputs
            .source
            .as_deref()
            .ok_or_else(|| HandlerError::new("format case has no source input"))?;
        let input_documents = case
            .input_documents()
            .map_err(|error| HandlerError::new(error.to_string()))?;
        case.tree_files()
            .map_err(|error| HandlerError::new(error.to_string()))?;

        let relative = Path::new(source).strip_prefix(tree).map_err(|_| {
            HandlerError::new("format source is outside its declared workspace tree")
        })?;
        let relative = relative.to_str().ok_or_else(|| {
            HandlerError::new("format source path is not valid UTF-8")
        })?;
        let root = case
            .tree_path()
            .map_err(|error| HandlerError::new(error.to_string()))?;
        let workspace = WorkspaceSnapshot::load_confined(&root)
            .map_err(|error| HandlerError::new(error.to_string()))?;
        let plan = plan_format(&root, PathBuf::from(relative))
            .map_err(|error| HandlerError::new(error.to_string()))?;
        let declared_source = input_documents
            .iter()
            .find(|document| document.source_id == source)
            .ok_or_else(|| HandlerError::new("format source input was not acquired"))?;
        if declared_source.text != plan.original_text() {
            return Err(HandlerError::new(
                "format tree source differs from the declared immutable source input",
            ));
        }
        if !workspace
            .source()
            .documents()
            .any(|document| document.source_id() == plan.relative_path())
        {
            return Err(HandlerError::new(
                "format target is absent from its captured workspace source snapshot",
            ));
        }
        let diagnostics = plan.diagnostics().to_vec();
        let accepted = !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.level() == vibra_diagnostics::Level::Error);
        Ok(CaseObservation {
            accepted,
            diagnostics,
            formatted: Some(plan.formatted_text().to_owned()),
            ..CaseObservation::default()
        })
    }
}
