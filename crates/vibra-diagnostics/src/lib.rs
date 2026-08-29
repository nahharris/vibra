//! The one Vibra diagnostic model.
//!
//! This crate owns source spans, the closed diagnostic code registry, the
//! fixed level of each code, and the structured diagnostic record that every
//! other crate emits. `docs/spec/07-diagnostics-and-conformance.md` is the
//! normative definition.
//!
//! # Position in the architecture
//!
//! This is the workspace's leaf crate: it depends on no other Vibra crate.
//! Diagnostics are a language surface, so nothing here may know about the
//! CLI, MCP, JSON, or the filesystem.
//!
//! # Status
//!
//! Milestone 1 step 2 is in progress. Spans and display-position derivation
//! are complete; the code registry and diagnostic record follow. See
//! `docs/roadmap/milestone-1/README.md`.

mod line_index;
mod span;

pub use line_index::{LineIndex, Position};
pub use span::ByteSpan;
