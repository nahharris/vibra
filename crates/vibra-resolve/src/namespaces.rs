//! The flat module scope and the member namespaces.
//!
//! `docs/spec/02-type-system.md`, "Namespaces and resolution": a module has
//! type, value, and interface namespaces, and no top-level declaration or
//! import may reuse a name another already uses, so a name denotes one
//! entity in every position. A repeated top-level name or import name is
//! `@name.redeclaration` at the later introduction, related to the earlier
//! one; the later binder still binds. `docs/spec/07-diagnostics-and-conformance.md`,
//! "Availability, discovery and resolution", adds a repeated test name.
//!
//! "Declared types": the record fields, cases, and nested members of one
//! type share one member namespace, and so do the members of one interface,
//! its associated types included. Two members of one owner with one name are
//! `@name.memberCollision` at the later name, related to the first.

use std::collections::BTreeMap;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};

use crate::graph::{Binding, DeclView, Graph};
use crate::identity::DeclarationKind;

/// A diagnostic at `span` of `source_id`.
pub(crate) fn at(
    code: DiagnosticCode,
    source_id: &str,
    span: ByteSpan,
    message: impl Into<String>,
) -> Diagnostic {
    Diagnostic::new(code, span, message).with_source_id(source_id)
}

/// Builds every module's flat scope and reports repeated names.
pub(crate) fn module_scopes(graph: &mut Graph<'_>, out: &mut Vec<Diagnostic>) {
    for module in 0..graph.modules.len() {
        let Some(data) = graph.modules.get(module) else {
            continue;
        };
        let source_id = data.source_id.clone();
        let mut names: Vec<(String, ByteSpan, Binding)> = Vec::new();
        let mut tests: Vec<(String, ByteSpan)> = Vec::new();
        for &index in &data.imports {
            if let Some((name, span)) = graph
                .imports
                .get(index)
                .and_then(|import| import.name.clone())
            {
                names.push((name, span, Binding::Import(index)));
            }
        }
        for &index in &data.top {
            let Some(decl) = graph.decls.get(index) else {
                continue;
            };
            if decl.kind() == DeclarationKind::Test {
                tests.push((decl.name.clone(), decl.name_span));
            } else {
                names.push((decl.name.clone(), decl.name_span, Binding::Decl(index)));
            }
        }
        names.sort_by_key(|(_, span, _)| span.start());
        let mut scope: BTreeMap<String, (Binding, ByteSpan)> = BTreeMap::new();
        for (name, span, binding) in names {
            if let Some((_, earlier)) = scope.get(&name) {
                out.push(
                    at(
                        DiagnosticCode::NameRedeclaration,
                        &source_id,
                        span,
                        format!("`{name}` is already declared in this module"),
                    )
                    .with_related(*earlier, "the earlier declaration"),
                );
            }
            scope.insert(name, (binding, span));
        }
        repeated(
            &tests,
            &source_id,
            DiagnosticCode::NameRedeclaration,
            "test",
            out,
        );
        if let Some(data) = graph.modules.get_mut(module) {
            data.scope = scope
                .into_iter()
                .map(|(name, (binding, _))| (name, binding))
                .collect();
        }
    }
}

/// Reports every member namespace's repeated names: the members of each type
/// and interface, and the fields of each case.
pub(crate) fn member_namespaces(graph: &Graph<'_>, out: &mut Vec<Diagnostic>) {
    for decl in &graph.decls {
        let source_id = graph.source_id(decl.module);
        let members = decl
            .members
            .iter()
            .map(|member| (member.name.clone(), member.span))
            .collect::<Vec<_>>();
        repeated(
            &members,
            source_id,
            DiagnosticCode::NameMemberCollision,
            "member",
            out,
        );
        if let DeclView::Case(case) = decl.view
            && let Some(fields) = case.fields()
        {
            let fields = fields
                .iter()
                .map(|(name, _)| (name.text().to_owned(), name.span()))
                .collect::<Vec<_>>();
            repeated(
                &fields,
                source_id,
                DiagnosticCode::NameMemberCollision,
                "field",
                out,
            );
        }
    }
}

/// Reports each name of `names`, in written order, that an earlier entry
/// already has, as `code` at the later one, related to the first.
pub(crate) fn repeated(
    names: &[(String, ByteSpan)],
    source_id: &str,
    code: DiagnosticCode,
    noun: &str,
    out: &mut Vec<Diagnostic>,
) {
    let mut seen: BTreeMap<&str, ByteSpan> = BTreeMap::new();
    for (name, span) in names {
        if let Some(first) = seen.get(name.as_str()) {
            out.push(
                at(
                    code,
                    source_id,
                    *span,
                    format!("another {noun} is already named `{name}`"),
                )
                .with_related(*first, format!("the first {noun} named `{name}`")),
            );
        } else {
            seen.insert(name, *span);
        }
    }
}
