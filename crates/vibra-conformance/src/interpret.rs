//! The `interpret` operation under `interpreterV2`.
//!
//! `docs/spec/07-diagnostics-and-conformance.md`, "Case operations and
//! inputs": an `interpret` case has one source, checked as the only module
//! of a one-module library workspace (`docs/spec/05-tooling.md`, "One source
//! alone"). An accepted source runs from its first function without
//! parameters in the reference interpreter, and the case observes the
//! `@value.v2` document of the result. A run that ends with a host event
//! establishes no result: its verdict is `unknown`, and it observes the
//! event and its diagnostic.

use vibra_interp::MemoryBudget;
use vibra_types::interpret;

use crate::corpus::Case;
use crate::manifest::Verdict;
use crate::profile::{
    CaseObservation, ConformanceOperation, HandlerError, ProfileHandler,
};

/// The memory budget every `interpret` case runs under.
pub const INTERPRET_BUDGET: MemoryBudget = MemoryBudget::DEFAULT;

/// The verdict of a check: `unsupported` when every error is
/// `@tool.unavailable`, `refuted` for any other error, `accepted` otherwise.
pub(crate) const fn check_verdict(has_errors: bool, only_unavailable: bool) -> Verdict {
    if only_unavailable {
        Verdict::Unsupported
    } else if has_errors {
        Verdict::Refuted
    } else {
        Verdict::Accepted
    }
}

/// The `interpret` handler: checks the case's source and runs it.
#[derive(Clone, Copy, Debug, Default)]
pub struct InterpretHandler;

impl ProfileHandler for InterpretHandler {
    fn can_run(&self, case: &Case) -> bool {
        case.manifest().operation == ConformanceOperation::Interpret
    }

    fn run(&self, case: &Case) -> Result<CaseObservation, HandlerError> {
        let source_id = case.manifest().inputs.source.as_deref().ok_or_else(|| {
            HandlerError::new("an interpret case declares one source")
        })?;
        let source = case
            .read_file(source_id)
            .map_err(|error| HandlerError::new(error.to_string()))?;
        let outcome = interpret(source_id, &source, INTERPRET_BUDGET);
        let check = &outcome.check;
        let mut observation = CaseObservation::new(check_verdict(
            check.has_errors(),
            check.only_unavailable(),
        ));
        observation.diagnostics = check.diagnostics().to_vec();
        match outcome.run {
            None => {}
            Some(Ok(execution)) => observation.result = Some(execution.canonical),
            Some(Err(error)) if error.is_host_event() => {
                observation.verdict = Verdict::Unknown;
                observation.host_event = Some(error.code());
                observation.diagnostics.push(error.diagnostic());
            }
            Some(Err(error)) => {
                return Err(HandlerError::new(format!(
                    "the run ended with {}, which this handler does not observe yet",
                    error.code().as_atom()
                )));
            }
        }
        Ok(observation)
    }
}
