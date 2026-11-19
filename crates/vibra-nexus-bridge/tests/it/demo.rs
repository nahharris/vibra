//! The foundation's stepper scenarios, through the bridge: each test runs the
//! stepper and the bridge on the same program and requires the same
//! observation, unbounded and with small quotas.

use vibra_host::AuditEvent;
use vibra_ir::boundary::TrapCode;
use vibra_nexus::{Cell, Outcome, Trap, UNBOUNDED};

use crate::fixture::{self, LIMIT, MIB, run_bridge};
use crate::stepper;

fn stdout_event(text: &str) -> String {
    format!(
        "(record kind: @host op: \"io.stdout-text\" args: (array \"{text}\") result: \
         (record kind: @enum type: @std.result.result variant: @ok))"
    )
}

#[test]
fn the_bridge_drives_the_demo() {
    for trap in [true, false] {
        for quota in [UNBOUNDED, 1, 2, 7] {
            let expected = stepper::execute(&stepper::demo(1000, trap), LIMIT, quota);
            let ran = run_bridge(&fixture::demo(1000, trap), LIMIT, quota);
            assert_eq!(ran, expected, "trap {trap}, quota {quota}");
            assert_eq!(ran.stats.tail_transfers, 1000);
            if trap {
                assert_eq!(
                    ran.outcome,
                    Outcome::Trapped(Trap {
                        code: TrapCode::DivisionByZero,
                        origin: 2
                    })
                );
            } else {
                assert_eq!(ran.outcome, Outcome::Completed);
            }
            assert_eq!(
                ran.audit
                    .iter()
                    .map(AuditEvent::to_vibon)
                    .collect::<Vec<_>>(),
                [stdout_event("done\\n")],
                "the host operation runs exactly once"
            );
            if quota == 1 {
                assert!(ran.stats.yields >= 1000);
            }
        }
    }
}

#[test]
fn bounded_tail_transfers_keep_the_frame_count_and_the_live_size_bounded() {
    for rounds in [10_000, 100_000] {
        for quota in [UNBOUNDED, 1] {
            let ran = run_bridge(&fixture::demo(rounds, false), LIMIT, quota);
            assert_eq!(
                ran,
                stepper::execute(&stepper::demo(rounds, false), LIMIT, quota)
            );
            assert_eq!(ran.outcome, Outcome::Completed);
            assert_eq!(ran.result, Cell::i64(0));
            assert_eq!(ran.stats.max_depth, 2, "the entry and one activation");
            assert_eq!(ran.stats.tail_transfers, rounds);
            assert_eq!(ran.live, 0, "a finished run holds nothing");
        }
    }
    let small = run_bridge(&fixture::demo(100_000, false), MIB, UNBOUNDED);
    assert_eq!(small.outcome, Outcome::Completed);
}

#[test]
fn non_tail_recursion_is_bounded_by_memory_and_uses_no_native_stack() {
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let ran = run_bridge(&fixture::deep(100_000), LIMIT, UNBOUNDED);
            assert_eq!(
                ran,
                stepper::execute(&stepper::deep(100_000), LIMIT, UNBOUNDED)
            );
            assert_eq!(ran.outcome, Outcome::Completed);
            assert_eq!(ran.result, Cell::i64(100_000));
            assert_eq!(ran.stats.max_depth, 100_002);
            assert_eq!(ran.live, 0);
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn exhausting_the_limit_is_the_host_event_and_a_larger_limit_recovers() {
    for quota in [UNBOUNDED, 3] {
        let small = run_bridge(&fixture::deep(100_000), 64 * 1024, quota);
        assert_eq!(small.outcome, Outcome::MemoryExhausted);
        assert!(small.audit.is_empty());
        assert_eq!(
            small,
            stepper::execute(&stepper::deep(100_000), 64 * 1024, quota)
        );
    }
    assert_eq!(
        run_bridge(&fixture::deep(100_000), LIMIT, UNBOUNDED).outcome,
        Outcome::Completed
    );
}

#[test]
fn a_module_value_is_initialized_once_however_often_it_is_read() {
    for quota in [UNBOUNDED, 1] {
        let ran = run_bridge(&fixture::module_value(), LIMIT, quota);
        assert_eq!(
            ran,
            stepper::execute(&stepper::module_value(), LIMIT, quota)
        );
        assert_eq!(ran.outcome, Outcome::Completed);
        assert_eq!(ran.result, Cell::i64(7));
        assert_eq!(
            ran.audit
                .iter()
                .map(AuditEvent::to_vibon)
                .collect::<Vec<_>>(),
            [stdout_event("init\\n")],
            "quota {quota}"
        );
        assert_eq!(ran.live, 64, "only the module state's `str` is held");
    }
}

#[test]
fn a_function_value_called_with_another_arity_is_a_toolchain_defect() {
    let ran = run_bridge(&fixture::demo_with_arity_defect(), LIMIT, UNBOUNDED);
    assert_eq!(
        ran.outcome,
        Outcome::Trapped(Trap {
            code: TrapCode::InvalidCheckedProgram,
            origin: 0
        })
    );
    assert!(ran.audit.is_empty());
    assert_eq!(
        ran,
        stepper::execute(&stepper::demo_with_arity_defect(), LIMIT, UNBOUNDED)
    );
}
