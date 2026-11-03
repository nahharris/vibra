//! The `vibra-native` integration tests, one target: a module for each family
//! of natives and a `support` module for the memory they share.
//!
//! Run them with `cargo test --locked -p vibra-native --test it`.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    missing_docs
)]

mod arena_natives;
mod number_natives;
mod support;
