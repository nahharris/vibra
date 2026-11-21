//! The deterministic generator of a host world: SplitMix64.
//!
//! `docs/spec/06-runtime.md`, "The host world": the generator is SplitMix64
//! started from `seed`. A call for `count` bytes takes `ceil(count / 8)` outputs
//! in turn, writes each as 8 little-endian bytes, and keeps the first `count`
//! of them. The state continues across calls, and the unused bytes of a call are
//! discarded.

use crate::provider::try_zeroed;

/// SplitMix64.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// A generator whose state is `seed`.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// The next 64-bit output.
    pub const fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Exactly `count` bytes, or `None` when the host cannot allocate them.
    pub fn bytes(&mut self, count: u64) -> Option<Vec<u8>> {
        let mut bytes = try_zeroed(count)?;
        for chunk in bytes.chunks_mut(8) {
            let output = self.next_u64().to_le_bytes();
            let length = chunk.len();
            chunk.copy_from_slice(output.get(..length)?);
        }
        Some(bytes)
    }
}
