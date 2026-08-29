//! The conformance corpus runner.
//!
//! `docs/spec/07-diagnostics-and-conformance.md` requires a
//! backend-independent corpus organized by specification rule rather than by
//! compiler module, and a set of capability profiles from `reader-v1` through
//! `full-v1`. This crate loads that corpus and dispatches each case to the
//! profile it addresses.
//!
//! # Position in the architecture
//!
//! This crate sits above every other node and may depend on all of them. That
//! also makes it the right home for workspace-wide structural invariants,
//! such as the dependency-direction test in `tests/`, which no single
//! language crate can check from the inside.
//!
//! # Status
//!
//! Milestone 1 step 1 created this crate and its dependency-direction test.
//! The corpus layout and runner arrive in step 3; see
//! `docs/roadmap/milestone-1/README.md`.
