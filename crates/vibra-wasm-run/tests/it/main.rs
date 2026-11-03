//! The `vibra-wasm-run` integration tests, one target: a module for each
//! subject and a `support` module of hand-built modules.
//!
//! Run them with `cargo test --locked -p vibra-wasm-run --test it`.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

mod native_faults;
mod runner_protocol;
mod support;
mod validation;
