//! The `vibra-resolve` integration tests, one test target.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    missing_docs
)]

mod support;

mod declarations;
mod identity;
mod imports;
mod paths;
mod snapshot;
mod stdlib;
