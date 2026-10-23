//! Static and interpreter handlers for immutable Step 12 workspace snapshots.

use vibra_diagnostics::Diagnostic;
use vibra_host::HostWorld;
use vibra_types::{Stdlib, load_stdlib};
use vibra_workspace::{WorkspaceSnapshot, project::TargetKind};

use crate::corpus::{Case, HostWorldError};
use crate::manifest::ConformanceOperation;
use crate::runner::{
    CaseObservation, ExecutionObservation, HandlerError, ProfileHandler,
    WasmObservation,
};
use crate::test_run::{TestRow, format_test_run};

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
        let verification = verified_bootstrap()?;
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
        // The case's host world is validated before anything runs: a malformed
        // document fails this case with its data diagnostics. The providers that
        // consume a fresh copy for each backend arrive with the first host
        // operation.
        if let Err(diagnostics) = load_host_world(case)? {
            return Ok(CaseObservation {
                accepted: false,
                diagnostics,
                ..CaseObservation::default()
            });
        }
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
        let verification = verified_bootstrap()?;
        let result = vibra_workspace::semantic::run_target_with_budget(
            &snapshot,
            target,
            verification.as_ref(),
            crate::runner::interpreter_budget(),
        );
        let accepted =
            result.check().status() == vibra_workspace::semantic::CheckStatus::Accepted;
        let mut host_event = None;
        // Both backends run the one checked program of the target.
        let wasm = if accepted {
            result
                .check()
                .program_for_target(target)
                .map(crate::wasm::observe)
        } else {
            None
        };
        let interpreter = if accepted {
            match result.outcome() {
                Some(vibra_workspace::semantic::RunOutcome::Program(execution)) => {
                    Some(ExecutionObservation {
                        result: Some(execution.canonical_result()),
                        audit_trace: execution.audit_trace().to_vec(),
                    })
                }
                // A host event ends the run with no result and no trace.
                Some(vibra_workspace::semantic::RunOutcome::InterpreterFailure(
                    error,
                )) if error.is_host_event() => {
                    host_event = crate::types::host_event_atom(error);
                    None
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
            host_event,
            wasm,
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
        if let Err(diagnostics) = load_host_world(case)? {
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
        let verification = verified_bootstrap()?;
        let result = vibra_workspace::semantic::run_tests_with_budget(
            &snapshot,
            None,
            verification.as_ref(),
            crate::runner::interpreter_budget(),
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
            // A host event ends the run with no result and no trace; the
            // event is the diagnostic's code. The Wasm backend runs the same
            // tests and must end with the same event.
            vibra_workspace::semantic::TestSuiteStatus::OperationalFailure => {
                let Some(event) = result
                    .diagnostics()
                    .first()
                    .map(|diagnostic| diagnostic.code().as_atom().to_owned())
                else {
                    return Err(HandlerError::new(
                        "workspace-test stopped with no host event",
                    ));
                };
                return Ok(CaseObservation {
                    accepted: true,
                    host_event: Some(event),
                    wasm: Some(crate::wasm::observe_stopped_tests(
                        result.stopped_programs(),
                    )),
                    ..CaseObservation::default()
                });
            }
        };
        Ok(CaseObservation {
            accepted,
            diagnostics: result.diagnostics().to_vec(),
            interpreter: Some(ExecutionObservation {
                result: Some(format_test_run(
                    command_result,
                    &result
                        .items()
                        .iter()
                        .map(TestRow::from_item)
                        .collect::<Vec<_>>(),
                )),
                audit_trace: Vec::new(),
            }),
            // Both backends run the tests of the one workspace.
            wasm: accepted.then(|| match crate::wasm::observe_tests(result.items()) {
                crate::wasm::TestsObserved::Rows(rows) => {
                    WasmObservation::Completed(ExecutionObservation {
                        result: Some(format_test_run(
                            crate::test_run::command_result(&rows),
                            &rows,
                        )),
                        audit_trace: Vec::new(),
                    })
                }
                crate::wasm::TestsObserved::Stopped(observation) => observation,
            }),
            ..CaseObservation::default()
        })
    }
}

/// Decodes the case's host world. A world that does not decode is the data
/// diagnostics of its violation, which fail the case; a world that cannot be
/// read is a harness failure.
fn load_host_world(
    case: &Case,
) -> Result<Result<HostWorld, Vec<Diagnostic>>, HandlerError> {
    match case.host_world() {
        Ok(world) => Ok(Ok(world)),
        Err(HostWorldError::Invalid(diagnostics)) => Ok(Err(diagnostics)),
        Err(HostWorldError::Corpus(error)) => Err(HandlerError::new(error.to_string())),
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

/// The standard library every workspace is checked against: its vocabulary
/// module is in every checked graph.
fn verified_bootstrap() -> Result<Option<Stdlib>, HandlerError> {
    load_stdlib().map(Some).map_err(|error| {
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
