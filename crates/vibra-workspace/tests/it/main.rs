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
mod graph;
mod project;
mod revision;
mod snapshot;
