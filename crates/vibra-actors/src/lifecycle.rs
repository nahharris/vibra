//! The lifecycle state machine of one actor.
//!
//! | From | Transition | To |
//! | --- | --- | --- |
//! | `Spawned`, `Runnable` | `begin` | `Running` |
//! | `Running` | `pause` | `Runnable` |
//! | `Running` | `park(on)` | `Waiting(on)` |
//! | `Waiting` | `wake` | `Runnable` |
//! | `Running` | `exit(kind)` | `Exited(kind)` |
//!
//! Every other pair is a [`LifecycleError`]. `Exited` is terminal.

use crate::ids::{SlotId, Time};

/// What a waiting actor is parked on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaitOn {
    /// A receive on an empty mailbox.
    Receive {
        /// The virtual time at which the wait times out, if it has a limit.
        deadline: Option<Time>,
    },
    /// A reply slot that has no reply yet.
    Reply {
        /// The slot waited on.
        slot: SlotId,
        /// The virtual time at which the wait times out, if it has a limit.
        deadline: Option<Time>,
    },
}

impl WaitOn {
    /// The virtual time at which this wait times out, if it has a limit.
    #[must_use]
    pub fn deadline(self) -> Option<Time> {
        match self {
            Self::Receive { deadline } | Self::Reply { deadline, .. } => deadline,
        }
    }
}

/// How an actor ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitKind {
    /// The actor performed the exit action.
    Exited,
    /// The actor trapped with this code; no other actor is affected.
    Trapped(u32),
}

/// Where an actor is in its life.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lifecycle {
    /// Created and not yet run; it can be scheduled.
    Spawned,
    /// Ready to run with an answer pending; it can be scheduled.
    Runnable,
    /// Inside `run_to_action`; it cannot be scheduled.
    Running,
    /// Parked until a delivery, a reply, a closed slot, or a timeout.
    Waiting(WaitOn),
    /// Finished. Terminal.
    Exited(ExitKind),
}

/// A transition the table does not allow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LifecycleError {
    /// The state the actor was in.
    pub from: Lifecycle,
    /// The name of the refused transition.
    pub transition: &'static str,
}

impl Lifecycle {
    /// Whether the scheduler may run an actor in this state.
    #[must_use]
    pub fn is_schedulable(self) -> bool {
        matches!(self, Self::Spawned | Self::Runnable)
    }

    fn refuse(self, transition: &'static str) -> Result<Self, LifecycleError> {
        Err(LifecycleError {
            from: self,
            transition,
        })
    }

    /// `Spawned` or `Runnable` to `Running`.
    pub fn begin(self) -> Result<Self, LifecycleError> {
        if self.is_schedulable() {
            Ok(Self::Running)
        } else {
            self.refuse("begin")
        }
    }

    /// `Running` to `Runnable`.
    pub fn pause(self) -> Result<Self, LifecycleError> {
        match self {
            Self::Running => Ok(Self::Runnable),
            _ => self.refuse("pause"),
        }
    }

    /// `Running` to `Waiting(on)`.
    pub fn park(self, on: WaitOn) -> Result<Self, LifecycleError> {
        match self {
            Self::Running => Ok(Self::Waiting(on)),
            _ => self.refuse("park"),
        }
    }

    /// `Waiting` to `Runnable`.
    pub fn wake(self) -> Result<Self, LifecycleError> {
        match self {
            Self::Waiting(_) => Ok(Self::Runnable),
            _ => self.refuse("wake"),
        }
    }

    /// `Running` to `Exited(kind)`.
    pub fn exit(self, kind: ExitKind) -> Result<Self, LifecycleError> {
        match self {
            Self::Running => Ok(Self::Exited(kind)),
            _ => self.refuse("exit"),
        }
    }
}
