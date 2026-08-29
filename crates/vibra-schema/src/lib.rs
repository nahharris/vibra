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
//! yet. Every `$ref` is internal, so no resolution is required to validate a
//! document.
//!
//! This is provisional: the intended home is an HTTPS identifier once the
//! project's domain exists. Because identifiers are contracts, that switch
//! must be made across every published schema at once and before v1 is
//! released. See design decision D5 in `docs/roadmap/milestone-1/README.md`.
//!
//! The `v1` in an identifier and in [`SCHEMA_VERSION`] is the schema's major
//! version, not the language's. The charter versions machine schemas
//! independently of the source language after 1.0.
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
