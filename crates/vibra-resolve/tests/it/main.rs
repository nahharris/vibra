//! The `vibra-resolve` integration tests, one target: a module for each subject
//! and a `support` module for what they share.
//!
//! Run them with `cargo test --locked -p vibra-resolve --test it`.

#![allow(clippy::expect_used, clippy::indexing_slicing, missing_docs)]

mod binding_scopes;
mod reserved_binders;
mod resolution;
mod support;
mod vocabulary_binders;
