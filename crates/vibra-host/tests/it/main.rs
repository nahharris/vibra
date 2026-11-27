//! The `vibra-host` integration tests, one target: a module for each subject.
//!
//! Run them with `cargo test --locked -p vibra-host --test it`.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

mod broker;
mod dual;
mod fs;
mod policy;
mod recipe;
mod scripted;
mod startup;
mod system;
mod test_world;
mod trace;
mod world;
