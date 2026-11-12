//! The dense integer identities the kernel hands out.

macro_rules! dense_id {
    ($(#[$meta:meta])* $name:ident($inner:ty)) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub $inner);
    };
}

dense_id!(
    /// Names one actor. Ids are assigned in spawn order starting at 0 and are
    /// never reused, so an id names the same actor for the whole run.
    ActorId(u32)
);
dense_id!(
    /// Names one reply slot. Ids are assigned in creation order starting at 0
    /// and are never reused.
    SlotId(u32)
);
dense_id!(
    /// Names one accepted message. Ids are assigned in acceptance order
    /// starting at 0; a refused send takes none.
    MessageId(u64)
);
dense_id!(
    /// A point of virtual time. Time starts at 0 and moves only when the
    /// scheduler decides to advance it.
    Time(u64)
);
