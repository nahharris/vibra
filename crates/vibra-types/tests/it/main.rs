//! The `vibra-types` integration tests, one target: a module for each subject
//! and a `support` module for what they share.
//!
//! Run them with `cargo test --locked -p vibra-types --test it`.

#![allow(clippy::expect_used, clippy::indexing_slicing, missing_docs)]

mod contract_members;
mod effects;
mod generics;
mod nominal_types;
mod primitive_table;
mod reserved_binders;
mod support;
