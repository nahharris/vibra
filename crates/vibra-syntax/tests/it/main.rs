//! The `vibra-syntax` integration tests, one test target.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    missing_docs
)]

mod support;

mod callable;
mod data;
mod examples;
mod expr;
mod lexer;
mod nesting;
mod schema;
mod source;
mod tree;
mod types;
