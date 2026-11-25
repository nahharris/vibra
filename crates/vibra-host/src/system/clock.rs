//! The clock adapters: `time.nowWall` and `time.nowMonotonic`.
//!
//! `docs/spec/06-runtime.md`, "Clocks and randomness": the wall clock is
//! `I64` nanoseconds since the Unix epoch in UTC, ignoring leap seconds; the
//! monotonic clock is `U64` nanoseconds from an origin fixed for the
//! instance, and never decreases within it; a value that does not fit its
//! type is `unavailable`. Neither moves a byte.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use vibra_ir::authority::HostOk;

use super::Answer;
use crate::driver::Progress;

/// `time.nowWall`.
pub(super) fn wall() -> Answer {
    let nanoseconds = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(after) => i64::try_from(after.as_nanos()).ok(),
        // A time before the epoch is a negative count.
        Err(before) => i128::try_from(before.duration().as_nanos())
            .ok()
            .and_then(|nanoseconds| i64::try_from(-nanoseconds).ok()),
    };
    match nanoseconds {
        Some(nanoseconds) => Answer::Ok(HostOk::I64(nanoseconds), Progress::NONE),
        None => Answer::Err("unavailable", Progress::NONE),
    }
}

/// One acquired monotonic clock: its origin, taken when it was acquired,
/// and the last value it returned.
#[derive(Clone, Copy, Debug)]
pub(super) struct Monotonic {
    origin: Instant,
    last: u64,
}

impl Monotonic {
    /// A clock whose origin is now.
    pub(super) fn start() -> Self {
        Self {
            origin: Instant::now(),
            last: 0,
        }
    }

    /// `time.nowMonotonic`: the nanoseconds since the origin, never less
    /// than the last value returned.
    pub(super) fn read(&mut self) -> Answer {
        let elapsed = Instant::now().saturating_duration_since(self.origin);
        match u64::try_from(elapsed.as_nanos()) {
            Ok(now) => {
                self.last = self.last.max(now);
                Answer::Ok(HostOk::U64(self.last), Progress::NONE)
            }
            Err(_) => Answer::Err("unavailable", Progress::NONE),
        }
    }
}
