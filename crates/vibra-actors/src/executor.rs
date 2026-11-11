//! The seam between the kernel and whatever runs an actor's code.

use crate::address::Address;
use crate::events::SendOutcome;
use crate::ids::{ActorId, MessageId, SlotId};
use crate::reply::{Awaited, ReplyOutcome};

/// A message as the receiver sees it. The kernel never inspects `message`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Envelope<M> {
    /// The accepted message's id.
    pub id: MessageId,
    /// The sender.
    pub from: ActorId,
    /// The payload.
    pub message: M,
    /// The reply slot the receiver now owes a reply to, if the sender
    /// attached one.
    pub reply: Option<SlotId>,
}

/// What an actor asks of the kernel when it stops. Time limits are in
/// virtual ticks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action<M> {
    /// Send `message` to `to`, optionally attaching a reply slot the sender
    /// owns. Answered with [`Answer::Sent`].
    Send {
        /// The receiver's address.
        to: Address,
        /// The payload.
        message: M,
        /// A slot owned by the sender that the receiver may reply to.
        reply: Option<SlotId>,
    },
    /// Take the oldest message, parking while the mailbox is empty. Answered
    /// with [`Answer::Received`] or [`Answer::ReceiveTimedOut`].
    Receive {
        /// Ticks after which the wait gives up, if any.
        timeout: Option<u64>,
    },
    /// Create a reply slot owned by the actor. Answered with
    /// [`Answer::Slot`].
    NewSlot,
    /// Wait for the reply to a slot the actor owns. Answered with
    /// [`Answer::Awaited`].
    Wait {
        /// The slot.
        slot: SlotId,
        /// Ticks after which the wait gives up, if any.
        timeout: Option<u64>,
    },
    /// Reply to a slot the actor holds. Answered with [`Answer::Replied`].
    Reply {
        /// The slot.
        slot: SlotId,
        /// The payload.
        message: M,
    },
    /// A host operation, recorded with an opaque tag. Answered with
    /// [`Answer::Continue`].
    Host(u64),
    /// End the actor normally.
    Exit,
    /// End the actor with a trap code; no other actor is affected.
    Trap(u32),
}

/// What the kernel tells an actor when it runs again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answer<M> {
    /// The first activation.
    Start,
    /// A host action was recorded.
    Continue,
    /// The result of a send.
    Sent(SendOutcome),
    /// The message a receive took.
    Received(Envelope<M>),
    /// A receive reached its deadline with an empty mailbox.
    ReceiveTimedOut,
    /// The slot a `NewSlot` created.
    Slot(SlotId),
    /// What a wait on a slot observed.
    Awaited(Awaited<M>),
    /// What a reply did.
    Replied(ReplyOutcome),
}

/// Runs actor code up to its next kernel action.
pub trait Executor {
    /// The state of one actor: whatever the executor needs to continue it.
    type Actor;
    /// The payload of a message. The kernel moves it and never inspects it.
    type Message;

    /// Runs `actor` until it needs the kernel. `answer` is the kernel's
    /// answer to the action the actor stopped at, or [`Answer::Start`] the
    /// first time.
    ///
    /// The kernel calls this only for an actor that is not waiting, not
    /// exited, and not already inside this call.
    fn run_to_action(
        &mut self,
        actor: &mut Self::Actor,
        answer: Answer<Self::Message>,
    ) -> Action<Self::Message>;
}
