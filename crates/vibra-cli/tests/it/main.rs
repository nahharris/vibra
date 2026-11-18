//! The `vibra-cli` host tests, one test target.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    missing_docs
)]

mod support;

mod exit;
mod help;
mod invalid_input;
mod inventory;
mod unavailable;
