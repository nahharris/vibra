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
//! # Handlers
//!
//! The corpus layout, neutral manifest decoder, profile dispatcher, and
//! backend-independent runner are the common path. Handlers register on it for
//! the syntax and formatter, structural query snapshots, confined source-graph
//! observations, the filesystem-free resolver observation and its
//! `@resolved.v1` artifact, and the tooling-profile snapshot-backed formatter
//! observation. A profile with no handler reports its cases unavailable. Each
//! accepted executable case also runs in the WebAssembly backend, against the
//! case's one expectation, and a case on which either backend disagrees fails.

pub mod baseline;
mod corpus;
mod format;
mod graph;
mod manifest;
mod native;
mod profile;
mod project;
mod reader;
mod resolve;
mod runner;
mod standard;
mod test_run;
mod types;
mod wasm;
mod workspace_index;
mod workspace_query;
mod workspace_semantic;

pub use corpus::{
    Case, CaseInputDocument, CaseTreeFile, Corpus, CorpusError, HostWorldError,
    corpus_root, workspace_root,
};
pub use format::ToolingV1FormatHandler;
pub use graph::StaticV1SourceGraphHandler;
pub use manifest::{
    CaseExpectations, CaseInputs, CaseManifest, ConformanceOperation,
    ExpectedDiagnostic, ExpectedExecution, ExpectedFix, ExpectedQuery,
    ExpectedRelatedSpan, MANIFEST_FILE_NAME, ManifestError, NORMATIVE_SECTION_IDS,
};
pub use native::{
    COLUMNS as NATIVE_PARITY_COLUMNS, INVENTORY_FILE as NATIVE_PARITY_FILE, Inventory,
    InventoryCell, InventoryRow, NativeCounts, NativeObservation, NativeStatus,
    StepperRun, owning_package,
};
pub use profile::{ConformanceProfile, UnknownProfile};
pub use project::StaticV1ProjectHandler;
pub use reader::ReaderV1Handler;
pub use resolve::StaticV1ResolveHandler;
pub use runner::{
    BackendObservation, BackendStatuses, CaseObservation, CaseReport, CaseStatus,
    ConformanceRunner, DispatchResult, ExecutionObservation, HandlerError,
    INSTANCE_MEMORY_LIMIT_BYTES, InterpreterCounts, ProfileDispatcher, ProfileHandler,
    QueryObservation, RunReport, WasmCounts, WasmObservation, WasmStatus,
};
pub use standard::standard_dispatcher;
pub use test_run::{
    RowFailure, RowOrigin, RowTrap, TestRow, command_result, format_test_run,
};
pub use types::InterpreterV1Handler;
pub use wasm::{
    TestsObserved, observe as observe_wasm, observe_tests as observe_wasm_tests,
};
pub use workspace_index::ToolingV1IndexHandler;
pub use workspace_query::ToolingV1QueryHandler;
pub use workspace_semantic::{
    InterpreterV1WorkspaceRunHandler, InterpreterV1WorkspaceTestHandler,
    StaticV1WorkspaceCheckHandler,
};
