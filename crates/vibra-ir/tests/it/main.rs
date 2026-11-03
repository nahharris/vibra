//! The `vibra-ir` integration tests, one target: a module for each subject and
//! a `support` module for what they share.
//!
//! Run them with `cargo test --locked -p vibra-ir --test it`.

#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

mod contract_members;
mod effects;
mod externals;
mod generics;
mod host_registry_snapshot;
mod nominal_types;
mod support;
