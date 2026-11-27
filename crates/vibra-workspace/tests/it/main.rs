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
mod probes;
mod project;
mod recursion;
mod resolve;
mod revision;
mod sequences;
mod snapshot;
mod stack;
mod test_runner;

// The tests of M2 steps 25 (`entry`), 26 (`binding`), 27 (`dependencies`),
// 29 (`run`), and 33 (`dual_run`, `attacks_run`). Each step fills its files.
mod attacks_run;
mod binding;
mod dependencies;
mod dual_run;
mod entry;
mod run;
