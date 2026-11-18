//! The `vibra-cli` integration tests, one target: a module for each subject and
//! a `support` module for what they share.
//!
//! Run them with `cargo test --locked -p vibra-cli --test it`.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

mod call_flow;
mod check_and_run;
mod codebase;
mod demo_evidence;
mod entry_outcomes;
mod examples;
mod filesystem;
mod init_consent;
mod init_plan;
mod project_commands;
mod review_regressions;
mod run_arguments;
mod support;
mod test_command;
