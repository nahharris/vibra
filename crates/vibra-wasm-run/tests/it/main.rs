//! The `vibra-wasm-run` integration tests, one target: a module for each
//! subject, a `support` module of hand-built modules, and a `programs`
//! module of hand-built checked programs.
//!
//! Run them with `cargo test --locked -p vibra-wasm-run --test it`.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

mod emitted;
mod native_faults;
mod programs;
mod runner_protocol;
mod support;
mod unsupported;
mod validation;
