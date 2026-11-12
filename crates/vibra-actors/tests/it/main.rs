//! The `vibra-actors` integration tests, all over a scripted executor: no
//! language is involved.
//!
//! Run them with `cargo test --locked -p vibra-actors --test it`.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    missing_docs
)]

mod support;

mod invariants;
mod kernel;
mod replay;
