//! Build [`WasiEnvBuilder`](wasmer_wasix::WasiEnvBuilder): stdio inheritance, argv, preopened dirs.

use crate::async_runtime::{CapabilityGrant, ScopeLimits};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use wasmer_wasix::{WasiEnv, WasiEnvBuilder, WasiStateCreationError};

/// Configuration for [`super::run_module`](crate::runtime::run_module).
#[derive(Debug, Clone)]
pub struct RunConfig {
    /// `argv[0]`-style program name visible to WASI.
    pub program_name: String,
    /// Extra argv entries after `program_name` (MVP: often empty).
    pub argv: Vec<String>,
    /// Host directories preopened at the WASI virtual root (`/`).
    pub preopen_host_dirs: Vec<PathBuf>,
    /// Optional isolated environment used by embedders and tests. When set,
    /// host environment operations read and mutate this map instead of the
    /// runner process environment.
    pub injected_environment: Option<Arc<Mutex<std::collections::BTreeMap<String, String>>>>,
    /// Optional deterministic clock used by embedders and tests.
    pub injected_clock: Option<Arc<Mutex<InjectedClock>>>,
    /// Optional deterministic byte source used by embedders and tests.
    pub injected_random: Option<Arc<Mutex<InjectedRandom>>>,
    /// Maximum byte length the runtime will allocate for a single
    /// program-controlled buffer (e.g. `read-raw`, `random.bytes`). Guards
    /// against user-controlled out-of-memory via unbounded length arguments.
    pub max_alloc_len: usize,
    /// Maximum number of concurrently open owned host-resource handles the
    /// runtime may hold (excluding borrowed standard streams). `0` means
    /// unlimited. Opening past this cap yields a matchable `too-many-open-files`
    /// filesystem error instead of exhausting OS file descriptors.
    pub max_open_files: usize,
    /// Program-level host authority. `Some` is an explicit authority policy,
    /// including `Some(vec![])` for a fail-closed manifest; `None` is retained
    /// for direct module/embedding calls that have no project manifest.
    pub capability_grants: Option<Vec<CapabilityGrant>>,
    /// Coarse execution budgets. Capability authority remains independent
    /// from these limits.
    pub scope_limits: ScopeLimits,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            program_name: "vibra".to_string(),
            argv: Vec::new(),
            preopen_host_dirs: Vec::new(),
            injected_environment: None,
            injected_clock: None,
            injected_random: None,
            max_alloc_len: 64 * 1024 * 1024,
            max_open_files: 1024,
            capability_grants: None,
            scope_limits: ScopeLimits::default(),
        }
    }
}

/// Deterministic wall and monotonic clock state for an isolated run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InjectedClock {
    pub unix_millis: u64,
    pub monotonic_millis: u64,
}

/// Small, reproducible pseudo-random source for tests and embedding fixtures.
///
/// This is deliberately not cryptographic. Production randomness continues to
/// use the operating system unless an embedder explicitly injects this source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InjectedRandom {
    state: u64,
    buffered: [u8; 8],
    offset: usize,
}

impl InjectedRandom {
    pub fn new(seed: u64) -> Self {
        Self {
            // SplitMix64 permits zero, but offsetting the initial state also
            // keeps the seed itself out of the first output word.
            state: seed,
            buffered: [0; 8],
            offset: 8,
        }
    }

    pub fn fill_bytes(&mut self, output: &mut [u8]) {
        let mut written = 0;
        while written < output.len() {
            if self.offset == self.buffered.len() {
                self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
                let mut word = self.state;
                word = (word ^ (word >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
                word = (word ^ (word >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
                word ^= word >> 31;
                self.buffered = word.to_le_bytes();
                self.offset = 0;
            }
            let available = self.buffered.len() - self.offset;
            let count = available.min(output.len() - written);
            output[written..written + count]
                .copy_from_slice(&self.buffered[self.offset..self.offset + count]);
            self.offset += count;
            written += count;
        }
    }

    pub(crate) fn state_for_worker(&self) -> u64 {
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::InjectedRandom;

    #[test]
    fn injected_random_is_seeded_reproducibly_and_streams_across_calls() {
        let mut first = InjectedRandom::new(42);
        let mut prefix = [0; 8];
        let mut suffix = [0; 5];
        first.fill_bytes(&mut prefix);
        first.fill_bytes(&mut suffix);

        assert_eq!(prefix, [149, 110, 235, 47, 38, 50, 215, 189]);

        let mut replay = InjectedRandom::new(42);
        let mut first_part = [0; 3];
        let mut second_part = [0; 10];
        replay.fill_bytes(&mut first_part);
        replay.fill_bytes(&mut second_part);
        let mut combined = Vec::from(first_part);
        combined.extend(second_part);
        assert_eq!(&combined[..8], &prefix);
        assert_eq!(&combined[8..], &suffix);

        let mut other = InjectedRandom::new(43);
        let mut other_prefix = [0; 8];
        other.fill_bytes(&mut other_prefix);
        assert_ne!(other_prefix, prefix);
    }
}

/// Construct a [`WasiEnvBuilder`] from [`RunConfig`].
pub fn build_wasi_builder(config: RunConfig) -> Result<WasiEnvBuilder, WasiStateCreationError> {
    let mut builder = WasiEnv::builder(config.program_name);
    if !config.argv.is_empty() {
        builder.add_args(config.argv);
    }

    for d in config.preopen_host_dirs {
        builder = builder.preopen_dir(d)?;
    }

    Ok(builder)
}
