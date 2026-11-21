//! Declaration headers: type parameters, `given` keys, and the prelude
//! reservations.
//!
//! `docs/spec/02-type-system.md`, "Generics": a `given` key MUST be a type
//! parameter of the same header, or it is `@name.unknownSymbol` at the key;
//! a repeated key is `@name.redeclaration` at the later entry, related to
//! the earlier. A nested member sees its owner's parameters and declares
//! only additional ones; redeclaring an inherited name is
//! `@name.genericRedeclaration`, and an `impl` block declares none, so its
//! members see the parameters of the `type` or `interface` it is in.
//! "Associated types": an associated type with the name of a parameter of
//! its interface is `@name.genericRedeclaration` at the associated type,
//! related to the parameter. `docs/spec/07-diagnostics-and-conformance.md`
//! adds a repeated type parameter, at the later parameter.
//!
//! "The prelude": declaring a type, interface, or type parameter with a
//! prelude name is `@name.reservedDeclaration`; a module value, `fn`,
//! `proc`, or import spelled as one is `@name.reservedValueSpelling`. Both
//! are reported at the name, and the name still binds.

use vibra_diagnostics::{Diagnostic, DiagnosticCode};
use vibra_syntax::source::{Given, Ident, TypeBody, TypeForm};

use crate::graph::{Decl, DeclView, Graph, ImportTarget};
use crate::identity::DeclarationKind;
use crate::namespaces::{at, repeated};

/// The type parameters `decl`'s view declares.
pub(crate) fn own_params<'a>(view: DeclView<'a>) -> Vec<Ident<'a>> {
    match view {
        DeclView::Type(view) => view.params(),
        DeclView::Interface(view) => view.params(),
        DeclView::Callable(view) => view.params(),
        DeclView::Let(_)
        | DeclView::Test(_)
        | DeclView::Case(_)
        | DeclView::Assoc(_) => Vec::new(),
    }
}

/// Checks every header of the graph.
pub(crate) fn headers(graph: &Graph<'_>, out: &mut Vec<Diagnostic>) {
    for decl in &graph.decls {
        let source_id = graph.source_id(decl.module);
        let inherited = decl
            .owner
            .and_then(|owner| graph.decls.get(owner))
            .map(|owner| own_params(owner.view))
            .unwrap_or_default();
        let given = match decl.view {
            DeclView::Type(view) => view.given(),
            DeclView::Interface(view) => view.given(),
            DeclView::Callable(view) => view.given(),
            DeclView::Let(_)
            | DeclView::Test(_)
            | DeclView::Case(_)
            | DeclView::Assoc(_) => None,
        };
        header(
            graph,
            (source_id, graph.is_embedded(decl.module)),
            &own_params(decl.view),
            &inherited,
            given,
            out,
        );
        if decl.kind() == DeclarationKind::AssociatedType
            && let Some(parameter) = inherited
                .iter()
                .find(|parameter| parameter.text() == decl.name)
        {
            out.push(
                at(
                    DiagnosticCode::NameGenericRedeclaration,
                    source_id,
                    decl.name_span,
                    format!(
                        "the associated type `{}` has the name of a type parameter",
                        decl.name
                    ),
                )
                .with_related(parameter.span(), "the type parameter"),
            );
        }
    }
    for module in &graph.modules {
        for block in &module.impls {
            let inherited = graph
                .decls
                .get(block.owner)
                .map(|owner| own_params(owner.view))
                .unwrap_or_default();
            for member in block.view.members() {
                header(
                    graph,
                    (&module.source_id, module.embedded),
                    &member.params(),
                    &inherited,
                    member.given(),
                    out,
                );
            }
        }
    }
}

/// Checks one header of the module `source_id`: its parameters against each
/// other, against the `inherited` ones, and, outside the embedded unit,
/// against the prelude; and its `given` keys.
fn header(
    graph: &Graph<'_>,
    (source_id, embedded): (&str, bool),
    params: &[Ident<'_>],
    inherited: &[Ident<'_>],
    given: Option<Given<'_>>,
    out: &mut Vec<Diagnostic>,
) {
    for (index, parameter) in params.iter().enumerate() {
        let earlier = params
            .get(..index)
            .unwrap_or_default()
            .iter()
            .any(|earlier| earlier.text() == parameter.text());
        if earlier
            || inherited
                .iter()
                .any(|outer| outer.text() == parameter.text())
        {
            out.push(at(
                DiagnosticCode::NameGenericRedeclaration,
                source_id,
                parameter.span(),
                format!(
                    "the type parameter `{}` is already declared",
                    parameter.text()
                ),
            ));
        }
        if !embedded {
            reserve(
                graph,
                source_id,
                *parameter,
                DiagnosticCode::NameReservedDeclaration,
                out,
            );
        }
    }
    let Some(given) = given else {
        return;
    };
    let entries = given.entries();
    for (key, _) in &entries {
        if !params
            .iter()
            .any(|parameter| parameter.text() == key.text())
        {
            out.push(at(
                DiagnosticCode::NameUnknownSymbol,
                source_id,
                key.span(),
                format!("`{}` is not a type parameter of this header", key.text()),
            ));
        }
    }
    let keys = entries
        .iter()
        .map(|(key, _)| (key.text().to_owned(), key.span()))
        .collect::<Vec<_>>();
    repeated(
        &keys,
        source_id,
        DiagnosticCode::NameRedeclaration,
        "`given` entry",
        out,
    );
}

/// Reports `name` as `code` when it is a prelude name. A reserved word in a
/// name position is the reader's diagnostic and is not reported again.
pub(crate) fn reserve(
    graph: &Graph<'_>,
    source_id: &str,
    name: Ident<'_>,
    code: DiagnosticCode,
    out: &mut Vec<Diagnostic>,
) {
    if !name.is_reserved() && graph.is_prelude_name(name.text()) {
        out.push(at(
            code,
            source_id,
            name.span(),
            format!("`{}` is a prelude name", name.text()),
        ));
    }
}

/// Reports every declaration, member, and import spelled as a prelude name.
/// The embedded standard library introduces the prelude names, so its
/// modules are not checked.
pub(crate) fn reservations(graph: &Graph<'_>, out: &mut Vec<Diagnostic>) {
    for decl in &graph.decls {
        if graph.is_embedded(decl.module) {
            continue;
        }
        let code = match decl.kind() {
            DeclarationKind::Type
            | DeclarationKind::Interface
            | DeclarationKind::AssociatedType => {
                DiagnosticCode::NameReservedDeclaration
            }
            DeclarationKind::Fn | DeclarationKind::Proc | DeclarationKind::Value => {
                DiagnosticCode::NameReservedValueSpelling
            }
            DeclarationKind::Module | DeclarationKind::Case | DeclarationKind::Test => {
                continue;
            }
        };
        if let Some(name) = declared_name(decl.view) {
            reserve(graph, graph.source_id(decl.module), name, code, out);
        }
    }
    for module in graph.modules.iter().filter(|module| !module.embedded) {
        for block in &module.impls {
            for member in block.view.members() {
                if let Some(name) = member.name() {
                    reserve(
                        graph,
                        &module.source_id,
                        name,
                        DiagnosticCode::NameReservedValueSpelling,
                        out,
                    );
                }
            }
        }
    }
    for import in &graph.imports {
        let Some((name, span)) = &import.name else {
            continue;
        };
        let written = import.view.alias().is_none_or(|alias| !alias.is_reserved());
        if written && !graph.is_embedded(import.module) && graph.is_prelude_name(name) {
            out.push(at(
                DiagnosticCode::NameReservedValueSpelling,
                graph.source_id(import.module),
                *span,
                format!("`{name}` is a prelude name"),
            ));
        }
    }
}

/// The casing a name is written in by convention.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Casing {
    /// Types, interfaces, associated types, and type parameters.
    Pascal,
    /// Every other name.
    Camel,
}

/// Reports `name` as `@style.casing` when its first letter departs from
/// `casing`. A reserved word in a name position is the reader's diagnostic.
fn case(source_id: &str, name: Ident<'_>, casing: Casing, out: &mut Vec<Diagnostic>) {
    let Some(first) = name.text().chars().next() else {
        return;
    };
    let fits = match casing {
        Casing::Pascal => first.is_ascii_uppercase(),
        Casing::Camel => first.is_ascii_lowercase(),
    };
    if !fits && !name.is_reserved() {
        let convention = match casing {
            Casing::Pascal => "PascalCase",
            Casing::Camel => "camelCase",
        };
        out.push(at(
            DiagnosticCode::StyleCasing,
            source_id,
            name.span(),
            format!("`{}` is written in {convention} by convention", name.text()),
        ));
    }
}

/// `docs/spec/01-source-language.md`, "Identifiers": types, interfaces, and
/// type parameters are written in PascalCase and every other name in
/// camelCase; a name that departs from the convention is the warning
/// `@style.casing` at the name, and the program's meaning does not change.
/// The names checked are the declarations', members' and fields', the type
/// parameters', and the names an import's `as` gives. Local binders are the
/// body resolver's.
pub(crate) fn casing(graph: &Graph<'_>, out: &mut Vec<Diagnostic>) {
    for decl in &graph.decls {
        let source_id = graph.source_id(decl.module);
        let casing = match decl.kind() {
            DeclarationKind::Type
            | DeclarationKind::Interface
            | DeclarationKind::AssociatedType => Casing::Pascal,
            DeclarationKind::Fn
            | DeclarationKind::Proc
            | DeclarationKind::Value
            | DeclarationKind::Case => Casing::Camel,
            DeclarationKind::Module | DeclarationKind::Test => continue,
        };
        if let Some(name) = declared_name(decl.view) {
            case(source_id, name, casing, out);
        }
        for parameter in own_params(decl.view) {
            case(source_id, parameter, Casing::Pascal, out);
        }
        match decl.view {
            DeclView::Type(view) => {
                if let Some(TypeBody::Of(body)) = view.body()
                    && let Some(TypeForm::Record(fields)) = body.form()
                {
                    for (field, _) in fields {
                        case(source_id, field, Casing::Camel, out);
                    }
                }
            }
            DeclView::Case(view) => {
                for (field, _) in view.fields().unwrap_or_default() {
                    case(source_id, field, Casing::Camel, out);
                }
            }
            _ => {}
        }
    }
    for module in &graph.modules {
        for block in &module.impls {
            for member in block.view.members() {
                if let Some(name) = member.name() {
                    case(&module.source_id, name, Casing::Camel, out);
                }
                for parameter in member.params() {
                    case(&module.source_id, parameter, Casing::Pascal, out);
                }
            }
        }
    }
    for import in &graph.imports {
        let Some(alias) = import.view.alias() else {
            continue;
        };
        let casing = match import.target {
            ImportTarget::Decl(decl) => match graph.decls.get(decl).map(Decl::kind) {
                Some(DeclarationKind::Type | DeclarationKind::Interface) => {
                    Casing::Pascal
                }
                _ => Casing::Camel,
            },
            ImportTarget::Module(_) => Casing::Camel,
            ImportTarget::Unresolved => continue,
        };
        case(graph.source_id(import.module), alias, casing, out);
    }
}

/// The name a declaration's view spells.
pub(crate) fn declared_name(view: DeclView<'_>) -> Option<Ident<'_>> {
    match view {
        DeclView::Type(view) => view.name(),
        DeclView::Interface(view) => view.name(),
        DeclView::Callable(view) => view.name(),
        DeclView::Let(view) => view.name(),
        DeclView::Case(view) => view.name(),
        DeclView::Assoc(view) => view.name(),
        DeclView::Test(_) => None,
    }
}
