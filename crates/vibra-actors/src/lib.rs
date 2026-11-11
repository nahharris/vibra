//! The backend-neutral actor kernel and its simulation scheduler.
//!
//! One implementation of actor identity, bounded mailboxes, one-shot reply
//! slots, exit, virtual-time timeouts, and the event record, driven through an
//! [`Executor`] that runs an actor's code up to its next kernel action. The
//! kernel never inspects a message payload, reads no clock, starts no thread,
//! and has no dependency.
//!
//! # Model
//!
//! An actor is a receive loop. Its activation runs until it performs a kernel
//! action ([`Action`]); the kernel answers it ([`Answer`]) and the scheduler
//! picks what happens next. A receive on an empty mailbox parks the actor
//! until a delivery, and a wait on a reply slot parks it until the reply, the
//! exit of the actor that owed it, or a timeout.
//!
//! A schedule is the sequence of [`Decision`]s the scheduler took at kernel
//! actions: which runnable actor acts next, or how far virtual time advances.
//! It records no quota and no step count. [`run`] takes each decision from a
//! [`Recorded`] list or a [`Seeded`] generator and returns the decisions it
//! took, so a run can be replayed and its [`Event`] record compared.
//!
//! # Provisional outcomes
//!
//! The typed outcomes here (a full mailbox refuses a send, a send to an exited
//! actor is refused, a second reply is refused and the first stands, a waiter
//! is woken with an exit outcome when the actor that owed its reply exits, a
//! timeout fires at its virtual deadline) are working assumptions until the
//! actor specification (X4-S1) is approved. Address scopes and the schedule
//! document format are open and are not implemented: see [`Address`].

mod address;
mod events;
mod executor;
mod ids;
mod kernel;
mod lifecycle;
mod mailbox;
mod reply;
mod schedule;

pub use address::Address;
pub use events::{Event, Refusal, SendOutcome, WakeCause};
pub use executor::{Action, Answer, Envelope, Executor};
pub use ids::{ActorId, MessageId, SlotId, Time};
pub use kernel::{
    Decision, DecisionError, Kernel, RequestError, SpawnError, StepError, View,
};
pub use lifecycle::{ExitKind, Lifecycle, LifecycleError, WaitOn};
pub use mailbox::{Mailbox, Push, ZeroBound};
pub use reply::{Awaited, ReplyOutcome};
pub use schedule::{
    DecisionSource, Recorded, Report, RunEnd, RunError, Seeded, SplitMix64, run,
    run_observed,
};
