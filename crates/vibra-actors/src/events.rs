//! The event record: what the kernel did, in order.

use crate::ids::{ActorId, MessageId, SlotId, Time};
use crate::lifecycle::{ExitKind, WaitOn};
use crate::reply::ReplyOutcome;

/// Why a send was refused. Provisional until X4-S1 is approved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The receiver's mailbox was at its bound.
    Full,
    /// The receiver had exited.
    Exited,
    /// The address did not admit the message.
    NotAdmitted,
}

/// What a send did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendOutcome {
    /// The message was queued (or handed straight to a waiting receiver).
    Accepted(MessageId),
    /// The message was dropped for this reason.
    Refused(Refusal),
}

/// Why a parked actor became runnable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WakeCause {
    /// A message was delivered to it.
    Delivery,
    /// The reply it waited for arrived.
    Reply,
    /// The actor that owed its reply exited.
    SlotClosed,
    /// Its wait reached its virtual deadline.
    TimedOut,
}

/// One thing the kernel did. Events carry no payload, so two records compare
/// equal exactly when the kernel took the same steps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// An actor was created.
    Spawn {
        /// The new actor.
        actor: ActorId,
        /// Its mailbox bound.
        bound: usize,
    },
    /// Virtual time moved.
    Advance {
        /// The new time.
        to: Time,
    },
    /// A send was accepted.
    SendAccepted {
        /// The sender.
        from: ActorId,
        /// The receiver.
        to: ActorId,
        /// The accepted message.
        message: MessageId,
    },
    /// A send was refused.
    SendRefused {
        /// The sender.
        from: ActorId,
        /// The receiver.
        to: ActorId,
        /// Why.
        reason: Refusal,
    },
    /// A message left a mailbox for its receiver.
    Deliver {
        /// The receiver.
        actor: ActorId,
        /// The delivered message.
        message: MessageId,
        /// Its sender.
        from: ActorId,
    },
    /// An actor parked.
    Park {
        /// The actor.
        actor: ActorId,
        /// What it waits on.
        on: WaitOn,
    },
    /// A parked actor became runnable.
    Wake {
        /// The actor.
        actor: ActorId,
        /// Why.
        cause: WakeCause,
    },
    /// A parked actor's wait reached its deadline; a `Wake` follows.
    Timeout {
        /// The actor.
        actor: ActorId,
        /// The deadline that was reached.
        deadline: Time,
    },
    /// An actor created a reply slot.
    NewSlot {
        /// The owner.
        owner: ActorId,
        /// The slot.
        slot: SlotId,
    },
    /// An actor replied to a slot.
    Reply {
        /// The replier.
        actor: ActorId,
        /// The slot.
        slot: SlotId,
        /// What the reply did.
        outcome: ReplyOutcome,
    },
    /// An actor performed a host action.
    Host {
        /// The actor.
        actor: ActorId,
        /// The opaque tag the executor gave the call.
        call: u64,
    },
    /// An actor ended.
    Exit {
        /// The actor.
        actor: ActorId,
        /// How.
        kind: ExitKind,
    },
}
