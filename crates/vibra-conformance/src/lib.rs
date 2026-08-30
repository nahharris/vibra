//! The conformance corpus runner.
//!
//! `docs/spec/07-diagnostics-and-conformance.md` requires a
//! backend-independent corpus organized by specification rule rather than by
//! compiler module, and a set of capability profiles from `reader-v1` through
//! `full-v1`. This crate loads that corpus and dispatches each case to the
//! profile it addresses.
//!
//! # Position in the architecture
//!
//! This crate sits above every other node and may depend on all of them. That
//! also makes it the right home for workspace-wide structural invariants,
//! such as the dependency-direction test in `tests/`, which no single
//! language crate can check from the inside.
//!
//! # Status
//!
//! Milestone 1 step 3 supplies the corpus layout, neutral manifest decoder,
//! profile dispatcher, and internal runner. Language backends are registered
//! by later milestones; an unregistered profile is reported as unavailable.

mod corpus;
mod manifest;
mod profile;
mod runner;

pub use corpus::{Case, Corpus, CorpusError};
pub use manifest::{
    CaseExpectations, CaseInputs, CaseManifest, ExpectedDiagnostic, ExpectedExecution,
    ExpectedFix, ExpectedRelatedSpan, MANIFEST_FILE_NAME, ManifestError,
    NORMATIVE_SECTION_IDS,
};
pub use profile::{ConformanceProfile, UnknownProfile};
pub use runner::{
    CaseObservation, CaseReport, CaseStatus, ConformanceRunner, DispatchResult,
    ExecutionObservation, HandlerError, ProfileDispatcher, ProfileHandler, RunReport,
};
