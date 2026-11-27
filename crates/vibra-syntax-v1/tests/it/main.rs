//! The `vibra-syntax-v1` integration tests, one target with a module for each
//! subject.
//!
//! Run them with `cargo test --locked -p vibra-syntax-v1 --test it`.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

mod data;
mod literals;
mod names;
mod reader;
