//! Seeded schedules over randomly scripted actors, and the invariants every
//! run must keep.

use std::collections::{BTreeMap, BTreeSet};

use vibra_actors::{
    ActorId, Event, ExitKind, Kernel, Lifecycle, MessageId, Recorded, Refusal,
    ReplyOutcome, RunEnd, Seeded, SlotId, SplitMix64, WakeCause, run, run_observed,
};

use crate::support::{Op, Scripted, kernel, script};

const BASE_SEED: u64 = 0x5EED_AC70_0000_0001;
const SEEDS: u64 = 10_000;
const LIMIT: u64 = 100_000;

/// The kernel for `seed`: 2 to 5 actors with random bounds and scripts.
pub(crate) fn scenario(seed: u64) -> Kernel<Scripted> {
    let mut rng = SplitMix64::new(seed.wrapping_mul(0x2545_F491_4F6C_DD1D));
    let actors = 2 + rng.below(4);
    let mut specs = Vec::new();
    for _ in 0..actors {
        let bound = 1 + usize::try_from(rng.below(3)).expect("small");
        let mut ops = Vec::new();
        for _ in 0..(2 + rng.below(9)) {
            let to = usize::try_from(rng.below(actors)).expect("small");
            let payload = rng.below(1000);
            let timeout = (rng.below(5) < 2).then(|| 1 + rng.below(6));
            ops.push(match rng.below(14) {
                0..=2 => Op::Send { to, payload },
                3..=4 => Op::Request {
                    to,
                    payload,
                    timeout,
                },
                5..=7 => Op::Receive { timeout },
                8..=10 => Op::ReplyHeld { payload },
                11 => Op::ReplyAgain { payload },
                12 => Op::Host(payload),
                _ if rng.below(3) == 0 => {
                    Op::Trap(u32::try_from(payload).expect("small"))
                }
                _ => Op::Exit,
            });
        }
        specs.push((ops, bound));
    }
    kernel(specs)
}

#[derive(Default)]
struct Tally {
    full: u32,
    exited: u32,
    timeouts: u32,
    replies: u32,
    second_replies: u32,
    closed_wakes: u32,
    traps: u32,
    blocked: u32,
    completed: u32,
}

fn check_run(seed: u64, k: &Kernel<Scripted>, tally: &mut Tally) {
    let events = k.events();
    let mut bounds: BTreeMap<ActorId, usize> = BTreeMap::new();
    let mut accepted: BTreeMap<MessageId, ActorId> = BTreeMap::new();
    let mut delivered: BTreeSet<MessageId> = BTreeSet::new();
    let mut depth: BTreeMap<ActorId, usize> = BTreeMap::new();
    let mut parked: BTreeSet<ActorId> = BTreeSet::new();
    let mut accepted_replies: BTreeSet<SlotId> = BTreeSet::new();
    let mut now = 0;

    for (i, event) in events.iter().enumerate() {
        match *event {
            Event::Spawn { actor, bound } => {
                bounds.insert(actor, bound);
            }
            Event::Advance { to } => {
                assert!(to.0 > now, "seed {seed}: time did not advance");
                now = to.0;
            }
            Event::SendAccepted { to, message, .. } => {
                assert!(
                    accepted.insert(message, to).is_none(),
                    "seed {seed}: {message:?} accepted twice"
                );
                *depth.entry(to).or_default() += 1;
            }
            Event::SendRefused { reason, .. } => match reason {
                Refusal::Full => tally.full += 1,
                Refusal::Exited => tally.exited += 1,
                Refusal::NotAdmitted => {
                    panic!("seed {seed}: nothing is unadmitted yet")
                }
            },
            Event::Deliver { actor, message, .. } => {
                assert_eq!(
                    accepted.get(&message),
                    Some(&actor),
                    "seed {seed}: {message:?} delivered to an actor it was not accepted for"
                );
                assert!(
                    delivered.insert(message),
                    "seed {seed}: {message:?} delivered twice"
                );
                let d = depth.entry(actor).or_default();
                *d = d
                    .checked_sub(1)
                    .expect("a delivery leaves a queued message");
            }
            Event::Park { actor, .. } => {
                assert!(parked.insert(actor), "seed {seed}: {actor:?} parked twice");
            }
            Event::Wake { actor, cause } => {
                assert!(
                    parked.remove(&actor),
                    "seed {seed}: {actor:?} woken without being parked"
                );
                if cause == WakeCause::SlotClosed {
                    tally.closed_wakes += 1;
                }
            }
            Event::Timeout { actor, .. } => {
                tally.timeouts += 1;
                assert!(
                    matches!(
                        events.get(i + 1),
                        Some(Event::Wake { actor: a, cause: WakeCause::TimedOut }) if *a == actor
                    ),
                    "seed {seed}: a timeout is not followed by its wake"
                );
            }
            Event::Reply { slot, outcome, .. } => match outcome {
                ReplyOutcome::Accepted => {
                    tally.replies += 1;
                    assert!(
                        accepted_replies.insert(slot),
                        "seed {seed}: {slot:?} filled twice"
                    );
                }
                ReplyOutcome::AlreadyReplied => tally.second_replies += 1,
                ReplyOutcome::OwnerExited => {}
            },
            Event::Exit { actor, kind } => {
                depth.insert(actor, 0);
                if matches!(kind, ExitKind::Trapped(_)) {
                    tally.traps += 1;
                }
            }
            Event::NewSlot { .. } | Event::Host { .. } => {}
        }
        for (actor, d) in &depth {
            let bound = bounds.get(actor).copied().unwrap_or(0);
            assert!(
                *d <= bound,
                "seed {seed}: {actor:?} holds {d} over its bound {bound}"
            );
        }
    }

    // No lost message: every accepted message was delivered exactly once, or
    // is still queued for a live receiver, or its receiver exited. And what
    // is queued is exactly the accepted, undelivered messages of live
    // receivers.
    let mut queued_all: BTreeSet<MessageId> = BTreeSet::new();
    for i in 0..k.actor_count() {
        let id = ActorId(u32::try_from(i).expect("small"));
        if matches!(k.lifecycle(id), Some(Lifecycle::Exited(_))) {
            continue;
        }
        for message in k.queued(id) {
            assert!(
                queued_all.insert(message),
                "seed {seed}: {message:?} queued twice"
            );
        }
    }
    for (message, receiver) in &accepted {
        if delivered.contains(message) {
            assert!(
                !queued_all.contains(message),
                "seed {seed}: {message:?} delivered and still queued"
            );
            continue;
        }
        let exited = matches!(k.lifecycle(*receiver), Some(Lifecycle::Exited(_)));
        assert!(
            exited || queued_all.contains(message),
            "seed {seed}: {message:?} for {receiver:?} was lost"
        );
    }
    for message in &queued_all {
        assert!(
            accepted.contains_key(message) && !delivered.contains(message),
            "seed {seed}: {message:?} is queued but was never accepted or was delivered"
        );
    }

    // Every parked actor was woken exactly once, or still waits.
    let waiting: BTreeSet<ActorId> = k.waiting().into_iter().collect();
    assert_eq!(
        parked, waiting,
        "seed {seed}: a park has no wake and no wait"
    );
}

#[test]
fn ten_thousand_seeded_schedules_keep_the_invariants() {
    let mut tally = Tally::default();
    for n in 0..SEEDS {
        let seed = BASE_SEED.wrapping_add(n);
        let mut k = scenario(seed);
        let mut over_bound = None;
        let report = run_observed(&mut k, &mut Seeded::new(seed), LIMIT, &mut |k| {
            for i in 0..k.actor_count() {
                let id = ActorId(u32::try_from(i).expect("small"));
                if k.queued(id).len() > k.mailbox_bound(id).unwrap_or(0) {
                    over_bound = Some(id);
                }
            }
        })
        .unwrap_or_else(|error| panic!("seed {seed}: the run failed: {error:?}"));
        assert!(
            over_bound.is_none(),
            "seed {seed}: {over_bound:?} over its bound"
        );
        assert!(
            matches!(report.end, RunEnd::Completed | RunEnd::Blocked),
            "seed {seed}: ended with {:?}",
            report.end
        );
        match report.end {
            RunEnd::Blocked => tally.blocked += 1,
            _ => tally.completed += 1,
        }
        check_run(seed, &k, &mut tally);
        for i in 0..k.actor_count() {
            let violations = &script(&k, u32::try_from(i).expect("small")).violations;
            assert!(violations.is_empty(), "seed {seed}: {violations:?}");
        }

        let mut replayed = scenario(seed);
        let replay = run(
            &mut replayed,
            &mut Recorded::new(report.decisions.clone()),
            LIMIT,
        )
        .unwrap_or_else(|error| panic!("seed {seed}: the replay failed: {error:?}"));
        assert_eq!(
            replay, report,
            "seed {seed}: the replay took other decisions"
        );
        assert_eq!(
            replayed.events(),
            k.events(),
            "seed {seed}: the replay recorded other events"
        );
    }

    // The scenarios must exercise what the invariants guard.
    for (name, count) in [
        ("full refusals", tally.full),
        ("sends to exited actors", tally.exited),
        ("timeouts", tally.timeouts),
        ("accepted replies", tally.replies),
        ("second replies", tally.second_replies),
        ("waiters woken by exit", tally.closed_wakes),
        ("traps", tally.traps),
        ("blocked runs", tally.blocked),
        ("completed runs", tally.completed),
    ] {
        assert!(count > 0, "no seed produced {name}");
    }
}
