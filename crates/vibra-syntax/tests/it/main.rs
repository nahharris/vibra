//! The `vibra-syntax` integration tests, one target with a module for each
//! subject.
//!
//! Run them with `cargo test --locked -p vibra-syntax --test it`.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

mod data;
mod declarations;
mod expressions;
mod literals;
mod names;
mod queries;
mod reader;
