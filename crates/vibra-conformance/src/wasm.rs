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

use std::collections::BTreeSet;
use std::sync::OnceLock;

use vibra_diagnostics::DiagnosticCode;
use vibra_host::{HostWorld, WorldProvider};
use vibra_ir::CheckedProgram;
use vibra_wasm::NotLowered;
use vibra_wasm::layout::{CellClass, STRIDE_CELLS, cells_offset, header, state};
use vibra_wasm_run::{Instance, Observed, Outcome, Runner, TestRun};
use vibra_workspace::semantic::{TestItem, TestItemStatus};

use crate::test_run::{RowFailure, RowOrigin, RowTrap, TestRow};

use crate::runner::{
    BackendObservation, ExecutionObservation, WasmObservation, wasm_memory_limit,
};

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
pub fn observe(program: &CheckedProgram, world: &HostWorld) -> WasmObservation {
    observe_entry(program, world).outcome
}

/// [`observe`], and whether the entry returned `err` when the run completed:
/// the module's result slot, read by the entry's result type, is classified
/// by its variant, as the interpreter classifies its own.
///
/// A run that completes is also held to the arena's balance
/// ([`assert_balanced`]); a run that does not balance fails the backend.
pub(crate) fn observe_entry(
    program: &CheckedProgram,
    world: &HostWorld,
) -> BackendObservation<WasmObservation> {
    let mut program_failed = None;
    let module = match vibra_wasm::emit(program) {
        Ok(module) => module,
        Err(not_lowered) => {
            return BackendObservation::new(not_lowered_observation(&not_lowered));
        }
    };
    let runner = match runner() {
        Ok(runner) => runner,
        Err(reason) => {
            return BackendObservation::new(WasmObservation::Failed { reason });
        }
    };
    // The entry's result type says how the host reads the result slot.
    let Some(entry) = program.functions().get(program.entry_index()) else {
        return BackendObservation::new(WasmObservation::Failed {
            reason: "the program's entry is not one of its functions".to_owned(),
        });
    };
    let result = entry.signature().result();
    let heads = |head: u32| module.descriptors().type_of(head).cloned();
    // The entry runs twice on the instance, and each run must balance: the
    // second run starts from the module values the first one left.
    let mut balanced_twice = |instance: &mut Instance| {
        assert_balanced(instance)?;
        let Ok(Outcome::Completed { result: slot, .. }) = instance.call_entry() else {
            return Ok(());
        };
        instance
            .observe_with(slot, &result, program.types(), &heads)
            .map_err(|stop| {
                format!("the second run's result is unreadable: {stop:?}")
            })?;
        assert_balanced(instance)
    };
    let outcome = match runner.run_observed_inspected(
        module.bytes(),
        &result,
        program.types(),
        &heads,
        Box::new(WorldProvider::new(world)),
        &mut balanced_twice,
    ) {
        Ok(Observed::Completed {
            value, audit_trace, ..
        }) => {
            program_failed = Some(value.is_err_variant());
            WasmObservation::Completed(ExecutionObservation {
                result: Some(value.canonical_observation(&result)),
                audit_trace,
            })
        }
        // A trap while the host reads the entry's result, such as a function
        // behind `any`, is the program's trap and not a backend failure.
        Ok(Observed::Stopped(Outcome::Trapped { code, .. })) => {
            WasmObservation::Trapped(code.diagnostic_code().as_atom().to_owned())
        }
        Ok(Observed::Stopped(outcome)) => stopped(&outcome),
        Err(error) => WasmObservation::Failed {
            reason: error.to_string(),
        },
    };
    BackendObservation {
        outcome,
        program_failed,
    }
}

/// What the emitter named when it could not lower a program, as the runner
/// counts it.
fn not_lowered_observation(not_lowered: &NotLowered) -> WasmObservation {
    WasmObservation::NotLowered {
        forms: not_lowered
            .forms()
            .iter()
            .map(|used| match used.detail() {
                Some(detail) => format!("{}:{detail}", used.form().name()),
                None => used.form().name().to_owned(),
            })
            .collect(),
    }
}

/// The arena's balance after a run that completed and whose result the host
/// released: the live size is exactly the bytes the module values reach, the
/// table's own block included, which an instance keeps for its life. Anything
/// else is an allocation no reference accounts for, or a block released twice.
/// A run that did not complete has no balance to check, since its activations
/// and the values they hold are abandoned with the instance.
fn assert_balanced(instance: &mut Instance) -> Result<(), String> {
    let live = instance
        .live_size()
        .map_err(|stop| format!("the live size is unreadable: {stop:?}"))?;
    let kept = module_values_bytes(instance)?;
    if live == kept {
        Ok(())
    } else {
        Err(format!(
            "the arena is not balanced: {live} bytes are live after the run, but the module values keep {kept}"
        ))
    }
}

/// The bytes of every block the instance's module-value table reaches, the
/// table's own block included.
fn module_values_bytes(instance: &mut Instance) -> Result<u64, String> {
    let table = word(instance, state::MODULE_VALUES)?;
    if table == 0 {
        return Ok(0);
    }
    let mut seen = BTreeSet::new();
    let mut pending = vec![table];
    let mut bytes = 0_u64;
    while let Some(block) = pending.pop() {
        if !seen.insert(block) {
            continue;
        }
        bytes += u64::from(word(instance, block + header::BLOCK_SIZE)?);
        let stride = (word(instance, block + header::KIND)? >> 8) & 0xFF;
        if stride != STRIDE_CELLS {
            continue;
        }
        let len = word(instance, block + header::LEN)?;
        let classes = read(instance, block + header::SIZE, len as usize)?;
        let cells = block + cells_offset(len);
        for (position, class) in (0_u32..).zip(classes) {
            if class == CellClass::Ref.code() {
                pending.push(word(instance, cells + 8 * position)?);
            }
        }
    }
    Ok(bytes)
}

/// The little-endian word at `address` of the instance's memory.
fn word(instance: &mut Instance, address: u32) -> Result<u32, String> {
    let bytes = read(instance, address, 4)?;
    <[u8; 4]>::try_from(bytes.as_slice())
        .map(u32::from_le_bytes)
        .map_err(|_| format!("a short read at {address}"))
}

fn read(
    instance: &mut Instance,
    address: u32,
    length: usize,
) -> Result<Vec<u8>, String> {
    instance
        .read_memory_for_tests(address, length)
        .map_err(|stop| format!("the memory at {address} is unreadable: {stop:?}"))
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
pub fn observe_tests(items: &[TestItem], world: &HostWorld) -> TestsObserved {
    let ran = items
        .iter()
        .enumerate()
        .filter_map(|(position, item)| {
            item.program().map(|program| (position, program))
        })
        .collect::<Vec<_>>();
    observe_ran(items.iter().map(TestRow::from_item).collect(), &ran, world)
}

/// Runs, in the Wasm backend, the tests of a suite that the interpreter ended
/// with a host event: the programs it ran up to and including the test that
/// ended it. The suite has no rows then, so only the backend's end is reported,
/// and a suite that completes in Wasm is reported as completed.
pub(crate) fn observe_stopped_tests(
    programs: &[CheckedProgram],
    world: &HostWorld,
) -> WasmObservation {
    let ran = programs.iter().enumerate().collect::<Vec<_>>();
    let rows = programs
        .iter()
        .map(|_| TestRow {
            name: String::new(),
            status: TestItemStatus::Passed.as_atom().to_owned(),
            failure: None,
            trap: None,
            audit_trace: Vec::new(),
        })
        .collect();
    match observe_ran(rows, &ran, world) {
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
    world: &HostWorld,
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
            return TestsObserved::Stopped(not_lowered_observation(&not_lowered));
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
        let run = runner.run_test_with_host(
            module.bytes(),
            index,
            program.types(),
            &heads,
            &operand_of,
            Box::new(WorldProvider::new(world)),
            &mut assert_balanced,
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
            Ok(TestRun::Passed { audit_trace, .. }) => TestRow {
                status: TestItemStatus::Passed.as_atom().to_owned(),
                failure: None,
                trap: None,
                audit_trace,
                ..base.clone()
            },
            Ok(TestRun::Trapped { code, origin }) => TestRow {
                status: TestItemStatus::Trap.as_atom().to_owned(),
                failure: None,
                trap: Some(RowTrap {
                    code: code.diagnostic_code().as_atom().to_owned(),
                    origin: origin_of(origin),
                }),
                audit_trace: Vec::new(),
                ..base.clone()
            },
            Ok(TestRun::AssertionFailed {
                failure,
                origin,
                expected,
                actual,
                audit_trace,
            }) => {
                let Some(origin) = origin_of(origin) else {
                    return TestsObserved::Stopped(WasmObservation::Failed {
                        reason: "a failed assertion has no origin".to_owned(),
                    });
                };
                TestRow {
                    status: TestItemStatus::AssertionFailed.as_atom().to_owned(),
                    failure: Some(RowFailure {
                        assertion: failure.symbol(),
                        expected,
                        actual,
                        origin,
                    }),
                    trap: None,
                    audit_trace,
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
    use vibra_ir::TestAssertion;
    use vibra_ir::boundary::TrapCode;

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

    /// A started instance of `source` that has run its entry, and the slot of
    /// the result the host still holds.
    fn ran(source: &str) -> (Instance, vibra_wasm_run::ResultSlot, CheckedProgram) {
        let checked = vibra_types::check_source("input.vib", source);
        let program = checked.program().expect("an accepted program").clone();
        let module = vibra_wasm::emit(&program).expect("the program lowers");
        let vibra_wasm_run::Started::Ready(mut instance) = runner()
            .expect("the engine configures")
            .start(module.bytes())
            .expect("a v1 module")
        else {
            panic!("the module's own memory exceeds the limit");
        };
        let Outcome::Completed { result, .. } = instance.call_entry().expect("runs")
        else {
            panic!("the entry did not complete");
        };
        (instance, result, program)
    }

    #[test]
    fn a_result_the_host_keeps_is_an_unbalanced_arena() {
        let (mut instance, slot, program) = ran("(defn main () str \"abc\")");
        let error = assert_balanced(&mut instance).expect_err("the result is held");
        assert!(error.starts_with("the arena is not balanced"), "{error}");
        // Releasing what the host held restores the balance.
        let ty = program.entry().signature().result();
        instance
            .observe(slot, &ty, program.types())
            .expect("the host reads the result");
        assert_balanced(&mut instance).expect("nothing is held");
    }

    #[test]
    fn module_values_are_kept_and_balanced() {
        let (mut instance, slot, program) = ran("(defn main () i32 1i32)");
        let ty = program.entry().signature().result();
        instance
            .observe(slot, &ty, program.types())
            .expect("the host reads the result");
        assert!(module_values_bytes(&mut instance).expect("readable") > 0);
        assert_balanced(&mut instance).expect("the module values are all that is live");
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
                failure: TestAssertion::True,
                origin: None,
                operands: None
            }),
            WasmObservation::Failed { .. }
        ));
    }
}
