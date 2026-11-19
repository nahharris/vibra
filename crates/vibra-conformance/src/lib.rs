//! The conformance corpus runner.
//!
//! `docs/spec/07-diagnostics-and-conformance.md` requires a corpus of Vibra
//! cases that is independent of any backend and organized by the rule each
//! case tests. This crate loads that corpus from `conformance/cases`, checks
//! every case against the rule index `conformance/rules.tsv`, dispatches each
//! case to a handler of its profile, and compares the handler's observations
//! with the expectations written from the specification. It has no mode that
//! writes an expectation.
//!
//! # Position in the architecture
//!
//! This crate sits above every other node and may depend on all of them. That
//! also makes it the home of workspace-wide structural invariants, such as
//! the architecture test in `tests/`, which no single crate can check from
//! the inside.
//!
//! # Handlers
//!
//! [`ReaderV2Handler`] runs the `reader` operation under `readerV2`;
//! [`ProjectDecodeHandler`] and [`SourceGraphHandler`] run `projectDecode`
//! and `sourceGraph` under `staticV2`. Every
//! other case is unavailable until the step that makes its observations
//! exist registers a handler.
//!
//! # Executors
//!
//! [WasmExecutor] runs the checked IR of an executable case on the
//! WebAssembly transition backend beside the reference interpreter, and the
//! runner reports its column per profile.

mod corpus;
mod manifest;
mod profile;
mod project;
mod reader;
mod rules;
mod runner;
mod standard;
mod wasm;

pub use corpus::{
    Case, CaseInputDocument, CaseTreeFile, Corpus, CorpusError, corpus_root,
    workspace_root,
};
pub use manifest::{
    CaseExpectations, CaseInputs, CaseManifest, ExpectedDiagnostic, ExpectedFix,
    ExpectedRelatedSpan, MANIFEST_FILE_NAME, ManifestError, Verdict,
};
pub use profile::{
    CaseObservation, ConformanceOperation, ConformanceProfile, HandlerError,
    ProfileHandler, UnknownOperation, UnknownProfile,
};
pub use project::{ProjectDecodeHandler, SourceGraphHandler};
pub use reader::ReaderV2Handler;
pub use rules::{
    IdentityParts, InvalidIdentity, RULE_PREFIXES, RULES_FILE_NAME, RULES_HEADER,
    RuleIndex, RuleRow, RuleStatus, RulesError, parse_identity,
};
pub use runner::{CaseReport, CaseStatus, ConformanceRunner, ProfileCounts, RunReport};
pub use standard::{DispatchResult, ProfileDispatcher, standard_dispatcher};
pub use wasm::{Executable, WasmExecutor};
