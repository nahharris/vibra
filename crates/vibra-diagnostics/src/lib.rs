//! The one Vibra diagnostic model.
//!
//! This crate owns source spans, the closed diagnostic code registry, the
//! fixed level of each code, and the structured diagnostic record that every
//! other crate emits. `docs/spec/07-diagnostics-and-conformance.md`,
//! "Diagnostics are a language surface", is the normative definition.
//!
//! Diagnostics are a language surface, not an error-reporting convenience:
//! every rejected construct has one, a code names a condition, and a code's
//! level is fixed by the specification rather than chosen by the code that
//! emits it.
//!
//! # Position in the architecture
//!
//! This is the leaf crate of the new pipeline: it depends on no other Vibra
//! crate. Nothing here may know about the CLI, MCP, JSON, or the filesystem.
//! The JSON contract for these types, `DiagnosticDocument` version 2, is
//! rendered by `vibra-schema`, which depends on this crate and is never
//! depended upon by it.

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
mod line_index;
mod registry;
mod span;

pub use diagnostic::{Diagnostic, DocumentRevision, Fix, RelatedSpan};
pub use line_index::{LineIndex, Position};
pub use registry::{
    DiagnosticCode, Domain, FixCapability, Level, UnknownDiagnosticCode,
};
pub use span::ByteSpan;
