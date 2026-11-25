//! The `vibra-conformance` integration tests, one test target.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    missing_docs
)]

mod support;

mod ambient_reads;
mod architecture_boundary;
mod deep_nesting;
mod diagnostic_registry;
mod effect_rows;
mod encoding_readback;
mod exit_evidence;
mod host_testing_feature;
mod interpret;
mod manifest;
mod owner_coverage;
mod rules_directory;
mod runner;
mod spec_examples;
mod test_root_callers;
