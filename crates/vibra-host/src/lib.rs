//! The host of the Vibra runtime: the broker that holds the grants of a
//! run, the drivers that act for it, the host world, the audit trace, and
//! startup.
//!
//! `docs/spec/06-runtime.md`, "Host calls", "Host world", "Audit trace", and
//! "Entry outcome", and `docs/spec/03-effects.md`, "Capability values" and
//! "Scope policies", fix what this crate does. The operations it dispatches
//! are the rows of the host registry of major 2 and of the test-world
//! registry, in `vibra_ir::external::host`.
//!
//! [`SplitMix64`] is the deterministic generator of a host world.
//!
//! # Position in the architecture
//!
//! No crate of the language pipeline depends on this one: it reads the
//! registries of `vibra-ir` and the scoped filesystem of `vibra-fs-scope`.
//! `vibra-workspace` is the one crate that holds this crate's broker and an
//! interpreter at once.

pub mod broker;
pub mod driver;
pub mod path;
pub mod policy;
pub mod recipe;
pub mod startup;
pub mod system;
pub mod test_world;
pub mod trace;
pub mod world;

mod random;

pub use random::SplitMix64;
