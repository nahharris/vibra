//! Addresses, and the one place where a send is admitted.

use crate::ids::ActorId;

/// A handle that lets an actor send to another. Only the kernel mints one, at
/// spawn, so an address always names an actor of the kernel that made it.
///
/// The representation is minimal on purpose: it names an actor and nothing
/// else. What an address admits (operation sets, bound scopes, narrowing) is
/// open under decision O4 and is not part of this type yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Address {
    actor: ActorId,
}

impl Address {
    pub(crate) fn new(actor: ActorId) -> Self {
        Self { actor }
    }

    /// The actor this address names.
    #[must_use]
    pub fn actor(self) -> ActorId {
        self.actor
    }
}

/// Whether `address` admits sending `message` to `target`.
///
/// This is the single place the kernel asks the question, so the answer can
/// change without touching the scheduler. Its representation is open under
/// decision O4 (address scopes and capability policies); today an address
/// admits every message to the actor it names and nothing else.
pub(crate) fn admits<M>(address: &Address, target: ActorId, _message: &M) -> bool {
    address.actor == target
}
