//! Vibra name resolution over an explicit source graph.
//!
//! `docs/spec/04-programs-and-packages.md`, "Modules and imports", and
//! `docs/spec/02-type-system.md`, "Declared types" and "Namespaces and
//! resolution", fix what this crate establishes: every declaration with its
//! anchor identity, the flat module scope, the imports with their derived
//! names, the acyclic import graph, access at the referring position, the
//! prelude reservations, the member namespaces, and the paths in type
//! positions. `docs/spec/07-diagnostics-and-conformance.md`, "Persistent
//! observations", fixes the `@resolved.v2` observation.
//!
//! [`resolve`] parses every module of a [`ResolveInput`] and returns a
//! [`Resolution`]. The embedded standard library enters as an
//! [`EmbeddedUnit`]: its modules are resolved with the local ones, a prelude
//! name resolves to the declaration its prelude table names, and the
//! observation holds only the local package. Local bindings and paths in
//! expressions are resolved by a later step.
//!
//! # Position in the architecture
//!
//! This crate depends on `vibra-diagnostics` and `vibra-syntax`. It reads no
//! file and no ambient state: the caller hands it every module's bytes, the
//! standard library's included.

mod constants;
mod declare;
mod graph;
mod identity;
mod imports;
mod input;
mod namespaces;
mod paths;
mod prelude;
mod scope;
mod snapshot;

pub use graph::Access;
pub use identity::{Anchor, DeclarationId, DeclarationKind};
pub use input::{
    EmbeddedUnit, PackageId, PreludeBinding, ResolveInput, SourceModule, SourceUnit,
    UnitKind,
};
pub use snapshot::{
    RESOLVED_FORMAT, Resolution, ResolvedDeclaration, ResolvedImport, ResolvedModule,
    ResolvedReference, VisibleLocals,
};

use graph::{Graph, ImportTarget};

/// Resolves every module of `input`.
#[must_use]
pub fn resolve(input: &ResolveInput) -> Resolution {
    let mut diagnostics = Vec::new();
    let documents = graph::parse(input, &mut diagnostics);
    let mut graph = Graph::collect(input, &documents);
    namespaces::module_scopes(&mut graph, &mut diagnostics);
    namespaces::member_namespaces(&graph, &mut diagnostics);
    declare::headers(&graph, &mut diagnostics);
    declare::reservations(&graph, &mut diagnostics);
    imports::resolve_imports(&mut graph, &mut diagnostics);
    declare::casing(&graph, &mut diagnostics);
    let mut references = Vec::new();
    let visible = paths::resolve_paths(&graph, &mut diagnostics, &mut references);
    let imports = graph
        .imports
        .iter()
        .filter(|import| !graph.is_embedded(import.module))
        .filter_map(|import| {
            let locator = import.view.locator()?;
            let (name, _) = import.name.clone()?;
            let to = match import.target {
                ImportTarget::Module(module) => graph
                    .modules
                    .get(module)
                    .map(|module| module.identity.clone()),
                ImportTarget::Decl(decl) => {
                    graph.decls.get(decl).map(|decl| decl.identity.clone())
                }
                ImportTarget::Unresolved => None,
            };
            Some(snapshot::ResolvedImport {
                source_id: graph.source_id(import.module).to_owned(),
                span: import.span,
                path: locator.text().to_owned(),
                name,
                to,
            })
        })
        .collect();
    let mut resolution = Resolution {
        package: input.package().clone(),
        modules: graph
            .modules
            .iter()
            .filter(|module| !module.embedded)
            .map(|module| snapshot::ResolvedModule {
                path: module.path.clone(),
                unit: module.unit.clone(),
                source_id: module.source_id.clone(),
            })
            .collect(),
        declarations: graph
            .decls
            .iter()
            .filter(|decl| !graph.is_embedded(decl.module))
            .map(|decl| snapshot::ResolvedDeclaration {
                identity: decl.identity.clone(),
                anchor: decl.anchor.clone(),
                access: decl.access,
                source_id: graph.source_id(decl.module).to_owned(),
                span: decl.span,
            })
            .collect(),
        imports,
        references: references
            .into_iter()
            .filter(|(source_id, _, _)| {
                !graph
                    .modules
                    .iter()
                    .any(|module| module.embedded && module.source_id == *source_id)
            })
            .map(|(source_id, span, to)| snapshot::ResolvedReference {
                source_id,
                span,
                to,
            })
            .collect(),
        diagnostics,
        visible,
    };
    resolution.sort();
    resolution
}
