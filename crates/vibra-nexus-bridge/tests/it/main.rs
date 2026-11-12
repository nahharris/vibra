//! Tests of the bridge: the proof cases of the foundation driven through the
//! `nexus_*` routines by hand-written step functions (standing in for generated
//! code), admission, the launcher's exit codes, and misuse.
//!
//! Run them with `cargo test --locked -p vibra-nexus-bridge --test it`.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    missing_docs,
    unreachable_pub
)]

mod admission;
mod demo;
#[cfg(feature = "harness")]
mod dump;
mod exit_codes;
mod fixture;
mod hygiene;
mod misuse;
mod shape;
mod stepper;
mod sys;
