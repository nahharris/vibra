//! Versioned JSON contracts for the Vibra CLI and MCP surfaces.
//!
//! JSON is the machine interchange format for tooling only. Vibra owns no
//! persistent JSON file: project, lock, and build data use the canonical
//! `.vibon` data grammar. Schema identifiers and major versions are contracts;
//! `docs/spec/05-tooling.md` is the normative definition.
//!
//! # Schema identifiers
//!
//! Published schemas are identified by URN, for example
//! `urn:vibra:schema:v1:diagnostic`. A URN names the contract without
//! asserting that a URL resolves to it, and Vibra has no schema-hosting domain
//! to promise. Every `$ref` is internal, so no resolution is required to
//! validate a document.
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
//! Milestone 1 step 2 published the diagnostic contracts. Query, edit-plan,
//! test-report, and command-result contracts follow in later milestones; see
//! `docs/roadmap/milestone-1/README.md`.

// Tests assert by panicking and index fixtures they just built. The library
// itself is still checked against these lints, because clippy builds the lib
// target without `cfg(test)` as well.
#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used
    )
)]

mod diagnostic;

pub use diagnostic::{
    DIAGNOSTIC_SCHEMA, DiagnosticDocument, FixDocument, PositionDocument,
    REGISTRY_ENTRY_SCHEMA, RegistryEntryDocument, RelatedSpanDocument, SCHEMA_VERSION,
    SpanDocument,
};
