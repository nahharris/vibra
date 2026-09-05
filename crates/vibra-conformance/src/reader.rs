//! The real `reader-v1` corpus adapter used by the internal entrypoint.

use vibra_fmt::format_document;
use vibra_syntax::{DocumentMode, parse_data, parse_source};

use crate::corpus::Case;
use crate::runner::{CaseObservation, HandlerError, ProfileHandler};

/// A syntax/formatter handler for the `reader-v1` conformance profile.
///
/// This is deliberately an internal library adapter, not the user-facing
/// `vibra` command. It reads each case's declared source path, selects the
/// grammar from that path's extension, and reports the actual parser and
/// formatter observations to the backend-independent runner.
#[derive(Clone, Copy, Debug, Default)]
pub struct ReaderV1Handler;

impl ProfileHandler for ReaderV1Handler {
    fn run(&self, case: &Case) -> Result<CaseObservation, HandlerError> {
        let inputs = &case.manifest().inputs;
        let mut paths = Vec::new();
        if let Some(source) = &inputs.source {
            paths.push((source, DocumentMode::Source));
        }
        if let Some(project) = &inputs.project {
            paths.push((project, DocumentMode::Data));
        }
        paths.extend(inputs.data.iter().map(|data| (data, DocumentMode::Data)));
        if paths.is_empty() {
            return Err(HandlerError::new(
                "reader-v1 case does not declare an input document",
            ));
        }

        let mut accepted = true;
        let mut diagnostics = Vec::new();
        let mut formatted = None;
        let format_relative = inputs
            .source
            .as_ref()
            .or(inputs.project.as_ref())
            .or_else(|| inputs.data.first());
        for (relative, mode) in paths {
            let source = case
                .read_file(relative)
                .map_err(|error| HandlerError::new(error.to_string()))?;
            let path = case
                .file(relative)
                .map_err(|error| HandlerError::new(error.to_string()))?;
            let document = match mode {
                DocumentMode::Source => parse_source(&path, &source),
                DocumentMode::Data => parse_data(&path, &source),
            }
            .map_err(|error| HandlerError::new(error.to_string()))?;
            accepted &= document.accepted();
            diagnostics.extend_from_slice(document.diagnostics());
            if format_relative.is_some_and(|candidate| candidate == relative) {
                let output = format_document(&document);
                // Prove canonical output is stable through the same loader
                // selected for the manifest role.
                let reparsed = match mode {
                    DocumentMode::Source => parse_source(&path, &output),
                    DocumentMode::Data => parse_data(&path, &output),
                }
                .map_err(|error| HandlerError::new(error.to_string()))?;
                let reformatted = format_document(&reparsed);
                if reformatted != output {
                    return Err(HandlerError::new(
                        "formatter output is not idempotent for the selected loader",
                    ));
                }
                formatted = Some(output);
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
