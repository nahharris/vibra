//! The `vibra-workspace` integration tests, one test target.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    missing_docs
)]

mod support;

mod discovery;
mod format;
mod graph;
mod project;
mod resolve;
mod revision;
mod snapshot;
mod test_runner;
