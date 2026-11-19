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
mod manifest;
mod runner;
mod spec_examples;
mod wasm_column;
mod wasm_support;
