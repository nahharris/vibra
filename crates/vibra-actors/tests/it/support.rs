//! A scripted executor: each actor performs a list of operations, and checks
//! that the kernel only ever runs it when it should.

use std::collections::VecDeque;

use vibra_actors::{
    Action, ActorId, Address, Answer, Decision, Executor, Kernel, SendOutcome, SlotId,
};

#[derive(Clone, Debug)]
pub(crate) enum Op {
    Send {
        to: usize,
        payload: u64,
    },
    Request {
        to: usize,
        payload: u64,
        timeout: Option<u64>,
    },
    Receive {
        timeout: Option<u64>,
    },
    ReplyHeld {
        payload: u64,
    },
    ReplyAgain {
        payload: u64,
    },
    Host(u64),
    Exit,
    Trap(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Expect {
    Start,
    Continue,
    Sent,
    Received,
    Slot,
    Awaited,
    Replied,
    Dead,
}

#[derive(Debug)]
enum Stage {
    Ready,
    AwaitSlot {
        to: usize,
        payload: u64,
        timeout: Option<u64>,
    },
    AwaitSent {
        slot: SlotId,
        timeout: Option<u64>,
    },
}

#[derive(Debug)]
pub(crate) struct Script {
    ops: VecDeque<Op>,
    stage: Stage,
    owed: VecDeque<SlotId>,
    last_replied: Option<SlotId>,
    expect: Expect,
    active: bool,
    /// Every answer the kernel gave, in order.
    pub(crate) answers: Vec<Answer<u64>>,
    /// Every way the kernel ran this actor when it should not have.
    pub(crate) violations: Vec<String>,
}

impl Script {
    pub(crate) fn new(ops: Vec<Op>) -> Self {
        Self {
            ops: ops.into(),
            stage: Stage::Ready,
            owed: VecDeque::new(),
            last_replied: None,
            expect: Expect::Start,
            active: false,
            answers: Vec::new(),
            violations: Vec::new(),
        }
    }

    fn check(&mut self, answer: &Answer<u64>) {
        let fits = matches!(
            (self.expect, answer),
            (Expect::Start, Answer::Start)
                | (Expect::Continue, Answer::Continue)
                | (Expect::Sent, Answer::Sent(_))
                | (
                    Expect::Received,
                    Answer::Received(_) | Answer::ReceiveTimedOut
                )
                | (Expect::Slot, Answer::Slot(_))
                | (Expect::Awaited, Answer::Awaited(_))
                | (Expect::Replied, Answer::Replied(_))
        );
        if !fits {
            self.violations.push(format!(
                "ran with {answer:?} while expecting {:?}",
                self.expect
            ));
        }
    }

    pub(crate) fn next(
        &mut self,
        answer: &Answer<u64>,
        peers: &[Address],
    ) -> Action<u64> {
        loop {
            match std::mem::replace(&mut self.stage, Stage::Ready) {
                Stage::AwaitSlot {
                    to,
                    payload,
                    timeout,
                } => {
                    if let (Answer::Slot(slot), Some(&to)) = (answer, peers.get(to)) {
                        self.stage = Stage::AwaitSent {
                            slot: *slot,
                            timeout,
                        };
                        self.expect = Expect::Sent;
                        return Action::Send {
                            to,
                            message: payload,
                            reply: Some(*slot),
                        };
                    }
                    self.expect = Expect::Dead;
                    return Action::Exit;
                }
                Stage::AwaitSent { slot, timeout } => {
                    if matches!(answer, Answer::Sent(SendOutcome::Accepted(_))) {
                        self.expect = Expect::Awaited;
                        return Action::Wait { slot, timeout };
                    }
                    // A refused request has nothing to wait for.
                }
                Stage::Ready => {}
            }
            let Some(op) = self.ops.pop_front() else {
                self.expect = Expect::Dead;
                return Action::Exit;
            };
            match op {
                Op::Send { to, payload } => {
                    let Some(&to) = peers.get(to) else { continue };
                    self.expect = Expect::Sent;
                    return Action::Send {
                        to,
                        message: payload,
                        reply: None,
                    };
                }
                Op::Request {
                    to,
                    payload,
                    timeout,
                } => {
                    self.stage = Stage::AwaitSlot {
                        to,
                        payload,
                        timeout,
                    };
                    self.expect = Expect::Slot;
                    return Action::NewSlot;
                }
                Op::Receive { timeout } => {
                    self.expect = Expect::Received;
                    return Action::Receive { timeout };
                }
                Op::ReplyHeld { payload } => {
                    if let Some(slot) = self.owed.pop_front() {
                        self.last_replied = Some(slot);
                        self.expect = Expect::Replied;
                        return Action::Reply {
                            slot,
                            message: payload,
                        };
                    }
                }
                Op::ReplyAgain { payload } => {
                    if let Some(slot) = self.last_replied {
                        self.expect = Expect::Replied;
                        return Action::Reply {
                            slot,
                            message: payload,
                        };
                    }
                }
                Op::Host(tag) => {
                    self.expect = Expect::Continue;
                    return Action::Host(tag);
                }
                Op::Exit => {
                    self.expect = Expect::Dead;
                    return Action::Exit;
                }
                Op::Trap(code) => {
                    self.expect = Expect::Dead;
                    return Action::Trap(code);
                }
            }
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct Scripted {
    pub(crate) peers: Vec<Address>,
}

impl Executor for Scripted {
    type Actor = Script;
    type Message = u64;

    fn run_to_action(
        &mut self,
        actor: &mut Script,
        answer: Answer<u64>,
    ) -> Action<u64> {
        if actor.active {
            actor
                .violations
                .push("entered while already running".to_owned());
        }
        actor.active = true;
        actor.check(&answer);
        if let Answer::Received(envelope) = &answer
            && let Some(slot) = envelope.reply
        {
            actor.owed.push_back(slot);
        }
        actor.answers.push(answer.clone());
        let action = actor.next(&answer, &self.peers);
        actor.active = false;
        action
    }
}

/// A kernel with one actor per `(ops, bound)`.
pub(crate) fn kernel(actors: Vec<(Vec<Op>, usize)>) -> Kernel<Scripted> {
    let mut kernel = Kernel::new(Scripted::default());
    for (ops, bound) in actors {
        let address = kernel.spawn(Script::new(ops), bound).expect("spawn");
        kernel.executor_mut().peers.push(address);
    }
    kernel
}

pub(crate) fn step(kernel: &mut Kernel<Scripted>, decision: Decision) {
    kernel.step(decision).expect("a valid step");
}

pub(crate) fn run_actor(kernel: &mut Kernel<Scripted>, actor: u32, times: usize) {
    for _ in 0..times {
        step(kernel, Decision::Run(ActorId(actor)));
    }
}

pub(crate) fn script(kernel: &Kernel<Scripted>, actor: u32) -> &Script {
    kernel.actor(ActorId(actor)).expect("an actor")
}
