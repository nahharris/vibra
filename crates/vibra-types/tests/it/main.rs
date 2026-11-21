//! The `vibra-types` integration tests, one target: a module for each subject
//! and a `support` module for what they share.
//!
//! Run them with `cargo test --locked -p vibra-types --test it`.

#![allow(clippy::expect_used)]

mod externals;
mod library;
mod loader;
mod manifest;
mod modules;
mod roles;
mod support;
