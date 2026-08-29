//! The canonical Vibra formatter.
//!
//! `docs/spec/01-source-language.md` defines exactly one canonical
//! representation. Formatting is idempotent and semantics-preserving: it may
//! normalize recoverable presentation but must never guess through a syntax,
//! binding, or type ambiguity.
//!
//! # Position in the architecture
//!
//! Depends on [`vibra_syntax`] and [`vibra_diagnostics`]. Nothing in the
//! language semantics may depend on this crate.
//!
//! # Status
//!
//! Milestone 1 step 1 created this crate. A minimal formatter arrives with
//! the reader spine in step 4 and gains rules alongside each grammar area
//! through step 9; see `docs/roadmap/milestone-1/README.md`.
