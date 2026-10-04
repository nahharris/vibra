//! The WebAssembly execution handler of the differential harness.
//!
//! An executable case has one expectation. The interpreter handlers check and
//! run the case, and hand the checked program to this module, which lowers it
//! with `vibra-wasm`, runs the module with `vibra-wasm-run` under the runner's
//! memory limit, and reports what the Wasm backend observed in the same terms
//! the interpreter does. The runner then holds both observations to the one
//! expectation (`docs/spec/07-diagnostics-and-conformance.md`, "Differential
//! execution"). One front end feeds both backends, so no backend re-checks a
//! program.

use std::sync::OnceLock;

use vibra_diagnostics::DiagnosticCode;
use vibra_ir::{CheckedProgram, ObservedValue, Type, Value};
use vibra_wasm_run::{Outcome, Runner};

use crate::runner::{ExecutionObservation, WasmObservation, wasm_memory_limit};

/// The one runner every case shares: an engine is costly to build and holds no
/// state between instances.
fn runner() -> Result<&'static Runner, String> {
    static RUNNER: OnceLock<Result<Runner, String>> = OnceLock::new();
    RUNNER
        .get_or_init(|| {
            Runner::new(wasm_memory_limit()).map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(Clone::clone)
}

/// Lowers `program`, runs its entry in the Wasm backend under the runner's
/// memory limit, and reports the observation. The interpreter handlers call it
/// on each program they run; it is public so that a host test can run a
/// hand-built program through the same path.
pub fn observe(program: &CheckedProgram) -> WasmObservation {
    let module = match vibra_wasm::emit(program) {
        Ok(module) => module,
        Err(not_lowered) => {
            return WasmObservation::NotLowered {
                forms: not_lowered
                    .forms()
                    .iter()
                    .map(|used| match used.detail() {
                        Some(detail) => format!("{}:{detail}", used.form().name()),
                        None => used.form().name().to_owned(),
                    })
                    .collect(),
            };
        }
    };
    let runner = match runner() {
        Ok(runner) => runner,
        Err(reason) => return WasmObservation::Failed { reason },
    };
    match runner.run_entry(module.bytes()) {
        Ok(outcome) => observation(&outcome),
        Err(error) => WasmObservation::Failed {
            reason: error.to_string(),
        },
    }
}

/// What a finished run observed.
fn observation(outcome: &Outcome) -> WasmObservation {
    match outcome {
        // The skeleton lowers only the empty `void` entry, whose result ID is
        // `0`. Reading any other result needs the arena accessors of a later
        // step, so an ID the module does not report as `0` is a failure
        // rather than a guess.
        Outcome::Completed { result: 0, .. } => {
            WasmObservation::Completed(ExecutionObservation {
                result: Some(
                    ObservedValue::Primitive(Value::Void)
                        .canonical_observation(&Type::Void),
                ),
                // Stage 4A has no host operation, so a run has no audit event.
                audit_trace: Vec::new(),
            })
        }
        Outcome::Completed { result, .. } => WasmObservation::Failed {
            reason: format!(
                "the module reported the result ID {result} for a void entry"
            ),
        },
        Outcome::MemoryExhausted => WasmObservation::HostEvent(
            DiagnosticCode::RuntimeMemoryExhausted.as_atom().to_owned(),
        ),
        Outcome::Trapped { code, origin } => WasmObservation::Failed {
            reason: format!(
                "the module stopped with the trap {} (origin {origin:?})",
                code.diagnostic_code().as_atom()
            ),
        },
        Outcome::AssertionFailed {
            failure, origin, ..
        } => WasmObservation::Failed {
            reason: format!(
                "the module stopped with a failed assertion {failure:?} (origin {origin:?})"
            ),
        },
        Outcome::Defect { cause } => WasmObservation::Failed {
            reason: format!(
                "{}: {cause}",
                Outcome::defect_trap().diagnostic_code().as_atom()
            ),
        },
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]
mod tests {
    use vibra_ir::boundary::{Failure, TrapCode};

    use super::*;
    use crate::runner::{INSTANCE_MEMORY_LIMIT_BYTES, interpreter_budget};

    #[test]
    fn one_limit_is_applied_to_both_backends() {
        assert_eq!(INSTANCE_MEMORY_LIMIT_BYTES, 64 * 1024 * 1024);
        assert_eq!(interpreter_budget().bytes(), INSTANCE_MEMORY_LIMIT_BYTES);
        assert_eq!(wasm_memory_limit().bytes(), INSTANCE_MEMORY_LIMIT_BYTES);
        assert_eq!(
            runner().expect("the engine configures").limit().bytes(),
            INSTANCE_MEMORY_LIMIT_BYTES
        );
    }

    #[test]
    fn a_void_result_is_the_one_the_interpreter_observes() {
        let observed = observation(&Outcome::Completed {
            result: 0,
            live_size: 0,
        });
        assert_eq!(
            observed,
            WasmObservation::Completed(ExecutionObservation {
                result: Some("(record type: @void value: void)\n".to_owned()),
                audit_trace: Vec::new(),
            })
        );
    }

    #[test]
    fn memory_exhaustion_is_the_host_event_in_both_backends() {
        assert_eq!(
            observation(&Outcome::MemoryExhausted),
            WasmObservation::HostEvent("@runtime.memory-exhausted".to_owned())
        );
    }

    #[test]
    fn a_stop_with_no_recorded_status_is_invalid_checked_program() {
        let WasmObservation::Failed { reason } = observation(&Outcome::Defect {
            cause: "the call stopped with no recorded status".to_owned(),
        }) else {
            panic!("a defect fails the Wasm backend");
        };
        assert!(
            reason.starts_with("@runtime.invalid-checked-program"),
            "{reason}"
        );
    }

    #[test]
    fn traps_and_failed_assertions_fail_the_backend_with_their_codes() {
        let WasmObservation::Failed { reason } = observation(&Outcome::Trapped {
            code: TrapCode::InvalidHostValue,
            origin: Some(3),
        }) else {
            panic!("a trap fails the Wasm backend in the skeleton");
        };
        assert!(reason.contains("@runtime.invalid-host-value"), "{reason}");
        assert!(matches!(
            observation(&Outcome::AssertionFailed {
                failure: Failure::True,
                origin: None,
                operands: None
            }),
            WasmObservation::Failed { .. }
        ));
    }

    #[test]
    fn a_result_id_the_skeleton_cannot_read_is_a_failure() {
        assert!(matches!(
            observation(&Outcome::Completed {
                result: 9,
                live_size: 0
            }),
            WasmObservation::Failed { .. }
        ));
    }
}
