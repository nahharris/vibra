//! The `vibra-fmt` integration tests, one target with a module for each
//! subject.
//!
//! Run them with `cargo test --locked -p vibra-fmt --test it`.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

mod comment_layout;
mod data;
mod deep_nesting;
mod effect_rows;
mod expressions;
mod literals;
