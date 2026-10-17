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
use vibra_ir::CheckedProgram;
use vibra_ir::boundary::Failure;
use vibra_wasm_run::{Observed, Outcome, Runner, TestRun};
use vibra_workspace::semantic::{TestItem, TestItemStatus};

use crate::test_run::{RowFailure, RowOrigin, RowTrap, TestRow};

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
    // The entry's result type says how the host reads the result slot.
    let Some(entry) = program.functions().get(program.entry_index()) else {
        return WasmObservation::Failed {
            reason: "the program's entry is not one of its functions".to_owned(),
        };
    };
    let result = entry.signature().result();
    let heads = |head: u32| module.descriptors().type_of(head).cloned();
    match runner.run_observed_with(module.bytes(), &result, program.types(), &heads) {
        Ok(Observed::Completed { value, .. }) => {
            WasmObservation::Completed(ExecutionObservation {
                result: Some(value.canonical_observation(&result)),
                // Stage 4A has no host operation, so a run has no audit event.
                audit_trace: Vec::new(),
            })
        }
        Ok(Observed::Stopped(outcome)) => stopped(&outcome),
        Err(error) => WasmObservation::Failed {
            reason: error.to_string(),
        },
    }
}

/// The Wasm backend's rows for the tests of a workspace, or what stopped the
/// backend instead.
#[derive(Debug)]
pub enum TestsObserved {
    /// Every test ran, or was never run, and has a row.
    Rows(Vec<TestRow>),
    /// The module could not be lowered, or the instance ended in a host event,
    /// or the backend failed.
    Stopped(WasmObservation),
}

/// Lowers the tests that ran in the interpreter to one test module and runs each
/// in a fresh instance through `vibra_v1_test`, at its position in canonical
/// discovery order. A test the interpreter did not run (`@test.unavailable`) has
/// the interpreter's row.
pub fn observe_tests(items: &[TestItem]) -> TestsObserved {
    let ran = items
        .iter()
        .enumerate()
        .filter_map(|(position, item)| {
            item.program().map(|program| (position, program))
        })
        .collect::<Vec<_>>();
    observe_ran(items.iter().map(TestRow::from_item).collect(), &ran)
}

/// Runs, in the Wasm backend, the tests of a suite that the interpreter ended
/// with a host event: the programs it ran up to and including the test that
/// ended it. The suite has no rows then, so only the backend's end is reported,
/// and a suite that completes in Wasm is reported as completed.
pub fn observe_stopped_tests(programs: &[CheckedProgram]) -> WasmObservation {
    let ran = programs.iter().enumerate().collect::<Vec<_>>();
    let rows = programs
        .iter()
        .map(|_| TestRow {
            name: String::new(),
            status: TestItemStatus::Passed.as_atom().to_owned(),
            failure: None,
            trap: None,
        })
        .collect();
    match observe_ran(rows, &ran) {
        TestsObserved::Rows(_) => {
            WasmObservation::Completed(ExecutionObservation::default())
        }
        TestsObserved::Stopped(observation) => observation,
    }
}

/// Runs the tests `ran`, at their positions among `rows`.
fn observe_ran(
    mut rows: Vec<TestRow>,
    ran: &[(usize, &CheckedProgram)],
) -> TestsObserved {
    let Some((_, first)) = ran.first() else {
        return TestsObserved::Rows(rows);
    };
    let entries = ran
        .iter()
        .map(|(_, program)| program.entry_index())
        .collect::<Vec<_>>();
    let module = match vibra_wasm::emit_tests(first, &entries) {
        Ok(module) => module,
        Err(not_lowered) => {
            return TestsObserved::Stopped(WasmObservation::NotLowered {
                forms: not_lowered
                    .forms()
                    .iter()
                    .map(|used| match used.detail() {
                        Some(detail) => format!("{}:{detail}", used.form().name()),
                        None => used.form().name().to_owned(),
                    })
                    .collect(),
            });
        }
    };
    let runner = match runner() {
        Ok(runner) => runner,
        Err(reason) => {
            return TestsObserved::Stopped(WasmObservation::Failed { reason });
        }
    };
    let heads = |head: u32| module.descriptors().type_of(head).cloned();
    let operand_of = |ordinal: u32| module.origins().operand_type(ordinal).cloned();
    let origin_of = |ordinal: Option<u32>| {
        ordinal
            .and_then(|ordinal| module.origins().origin(ordinal))
            .map(|origin| RowOrigin {
                source_id: origin.source_id().to_owned(),
                start: origin.span().start(),
                end: origin.span().end(),
            })
    };
    for (index, (position, program)) in (0_u32..).zip(ran) {
        let Some(base) = rows.get(*position).cloned() else {
            continue;
        };
        let run = runner.run_test(
            module.bytes(),
            index,
            program.types(),
            &heads,
            &operand_of,
        );
        let row = match run {
            Err(error) => {
                return TestsObserved::Stopped(WasmObservation::Failed {
                    reason: error.to_string(),
                });
            }
            Ok(TestRun::MemoryExhausted) => {
                return TestsObserved::Stopped(WasmObservation::HostEvent(
                    DiagnosticCode::RuntimeMemoryExhausted.as_atom().to_owned(),
                ));
            }
            Ok(TestRun::Defect { cause }) => {
                return TestsObserved::Stopped(WasmObservation::Failed {
                    reason: format!(
                        "{}: {cause}",
                        Outcome::defect_trap().diagnostic_code().as_atom()
                    ),
                });
            }
            Ok(TestRun::Passed { .. }) => TestRow {
                status: TestItemStatus::Passed.as_atom().to_owned(),
                failure: None,
                trap: None,
                ..base.clone()
            },
            Ok(TestRun::Trapped { code, origin }) => TestRow {
                status: TestItemStatus::Trap.as_atom().to_owned(),
                failure: None,
                trap: Some(RowTrap {
                    code: code.diagnostic_code().as_atom().to_owned(),
                    origin: origin_of(origin),
                }),
                ..base.clone()
            },
            Ok(TestRun::AssertionFailed {
                failure,
                origin,
                expected,
                actual,
            }) => {
                let Some(origin) = origin_of(origin) else {
                    return TestsObserved::Stopped(WasmObservation::Failed {
                        reason: "a failed assertion has no origin".to_owned(),
                    });
                };
                let member = match failure {
                    Failure::True => "true",
                    Failure::False => "false",
                    Failure::Equal => "equal",
                };
                TestRow {
                    status: TestItemStatus::AssertionFailed.as_atom().to_owned(),
                    failure: Some(RowFailure {
                        assertion: format!("@std.assert.{member}"),
                        expected,
                        actual,
                        origin,
                    }),
                    trap: None,
                    ..base.clone()
                }
            }
        };
        if let Some(slot) = rows.get_mut(*position) {
            *slot = row;
        }
    }
    TestsObserved::Rows(rows)
}

/// What a run that did not complete observed.
fn stopped(outcome: &Outcome) -> WasmObservation {
    match outcome {
        // `run_observed` reports a completed run as `Observed::Completed`.
        Outcome::Completed { .. } => WasmObservation::Failed {
            reason: "a completed run was reported as stopped".to_owned(),
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
    fn a_completed_run_is_not_a_stop() {
        assert!(matches!(
            stopped(&Outcome::Completed {
                result: vibra_wasm_run::ResultSlot::from_bits(0),
                live_size: 0
            }),
            WasmObservation::Failed { .. }
        ));
    }

    #[test]
    fn memory_exhaustion_is_the_host_event_in_both_backends() {
        assert_eq!(
            stopped(&Outcome::MemoryExhausted),
            WasmObservation::HostEvent("@runtime.memory-exhausted".to_owned())
        );
    }

    #[test]
    fn a_stop_with_no_recorded_status_is_invalid_checked_program() {
        let WasmObservation::Failed { reason } = stopped(&Outcome::Defect {
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
        let WasmObservation::Failed { reason } = stopped(&Outcome::Trapped {
            code: TrapCode::InvalidHostValue,
            origin: Some(3),
        }) else {
            panic!("a trap fails the Wasm backend in the skeleton");
        };
        assert!(reason.contains("@runtime.invalid-host-value"), "{reason}");
        assert!(matches!(
            stopped(&Outcome::AssertionFailed {
                failure: Failure::True,
                origin: None,
                operands: None
            }),
            WasmObservation::Failed { .. }
        ));
    }
}
