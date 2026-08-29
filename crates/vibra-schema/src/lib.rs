//! Versioned JSON contracts for the Vibra CLI and MCP surfaces.
//!
//! JSON is the machine interchange format for tooling only. Vibra owns no
//! persistent JSON file: project, lock, and build data use the canonical
//! `.vibon` data grammar. Schema IDs and major versions are contracts;
//! `docs/spec/05-tooling.md` is the normative definition.
//!
//! # Position in the architecture
//!
//! This crate translates language facts into a wire format, so the arrow
//! points one way only: it may depend on the language crates, and no compiler
//! phase may depend on it. The milestone 2 exit gate states that rule
//! directly.
//!
//! # Status
//!
//! Milestone 1 step 1 created this crate. The diagnostic JSON contract
//! arrives in step 2; see `docs/roadmap/milestone-1/README.md`.
