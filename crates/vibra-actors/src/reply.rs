//! One-shot reply slots.

use crate::ids::ActorId;

/// What a reply did. Provisional until X4-S1 is approved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplyOutcome {
    /// The reply filled the slot.
    Accepted,
    /// The slot already had a reply; the first stands and this one is dropped.
    AlreadyReplied,
    /// The actor that owns the slot has exited; the reply is dropped.
    OwnerExited,
}

/// What a wait on a reply slot observed. Provisional until X4-S1 is approved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Awaited<M> {
    /// The reply.
    Reply(M),
    /// The actor that owed the reply exited without sending it.
    Exited,
    /// The wait reached its virtual deadline first.
    TimedOut,
}

#[derive(Debug)]
enum State<M> {
    Empty,
    Filled(M),
    Closed,
    Consumed,
}

/// The result of taking from a slot.
#[derive(Debug)]
pub(crate) enum Take<M> {
    Ready(Awaited<M>),
    Pending,
    Consumed,
}

/// A slot that holds at most one reply.
///
/// Invariants: a slot is filled at most once; a filled or closed slot is
/// taken at most once; the holder is set only when the message that carries
/// the slot is delivered.
#[derive(Debug)]
pub(crate) struct ReplySlot<M> {
    pub(crate) owner: ActorId,
    pub(crate) holder: Option<ActorId>,
    pub(crate) attached: bool,
    state: State<M>,
}

impl<M> ReplySlot<M> {
    pub(crate) fn new(owner: ActorId) -> Self {
        Self {
            owner,
            holder: None,
            attached: false,
            state: State::Empty,
        }
    }

    pub(crate) fn reply(&mut self, message: M, owner_alive: bool) -> ReplyOutcome {
        if !owner_alive {
            return ReplyOutcome::OwnerExited;
        }
        match self.state {
            State::Empty => {
                self.state = State::Filled(message);
                ReplyOutcome::Accepted
            }
            _ => ReplyOutcome::AlreadyReplied,
        }
    }

    pub(crate) fn take(&mut self) -> Take<M> {
        match std::mem::replace(&mut self.state, State::Consumed) {
            State::Filled(message) => Take::Ready(Awaited::Reply(message)),
            State::Closed => Take::Ready(Awaited::Exited),
            State::Empty => {
                self.state = State::Empty;
                Take::Pending
            }
            State::Consumed => Take::Consumed,
        }
    }

    /// Closes an empty slot; returns whether it was empty.
    pub(crate) fn close(&mut self) -> bool {
        if matches!(self.state, State::Empty) {
            self.state = State::Closed;
            true
        } else {
            false
        }
    }
}
