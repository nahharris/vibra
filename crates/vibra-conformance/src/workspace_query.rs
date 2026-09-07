//! Tooling-v1 semantic workspace-query conformance adapter.

use vibra_diagnostics::LineIndex;
use vibra_schema::WorkspacePositionQueryDocument;
use vibra_workspace::WorkspaceSnapshot;

use crate::corpus::Case;
use crate::manifest::ConformanceOperation;
use crate::runner::{CaseObservation, HandlerError, ProfileHandler, QueryObservation};

/// Renders semantic source-position facts from one confined workspace.
///
/// The handler acquires the declared tree once, then all query results come
/// from that immutable snapshot. Query inputs stay case-relative in the
/// manifest (`tree/src/main.vib`), while the workspace uses the captured
/// project-relative source identity (`src/main.vib`).
#[derive(Clone, Copy, Debug, Default)]
pub struct ToolingV1QueryHandler;

impl ProfileHandler for ToolingV1QueryHandler {
    fn can_run(&self, case: &Case) -> bool {
        case.manifest().operation == ConformanceOperation::Query
    }

    fn run(&self, case: &Case) -> Result<CaseObservation, HandlerError> {
        // Walk the tree at the corpus boundary before handing it to the
        // workspace. This validates the case's confined-file contract and
        // prevents a handler from silently ignoring declared files.
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

        let tree_name = case.manifest().inputs.tree.as_deref().ok_or_else(|| {
            HandlerError::new("query case has no confined tree input")
        })?;
        let mut queries = Vec::new();
        for expected in &case.manifest().expectations.queries {
            let source_id = source_id_from_case_input(tree_name, &expected.input)
                .ok_or_else(|| {
                    HandlerError::new(format!(
                        "query input `{}` is outside the declared tree",
                        expected.input
                    ))
                })?;
            let source = workspace
                .source()
                .documents()
                .find(|document| document.source_id() == source_id)
                .ok_or_else(|| {
                    HandlerError::new(format!(
                        "query source `{source_id}` is not in the workspace snapshot"
                    ))
                })?;
            let text = std::str::from_utf8(source.bytes()).map_err(|_| {
                HandlerError::new(format!("query source `{source_id}` is not UTF-8"))
            })?;
            let query = workspace
                .query_position(source_id, expected.offset)
                .map_err(|error| HandlerError::new(error.to_string()))?;
            let index = LineIndex::new(text);
            let rendered =
                WorkspacePositionQueryDocument::render_with_source(&query, &index);
            let result = serde_json::to_string_pretty(&rendered)
                .map(|json| format!("{json}\n"))
                .map_err(|error| HandlerError::new(error.to_string()))?;
            queries.push(QueryObservation {
                input: expected.input.clone(),
                offset: expected.offset,
                result,
            });
        }

        Ok(CaseObservation {
            accepted: true,
            queries,
            ..CaseObservation::default()
        })
    }
}

fn source_id_from_case_input<'a>(tree: &str, input: &'a str) -> Option<&'a str> {
    input.strip_prefix(&format!("{tree}/"))
}
