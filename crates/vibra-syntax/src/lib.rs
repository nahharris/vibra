//! Vibra's reader: lexer, lossless recovery CST, document modes, and AST.
//!
//! One lexer serves both document grammars. The `.vib` source grammar and the
//! `.vibon` data grammar are selected by extension before parsing and are
//! never inferred from contents. `docs/spec/01-source-language.md` and the
//! VIBON section of `docs/spec/04-programs-and-packages.md` are the normative
//! definitions.
//!
//! # Position in the architecture
//!
//! This is the first node of the roadmap's dependency chain. It may depend on
//! [`vibra_diagnostics`] and nothing else in the workspace. It must never
//! depend on the formatter, the schemas, or any tool surface.
//!
//! # Status
//!
//! Milestone 1 step 1 created this crate. The reader spine arrives in step 4
//! and the language surface widens through step 9; see
//! `docs/roadmap/milestone-1/README.md`.
