//! Hand-scripted scenarios for each rule of the kernel.

use vibra_actors::{
    ActorId, Answer, Awaited, Decision, DecisionError, Event, ExitKind, Lifecycle,
    MessageId, Refusal, ReplyOutcome, SendOutcome, SlotId, StepError, Time, WaitOn,
    WakeCause,
};

use crate::support::{Op, kernel, run_actor, script, step};

fn send(to: usize, payload: u64) -> Op {
    Op::Send { to, payload }
}

fn received_payloads(answers: &[Answer<u64>]) -> Vec<u64> {
    answers
        .iter()
        .filter_map(|answer| match answer {
            Answer::Received(envelope) => Some(envelope.message),
            _ => None,
        })
        .collect()
}

#[test]
fn a_mailbox_at_its_bound_refuses_the_next_send_and_accepts_after_a_delivery() {
    // Actor 0 receives (bound 1); actor 1 sends three messages.
    let mut k = kernel(vec![
        (vec![Op::Receive { timeout: None }], 1),
        (vec![send(0, 1), send(0, 2), send(0, 3)], 1),
    ]);
    run_actor(&mut k, 1, 2); // the first send is accepted, the second is not
    assert_eq!(k.queued(ActorId(0)), [MessageId(0)]);
    run_actor(&mut k, 0, 1); // the receive takes the first message
    assert!(k.queued(ActorId(0)).is_empty());
    run_actor(&mut k, 1, 2); // the third send is accepted
    assert_eq!(k.queued(ActorId(0)), [MessageId(1)]);

    let sent: Vec<_> = script(&k, 1)
        .answers
        .iter()
        .filter_map(|answer| match answer {
            Answer::Sent(outcome) => Some(*outcome),
            _ => None,
        })
        .collect();
    assert_eq!(
        sent,
        [
            SendOutcome::Accepted(MessageId(0)),
            SendOutcome::Refused(Refusal::Full),
            SendOutcome::Accepted(MessageId(1)),
        ]
    );
    assert!(k.events().contains(&Event::SendRefused {
        from: ActorId(1),
        to: ActorId(0),
        reason: Refusal::Full
    }));
}

#[test]
fn messages_from_one_sender_arrive_in_send_order() {
    for seed in 0..200 {
        let mut k = kernel(vec![
            (vec![Op::Receive { timeout: None }; 6], 8),
            (vec![send(0, 10), send(0, 11), send(0, 12)], 1),
            (vec![send(0, 20), send(0, 21), send(0, 22)], 1),
        ]);
        let report =
            vibra_actors::run(&mut k, &mut vibra_actors::Seeded::new(seed), 10_000)
                .expect("run");
        assert_ne!(report.end, vibra_actors::RunEnd::LimitReached);
        let got = received_payloads(&script(&k, 0).answers);
        assert_eq!(got.len(), 6, "seed {seed}");
        for base in [10, 20] {
            let from_one: Vec<u64> = got
                .iter()
                .copied()
                .filter(|p| (base..base + 3).contains(p))
                .collect();
            assert_eq!(from_one, [base, base + 1, base + 2], "seed {seed}");
        }
    }
}

fn client_and_server(
    server: Vec<Op>,
) -> vibra_actors::Kernel<crate::support::Scripted> {
    kernel(vec![
        (server, 2),
        (
            vec![Op::Request {
                to: 0,
                payload: 1,
                timeout: None,
            }],
            2,
        ),
    ])
}

#[test]
fn a_second_reply_to_one_slot_is_refused_and_the_first_stands() {
    let mut k = client_and_server(vec![
        Op::Receive { timeout: None },
        Op::ReplyHeld { payload: 100 },
        Op::ReplyAgain { payload: 200 },
    ]);
    run_actor(&mut k, 1, 3); // creates the slot, sends, parks on it
    assert!(matches!(
        k.lifecycle(ActorId(1)),
        Some(Lifecycle::Waiting(WaitOn::Reply {
            slot: SlotId(0),
            ..
        }))
    ));
    run_actor(&mut k, 0, 4); // receives, replies, replies again, exits
    run_actor(&mut k, 1, 1);

    let replies: Vec<_> = k
        .events()
        .iter()
        .filter_map(|event| match event {
            Event::Reply { outcome, .. } => Some(*outcome),
            _ => None,
        })
        .collect();
    assert_eq!(
        replies,
        [ReplyOutcome::Accepted, ReplyOutcome::AlreadyReplied]
    );
    assert!(
        script(&k, 1)
            .answers
            .contains(&Answer::Awaited(Awaited::Reply(100)))
    );
    assert!(
        !script(&k, 1)
            .answers
            .contains(&Answer::Awaited(Awaited::Reply(200)))
    );
}

#[test]
fn a_waiter_is_woken_with_exit_when_its_actor_exits() {
    for exit in [Op::Exit, Op::Trap(7)] {
        let mut k =
            client_and_server(vec![Op::Receive { timeout: None }, exit.clone()]);
        run_actor(&mut k, 1, 3);
        run_actor(&mut k, 0, 2); // receives, then exits or traps
        let wakes: Vec<_> = k
            .events()
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    Event::Wake {
                        actor: ActorId(1),
                        ..
                    }
                )
            })
            .collect();
        assert_eq!(
            wakes,
            [&Event::Wake {
                actor: ActorId(1),
                cause: WakeCause::SlotClosed
            }]
        );
        run_actor(&mut k, 1, 1);
        assert!(
            script(&k, 1)
                .answers
                .contains(&Answer::Awaited(Awaited::Exited))
        );
        let kind = match exit {
            Op::Trap(code) => ExitKind::Trapped(code),
            _ => ExitKind::Exited,
        };
        assert_eq!(k.lifecycle(ActorId(0)), Some(Lifecycle::Exited(kind)));
    }
}

#[test]
fn a_slot_whose_message_was_never_received_closes_when_the_receiver_exits() {
    let mut k = client_and_server(vec![Op::Exit]);
    run_actor(&mut k, 1, 2); // creates the slot and sends; the server has not run
    run_actor(&mut k, 0, 1); // the server exits with the message still queued
    run_actor(&mut k, 1, 2); // the wait finds the slot closed at once
    assert!(
        script(&k, 1)
            .answers
            .contains(&Answer::Awaited(Awaited::Exited))
    );
}

#[test]
fn a_send_to_an_exited_actor_is_refused_as_exited() {
    let mut k = kernel(vec![(vec![Op::Exit], 1), (vec![send(0, 1)], 1)]);
    run_actor(&mut k, 0, 1);
    run_actor(&mut k, 1, 2);
    assert!(
        script(&k, 1)
            .answers
            .contains(&Answer::Sent(SendOutcome::Refused(Refusal::Exited)))
    );
}

#[test]
fn one_actor_runs_one_handler_at_a_time() {
    let mut k = kernel(vec![
        (vec![Op::Receive { timeout: None }], 1),
        (vec![Op::Exit], 1),
    ]);
    // A waiting actor and an exited actor cannot be run.
    run_actor(&mut k, 0, 1);
    run_actor(&mut k, 1, 1);
    for actor in [ActorId(0), ActorId(1)] {
        let before = k.events().len();
        let error = k.step(Decision::Run(actor)).expect_err("not runnable");
        assert!(matches!(
            error,
            StepError::Decision(DecisionError::NotRunnable { .. })
        ));
        assert_eq!(
            k.events().len(),
            before,
            "a refused decision changes nothing"
        );
    }
    // The detector itself fires when an actor is run with the wrong answer.
    let mut wrong = crate::support::Script::new(vec![]);
    let mut exec = crate::support::Scripted::default();
    vibra_actors::Executor::run_to_action(&mut exec, &mut wrong, Answer::Continue);
    assert_eq!(wrong.violations.len(), 1);
    // Over many random runs the executor is never entered twice at once and
    // never run with an answer that does not belong to the action it stopped at.
    for seed in 0..300 {
        let mut k = crate::invariants::scenario(seed);
        vibra_actors::run(&mut k, &mut vibra_actors::Seeded::new(seed), 100_000)
            .expect("run");
        for i in 0..k.actor_count() {
            let id = u32::try_from(i).expect("small");
            assert!(
                script(&k, id).violations.is_empty(),
                "seed {seed}, actor {id}: {:?}",
                script(&k, id).violations
            );
        }
    }
}

#[test]
fn a_receive_on_an_empty_mailbox_parks_and_a_delivery_wakes_it() {
    let mut k = kernel(vec![
        (vec![Op::Receive { timeout: None }], 1),
        (vec![send(0, 5)], 1),
    ]);
    run_actor(&mut k, 0, 1);
    assert_eq!(
        k.lifecycle(ActorId(0)),
        Some(Lifecycle::Waiting(WaitOn::Receive { deadline: None }))
    );
    assert!(k.runnable() == [ActorId(1)]);
    run_actor(&mut k, 1, 1);
    assert_eq!(k.lifecycle(ActorId(0)), Some(Lifecycle::Runnable));
    let tail = &k.events()[k.events().len() - 3..];
    assert_eq!(
        tail,
        [
            Event::SendAccepted {
                from: ActorId(1),
                to: ActorId(0),
                message: MessageId(0)
            },
            Event::Deliver {
                actor: ActorId(0),
                message: MessageId(0),
                from: ActorId(1)
            },
            Event::Wake {
                actor: ActorId(0),
                cause: WakeCause::Delivery
            },
        ]
    );
    run_actor(&mut k, 0, 1);
    assert_eq!(received_payloads(&script(&k, 0).answers), [5]);
}

#[test]
fn a_wait_times_out_at_its_virtual_deadline_and_not_before() {
    for receive in [true, false] {
        let ops = if receive {
            vec![Op::Receive { timeout: Some(5) }]
        } else {
            vec![Op::Request {
                to: 1,
                payload: 1,
                timeout: Some(5),
            }]
        };
        // The peer never replies: it receives nothing and parks forever.
        let mut k = kernel(vec![(ops, 1), (vec![Op::Receive { timeout: None }], 1)]);
        run_actor(&mut k, 0, if receive { 1 } else { 3 });
        assert_eq!(k.next_deadline(), Some(Time(5)));
        step(&mut k, Decision::Advance(4));
        assert_eq!(k.now(), Time(4));
        assert!(matches!(
            k.lifecycle(ActorId(0)),
            Some(Lifecycle::Waiting(_))
        ));
        assert!(
            !k.events()
                .iter()
                .any(|e| matches!(e, Event::Timeout { .. }))
        );
        step(&mut k, Decision::Advance(1));
        assert_eq!(k.now(), Time(5));
        assert_eq!(k.lifecycle(ActorId(0)), Some(Lifecycle::Runnable));
        let tail = &k.events()[k.events().len() - 3..];
        assert_eq!(
            tail,
            [
                Event::Advance { to: Time(5) },
                Event::Timeout {
                    actor: ActorId(0),
                    deadline: Time(5)
                },
                Event::Wake {
                    actor: ActorId(0),
                    cause: WakeCause::TimedOut
                },
            ]
        );
        run_actor(&mut k, 0, 1);
        let expected = if receive {
            Answer::ReceiveTimedOut
        } else {
            Answer::Awaited(Awaited::TimedOut)
        };
        assert_eq!(script(&k, 0).answers.last(), Some(&expected));
    }
}

#[test]
fn a_decision_that_cannot_be_taken_is_a_typed_error_and_changes_nothing() {
    let mut k = kernel(vec![(vec![Op::Exit], 1)]);
    assert_eq!(
        k.step(Decision::Run(ActorId(9))),
        Err(StepError::Decision(DecisionError::UnknownActor(ActorId(9))))
    );
    assert_eq!(
        k.step(Decision::Advance(0)),
        Err(StepError::Decision(DecisionError::ZeroAdvance))
    );
    step(&mut k, Decision::Advance(u64::MAX));
    assert_eq!(
        k.step(Decision::Advance(1)),
        Err(StepError::Decision(DecisionError::TimeOverflow))
    );
}

#[test]
fn an_invalid_request_is_a_typed_error_and_poisons_the_kernel() {
    use std::collections::VecDeque;
    use vibra_actors::{Action, Executor, Kernel, RequestError};

    struct Fixed;
    impl Executor for Fixed {
        type Actor = VecDeque<Action<u64>>;
        type Message = u64;
        fn run_to_action(
            &mut self,
            actor: &mut Self::Actor,
            _answer: Answer<u64>,
        ) -> Action<u64> {
            actor.pop_front().unwrap_or(Action::Exit)
        }
    }

    let cases: [(Action<u64>, RequestError); 3] = [
        (
            Action::Reply {
                slot: SlotId(0),
                message: 1,
            },
            RequestError::NotSlotHolder(SlotId(0)),
        ),
        (
            Action::Wait {
                slot: SlotId(0),
                timeout: None,
            },
            RequestError::NotSlotOwner(SlotId(0)),
        ),
        (
            Action::Wait {
                slot: SlotId(9),
                timeout: None,
            },
            RequestError::UnknownSlot(SlotId(9)),
        ),
    ];
    for (action, error) in cases {
        let mut k = Kernel::new(Fixed);
        k.spawn(VecDeque::from([Action::NewSlot]), 1)
            .expect("spawn");
        k.spawn(VecDeque::from([action]), 1).expect("spawn");
        step_fixed(&mut k, 0);
        assert_eq!(
            k.step(Decision::Run(ActorId(1))),
            Err(StepError::Request {
                actor: ActorId(1),
                error
            })
        );
        assert_eq!(k.step(Decision::Run(ActorId(0))), Err(StepError::Poisoned));
    }

    fn step_fixed(k: &mut Kernel<Fixed>, actor: u32) {
        k.step(Decision::Run(ActorId(actor))).expect("a valid step");
    }
}
