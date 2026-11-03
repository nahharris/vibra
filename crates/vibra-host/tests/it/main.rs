//! The `vibra-host` integration tests, one target: a module for each subject
//! and a `support` module for what they share.
//!
//! Run them with `cargo test --locked -p vibra-host --test it`.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

mod audit_events;
mod env_clock_random;
mod fs_operations;
mod host_world;
mod support;
mod system_provider;
