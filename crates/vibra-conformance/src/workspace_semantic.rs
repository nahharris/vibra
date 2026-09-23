//! Static and interpreter handlers for immutable Step 12 workspace snapshots.

use std::path::Path;

use vibra_diagnostics::Diagnostic;
use vibra_types::{BootstrapVerification, verify_bootstrap};
use vibra_workspace::{WorkspaceSnapshot, project::TargetKind};

use crate::corpus::Case;
use crate::manifest::ConformanceOperation;
use crate::runner::{
    CaseObservation, ExecutionObservation, HandlerError, ProfileHandler,
};

const BOOTSTRAP_PROVENANCE_FAILURE: &str = "bootstrap provenance verification failed";

/// Checks one declared project tree with the workspace semantic service.
#[derive(Clone, Copy, Debug, Default)]
pub struct StaticV1WorkspaceCheckHandler;

impl ProfileHandler for StaticV1WorkspaceCheckHandler {
    fn can_run(&self, case: &Case) -> bool {
        case.manifest().operation == ConformanceOperation::WorkspaceCheck
    }

    fn run(&self, case: &Case) -> Result<CaseObservation, HandlerError> {
        let snapshot = match load_workspace(case)? {
            WorkspaceLoad::Snapshot(snapshot) => snapshot,
            WorkspaceLoad::Diagnostics(diagnostics) => {
                return Ok(CaseObservation {
                    accepted: false,
                    diagnostics,
                    ..CaseObservation::default()
                });
            }
        };
        let verification = verified_bootstrap_if_used(&snapshot)?;
        let checked = match verification.as_ref() {
            Some(verification) => vibra_workspace::semantic::check_all_with_bootstrap(
                &snapshot,
                Some(verification),
            ),
            None => vibra_workspace::semantic::check_all(&snapshot),
        };
        Ok(CaseObservation {
            accepted: checked.status()
                == vibra_workspace::semantic::CheckStatus::Accepted,
            diagnostics: checked.diagnostics().to_vec(),
            ..CaseObservation::default()
        })
    }
}

/// Checks and interprets the unique binary target in one declared tree.
#[derive(Clone, Copy, Debug, Default)]
pub struct InterpreterV1WorkspaceRunHandler;

impl ProfileHandler for InterpreterV1WorkspaceRunHandler {
    fn can_run(&self, case: &Case) -> bool {
        case.manifest().operation == ConformanceOperation::WorkspaceRun
    }

    fn run(&self, case: &Case) -> Result<CaseObservation, HandlerError> {
        let snapshot = match load_workspace(case)? {
            WorkspaceLoad::Snapshot(snapshot) => snapshot,
            WorkspaceLoad::Diagnostics(diagnostics) => {
                return Ok(CaseObservation {
                    accepted: false,
                    diagnostics,
                    ..CaseObservation::default()
                });
            }
        };
        let target = unique_binary_target(&snapshot)?;
        let verification = verified_bootstrap_if_used(&snapshot)?;
        let result = match verification.as_ref() {
            Some(verification) => vibra_workspace::semantic::run_target_with_bootstrap(
                &snapshot,
                target,
                Some(verification),
            ),
            None => vibra_workspace::semantic::run_target(&snapshot, target),
        };
        let accepted =
            result.check().status() == vibra_workspace::semantic::CheckStatus::Accepted;
        let interpreter = if accepted {
            match result.outcome() {
                Some(vibra_workspace::semantic::RunOutcome::Program(execution)) => {
                    Some(ExecutionObservation {
                        result: Some(execution.canonical_result()),
                        audit_trace: execution.audit_trace().to_vec(),
                    })
                }
                Some(vibra_workspace::semantic::RunOutcome::InterpreterFailure(
                    error,
                )) => {
                    return Err(HandlerError::new(format!(
                        "checked-program interpreter invariant failed: {error}"
                    )));
                }
                None => {
                    return Err(HandlerError::new(
                        "accepted workspace run produced no interpreter result",
                    ));
                }
            }
        } else {
            None
        };
        Ok(CaseObservation {
            accepted,
            diagnostics: result.check().diagnostics().to_vec(),
            interpreter,
            ..CaseObservation::default()
        })
    }
}

enum WorkspaceLoad {
    Snapshot(Box<WorkspaceSnapshot>),
    Diagnostics(Vec<Diagnostic>),
}

fn load_workspace(case: &Case) -> Result<WorkspaceLoad, HandlerError> {
    case.input_documents()
        .map_err(|error| HandlerError::new(error.to_string()))?;
    case.tree_files()
        .map_err(|error| HandlerError::new(error.to_string()))?;
    let tree = case
        .tree_path()
        .map_err(|error| HandlerError::new(error.to_string()))?;
    match WorkspaceSnapshot::load_confined(tree) {
        Ok(snapshot) => Ok(WorkspaceLoad::Snapshot(Box::new(snapshot))),
        Err(error) if !error.diagnostics().is_empty() => {
            Ok(WorkspaceLoad::Diagnostics(error.diagnostics().to_vec()))
        }
        Err(error) => Err(HandlerError::new(error.to_string())),
    }
}

fn verified_bootstrap_if_used(
    snapshot: &WorkspaceSnapshot,
) -> Result<Option<BootstrapVerification>, HandlerError> {
    let requires_verification = snapshot
        .requires_bootstrap_verification()
        .map_err(|error| HandlerError::new(error.to_string()))?;
    if !requires_verification {
        return Ok(None);
    }
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    verify_bootstrap(repository).map(Some).map_err(|error| {
        HandlerError::new(format!("{BOOTSTRAP_PROVENANCE_FAILURE}: {error}"))
    })
}

fn unique_binary_target(
    snapshot: &WorkspaceSnapshot,
) -> Result<&vibra_workspace::project::Target, HandlerError> {
    let mut targets = snapshot
        .project()
        .project()
        .targets()
        .iter()
        .filter(|target| target.kind() == TargetKind::Bin);
    let Some(target) = targets.next() else {
        return Err(HandlerError::new(
            "workspace-run requires exactly one binary target, found none",
        ));
    };
    if targets.next().is_some() {
        return Err(HandlerError::new(
            "workspace-run requires exactly one binary target, found multiple",
        ));
    }
    Ok(target)
}
