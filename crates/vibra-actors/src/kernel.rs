//! The kernel: actors, mailboxes, reply slots, virtual time, and the event
//! record, advanced one decision at a time.

use std::collections::BTreeSet;

use crate::address::{Address, admits};
use crate::events::{Event, Refusal, SendOutcome, WakeCause};
use crate::executor::{Action, Answer, Envelope, Executor};
use crate::ids::{ActorId, MessageId, SlotId, Time};
use crate::lifecycle::{ExitKind, Lifecycle, LifecycleError, WaitOn};
use crate::mailbox::{Mailbox, Push, ZeroBound};
use crate::reply::{Awaited, ReplyOutcome, ReplySlot, Take};

/// One choice the scheduler makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    /// Run this actor from its current point to its next kernel action.
    Run(ActorId),
    /// Move virtual time forward by this many ticks (at least 1).
    Advance(u64),
}

/// Why a decision could not be taken. Nothing changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecisionError {
    /// No such actor.
    UnknownActor(ActorId),
    /// The actor is waiting, exited, or running.
    NotRunnable {
        /// The actor.
        actor: ActorId,
        /// Its state.
        state: Lifecycle,
    },
    /// An advance by zero ticks.
    ZeroAdvance,
    /// The advance would overflow virtual time.
    TimeOverflow,
}

/// Why an actor's request was invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestError {
    /// The address names no actor of this kernel.
    UnknownActor(ActorId),
    /// No such reply slot.
    UnknownSlot(SlotId),
    /// The actor does not own the slot.
    NotSlotOwner(SlotId),
    /// The actor does not hold the slot: no message that carried it has been
    /// delivered to the actor.
    NotSlotHolder(SlotId),
    /// The slot is already attached to an accepted message.
    SlotAlreadyAttached(SlotId),
    /// The slot's reply or closure was already taken by a wait.
    SlotConsumed(SlotId),
    /// A timeout would overflow virtual time.
    TimeOverflow,
    /// An identity space is exhausted.
    IdsExhausted,
}

/// Why a step failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepError {
    /// The decision was invalid; the kernel is unchanged.
    Decision(DecisionError),
    /// The actor's request was invalid. The kernel is poisoned afterwards.
    Request {
        /// The actor that made it.
        actor: ActorId,
        /// What was wrong.
        error: RequestError,
    },
    /// The kernel itself attempted a transition the lifecycle table refuses.
    /// This is a defect of the kernel; it is poisoned afterwards.
    Lifecycle(LifecycleError),
    /// An earlier step failed after changing state; no further step is taken.
    Poisoned,
}

impl From<LifecycleError> for StepError {
    fn from(error: LifecycleError) -> Self {
        Self::Lifecycle(error)
    }
}

/// Why an actor could not be spawned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnError {
    /// The mailbox bound was 0.
    ZeroBound,
    /// Actor ids are exhausted.
    IdsExhausted,
}

impl From<ZeroBound> for SpawnError {
    fn from(_: ZeroBound) -> Self {
        Self::ZeroBound
    }
}

#[derive(Debug)]
struct ActorRecord<A, M> {
    state: A,
    life: Lifecycle,
    mailbox: Mailbox<Envelope<M>>,
    answer: Answer<M>,
}

/// What a decision source may look at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    /// The actors that may run, in ascending id order.
    pub runnable: Vec<ActorId>,
    /// The current virtual time.
    pub now: Time,
    /// The earliest deadline of any waiting actor.
    pub next_deadline: Option<Time>,
}

/// The actor kernel.
///
/// Invariants: a mailbox never exceeds its bound; a message is delivered at
/// most once, and only from the mailbox it was accepted into; an actor is run
/// only while `Spawned` or `Runnable`; a parked actor is woken exactly once
/// per park; a reply slot is filled at most once; virtual time moves only on
/// an `Advance` decision and never backwards.
#[derive(Debug)]
pub struct Kernel<E: Executor> {
    executor: E,
    actors: Vec<ActorRecord<E::Actor, E::Message>>,
    slots: Vec<ReplySlot<E::Message>>,
    events: Vec<Event>,
    now: Time,
    next_message: u64,
    poisoned: bool,
}

fn index(id: u32) -> usize {
    usize::try_from(id).unwrap_or(usize::MAX)
}

impl<E: Executor> Kernel<E> {
    /// A kernel with no actors, driving `executor`.
    #[must_use]
    pub fn new(executor: E) -> Self {
        Self {
            executor,
            actors: Vec::new(),
            slots: Vec::new(),
            events: Vec::new(),
            now: Time(0),
            next_message: 0,
            poisoned: false,
        }
    }

    /// Creates an actor with a mailbox of `bound` messages (at least 1). The
    /// actor is `Spawned` and runs at its first scheduled turn.
    pub fn spawn(
        &mut self,
        actor: E::Actor,
        bound: usize,
    ) -> Result<Address, SpawnError> {
        let id =
            u32::try_from(self.actors.len()).map_err(|_| SpawnError::IdsExhausted)?;
        let mailbox = Mailbox::new(bound)?;
        self.actors.push(ActorRecord {
            state: actor,
            life: Lifecycle::Spawned,
            mailbox,
            answer: Answer::Start,
        });
        self.events.push(Event::Spawn {
            actor: ActorId(id),
            bound,
        });
        Ok(Address::new(ActorId(id)))
    }

    /// The executor.
    #[must_use]
    pub fn executor(&self) -> &E {
        &self.executor
    }

    /// The executor, mutably. Changing it never changes kernel state.
    pub fn executor_mut(&mut self) -> &mut E {
        &mut self.executor
    }

    /// An actor's state.
    #[must_use]
    pub fn actor(&self, id: ActorId) -> Option<&E::Actor> {
        self.actors.get(index(id.0)).map(|record| &record.state)
    }

    /// An actor's lifecycle state.
    #[must_use]
    pub fn lifecycle(&self, id: ActorId) -> Option<Lifecycle> {
        self.actors.get(index(id.0)).map(|record| record.life)
    }

    /// The number of actors spawned.
    #[must_use]
    pub fn actor_count(&self) -> usize {
        self.actors.len()
    }

    /// The messages queued for an actor, oldest first; empty for an unknown
    /// actor.
    #[must_use]
    pub fn queued(&self, id: ActorId) -> Vec<MessageId> {
        self.actors
            .get(index(id.0))
            .map(|record| record.mailbox.iter().map(|e| e.id).collect())
            .unwrap_or_default()
    }

    /// An actor's mailbox bound.
    #[must_use]
    pub fn mailbox_bound(&self, id: ActorId) -> Option<usize> {
        self.actors
            .get(index(id.0))
            .map(|record| record.mailbox.bound())
    }

    /// The event record so far.
    #[must_use]
    pub fn events(&self) -> &[Event] {
        &self.events
    }

    /// The current virtual time.
    #[must_use]
    pub fn now(&self) -> Time {
        self.now
    }

    /// The actors that may run, in ascending id order.
    #[must_use]
    pub fn runnable(&self) -> Vec<ActorId> {
        self.actors
            .iter()
            .enumerate()
            .filter(|(_, record)| record.life.is_schedulable())
            .filter_map(|(i, _)| u32::try_from(i).ok().map(ActorId))
            .collect()
    }

    /// The actors that are waiting, in ascending id order.
    #[must_use]
    pub fn waiting(&self) -> Vec<ActorId> {
        self.actors
            .iter()
            .enumerate()
            .filter(|(_, record)| matches!(record.life, Lifecycle::Waiting(_)))
            .filter_map(|(i, _)| u32::try_from(i).ok().map(ActorId))
            .collect()
    }

    /// The earliest deadline of any waiting actor.
    #[must_use]
    pub fn next_deadline(&self) -> Option<Time> {
        self.actors
            .iter()
            .filter_map(|record| match record.life {
                Lifecycle::Waiting(on) => on.deadline(),
                _ => None,
            })
            .min()
    }

    /// What a decision source may look at.
    #[must_use]
    pub fn view(&self) -> View {
        View {
            runnable: self.runnable(),
            now: self.now,
            next_deadline: self.next_deadline(),
        }
    }

    /// Takes one decision.
    pub fn step(&mut self, decision: Decision) -> Result<(), StepError> {
        if self.poisoned {
            return Err(StepError::Poisoned);
        }
        let result = match decision {
            Decision::Run(actor) => self.run_actor(actor),
            Decision::Advance(ticks) => self.advance(ticks),
        };
        if matches!(
            result,
            Err(StepError::Request { .. } | StepError::Lifecycle(_))
        ) {
            self.poisoned = true;
        }
        result
    }

    fn request(actor: ActorId, error: RequestError) -> StepError {
        StepError::Request { actor, error }
    }

    fn record_mut(
        &mut self,
        id: ActorId,
    ) -> Result<&mut ActorRecord<E::Actor, E::Message>, RequestError> {
        self.actors
            .get_mut(index(id.0))
            .ok_or(RequestError::UnknownActor(id))
    }

    fn slot_mut(
        &mut self,
        id: SlotId,
    ) -> Result<&mut ReplySlot<E::Message>, RequestError> {
        self.slots
            .get_mut(index(id.0))
            .ok_or(RequestError::UnknownSlot(id))
    }

    fn deadline_after(
        &self,
        timeout: Option<u64>,
    ) -> Result<Option<Time>, RequestError> {
        match timeout {
            None => Ok(None),
            Some(ticks) => self
                .now
                .0
                .checked_add(ticks)
                .map(|t| Some(Time(t)))
                .ok_or(RequestError::TimeOverflow),
        }
    }

    /// Whether `decision` could be taken now. Changes nothing.
    pub fn check(&self, decision: Decision) -> Result<(), DecisionError> {
        match decision {
            Decision::Run(actor) => {
                let record = self
                    .actors
                    .get(index(actor.0))
                    .ok_or(DecisionError::UnknownActor(actor))?;
                if record.life.is_schedulable() {
                    Ok(())
                } else {
                    Err(DecisionError::NotRunnable {
                        actor,
                        state: record.life,
                    })
                }
            }
            Decision::Advance(0) => Err(DecisionError::ZeroAdvance),
            Decision::Advance(ticks) => self
                .now
                .0
                .checked_add(ticks)
                .map(|_| ())
                .ok_or(DecisionError::TimeOverflow),
        }
    }

    fn run_actor(&mut self, id: ActorId) -> Result<(), StepError> {
        self.check(Decision::Run(id)).map_err(StepError::Decision)?;
        let record = self
            .actors
            .get_mut(index(id.0))
            .ok_or(StepError::Decision(DecisionError::UnknownActor(id)))?;
        record.life = record.life.begin()?;
        let answer = std::mem::replace(&mut record.answer, Answer::Continue);
        let action = self.executor.run_to_action(&mut record.state, answer);
        self.apply(id, action)
    }

    fn advance(&mut self, ticks: u64) -> Result<(), StepError> {
        self.check(Decision::Advance(ticks))
            .map_err(StepError::Decision)?;
        let to = self.now.0.saturating_add(ticks);
        self.now = Time(to);
        self.events.push(Event::Advance { to: self.now });
        let mut due: Vec<(Time, ActorId, WaitOn)> = Vec::new();
        for (i, record) in self.actors.iter().enumerate() {
            if let Lifecycle::Waiting(on) = record.life
                && let Some(deadline) = on.deadline()
                && deadline <= self.now
                && let Ok(i) = u32::try_from(i)
            {
                due.push((deadline, ActorId(i), on));
            }
        }
        due.sort_by_key(|(deadline, actor, _)| (*deadline, *actor));
        for (deadline, actor, on) in due {
            self.time_out(actor, deadline, on)?;
        }
        Ok(())
    }

    fn time_out(
        &mut self,
        actor: ActorId,
        deadline: Time,
        on: WaitOn,
    ) -> Result<(), StepError> {
        self.events.push(Event::Timeout { actor, deadline });
        let answer = match on {
            WaitOn::Receive { .. } => Answer::ReceiveTimedOut,
            WaitOn::Reply { .. } => Answer::Awaited(Awaited::TimedOut),
        };
        self.wake(actor, WakeCause::TimedOut, answer)
    }

    fn wake(
        &mut self,
        actor: ActorId,
        cause: WakeCause,
        answer: Answer<E::Message>,
    ) -> Result<(), StepError> {
        let record = self
            .record_mut(actor)
            .map_err(|error| Self::request(actor, error))?;
        record.life = record.life.wake()?;
        record.answer = answer;
        self.events.push(Event::Wake { actor, cause });
        Ok(())
    }

    fn resume(
        &mut self,
        actor: ActorId,
        answer: Answer<E::Message>,
    ) -> Result<(), StepError> {
        let record = self
            .record_mut(actor)
            .map_err(|error| Self::request(actor, error))?;
        record.life = record.life.pause()?;
        record.answer = answer;
        Ok(())
    }

    fn park(&mut self, actor: ActorId, on: WaitOn) -> Result<(), StepError> {
        let record = self
            .record_mut(actor)
            .map_err(|error| Self::request(actor, error))?;
        record.life = record.life.park(on)?;
        self.events.push(Event::Park { actor, on });
        if let Some(deadline) = on.deadline()
            && deadline <= self.now
        {
            self.time_out(actor, deadline, on)?;
        }
        Ok(())
    }

    fn apply(
        &mut self,
        actor: ActorId,
        action: Action<E::Message>,
    ) -> Result<(), StepError> {
        match action {
            Action::Send { to, message, reply } => self.send(actor, to, message, reply),
            Action::Receive { timeout } => self.receive(actor, timeout),
            Action::NewSlot => {
                let id = u32::try_from(self.slots.len())
                    .map_err(|_| Self::request(actor, RequestError::IdsExhausted))?;
                self.slots.push(ReplySlot::new(actor));
                self.events.push(Event::NewSlot {
                    owner: actor,
                    slot: SlotId(id),
                });
                self.resume(actor, Answer::Slot(SlotId(id)))
            }
            Action::Wait { slot, timeout } => self.wait(actor, slot, timeout),
            Action::Reply { slot, message } => self.reply(actor, slot, message),
            Action::Host(call) => {
                self.events.push(Event::Host { actor, call });
                self.resume(actor, Answer::Continue)
            }
            Action::Exit => self.exit(actor, ExitKind::Exited),
            Action::Trap(code) => self.exit(actor, ExitKind::Trapped(code)),
        }
    }

    fn send(
        &mut self,
        from: ActorId,
        to: Address,
        message: E::Message,
        reply: Option<SlotId>,
    ) -> Result<(), StepError> {
        let target = to.actor();
        let receiver_life = self
            .actors
            .get(index(target.0))
            .map(|record| record.life)
            .ok_or_else(|| Self::request(from, RequestError::UnknownActor(target)))?;
        if let Some(slot_id) = reply {
            let slot = self
                .slot_mut(slot_id)
                .map_err(|error| Self::request(from, error))?;
            if slot.owner != from {
                return Err(Self::request(from, RequestError::NotSlotOwner(slot_id)));
            }
            if slot.attached {
                return Err(Self::request(
                    from,
                    RequestError::SlotAlreadyAttached(slot_id),
                ));
            }
        }
        let message_id = MessageId(self.next_message);
        let outcome = if !admits(&to, target, &message) {
            SendOutcome::Refused(Refusal::NotAdmitted)
        } else if matches!(receiver_life, Lifecycle::Exited(_)) {
            SendOutcome::Refused(Refusal::Exited)
        } else {
            let next = self
                .next_message
                .checked_add(1)
                .ok_or_else(|| Self::request(from, RequestError::IdsExhausted))?;
            let envelope = Envelope {
                id: message_id,
                from,
                message,
                reply,
            };
            let record = self
                .record_mut(target)
                .map_err(|error| Self::request(from, error))?;
            match record.mailbox.push(envelope) {
                Push::Accepted => {
                    self.next_message = next;
                    SendOutcome::Accepted(message_id)
                }
                Push::Full(_) => SendOutcome::Refused(Refusal::Full),
            }
        };
        match outcome {
            SendOutcome::Accepted(message) => {
                self.events.push(Event::SendAccepted {
                    from,
                    to: target,
                    message,
                });
                if let Some(slot_id) = reply {
                    self.slot_mut(slot_id)
                        .map_err(|error| Self::request(from, error))?
                        .attached = true;
                }
                if matches!(receiver_life, Lifecycle::Waiting(WaitOn::Receive { .. })) {
                    self.hand_to_waiting_receiver(target)?;
                }
            }
            SendOutcome::Refused(reason) => self.events.push(Event::SendRefused {
                from,
                to: target,
                reason,
            }),
        }
        self.resume(from, Answer::Sent(outcome))
    }

    /// Pops the oldest message for `actor` and records its delivery.
    fn deliver(
        &mut self,
        actor: ActorId,
    ) -> Result<Option<Envelope<E::Message>>, StepError> {
        let record = self
            .record_mut(actor)
            .map_err(|error| Self::request(actor, error))?;
        let Some(envelope) = record.mailbox.pop() else {
            return Ok(None);
        };
        self.events.push(Event::Deliver {
            actor,
            message: envelope.id,
            from: envelope.from,
        });
        if let Some(slot_id) = envelope.reply {
            self.slot_mut(slot_id)
                .map_err(|error| Self::request(actor, error))?
                .holder = Some(actor);
        }
        Ok(Some(envelope))
    }

    fn hand_to_waiting_receiver(&mut self, actor: ActorId) -> Result<(), StepError> {
        if let Some(envelope) = self.deliver(actor)? {
            self.wake(actor, WakeCause::Delivery, Answer::Received(envelope))?;
        }
        Ok(())
    }

    fn receive(
        &mut self,
        actor: ActorId,
        timeout: Option<u64>,
    ) -> Result<(), StepError> {
        if let Some(envelope) = self.deliver(actor)? {
            return self.resume(actor, Answer::Received(envelope));
        }
        let deadline = self
            .deadline_after(timeout)
            .map_err(|error| Self::request(actor, error))?;
        self.park(actor, WaitOn::Receive { deadline })
    }

    fn wait(
        &mut self,
        actor: ActorId,
        slot_id: SlotId,
        timeout: Option<u64>,
    ) -> Result<(), StepError> {
        let deadline = self
            .deadline_after(timeout)
            .map_err(|error| Self::request(actor, error))?;
        let slot = self
            .slot_mut(slot_id)
            .map_err(|error| Self::request(actor, error))?;
        if slot.owner != actor {
            return Err(Self::request(actor, RequestError::NotSlotOwner(slot_id)));
        }
        match slot.take() {
            Take::Ready(awaited) => self.resume(actor, Answer::Awaited(awaited)),
            Take::Consumed => {
                Err(Self::request(actor, RequestError::SlotConsumed(slot_id)))
            }
            Take::Pending => self.park(
                actor,
                WaitOn::Reply {
                    slot: slot_id,
                    deadline,
                },
            ),
        }
    }

    fn reply(
        &mut self,
        actor: ActorId,
        slot_id: SlotId,
        message: E::Message,
    ) -> Result<(), StepError> {
        let owner = {
            let slot = self
                .slot_mut(slot_id)
                .map_err(|error| Self::request(actor, error))?;
            if slot.holder != Some(actor) {
                return Err(Self::request(actor, RequestError::NotSlotHolder(slot_id)));
            }
            slot.owner
        };
        let owner_alive = self
            .actors
            .get(index(owner.0))
            .is_some_and(|record| !matches!(record.life, Lifecycle::Exited(_)));
        let outcome = self
            .slot_mut(slot_id)
            .map_err(|error| Self::request(actor, error))?
            .reply(message, owner_alive);
        self.events.push(Event::Reply {
            actor,
            slot: slot_id,
            outcome,
        });
        if outcome == ReplyOutcome::Accepted {
            self.wake_slot_waiter(owner, slot_id, WakeCause::Reply)?;
        }
        self.resume(actor, Answer::Replied(outcome))
    }

    /// Wakes `owner` if it is parked on `slot_id`, handing it what the slot
    /// holds.
    fn wake_slot_waiter(
        &mut self,
        owner: ActorId,
        slot_id: SlotId,
        cause: WakeCause,
    ) -> Result<(), StepError> {
        let waits_here = self.actors.get(index(owner.0)).is_some_and(|record| {
            matches!(record.life, Lifecycle::Waiting(WaitOn::Reply { slot, .. }) if slot == slot_id)
        });
        if !waits_here {
            return Ok(());
        }
        let taken = self
            .slot_mut(slot_id)
            .map_err(|error| Self::request(owner, error))?
            .take();
        match taken {
            Take::Ready(awaited) => self.wake(owner, cause, Answer::Awaited(awaited)),
            Take::Pending | Take::Consumed => Ok(()),
        }
    }

    fn exit(&mut self, actor: ActorId, kind: ExitKind) -> Result<(), StepError> {
        let record = self
            .record_mut(actor)
            .map_err(|error| Self::request(actor, error))?;
        record.life = record.life.exit(kind)?;
        let queued: BTreeSet<SlotId> = record
            .mailbox
            .drain()
            .filter_map(|envelope| envelope.reply)
            .collect();
        self.events.push(Event::Exit { actor, kind });
        let mut closed = Vec::new();
        for (i, slot) in self.slots.iter_mut().enumerate() {
            let Ok(i) = u32::try_from(i) else { continue };
            let id = SlotId(i);
            if (slot.holder == Some(actor) || queued.contains(&id)) && slot.close() {
                closed.push((slot.owner, id));
            }
        }
        for (owner, id) in closed {
            self.wake_slot_waiter(owner, id, WakeCause::SlotClosed)?;
        }
        Ok(())
    }
}
