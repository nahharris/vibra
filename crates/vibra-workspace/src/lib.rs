//! Workspace-owned project data and later project orchestration.
//!
//! Workspace-owned project acquisition and source graph construction.
//!
//! [`project`] remains a pure decoder for the typed `@project.v1` schema.
//! [`discovery`], [`snapshot`], and [`source_graph`] form the bounded Step 3
//! filesystem boundary: they acquire one confined project tree into immutable
//! values and never resolve dependencies, contact a network, or consult a
//! cache or lock file.

#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used
    )
)]

pub mod discovery;
pub mod project;
pub mod snapshot;
pub mod source_graph;

use std::fmt;

use vibra_diagnostics::Diagnostic;

/// A workspace boundary failure with its stable diagnostics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceError {
    message: String,
    diagnostics: Vec<Diagnostic>,
}

impl WorkspaceError {
    /// Creates a failure from one human-facing message and diagnostics.
    #[must_use]
    pub fn new(message: impl Into<String>, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            message: message.into(),
            diagnostics,
        }
    }

    /// Creates a failure with no structured diagnostic.
    #[must_use]
    pub fn message(message: impl Into<String>) -> Self {
        Self::new(message, Vec::new())
    }

    /// Diagnostics emitted by the failed workspace operation.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

impl fmt::Display for WorkspaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for WorkspaceError {}

/// The confined project plus its immutable local source snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceSnapshot {
    project: discovery::DiscoveredProject,
    source: snapshot::SourceSnapshot,
}

impl WorkspaceSnapshot {
    /// Discovers a project from `start` and captures its local source tree.
    pub fn load(start: impl AsRef<std::path::Path>) -> Result<Self, WorkspaceError> {
        let project = discovery::discover_project(start)?;
        let source = snapshot::SourceSnapshot::capture(&project)?;
        Ok(Self { project, source })
    }

    /// Loads exactly `project.vibon` beneath an already confined tree root.
    ///
    /// This entry point never searches ancestors or siblings. Callers that
    /// receive a declared tree from another boundary must validate that tree
    /// before handing it here.
    pub fn load_confined(
        project_root: impl AsRef<std::path::Path>,
    ) -> Result<Self, WorkspaceError> {
        let project = discovery::discover_project_at(project_root)?;
        let source = snapshot::SourceSnapshot::capture(&project)?;
        Ok(Self { project, source })
    }

    /// The discovered and typed project.
    #[must_use]
    pub const fn project(&self) -> &discovery::DiscoveredProject {
        &self.project
    }

    /// The immutable source snapshot.
    #[must_use]
    pub const fn source(&self) -> &snapshot::SourceSnapshot {
        &self.source
    }

    /// Revision of the exact immutable project and source bytes in this snapshot.
    #[must_use]
    pub const fn revision(&self) -> &vibra_diagnostics::DocumentRevision {
        self.source.revision()
    }

    /// Builds the explicit source graph without consulting the filesystem.
    pub fn source_graph(&self) -> Result<source_graph::SourceGraph, WorkspaceError> {
        source_graph::SourceGraph::build(&self.project, self.source.clone())
    }
}
