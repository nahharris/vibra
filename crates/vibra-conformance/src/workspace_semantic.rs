//! Static and interpreter handlers for immutable Step 12 workspace snapshots.

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

/// Discovers, statically checks, and executes tests in one confined snapshot.
#[derive(Clone, Copy, Debug, Default)]
pub struct InterpreterV1WorkspaceTestHandler;

impl ProfileHandler for InterpreterV1WorkspaceTestHandler {
    fn can_run(&self, case: &Case) -> bool {
        case.manifest().operation == ConformanceOperation::WorkspaceTest
    }

    fn run(&self, case: &Case) -> Result<CaseObservation, HandlerError> {
        let snapshot = match load_workspace(case)? {
            WorkspaceLoad::Snapshot(snapshot) => snapshot,
            WorkspaceLoad::Diagnostics(diagnostics) => {
                return Ok(CaseObservation {
                    accepted: false,
                    diagnostics,
                    interpreter: Some(ExecutionObservation {
                        result: Some(format_test_run("@command.diagnostics", &[])),
                        audit_trace: Vec::new(),
                    }),
                    ..CaseObservation::default()
                });
            }
        };
        let verification = verified_bootstrap_if_used(&snapshot)?;
        let result = vibra_workspace::semantic::run_tests(
            &snapshot,
            None,
            verification.as_ref(),
        );
        let (command_result, accepted) = match result.status() {
            vibra_workspace::semantic::TestSuiteStatus::Ok => ("@command.ok", true),
            vibra_workspace::semantic::TestSuiteStatus::Diagnostics => {
                ("@command.diagnostics", false)
            }
            vibra_workspace::semantic::TestSuiteStatus::TestFailed => {
                ("@command.test-failed", true)
            }
            vibra_workspace::semantic::TestSuiteStatus::InvalidInput => {
                return Err(HandlerError::new(
                    "workspace-test without a selector produced invalid input",
                ));
            }
            vibra_workspace::semantic::TestSuiteStatus::Unavailable => {
                ("@command.unavailable", false)
            }
            vibra_workspace::semantic::TestSuiteStatus::Trap => ("@command.trap", true),
        };
        Ok(CaseObservation {
            accepted,
            diagnostics: result.diagnostics().to_vec(),
            interpreter: Some(ExecutionObservation {
                result: Some(format_test_run(command_result, result.items())),
                audit_trace: Vec::new(),
            }),
            ..CaseObservation::default()
        })
    }
}

fn format_test_run(
    command_result: &str,
    items: &[vibra_workspace::semantic::TestItem],
) -> String {
    use std::fmt::Write as _;

    let mut output = format!(
        "(record\n  format: @test-run.v1\n  result: {command_result}\n  tests: (array"
    );
    if items.is_empty() {
        output.push_str(")\n)\n");
        return output;
    }
    output.push('\n');
    for item in items {
        let _ = write!(
            output,
            "    (record name: {} result: {}",
            vibon_string(item.name()),
            item.status().as_atom()
        );
        if let Some(failure) = item.failure() {
            let span = failure.primary_span();
            let _ = write!(
                output,
                " failure: (record assertion: {} expected: {} actual: {} primary-span: (record source-id: {} start: {}u64 end: {}u64))",
                failure.assertion(),
                vibon_string(failure.expected()),
                vibon_string(failure.actual()),
                vibon_string(failure.source_id()),
                span.start(),
                span.end()
            );
        }
        if let Some(trap) = item.trap() {
            let _ = write!(
                output,
                " trap: (record trap-code: {}",
                vibon_string(trap.trap_code())
            );
            if let Some(origin) = trap.origin() {
                let span = origin.span();
                let _ = write!(
                    output,
                    " origin: (record source-id: {} start: {}u64 end: {}u64)",
                    vibon_string(origin.source_id()),
                    span.start(),
                    span.end()
                );
            }
            output.push(')');
        }
        output.push_str(
            " audit-trace: (record format: @audit-trace.v1 events: (array)))\n",
        );
    }
    output.push_str("  )\n)\n");
    output
}

fn vibon_string(value: &str) -> String {
    vibra_ir::Value::Str(value.to_owned()).canonical_vibon()
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
    verify_bootstrap().map(Some).map_err(|error| {
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
