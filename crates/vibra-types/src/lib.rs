//! The Vibra checker.
//!
//! This crate holds the embedded standard library and its loader
//! ([`load_stdlib`]): the `@stdlibManifest.v2` manifest, the digests of the
//! modules, the external declarations checked against the `@compiler`
//! registry, the nine language roles each claimed exactly once, and the
//! prelude. [`StdlibOverlay::embedded_unit`] hands the loaded library to
//! resolution as its embedded unit `@std`. The authority is `docs/spec`:
//! "Language core and standard
//! library" and "The prelude" of the type chapter, "The standard library" of
//! the programs chapter, and "Compiler intrinsic registry" and "External
//! providers" of the runtime chapter.
//!
//! # Position in the architecture
//!
//! The crate reads no file and no ambient state: the standard library is
//! embedded at build time and every input is bytes.

mod check;
mod stdlib;

pub use check::{
    CheckResult, CheckedTest, Interpretation, Obligation, ObligationKind,
    ObligationStatus, check_source, check_workspace, interpret,
};

pub use stdlib::{
    LibraryNames, OverlayModule, PRELUDE, Role, STDLIB_UNIT, Stdlib, StdlibDeclaration,
    StdlibError, StdlibInputs, StdlibModule, StdlibOverlay, check_externals,
    declaration_identity, load_stdlib, load_stdlib_from, manifest_schema,
};
