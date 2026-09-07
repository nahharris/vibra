//! The real `reader-v1` corpus adapter used by the internal entrypoint.

use vibra_diagnostics::LineIndex;
use vibra_fmt::format_document;
use vibra_schema::SourcePositionQueryDocument;
use vibra_syntax::{DocumentMode, parse_data, parse_source};

use crate::corpus::Case;
use crate::manifest::ConformanceOperation;
use crate::runner::{CaseObservation, HandlerError, ProfileHandler, QueryObservation};

/// A syntax/formatter handler for the `reader-v1` conformance profile.
///
/// This is deliberately an internal library adapter, not the user-facing
/// `vibra` command. It reads each case's declared source path, selects the
/// grammar from that path's extension, and reports the actual parser and
/// formatter observations to the backend-independent runner.
#[derive(Clone, Copy, Debug, Default)]
pub struct ReaderV1Handler;

impl ProfileHandler for ReaderV1Handler {
    fn can_run(&self, case: &Case) -> bool {
        case.manifest().operation == ConformanceOperation::Reader
    }

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
        let attach_source_ids = paths.len() > 1;

        let mut accepted = true;
        let mut diagnostics = Vec::new();
        let mut formatted = None;
        let mut query_results = Vec::new();
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
            diagnostics.extend(document.diagnostics().iter().cloned().map(
                |diagnostic| {
                    if attach_source_ids {
                        diagnostic.with_source_id(relative.as_str())
                    } else {
                        diagnostic
                    }
                },
            ));
            for (query_order, expected) in case
                .manifest()
                .expectations
                .queries
                .iter()
                .enumerate()
                .filter(|(_, expected)| expected.input == *relative)
            {
                let query = document
                    .query_position(expected.offset)
                    .map_err(|error| HandlerError::new(error.to_string()))?;
                let index = LineIndex::new(&source);
                let rendered = SourcePositionQueryDocument::render_with_source(
                    &query,
                    &index,
                    Some(relative.as_str()),
                );
                let result = serde_json::to_string_pretty(&rendered)
                    .map(|json| format!("{json}\n"))
                    .map_err(|error| HandlerError::new(error.to_string()))?;
                query_results.push((
                    query_order,
                    QueryObservation {
                        input: expected.input.clone(),
                        offset: expected.offset,
                        result,
                    },
                ));
            }
            // Run the formatter through every declared loader. This matters
            // for parity cases: a data input must exercise the data formatter
            // even when the manifest snapshot belongs to the source input.
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
            if format_relative.is_some_and(|candidate| candidate == relative) {
                formatted = Some(output);
            }
        }

        query_results.sort_by_key(|(order, _)| *order);
        let queries = query_results.into_iter().map(|(_, query)| query).collect();

        Ok(CaseObservation {
            accepted,
            diagnostics,
            formatted,
            queries,
            ..CaseObservation::default()
        })
    }
}
