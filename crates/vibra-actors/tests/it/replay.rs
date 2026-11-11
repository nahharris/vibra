//! Replay: a run's recorded decisions reproduce its event record, and a
//! schedule that cannot be followed is refused.

use vibra_actors::{
    ActorId, Decision, DecisionError, Lifecycle, Recorded, RunEnd, RunError, Seeded,
    WaitOn, run,
};

use crate::invariants::scenario;
use crate::support::{Op, kernel};

#[test]
fn a_recorded_schedule_replays_to_the_same_event_record() {
    for seed in 0..50 {
        let mut first = scenario(seed);
        let report = run(&mut first, &mut Seeded::new(seed), 100_000).expect("run");
        let mut second = scenario(seed);
        let replay = run(
            &mut second,
            &mut Recorded::new(report.decisions.clone()),
            100_000,
        )
        .expect("replay");
        assert_eq!(replay, report, "seed {seed}");
        assert_eq!(second.events(), first.events(), "seed {seed}");
    }
}

#[test]
fn a_schedule_that_names_a_non_runnable_actor_is_an_invalid_replay() {
    let build = || kernel(vec![(vec![Op::Receive { timeout: None }], 1)]);

    // Actor 0 parks on its first run, so a second run of it is not allowed.
    let mut k = build();
    let schedule = vec![Decision::Run(ActorId(0)), Decision::Run(ActorId(0))];
    assert_eq!(
        run(&mut k, &mut Recorded::new(schedule), 100),
        Err(RunError::InvalidDecision {
            index: 1,
            error: DecisionError::NotRunnable {
                actor: ActorId(0),
                state: Lifecycle::Waiting(WaitOn::Receive { deadline: None }),
            },
        })
    );

    let mut k = build();
    assert_eq!(
        run(
            &mut k,
            &mut Recorded::new(vec![Decision::Run(ActorId(3))]),
            100
        ),
        Err(RunError::InvalidDecision {
            index: 0,
            error: DecisionError::UnknownActor(ActorId(3)),
        })
    );
}

#[test]
fn a_schedule_with_decisions_left_over_or_missing_is_not_a_replay() {
    let build = || kernel(vec![(vec![Op::Host(1)], 1)]);
    let mut k = build();
    let full = run(&mut k, &mut Seeded::new(0), 100).expect("run");
    assert_eq!(full.end, RunEnd::Completed);

    let mut surplus = full.decisions.clone();
    surplus.push(Decision::Advance(1));
    assert_eq!(
        run(&mut build(), &mut Recorded::new(surplus), 100),
        Err(RunError::SurplusDecisions { remaining: 1 })
    );

    let short = full.decisions[..full.decisions.len() - 1].to_vec();
    let report = run(&mut build(), &mut Recorded::new(short), 100).expect("run");
    assert_eq!(report.end, RunEnd::ScheduleExhausted);
}
