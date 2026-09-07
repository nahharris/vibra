//! Static-v1 declaration-resolution conformance adapter.

use vibra_resolve::{
    ReferencePath, ResolveInput, Resolver, SourceModule, SourceUnit,
    TargetKind as ResolveTargetKind,
};
use vibra_workspace::WorkspaceSnapshot;
use vibra_workspace::project::TargetKind;

use crate::corpus::Case;
use crate::manifest::ConformanceOperation;
use crate::runner::{CaseObservation, HandlerError, ProfileHandler};

/// Adapts the confined Step 3 workspace snapshot to the filesystem-free
/// resolver input and returns its canonical resolved artifact.
#[derive(Clone, Copy, Debug, Default)]
pub struct StaticV1ResolveHandler;

impl ProfileHandler for StaticV1ResolveHandler {
    fn can_run(&self, case: &Case) -> bool {
        case.manifest().operation == ConformanceOperation::Resolve
    }

    fn run(&self, case: &Case) -> Result<CaseObservation, HandlerError> {
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
        let project = workspace.project().project();
        let units = graph
            .units()
            .iter()
            .map(|unit| {
                let target = project
                    .targets()
                    .iter()
                    .find(|target| target.name().atom().value() == unit.name());
                let entry = target.and_then(|target| {
                    target.entry().map(|entry| {
                        ReferencePath::new(
                            entry.atom().segments().iter().cloned(),
                            entry.origin().source_id().to_owned(),
                            entry.span(),
                        )
                    })
                });
                let kind = match unit.kind() {
                    TargetKind::Bin => ResolveTargetKind::Bin,
                    TargetKind::Lib => ResolveTargetKind::Lib,
                };
                let modules = unit
                    .modules()
                    .iter()
                    .map(|module| {
                        SourceModule::new(
                            module.id().unit(),
                            module.id().segments().iter().cloned(),
                            module.source_id(),
                            module.bytes(),
                        )
                    })
                    .collect();
                SourceUnit::new(unit.name(), kind, entry, modules)
            })
            .collect::<Vec<_>>();
        let package = project.package();
        let input =
            ResolveInput::new(package.name().value(), package.version().value(), units);
        let resolved = Resolver::resolve(input);
        let mut diagnostics = graph.diagnostics().to_vec();
        diagnostics.extend(resolved.diagnostics().iter().cloned());
        let accepted = diagnostics.is_empty() && resolved.accepted();
        Ok(CaseObservation {
            accepted,
            diagnostics,
            resolved: Some(resolved.canonical_vibon()),
            ..CaseObservation::default()
        })
    }
}
