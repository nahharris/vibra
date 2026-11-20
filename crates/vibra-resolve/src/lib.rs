//! Vibra name resolution over an explicit source graph.
//!
//! `docs/spec/04-programs-and-packages.md`, "Modules and imports", and
//! `docs/spec/02-type-system.md`, "Declared types" and "Namespaces and
//! resolution", fix what this crate establishes: every declaration with its
//! anchor identity, the flat module scope, the imports with their derived
//! names, the acyclic import graph, access at the referring position, the
//! prelude reservations, the member namespaces, and the paths in type
//! positions. `docs/spec/01-source-language.md`, "Names and paths",
//! "Bodies and statements", and "Patterns", fix what it establishes inside
//! bodies: the lexical scopes, the binders and constant patterns, and the
//! paths in expressions. `docs/spec/07-diagnostics-and-conformance.md`,
//! "Persistent observations", fixes the `@resolved.v2` observation.
//!
//! [`resolve`] parses every module of a [`ResolveInput`] and returns a
//! [`Resolution`]. The embedded standard library enters as an
//! [`EmbeddedUnit`]: its modules are resolved with the local ones, a prelude
//! name resolves to the declaration its prelude table names, and the
//! observation holds only the local package. [`Resolution::visible_locals`]
//! lists the locals visible at each statement of a body.
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
pub use identity::{
    Anchor, DeclarationId, DeclarationKind, implementation_identity,
    implementation_name,
};
pub use input::{
    EmbeddedUnit, PackageId, PreludeBinding, ResolveInput, SourceModule, SourceUnit,
    UnitKind,
};
pub use snapshot::{
    RESOLVED_FORMAT, Resolution, ResolvedDeclaration, ResolvedImport, ResolvedModule,
    ResolvedReference, VisibleLocals,
};

use graph::{Graph, ImportTarget};

/// The declaration record of every member of a local `impl` block. Its
/// identity is its block's, `.`, and its name; its access is that of the
/// type or interface the block is nested in.
fn implementation_members(
    graph: &Graph<'_>,
    documents: &[vibra_syntax::Document],
) -> Vec<snapshot::ResolvedDeclaration> {
    let mut records = Vec::new();
    for (module, data) in graph.modules.iter().enumerate() {
        if data.embedded {
            continue;
        }
        let text = documents
            .get(module)
            .and_then(vibra_syntax::Document::text)
            .unwrap_or_default();
        for block in &data.impls {
            let (Some(owner), Some(target)) =
                (graph.decls.get(block.owner), block.view.target())
            else {
                continue;
            };
            let span = target.span();
            let written = text.get(span.start()..span.end()).unwrap_or_default();
            let name = implementation_name(written);
            let identity = implementation_identity(
                data.package.name(),
                data.package.version(),
                &data.path,
                &owner.name,
                written,
            );
            for member in block.view.members() {
                let Some(member_name) = member.name() else {
                    continue;
                };
                let kind = match member.kind() {
                    vibra_syntax::source::CallableKind::Fn => DeclarationKind::Fn,
                    vibra_syntax::source::CallableKind::Proc => DeclarationKind::Proc,
                };
                records.push(snapshot::ResolvedDeclaration {
                    identity: identity.member(member_name.text()),
                    anchor: Anchor::new(
                        data.package.name(),
                        data.package.version(),
                        data.path.clone(),
                        owner.name.clone(),
                        kind,
                        member_name.text(),
                    ),
                    implementation: Some(name.clone()),
                    access: owner.access,
                    source_id: data.source_id.clone(),
                    span: member.construct().span(),
                });
            }
        }
    }
    records
}

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
                implementation: None,
                access: decl.access,
                source_id: graph.source_id(decl.module).to_owned(),
                span: decl.span,
            })
            .chain(implementation_members(&graph, &documents))
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
