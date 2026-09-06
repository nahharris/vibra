//! Static-v1 source-graph conformance adapter.

use std::fmt::Write;

use vibra_workspace::WorkspaceSnapshot;
use vibra_workspace::project::{DependencyKind, TargetKind};
use vibra_workspace::source_graph::{DependencySource, DependencyStatus, SourceGraph};

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
        let graph = workspace
            .source_graph()
            .map_err(|error| HandlerError::new(error.to_string()))?;
        let diagnostics = graph.diagnostics().to_vec();
        Ok(CaseObservation {
            accepted: diagnostics.is_empty(),
            diagnostics,
            graph: Some(canonical_graph(&graph)),
            ..CaseObservation::default()
        })
    }
}

fn canonical_graph(graph: &SourceGraph) -> String {
    let mut output = String::new();
    for unit in graph.units() {
        let kind = match unit.kind() {
            TargetKind::Bin => "bin",
            TargetKind::Lib => "lib",
        };
        let _ = writeln!(output, "unit @{} kind {kind}", unit.name());
        for module in unit.modules() {
            let _ = writeln!(
                output,
                "module {} source {} bytes {}",
                module.id().as_atom(),
                module.source_id(),
                hex_bytes(module.bytes())
            );
        }
    }
    for dependency in graph.dependencies() {
        let source = match dependency.source() {
            DependencySource::Path(path) => format!("path:{path}"),
            DependencySource::Git { url, rev } => format!("git:{url}@{rev}"),
        };
        let target = dependency.target().unwrap_or("-");
        let status = match dependency.status() {
            DependencyStatus::Unsupported => "unsupported",
        };
        let kind = match dependency.kind() {
            DependencyKind::Path => "path",
            DependencyKind::Git => "git",
        };
        let _ = writeln!(
            output,
            "dependency @{} kind {kind} source {source} target {target} status {status}",
            dependency.alias()
        );
    }
    output
}

fn hex_bytes(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        let _ = write!(output, "{byte:02x}");
    }
    output
}
