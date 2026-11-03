//! The `vibra-workspace` integration tests, one target: a module for each
//! subject and a `support` module for what they share.
//!
//! Run them with `cargo test --locked -p vibra-workspace --test it`.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

mod check_and_run;
mod effect_metadata;
mod format_plan;
mod project_decoder;
mod semantic_queries;
mod source_graph;
mod source_graph_tests;
mod support;
mod target_consent;
mod test_runner;
mod workspace_tests;
