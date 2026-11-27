//! The `vibra-cli` host tests, one test target.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    missing_docs
)]

mod support;

// The inputs of the recursion probes, the same file the tests of
// `vibra-workspace` run the stages over; this crate uses some of them.
#[allow(dead_code)]
#[path = "../../../vibra-workspace/tests/it/probes.rs"]
mod probes;

mod check;
mod examples;
mod exit;
mod fmt;
mod gate;
mod help;
mod invalid_input;
mod inventory;
mod limits;
mod nesting;
mod run_command;
mod sequences;
mod test_command;
mod unavailable;
