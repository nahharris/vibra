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

pub mod confined_fs;
pub mod discovery;
pub mod format_plan;
pub mod project;
pub mod query;
pub mod semantic;
pub mod snapshot;
pub mod source_graph;
mod test_runner;

use std::collections::BTreeMap;
use std::fmt;

use vibra_diagnostics::Diagnostic;

/// A workspace boundary failure with its stable diagnostics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceError {
    message: String,
    diagnostics: Vec<Diagnostic>,
    source_texts: BTreeMap<String, String>,
}

impl WorkspaceError {
    /// Creates a failure from one human-facing message and diagnostics.
    #[must_use]
    pub fn new(message: impl Into<String>, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            message: message.into(),
            diagnostics,
            source_texts: BTreeMap::new(),
        }
    }

    /// Adds captured source text used to render this error's byte spans.
    #[must_use]
    pub fn with_source_text(
        mut self,
        source_id: impl Into<String>,
        text: impl Into<String>,
    ) -> Self {
        self.source_texts.insert(source_id.into(), text.into());
        self
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

    /// Captured source documents that own diagnostics in this error.
    #[must_use]
    pub fn source_texts(&self) -> &BTreeMap<String, String> {
        &self.source_texts
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

    /// Resolves the immutable local source graph without reading the filesystem.
    pub fn resolve(&self) -> Result<vibra_resolve::ResolvedSnapshot, WorkspaceError> {
        let graph = self.source_graph()?;
        self.resolve_graph(&graph, None)
    }

    /// Resolves the captured local source graph with an already verified M2
    /// bootstrap package overlay.
    pub fn resolve_with_bootstrap(
        &self,
        verification: &vibra_types::BootstrapVerification,
    ) -> Result<vibra_resolve::ResolvedSnapshot, WorkspaceError> {
        let graph = self.source_graph()?;
        self.resolve_graph(&graph, Some(verification))
    }

    /// Whether this immutable workspace contains an exact mapped bootstrap
    /// import in any captured local unit, including `@tests`, that requires
    /// signed provenance before checking.
    ///
    /// The scan resolves only the already captured source graph. It does not
    /// inspect dependencies or consult the filesystem.
    pub fn requires_bootstrap_verification(&self) -> Result<bool, WorkspaceError> {
        let resolved = self.resolve()?;
        Ok(resolved.imports().iter().any(|import| {
            semantic::is_bootstrap_import_path(import.written())
                && resolved.modules().iter().any(|module| {
                    module.package() == resolved.package()
                        && module.source_id() == import.source_id()
                })
        }))
    }

    /// Whether the selected test modules and their import closure use a
    /// bootstrap module whose signed provenance must be verified.
    ///
    /// This query is separate from target check/run scope. With no selector,
    /// it scans the import closures of modules that declare tests. An explicit
    /// selector scans only its exact test module when that test exists.
    pub fn requires_test_bootstrap_verification(
        &self,
        selector: Option<&semantic::TestSelector>,
    ) -> Result<bool, WorkspaceError> {
        use vibra_syntax::{Declaration, Literal};

        let resolved = self.resolve()?;
        let local = resolved.package();
        let mut source_ids = resolved
            .modules()
            .iter()
            .filter(|module| module.package() == local && module.unit() == "tests")
            .filter(|module| {
                let module_atom = if module.segments().is_empty() {
                    "@tests".to_owned()
                } else {
                    format!("@tests.{}", module.segments().join("."))
                };
                if selector.is_some_and(|selector| selector.module() != module_atom) {
                    return false;
                }
                module.ast().is_some_and(|ast| {
                    ast.declarations().iter().any(|declaration| {
                        let Declaration::Test(test) = declaration else {
                            return false;
                        };
                        match selector {
                            Some(selector) => matches!(
                                test.name(),
                                Literal::String(name) if name.value() == selector.name()
                            ),
                            None => true,
                        }
                    })
                })
            })
            .map(|module| module.source_id().to_owned())
            .collect::<std::collections::BTreeSet<_>>();

        loop {
            let before = source_ids.len();
            let current_source_ids = source_ids.clone();
            for import in resolved
                .imports()
                .iter()
                .filter(|import| current_source_ids.contains(import.source_id()))
            {
                if semantic::is_bootstrap_import_path(import.written()) {
                    return Ok(true);
                }
                let Some(target) = import.module() else {
                    continue;
                };
                if target.package() != local {
                    continue;
                }
                if let Some(module) = resolved.modules().iter().find(|module| {
                    module.package() == target.package()
                        && module.unit() == target.unit()
                        && module.segments() == target.segments()
                }) {
                    source_ids.insert(module.source_id().to_owned());
                }
            }
            if source_ids.len() == before {
                break;
            }
        }

        Ok(false)
    }

    pub(crate) fn resolve_graph(
        &self,
        graph: &source_graph::SourceGraph,
        verification: Option<&vibra_types::BootstrapVerification>,
    ) -> Result<vibra_resolve::ResolvedSnapshot, WorkspaceError> {
        let package = self.project.project().package();
        let units = graph
            .units()
            .iter()
            .map(|unit| {
                let target = self
                    .project
                    .project()
                    .targets()
                    .iter()
                    .find(|target| target.name().atom().value() == unit.name());
                let entry = target.and_then(|target| {
                    target.entry().map(|entry| {
                        vibra_resolve::ReferencePath::new(
                            entry.atom().segments().iter().cloned(),
                            entry.origin().source_id().to_owned(),
                            entry.span(),
                        )
                    })
                });
                let kind = match unit.kind() {
                    project::TargetKind::Bin => vibra_resolve::TargetKind::Bin,
                    project::TargetKind::Lib => vibra_resolve::TargetKind::Lib,
                };
                let modules = unit
                    .modules()
                    .iter()
                    .map(|module| {
                        vibra_resolve::SourceModule::new(
                            module.id().unit(),
                            module.id().segments().iter().cloned(),
                            module.source_id(),
                            module.bytes(),
                        )
                    })
                    .collect();
                vibra_resolve::SourceUnit::new(unit.name(), kind, entry, modules)
            })
            .collect();
        let mut input = vibra_resolve::ResolveInput::new(
            package.name().value(),
            package.version().value(),
            units,
        )
        .with_reserved_import_paths([
            ("std".to_owned(), vec!["text".to_owned()]),
            ("std".to_owned(), vec!["assert".to_owned()]),
        ]);
        if let Some(verification) = verification {
            let (overlay_package, modules) = verification.resolver_overlay();
            input = input.with_verified_overlay(
                overlay_package.name(),
                overlay_package.version(),
                modules,
            );
        }
        Ok(vibra_resolve::Resolver::resolve(input))
    }

    /// Queries semantic and structural facts at one captured source position.
    pub fn query_position(
        &self,
        source_id: &str,
        offset: usize,
    ) -> Result<query::WorkspacePositionQuery, query::WorkspaceQueryError> {
        query::query_position(self, source_id, offset)
    }
}
