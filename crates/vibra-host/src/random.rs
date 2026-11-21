//! The deterministic generator of a host world: SplitMix64.
//!
//! `docs/spec/06-runtime.md`, "Host world": the generator is SplitMix64
//! started from `seed`. A call for `count` bytes takes `ceil(count / 8)` outputs
//! in turn, writes each as 8 little-endian bytes, and keeps the first `count`
//! of them. The state continues across calls, and the unused bytes of a call are
//! discarded.

/// Allocates a zero-filled vector of `count` bytes, or reports that the host
/// cannot.
fn try_zeroed(count: u64) -> Option<Vec<u8>> {
    let count = usize::try_from(count).ok()?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(count).ok()?;
    bytes.resize(count, 0);
    Some(bytes)
}

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

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::SplitMix64;

    fn outputs(seed: u64, count: u64) -> Option<Vec<u8>> {
        SplitMix64::new(seed).bytes(count)
    }

    fn le(values: &[u64]) -> Vec<u8> {
        values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect()
    }

    #[test]
    fn splitmix64_vectors_for_seed_zero() {
        let expected = le(&[
            0xE220_A839_7B1D_CDAF,
            0x6E78_9E6A_A1B9_65F4,
            0x06C4_5D18_8009_454F,
            0xF88B_B8A8_724C_81EC,
        ]);
        assert_eq!(outputs(0, 0), Some(Vec::new()));
        assert_eq!(outputs(0, 1), Some(expected[..1].to_vec()));
        assert_eq!(outputs(0, 8), Some(expected[..8].to_vec()));
        assert_eq!(outputs(0, 9), Some(expected[..9].to_vec()));
        assert_eq!(outputs(0, 32), Some(expected));
    }

    #[test]
    fn splitmix64_vectors_for_a_second_seed() {
        let expected = le(&[
            6_457_827_717_110_365_317,
            3_203_168_211_198_807_973,
            9_817_491_932_198_370_423,
            4_593_380_528_125_082_431,
            16_408_922_859_458_223_821,
        ]);
        assert_eq!(outputs(1_234_567, 40), Some(expected.clone()));
        assert_eq!(outputs(1_234_567, 17), Some(expected[..17].to_vec()));
    }

    #[test]
    fn the_state_continues_across_calls_and_discards_unused_bytes() {
        let all = le(&[
            0xE220_A839_7B1D_CDAF,
            0x6E78_9E6A_A1B9_65F4,
            0x06C4_5D18_8009_454F,
            0xF88B_B8A8_724C_81EC,
        ]);
        let mut generator = SplitMix64::new(0);
        // 3 bytes take one output; its other 5 bytes are discarded.
        assert_eq!(generator.bytes(3), Some(all[..3].to_vec()));
        // The next call starts at the second output.
        assert_eq!(generator.bytes(9), Some(all[8..17].to_vec()));
        // A call of 0 takes no output.
        assert_eq!(generator.bytes(0), Some(Vec::new()));
        assert_eq!(generator.next_u64(), 0xF88B_B8A8_724C_81EC);
    }
}
