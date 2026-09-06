//! Static and interpreter conformance adapters for the Step 5 primitive path.

use crate::corpus::Case;
use crate::manifest::ConformanceOperation;
use crate::runner::{
    CaseObservation, ExecutionObservation, HandlerError, ProfileHandler,
};
use vibra_types::check_source;

/// Runs source type checking and returns the canonical checked-program
/// observation.
#[derive(Clone, Copy, Debug, Default)]
pub struct StaticV1TypeHandler;

impl ProfileHandler for StaticV1TypeHandler {
    fn can_run(&self, case: &Case) -> bool {
        case.manifest().operation == ConformanceOperation::TypeCheck
    }

    fn run(&self, case: &Case) -> Result<CaseObservation, HandlerError> {
        let source_id =
            case.manifest().inputs.source.as_deref().ok_or_else(|| {
                HandlerError::new("type-check case needs a source input")
            })?;
        let source = case
            .read_file(source_id)
            .map_err(|error| HandlerError::new(error.to_string()))?;
        let checked = check_source(source_id, &source);
        let accepted = checked.accepted() && checked.program().is_some();
        Ok(CaseObservation {
            accepted,
            diagnostics: checked.diagnostics().to_vec(),
            types: checked.program().map(|program| program.canonical_vibon()),
            ..CaseObservation::default()
        })
    }
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
        let execution = vibra_interp::run(program)
            .map_err(|error| HandlerError::new(error.to_string()))?;
        Ok(CaseObservation {
            accepted: checked.accepted(),
            diagnostics: checked.diagnostics().to_vec(),
            interpreter: Some(ExecutionObservation {
                result: Some(execution.canonical_result()),
                audit_trace: execution.audit_trace().to_vec(),
            }),
            ..CaseObservation::default()
        })
    }
}
