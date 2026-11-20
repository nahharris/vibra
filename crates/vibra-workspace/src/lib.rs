//! The Vibra workspace: the project file, discovery, the confined source
//! snapshot, the workspace revision, and the source graph.
//!
//! `docs/spec/04-programs-and-packages.md`, "Project file" and "Discovery and
//! the source snapshot", fix the `@project.v2` document, how a project is
//! found, the target-root rules, and the module path rules.
//! `docs/spec/07-diagnostics-and-conformance.md`, "Persistent observations",
//! fixes the `@sourceGraph.v1` observation, and
//! `docs/spec/05-tooling.md`, "Snapshot, claim and witness identity", the
//! workspace revision.
//!
//! The pipeline is [`decode_project`] (no path is read), [`discover_project`]
//! or [`discover_project_at`] (the one marker), [`SourceSnapshot::capture`]
//! (the target roots are checked before any source directory is listed, and
//! no source byte is read before the walk's checks pass), and
//! [`SourceGraph::build`]. Later phases receive the snapshot and no
//! filesystem access; [`resolve_input`] hands its modules to resolution,
//! [`check_snapshot`] checks the targets it covers, and [`plan_format`]
//! formats one confined document, checking a captured module's candidate
//! text in place of its bytes. [`run_tests`] discovers, checks, and runs the
//! tests of a snapshot, and [`TestRun::document`] is its `@testRun.v2`
//! observation.
//!
//! # Position in the architecture
//!
//! This crate depends on `vibra-diagnostics`, `vibra-fmt`, `vibra-interp`,
//! `vibra-ir`, `vibra-resolve`, `vibra-syntax`, and `vibra-types`. It is
//! the tooling crate that reads a project's files; no language crate depends
//! on it.

mod discovery;
mod format_plan;
mod project;
mod resolve;
mod revision;
mod semantic;
mod snapshot;
mod source_graph;
mod test_runner;

pub use discovery::{DiscoveredProject, discover_project, discover_project_at};
pub use format_plan::{
    Discovery, FormatPlan, InvalidPath, WorkspaceCapture, format_mode, plan_format,
};
pub use project::{
    Availability, Dependency, DependencyKind, PROJECT_FILE_NAME, PROJECT_FORMAT,
    Package, Project, Target, TargetKind, decode_project, project_schema,
};
pub use resolve::resolve_input;
pub use revision::{DuplicateRecord, RevisionKind, RevisionRecord, workspace_revision};
pub use semantic::{WorkspaceCheck, check_snapshot};
pub use snapshot::{SourceFile, SourceSnapshot, SourceUnit, TESTS_ROOT, UnitKind};
pub use source_graph::{GRAPH_FORMAT, GraphDependency, SourceGraph};
pub use test_runner::{
    InvalidSelection, SuiteResult, TEST_RUN_FORMAT, TestItem, TestOutcome, TestRun,
    TestSelector, run_tests,
};
