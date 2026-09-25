//! Static-v1 project-schema conformance adapter.

use vibra_fmt::format_document;
use vibra_syntax::parse_data;
use vibra_workspace::project::{ProjectDecoder, ProjectOrigin};

use crate::corpus::Case;
use crate::manifest::ConformanceOperation;
use crate::runner::{CaseObservation, HandlerError, ProfileHandler};

/// Runs the pure M2 project decoder without a filesystem or source graph.
///
/// The handler deliberately accepts only a declared project input. It reads
/// the case fixture, lets the extension-selected data loader validate the
/// generic VIBON grammar, then passes the resulting tree and case-relative
/// source identity to `vibra-workspace`. No path is opened by the decoder.
#[derive(Clone, Copy, Debug, Default)]
pub struct StaticV1ProjectHandler;

impl ProfileHandler for StaticV1ProjectHandler {
    fn can_run(&self, case: &Case) -> bool {
        case.manifest().operation == ConformanceOperation::ProjectDecode
    }

    fn run(&self, case: &Case) -> Result<CaseObservation, HandlerError> {
        let relative = case.manifest().inputs.project.as_ref().ok_or_else(|| {
            HandlerError::new("static-v1 project case needs a project input")
        })?;
        let source = case
            .read_file(relative)
            .map_err(|error| HandlerError::new(error.to_string()))?;
        let path = case
            .file(relative)
            .map_err(|error| HandlerError::new(error.to_string()))?;
        let document = parse_data(&path, &source)
            .map_err(|error| HandlerError::new(error.to_string()))?;
        let mut diagnostics = document
            .diagnostics()
            .iter()
            .cloned()
            .map(|diagnostic| diagnostic.with_source_id(relative.as_str()))
            .collect::<Vec<_>>();
        let mut accepted = document.accepted();
        let mut formatted = document.data().map(|_| format_document(&document));

        if let Some(root) = document.data() {
            let decoded = ProjectDecoder::decode(root, ProjectOrigin::new(relative));
            accepted &= decoded.accepted();
            diagnostics.extend(decoded.diagnostics().iter().cloned());
            if let Some(project) = decoded.project() {
                formatted = Some(project.canonical_vibon());
            }
        } else {
            accepted = false;
        }

        if let Some(output) = &formatted {
            let reparsed = parse_data(&path, output)
                .map_err(|error| HandlerError::new(error.to_string()))?;
            if !reparsed.accepted() {
                return Err(HandlerError::new(
                    "project formatter output is not accepted by the data loader",
                ));
            }
            if format_document(&reparsed) != *output {
                return Err(HandlerError::new(
                    "project formatter output is not idempotent",
                ));
            }
        }

        Ok(CaseObservation {
            accepted,
            diagnostics,
            formatted,
            ..CaseObservation::default()
        })
    }
}
