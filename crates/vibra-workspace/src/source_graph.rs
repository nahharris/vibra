//! The `@sourceGraph.v1` observation.
//!
//! `docs/spec/07-diagnostics-and-conformance.md`, "Persistent observations":
//! the graph holds exact source bytes and no parsed declaration. `units` is
//! every unit of the snapshot sorted by `name`, `@tests` always among them;
//! a unit's `modules` are sorted by `path`, each with its `sourceId` and its
//! exact bytes in lower-case hexadecimal. `dependencies` is every declared
//! dependency sorted by `alias`, with its `kind`, `source`, optional
//! `target`, `status`, and the span record of its project entry.
//!
//! `docs/spec/04-programs-and-packages.md`, "Embedded modules": a project
//! module whose source identity collides with an embedded one is
//! `@module.sourceIdCollision`.

use std::fmt::Write as _;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_syntax::canonical_string;

use crate::project::{Availability, DependencyKind};
use crate::snapshot::{SourceSnapshot, UnitKind};

/// The format atom of the source graph.
pub const GRAPH_FORMAT: &str = "@sourceGraph.v1";

/// One module of a graph unit.
#[derive(Clone, Debug, PartialEq, Eq)]
struct GraphModule {
    path: String,
    source_id: String,
    bytes_hex: String,
}

/// One unit of the graph.
#[derive(Clone, Debug, PartialEq, Eq)]
struct GraphUnit {
    name: String,
    kind: UnitKind,
    modules: Vec<GraphModule>,
}

/// One dependency record of the graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphDependency {
    alias: String,
    kind: DependencyKind,
    source: String,
    target: Option<String>,
    availability: Availability,
    source_id: String,
    span: ByteSpan,
}

impl GraphDependency {
    /// The dependency name.
    #[must_use]
    pub fn alias(&self) -> &str {
        &self.alias
    }

    /// The status atom: `@unavailable` for every dependency of this release.
    #[must_use]
    pub const fn status(&self) -> &'static str {
        match self.availability {
            Availability::Available => "@captured",
            Availability::Unavailable => "@unavailable",
        }
    }

    /// The span of the project entry.
    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }
}

/// The source graph of one snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceGraph {
    units: Vec<GraphUnit>,
    dependencies: Vec<GraphDependency>,
}

fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(text, "{byte:02x}");
    }
    text
}

impl SourceGraph {
    /// The graph of `snapshot`. A module whose source identity equals one of
    /// `embedded` (the source identities of the embedded modules, each its
    /// module atom) is `@module.sourceIdCollision`.
    pub fn build(
        snapshot: &SourceSnapshot,
        embedded: &[&str],
    ) -> Result<Self, Vec<Diagnostic>> {
        let collisions = snapshot
            .modules()
            .filter(|module| embedded.contains(&module.source_id()))
            .map(|module| {
                Diagnostic::new(
                    DiagnosticCode::ModuleSourceIdCollision,
                    ByteSpan::empty_at(0),
                    "the module's source identity is an embedded module's",
                )
                .with_source_id(module.source_id())
            })
            .collect::<Vec<_>>();
        if !collisions.is_empty() {
            return Err(collisions);
        }
        let mut units = snapshot
            .units()
            .iter()
            .map(|unit| {
                let mut modules = unit
                    .modules()
                    .iter()
                    .map(|module| GraphModule {
                        path: module.path().to_owned(),
                        source_id: module.source_id().to_owned(),
                        bytes_hex: hex(module.bytes()),
                    })
                    .collect::<Vec<_>>();
                modules.sort_by(|left, right| {
                    left.path.as_bytes().cmp(right.path.as_bytes())
                });
                GraphUnit {
                    name: unit.name().to_owned(),
                    kind: unit.kind(),
                    modules,
                }
            })
            .collect::<Vec<_>>();
        units.sort_by(|left, right| left.name.as_bytes().cmp(right.name.as_bytes()));
        let project = snapshot.project().project();
        let mut dependencies = project
            .dependencies()
            .iter()
            .map(|dependency| GraphDependency {
                alias: dependency.name().to_owned(),
                kind: dependency.kind(),
                source: dependency.source().to_owned(),
                target: dependency.target().map(str::to_owned),
                availability: dependency.availability(),
                source_id: project.source_id().to_owned(),
                span: dependency.span(),
            })
            .collect::<Vec<_>>();
        dependencies.sort_by(|left, right| left.alias.cmp(&right.alias));
        Ok(Self {
            units,
            dependencies,
        })
    }

    /// The dependency records, sorted by alias.
    #[must_use]
    pub fn dependencies(&self) -> &[GraphDependency] {
        &self.dependencies
    }

    /// The canonical `@sourceGraph.v1` text: one line, no trailing newline.
    #[must_use]
    pub fn canonical_text(&self) -> String {
        let mut out = format!("{{format: {GRAPH_FORMAT} units: [");
        for (index, unit) in self.units.iter().enumerate() {
            if index > 0 {
                out.push(' ');
            }
            let _ = write!(
                out,
                "{{name: {} kind: {} modules: [",
                unit.name,
                unit.kind.as_atom()
            );
            for (index, module) in unit.modules.iter().enumerate() {
                if index > 0 {
                    out.push(' ');
                }
                let _ = write!(
                    out,
                    "{{path: {} sourceId: {} bytesHex: {}}}",
                    module.path,
                    canonical_string(&module.source_id),
                    canonical_string(&module.bytes_hex)
                );
            }
            out.push_str("]}");
        }
        out.push_str("] dependencies: [");
        for (index, dependency) in self.dependencies.iter().enumerate() {
            if index > 0 {
                out.push(' ');
            }
            let _ = write!(
                out,
                "{{alias: {} kind: {} source: {}",
                canonical_string(&dependency.alias),
                dependency.kind.as_atom(),
                canonical_string(&dependency.source)
            );
            if let Some(target) = &dependency.target {
                let _ = write!(out, " target: {target}");
            }
            let _ = write!(
                out,
                " status: {} span: {{sourceId: {} start: {}u64 end: {}u64}}}}",
                dependency.status(),
                canonical_string(&dependency.source_id),
                dependency.span.start(),
                dependency.span.end()
            );
        }
        out.push_str("]}");
        out
    }
}
