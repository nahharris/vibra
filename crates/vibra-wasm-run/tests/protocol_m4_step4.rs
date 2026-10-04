//! The runner's reading of what a module recorded
//! (`docs/spec/06-runtime.md`, "WebAssembly boundary" and "Traps") and its
//! memory limit (`docs/spec/07-diagnostics-and-conformance.md`,
//! "Differential execution").

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

mod common;

use common::{
    GLOBAL_ACTUAL, GLOBAL_EXPECTED, GLOBAL_FAILURE, GLOBAL_RESULT, Spec, module, record,
};
use vibra_ir::boundary::{Failure, TrapCode};
use vibra_wasm_run::{MemoryLimit, Outcome, ResultSlot, Runner, RunnerError};
use wasm_encoder::Instruction;

const PAGE: usize = 65_536;

fn runner() -> Runner {
    Runner::new(MemoryLimit::new(64 * 1024 * 1024)).expect("the engine configures")
}

fn run(spec: &Spec<'_>) -> Outcome {
    runner()
        .run_entry(&module(spec))
        .expect("a runnable module")
}

#[test]
fn a_returning_call_has_completed() {
    assert_eq!(
        run(&Spec::default()),
        Outcome::Completed {
            result: ResultSlot::from_bits(0),
            live_size: 0
        }
    );
}

#[test]
fn a_completed_entry_reports_its_result_id() {
    let spec = Spec {
        entry: vec![
            Instruction::I64Const(5),
            Instruction::GlobalSet(GLOBAL_RESULT),
        ],
        ..Spec::default()
    };
    assert_eq!(
        run(&spec),
        Outcome::Completed {
            result: ResultSlot::from_bits(5),
            live_size: 0
        }
    );
}

#[test]
fn a_recorded_trap_is_read_with_its_code_and_origin() {
    for (code, trap) in [
        (1, TrapCode::InvalidCheckedProgram),
        (2, TrapCode::UnobservableFunction),
        (3, TrapCode::InvalidHostValue),
    ] {
        let mut entry = record(1, code, 7);
        entry.push(Instruction::Unreachable);
        assert_eq!(
            run(&Spec {
                entry,
                ..Spec::default()
            }),
            Outcome::Trapped {
                code: trap,
                origin: Some(7)
            }
        );
    }
}

#[test]
fn origin_zero_is_no_origin() {
    let mut entry = record(1, 1, 0);
    entry.push(Instruction::Unreachable);
    assert_eq!(
        run(&Spec {
            entry,
            ..Spec::default()
        }),
        Outcome::Trapped {
            code: TrapCode::InvalidCheckedProgram,
            origin: None
        }
    );
}

#[test]
fn a_stop_with_no_recorded_status_is_the_toolchain_defect() {
    let outcome = run(&Spec {
        entry: vec![Instruction::Unreachable],
        ..Spec::default()
    });
    let Outcome::Defect { cause } = outcome else {
        panic!("expected a defect, got {outcome:?}");
    };
    assert!(cause.contains("no recorded status"), "{cause}");
    // The defect is reported as the registered trap with no origin.
    assert_eq!(Outcome::defect_trap(), TrapCode::InvalidCheckedProgram);
}

#[test]
fn a_status_outside_the_table_is_a_defect() {
    let mut entry = record(9, 0, 0);
    entry.push(Instruction::Unreachable);
    assert!(matches!(
        run(&Spec {
            entry,
            ..Spec::default()
        }),
        Outcome::Defect { .. }
    ));
}

#[test]
fn a_trap_code_outside_the_table_is_a_defect() {
    let mut entry = record(1, 9, 0);
    entry.push(Instruction::Unreachable);
    assert!(matches!(
        run(&Spec {
            entry,
            ..Spec::default()
        }),
        Outcome::Defect { .. }
    ));
}

#[test]
fn a_call_that_returns_with_a_recorded_stop_status_is_a_defect() {
    // Only a stop records a trap, so a normal return beside one is a defect.
    let spec = Spec {
        entry: record(1, 1, 0),
        ..Spec::default()
    };
    let Outcome::Defect { cause } = run(&spec) else {
        panic!("expected a defect");
    };
    assert!(cause.contains("returned"), "{cause}");
}

#[test]
fn the_recorded_memory_event_is_the_host_event() {
    let mut entry = record(2, 0, 0);
    entry.push(Instruction::Unreachable);
    assert_eq!(
        run(&Spec {
            entry,
            ..Spec::default()
        }),
        Outcome::MemoryExhausted
    );
}

#[test]
fn a_failed_assertion_is_read_with_its_operands() {
    let mut entry = record(3, 0, 4);
    entry.extend([
        Instruction::I32Const(3),
        Instruction::GlobalSet(GLOBAL_FAILURE),
        Instruction::I64Const(11),
        Instruction::GlobalSet(GLOBAL_EXPECTED),
        Instruction::I64Const(-1),
        Instruction::GlobalSet(GLOBAL_ACTUAL),
        Instruction::Unreachable,
    ]);
    assert_eq!(
        run(&Spec {
            entry,
            failure_exports: true,
            ..Spec::default()
        }),
        Outcome::AssertionFailed {
            failure: Failure::Equal,
            origin: Some(4),
            operands: Some((11, u64::MAX)),
        }
    );
    let mut entry = record(3, 0, 5);
    entry.extend([
        Instruction::I32Const(1),
        Instruction::GlobalSet(GLOBAL_FAILURE),
        Instruction::Unreachable,
    ]);
    assert_eq!(
        run(&Spec {
            entry,
            failure_exports: true,
            ..Spec::default()
        }),
        Outcome::AssertionFailed {
            failure: Failure::True,
            origin: Some(5),
            operands: None,
        }
    );
}

#[test]
fn growth_past_the_limit_is_the_host_event_even_if_the_module_continues() {
    // `memory.grow` returns -1 when the limiter refuses, and this module
    // ignores it and returns normally.
    let spec = Spec {
        entry: vec![
            Instruction::I32Const(100),
            Instruction::MemoryGrow(0),
            Instruction::Drop,
        ],
        ..Spec::default()
    };
    let runner = Runner::new(MemoryLimit::new(2 * PAGE)).expect("engine");
    assert_eq!(
        runner.run_entry(&module(&spec)).expect("runnable"),
        Outcome::MemoryExhausted
    );
}

#[test]
fn growth_within_the_limit_succeeds() {
    let spec = Spec {
        entry: vec![
            Instruction::I32Const(1),
            Instruction::MemoryGrow(0),
            Instruction::Drop,
        ],
        ..Spec::default()
    };
    let runner = Runner::new(MemoryLimit::new(2 * PAGE)).expect("engine");
    assert_eq!(
        runner.run_entry(&module(&spec)).expect("runnable"),
        Outcome::Completed {
            result: ResultSlot::from_bits(0),
            live_size: 0
        }
    );
}

#[test]
fn the_limit_exactly_at_the_modules_need_runs_and_one_page_under_does_not() {
    // The empty module defines one page, so its need is one page.
    let empty = module(&Spec::default());
    let exactly = Runner::new(MemoryLimit::new(PAGE)).expect("engine");
    assert_eq!(
        exactly.run_entry(&empty).expect("runnable"),
        Outcome::Completed {
            result: ResultSlot::from_bits(0),
            live_size: 0
        }
    );
    let under = Runner::new(MemoryLimit::new(0)).expect("engine");
    assert_eq!(
        under.run_entry(&empty).expect("runnable"),
        Outcome::MemoryExhausted
    );

    // The same boundary at a larger need: three pages.
    let three = module(&Spec {
        pages: 3,
        ..Spec::default()
    });
    let exactly = Runner::new(MemoryLimit::new(3 * PAGE)).expect("engine");
    assert!(matches!(
        exactly.run_entry(&three).expect("runnable"),
        Outcome::Completed { .. }
    ));
    let under = Runner::new(MemoryLimit::new(2 * PAGE)).expect("engine");
    assert_eq!(
        under.run_entry(&three).expect("runnable"),
        Outcome::MemoryExhausted
    );
}

#[test]
fn nan_canonicalization_is_on_in_the_engine() {
    // A NaN with a payload, loaded from memory so that nothing folds it away,
    // goes through an arithmetic instruction. The engine returns the canonical
    // quiet NaN where it would otherwise propagate the payload.
    let memarg = wasm_encoder::MemArg {
        offset: 0,
        align: 2,
        memory_index: 0,
    };
    let entry = vec![
        Instruction::I32Const(0),
        Instruction::I32Const(0x7fc0_0001),
        Instruction::I32Store(memarg),
        Instruction::I32Const(0),
        Instruction::F32Load(memarg),
        Instruction::F32Const(0.0_f32.into()),
        Instruction::F32Add,
        Instruction::I32ReinterpretF32,
        Instruction::I64ExtendI32U,
        Instruction::GlobalSet(GLOBAL_RESULT),
    ];
    assert_eq!(
        run(&Spec {
            entry,
            ..Spec::default()
        }),
        Outcome::Completed {
            result: ResultSlot::from_bits(0x7fc0_0000),
            live_size: 0
        }
    );
}

#[test]
fn a_native_import_nobody_supplies_is_refused_by_name() {
    let spec = Spec {
        imports: vec![("vibra_native_v1", "text_length")],
        ..Spec::default()
    };
    let error = runner()
        .run_entry(&module(&spec))
        .expect_err("no native is supplied yet");
    assert_eq!(
        error,
        RunnerError::UnsuppliedImport {
            module: "vibra_native_v1".to_owned(),
            name: "text_length".to_owned(),
        }
    );
}

#[test]
fn a_module_that_is_not_v1_never_reaches_the_engine() {
    let spec = Spec {
        imports: vec![("env", "print")],
        ..Spec::default()
    };
    assert!(matches!(
        runner().run_entry(&module(&spec)),
        Err(RunnerError::Invalid(_))
    ));
}

#[test]
fn the_runner_reports_its_limit() {
    let runner = Runner::new(MemoryLimit::new(PAGE)).expect("engine");
    assert_eq!(runner.limit(), MemoryLimit::new(PAGE));
    assert_eq!(runner.limit().bytes(), PAGE);
}
