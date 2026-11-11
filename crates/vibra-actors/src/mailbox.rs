//! Bounded first-in first-out mailboxes.

use std::collections::VecDeque;

/// The error of asking for a mailbox that could hold nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZeroBound;

/// The result of pushing onto a mailbox.
#[derive(Debug, PartialEq, Eq)]
pub enum Push<T> {
    /// The item was queued at the back.
    Accepted,
    /// The mailbox was at its bound; the item is handed back and nothing
    /// changed.
    Full(T),
}

/// A first-in first-out queue whose length never exceeds its bound.
///
/// Invariants: the bound is at least 1 and is fixed at creation; `len` is
/// never greater than `bound`; items leave in the order they entered.
#[derive(Debug)]
pub struct Mailbox<T> {
    queue: VecDeque<T>,
    bound: usize,
}

impl<T> Mailbox<T> {
    /// A mailbox holding at most `bound` items, or [`ZeroBound`] when `bound`
    /// is 0.
    pub fn new(bound: usize) -> Result<Self, ZeroBound> {
        if bound == 0 {
            return Err(ZeroBound);
        }
        Ok(Self {
            queue: VecDeque::new(),
            bound,
        })
    }

    /// Queues `item` at the back unless the mailbox is at its bound.
    pub fn push(&mut self, item: T) -> Push<T> {
        if self.queue.len() >= self.bound {
            return Push::Full(item);
        }
        self.queue.push_back(item);
        Push::Accepted
    }

    /// Removes and returns the oldest item.
    pub fn pop(&mut self) -> Option<T> {
        self.queue.pop_front()
    }

    /// The number of queued items.
    #[must_use]
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Whether nothing is queued.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// The most items the mailbox can hold.
    #[must_use]
    pub fn bound(&self) -> usize {
        self.bound
    }

    /// The queued items, oldest first.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.queue.iter()
    }

    /// Removes every queued item, oldest first.
    pub fn drain(&mut self) -> impl Iterator<Item = T> {
        self.queue.drain(..)
    }
}
