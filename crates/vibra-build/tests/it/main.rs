//! Tests of the build driver that need no runtime archive and run no generated
//! code: discovery, the link command, and the unsupported-host answer. The
//! tests that link and launch programs are in `vibra-conformance`
//! (`linked_exe`, ledger N30).
//!
//! Run them with `cargo test --locked --offline -p vibra-build --all-targets`.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    missing_docs
)]

mod command;
mod discovery;
mod unsupported;
