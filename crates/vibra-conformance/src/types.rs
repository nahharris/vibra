//! The interpreter conformance adapter for one source document.

use crate::corpus::Case;
use crate::manifest::ConformanceOperation;
use crate::runner::{
    BackendObservation, CaseObservation, ExecutionObservation, HandlerError,
    ProfileHandler,
};
use vibra_types::check_source;

/// The atom of the host event an interpreter failure is, such as
/// `@runtime.memory-exhausted`.
pub(crate) fn host_event_atom(error: &vibra_interp::RuntimeError) -> Option<String> {
    error
        .host_diagnostic()
        .map(|diagnostic| diagnostic.code().as_atom().to_owned())
}

/// Checks and executes one source document through the M2 reference
/// interpreter.  The interpreter receives only the checked IR returned by
/// `vibra-types`.
#[derive(Clone, Copy, Debug, Default)]
pub struct InterpreterV1Handler;

impl ProfileHandler for InterpreterV1Handler {
    fn can_run(&self, case: &Case) -> bool {
        case.manifest().operation == ConformanceOperation::Interpret
    }

    fn run(&self, case: &Case) -> Result<CaseObservation, HandlerError> {
        let source_id =
            case.manifest().inputs.source.as_deref().ok_or_else(|| {
                HandlerError::new("interpret case needs a source input")
            })?;
        let source = case
            .read_file(source_id)
            .map_err(|error| HandlerError::new(error.to_string()))?;
        let checked = check_source(source_id, &source);
        let Some(program) = checked.program() else {
            return Ok(CaseObservation {
                accepted: false,
                diagnostics: checked.diagnostics().to_vec(),
                ..CaseObservation::default()
            });
        };
        let execution = match vibra_interp::Interpreter::run_with_budget(
            program,
            crate::runner::interpreter_budget(),
        ) {
            Ok(execution) => execution,
            // A host event ends the run with no result and no trace, so the
            // case records the event instead of snapshots.
            Err(error) if error.is_host_event() => {
                return Ok(CaseObservation {
                    accepted: checked.accepted(),
                    diagnostics: checked.diagnostics().to_vec(),
                    host_event: host_event_atom(&error),
                    wasm: Some(crate::wasm::observe_entry(
                        program,
                        &vibra_host::HostWorld::default(),
                    )),
                    native: Some(crate::native::observe_entry(
                        program,
                        &vibra_host::HostWorld::default(),
                    )),
                    entry: crate::native::entry_job(case, program, false),
                    ..CaseObservation::default()
                });
            }
            Err(error) => return Err(HandlerError::new(error.to_string())),
        };
        Ok(CaseObservation {
            accepted: checked.accepted(),
            diagnostics: checked.diagnostics().to_vec(),
            interpreter: Some(BackendObservation {
                outcome: ExecutionObservation {
                    result: Some(execution.canonical_result()),
                    audit_trace: execution.audit_trace().to_vec(),
                },
                program_failed: Some(execution.program_failed()),
            }),
            wasm: Some(crate::wasm::observe_entry(
                program,
                &vibra_host::HostWorld::default(),
            )),
            native: Some(crate::native::observe_entry(
                program,
                &vibra_host::HostWorld::default(),
            )),
            entry: crate::native::entry_job(case, program, false),
            ..CaseObservation::default()
        })
    }
}
