//! The version 2 JSON contracts of the Vibra command line.
//!
//! `docs/spec/05-tooling.md`, "Versioned envelope and results" and "Command
//! payloads", and `docs/spec/07-diagnostics-and-conformance.md`, "Diagnostic
//! documents and spans", are the normative definitions. This crate renders
//! the language model of `vibra-diagnostics` into those documents and
//! validates every document against its embedded schema, because a producer
//! MUST validate what it emits.
//!
//! # Position in the architecture
//!
//! The crate depends only on `vibra-diagnostics` among the Vibra crates. No
//! language phase depends on it.

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

mod capability;
mod diagnostic;
mod envelope;
mod validate;

pub use capability::CommandCapability;
pub use diagnostic::{
    DiagnosticDocument, DiagnosticRenderError, FixDocument, PositionDocument,
    RelatedSpanDocument, SpanDocument,
};
pub use envelope::{CommandOutcome, Envelope, HelpPayload, Limits, Payload, Status};
pub use validate::{SchemaViolation, validate_command_result, validate_diagnostic};

/// The major version of every document this crate publishes.
pub const SCHEMA_VERSION: u32 = 2;

/// The identifier of the command result schema.
pub const COMMAND_RESULT_SCHEMA_ID: &str = "urn:vibra:schema:v2:command-result";

/// The identifier of the diagnostic schema.
pub const DIAGNOSTIC_SCHEMA_ID: &str = "urn:vibra:schema:v2:diagnostic";

/// The published JSON Schema of [`Envelope`].
pub const COMMAND_RESULT_SCHEMA: &str =
    include_str!("../schemas/v2/command-result.json");

/// The published JSON Schema of [`DiagnosticDocument`], with its span.
pub const DIAGNOSTIC_SCHEMA: &str = include_str!("../schemas/v2/diagnostic.json");
