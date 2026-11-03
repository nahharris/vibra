//! The `vibra-schema` integration tests, one target: a module for each schema
//! contract and a `support` module for what they share.
//!
//! Run them with `cargo test --locked -p vibra-schema --test it`.

// A test asserts by panicking.
#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    missing_docs
)]

mod command_result;
mod diagnostics;
mod index;
mod position_query;
mod position_query_vocabulary;
mod semantic_query;
mod support;
