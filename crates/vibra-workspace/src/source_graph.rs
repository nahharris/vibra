//! Explicit immutable source/unit graph construction.

use std::collections::BTreeMap;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};

use crate::project::{DependencyKind, Project};
use crate::snapshot::SourceSnapshot;

/// A canonical target-relative module identity.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModuleId {
    unit: String,
    segments: Vec<String>,
}

impl ModuleId {
    /// Creates a module identity from a unit and target-relative segments.
    #[must_use]
    pub fn new(
        unit: impl Into<String>,
        segments: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            unit: unit.into(),
            segments: segments.into_iter().collect(),
        }
    }

    /// Unit name without its `@` marker.
    #[must_use]
    pub fn unit(&self) -> &str {
        &self.unit
    }

    /// Target-relative module components.
    #[must_use]
    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    /// Canonical atom spelling.
    #[must_use]
    pub fn as_atom(&self) -> String {
        if self.segments.is_empty() {
            format!("@{}", self.unit)
        } else {
            format!("@{}.{}", self.unit, self.segments.join("."))
        }
    }
}

/// A source module in the explicit graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceModule {
    id: ModuleId,
    source_id: String,
    bytes: Vec<u8>,
}

impl SourceModule {
    /// Canonical module identity.
    #[must_use]
    pub const fn id(&self) -> &ModuleId {
        &self.id
    }

    /// Immutable project-relative source identity.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// Exact immutable source bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// One local target unit in the graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceUnit {
    name: String,
    kind: crate::project::TargetKind,
    root: std::path::PathBuf,
    modules: Vec<SourceModule>,
}

impl SourceUnit {
    /// Unit name without its `@` marker.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Target kind.
    #[must_use]
    pub const fn kind(&self) -> crate::project::TargetKind {
        self.kind
    }

    /// Canonical target root.
    #[must_use]
    pub fn root(&self) -> &std::path::Path {
        &self.root
    }

    /// Modules in stable source-ID order.
    #[must_use]
    pub fn modules(&self) -> &[SourceModule] {
        &self.modules
    }
}

/// Whether a dependency can be consumed by this graph slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DependencyStatus {
    /// Ordinary dependency delivery is deferred to M5.
    Unsupported,
}

/// The explicit source declaration retained by a dependency edge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DependencySource {
    /// A local path declaration, retained without opening the path.
    Path(String),
    /// An HTTPS Git declaration, retained without fetching it.
    Git {
        /// The declared HTTPS URL.
        url: String,
        /// The declared exact revision.
        rev: String,
    },
}

/// One unresolved dependency declaration in the graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyEdge {
    alias: String,
    kind: DependencyKind,
    source: DependencySource,
    target: Option<String>,
    span: ByteSpan,
    source_id: String,
    status: DependencyStatus,
}

impl DependencyEdge {
    /// Alias without its `@` marker.
    #[must_use]
    pub fn alias(&self) -> &str {
        &self.alias
    }

    /// Declared dependency kind.
    #[must_use]
    pub const fn kind(&self) -> DependencyKind {
        self.kind
    }

    /// Original path or Git declaration.
    #[must_use]
    pub const fn source(&self) -> &DependencySource {
        &self.source
    }

    /// Optional library target value.
    #[must_use]
    pub fn target(&self) -> Option<&str> {
        self.target.as_deref()
    }

    /// Dependency record span.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }

    /// Source identity owning the dependency declaration.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// Delivery status.
    #[must_use]
    pub const fn status(&self) -> DependencyStatus {
        self.status
    }
}

/// The source graph consumed by later resolver phases.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceGraph {
    units: Vec<SourceUnit>,
    dependencies: Vec<DependencyEdge>,
    modules: BTreeMap<ModuleId, usize>,
    diagnostics: Vec<Diagnostic>,
}

impl SourceGraph {
    /// Builds a graph from one discovered project and immutable snapshot.
    #[must_use]
    pub fn build(project: &Project, snapshot: SourceSnapshot) -> Self {
        let mut units = Vec::with_capacity(snapshot.units().len());
        let mut modules = BTreeMap::new();
        for unit_snapshot in snapshot.units() {
            let mut unit_modules = Vec::with_capacity(unit_snapshot.modules().len());
            for document in unit_snapshot.modules() {
                let id = ModuleId::new(
                    unit_snapshot.name().to_owned(),
                    document.module_segments().iter().cloned(),
                );
                let module_index = modules.len();
                modules.insert(id.clone(), module_index);
                unit_modules.push(SourceModule {
                    id,
                    source_id: document.source_id().to_owned(),
                    bytes: document.bytes().to_vec(),
                });
            }
            units.push(SourceUnit {
                name: unit_snapshot.name().to_owned(),
                kind: unit_snapshot.kind(),
                root: unit_snapshot.root().to_path_buf(),
                modules: unit_modules,
            });
        }

        let mut dependencies = Vec::with_capacity(project.dependencies().len());
        let mut diagnostics = Vec::new();
        for dependency in project.dependencies() {
            let source = match dependency.kind() {
                DependencyKind::Path => DependencySource::Path(
                    dependency
                        .path()
                        .map(|value| value.value().to_owned())
                        .unwrap_or_default(),
                ),
                DependencyKind::Git => DependencySource::Git {
                    url: dependency
                        .git()
                        .map(|value| value.value().to_owned())
                        .unwrap_or_default(),
                    rev: dependency
                        .rev()
                        .map(|value| value.value().to_owned())
                        .unwrap_or_default(),
                },
            };
            let alias = dependency.alias().atom().value().to_owned();
            dependencies.push(DependencyEdge {
                alias,
                kind: dependency.kind(),
                source,
                target: dependency
                    .target()
                    .map(|value| value.atom().value().to_owned()),
                span: dependency.span(),
                source_id: dependency.origin().source_id().to_owned(),
                status: DependencyStatus::Unsupported,
            });
            diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::ToolUnavailable,
                    dependency.alias().span(),
                    "ordinary dependency delivery is unavailable in M2",
                )
                .with_source_id(dependency.origin().source_id()),
            );
        }

        Self {
            units,
            dependencies,
            modules,
            diagnostics,
        }
    }

    /// Local target units in project order.
    #[must_use]
    pub fn units(&self) -> &[SourceUnit] {
        &self.units
    }

    /// Explicit unresolved dependency edges in project order.
    #[must_use]
    pub fn dependencies(&self) -> &[DependencyEdge] {
        &self.dependencies
    }

    /// Graph construction diagnostics, including unsupported dependencies.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Whether graph construction produced no diagnostics.
    #[must_use]
    pub fn accepted(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// Finds a module by explicit unit and target-relative components.
    #[must_use]
    pub fn module(&self, id: &ModuleId) -> Option<&SourceModule> {
        let index = *self.modules.get(id)?;
        self.units.iter().flat_map(|unit| unit.modules()).nth(index)
    }

    /// Finds a module by its explicit unit and component list.
    #[must_use]
    pub fn lookup(&self, unit: &str, segments: &[&str]) -> Option<&SourceModule> {
        let id =
            ModuleId::new(unit, segments.iter().map(|segment| (*segment).to_owned()));
        self.module(&id)
    }

    /// All modules in deterministic graph order.
    pub fn modules(&self) -> impl Iterator<Item = &SourceModule> {
        self.units.iter().flat_map(|unit| unit.modules.iter())
    }
}

/// Builds a graph from an immutable source snapshot.
#[must_use]
pub fn build_source_graph(project: &Project, snapshot: SourceSnapshot) -> SourceGraph {
    SourceGraph::build(project, snapshot)
}
