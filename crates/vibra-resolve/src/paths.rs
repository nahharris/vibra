//! Paths in type positions and in section operands.
//!
//! `docs/spec/02-type-system.md`, "Namespaces and resolution": a type
//! position resolves across the type and interface namespaces together; a
//! position that expects an entity decides only which kind is required, and
//! a path that resolves to another kind is `@name.wrongEntityKind`, naming
//! what it found. A name that resolves to nothing is `@name.unknownSymbol`
//! (`docs/spec/07-diagnostics-and-conformance.md`). "Bounds": a bound that
//! does not resolve to an interface is `@name.wrongEntityKind` at the bound.
//! "Implementations": the target of an `impl` in a `type` resolves to an
//! interface, and the target of an `impl` in an `interface` to a type.
//! "Associated types": an associated type is named only by its bare name
//! inside its interface or an `impl` of it, or by a label; there is no
//! projection path, so in `I.Item` and `Once.Item` the part `Item` is
//! `@name.unknownSymbol`, related to the parameter or the type. A choice is
//! resolved before the associated names of its own `impl` are in scope.
//! "Dispatch": `Self` is visible inside a `type`, an `interface`, and an
//! `impl`, and nowhere else.
//!
//! Every path that resolves to a declaration with an identity is a
//! reference of the `@resolved.v2` observation.

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_syntax::source::{Callable, Ident, Path, Row, TypeBody, TypeExpr, TypeForm};

use crate::declare::own_params;
use crate::graph::{
    Access, Binding, DeclView, Graph, ImplData, ImportTarget, MemberKind,
};
use crate::identity::{DeclarationId, DeclarationKind};
use crate::imports::top_level;
use crate::namespaces::at;
use crate::prelude::{ANY, SELF};

/// What a name or a path denotes.
#[derive(Clone, Copy, Debug)]
enum Entity {
    /// A local declaration.
    Decl(usize),
    /// A module, local or embedded, through an import.
    Module(usize),
    /// The predeclared interface `Any`.
    Any,
    /// A type parameter, declared at the span.
    Param(ByteSpan),
    /// `Self`: the enclosing type, or the receiver of an interface.
    SelfType(Option<usize>),
    /// The bare name of an associated type: its declaration when it is
    /// known, and the span that brings the name into scope.
    Associated(Option<usize>, ByteSpan),
    /// A name whose resolution already failed and was reported.
    Failed,
}

/// What a position requires.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Position {
    /// A type: a type, an interface value type, a parameter, or `Self`.
    Type,
    /// An interface: a bound, or the target of an `impl` in a `type`.
    Interface,
    /// The target of an `impl` in an `interface`: a type.
    Receiver,
    /// An entry of an effect row; its kind is the effect checker's.
    Row,
}

/// The names visible at a type position, besides the module scope and the
/// prelude.
#[derive(Clone, Debug, Default)]
struct Scope {
    params: Vec<(String, ByteSpan)>,
    self_type: Option<Option<usize>>,
    associated: Vec<(String, Option<usize>, ByteSpan)>,
}

impl Scope {
    fn with_params(&self, params: &[Ident<'_>]) -> Self {
        let mut scope = self.clone();
        scope.params.extend(
            params
                .iter()
                .map(|param| (param.text().to_owned(), param.span())),
        );
        scope
    }
}

/// One resolved reference: its source, its bytes, and the identity.
pub(crate) type Reference = (String, ByteSpan, DeclarationId);

/// Resolves every path in a type position of the graph.
pub(crate) fn resolve_paths(
    graph: &Graph<'_>,
    out: &mut Vec<Diagnostic>,
    references: &mut Vec<Reference>,
) {
    let mut walker = Walker {
        graph,
        module: 0,
        out,
        references,
    };
    for module in 0..graph.modules.len() {
        walker.module = module;
        let Some(data) = graph.modules.get(module) else {
            continue;
        };
        for &index in &data.top {
            walker.declaration(index);
        }
        for block in &data.impls {
            walker.impl_block(*block);
        }
    }
}

struct Walker<'g, 'a, 'o> {
    graph: &'g Graph<'a>,
    module: usize,
    out: &'o mut Vec<Diagnostic>,
    references: &'o mut Vec<Reference>,
}

impl<'a> Walker<'_, 'a, '_> {
    fn source_id(&self) -> &str {
        self.graph.source_id(self.module)
    }

    fn report(&mut self, diagnostic: Diagnostic) {
        self.out.push(diagnostic);
    }

    /// The scope inside the `type` or `interface` `owner`.
    fn owner_scope(&self, owner: usize) -> Scope {
        let Some(decl) = self.graph.decls.get(owner) else {
            return Scope::default();
        };
        let mut scope = Scope::default().with_params(&own_params(decl.view));
        match decl.view {
            DeclView::Type(_) => scope.self_type = Some(Some(owner)),
            DeclView::Interface(_) => {
                scope.self_type = Some(None);
                scope.associated = self.associated_of(owner);
            }
            _ => {}
        }
        scope
    }

    /// The associated types `interface` declares.
    fn associated_of(
        &self,
        interface: usize,
    ) -> Vec<(String, Option<usize>, ByteSpan)> {
        self.graph
            .decls
            .iter()
            .enumerate()
            .filter(|(_, decl)| {
                decl.owner == Some(interface)
                    && decl.kind() == DeclarationKind::AssociatedType
            })
            .map(|(index, decl)| (decl.name.clone(), Some(index), decl.name_span))
            .collect()
    }

    fn declaration(&mut self, index: usize) {
        let Some(decl) = self.graph.decls.get(index) else {
            return;
        };
        match decl.view {
            DeclView::Type(view) => {
                let scope = self.owner_scope(index);
                match view.body() {
                    Some(TypeBody::Of(body)) => {
                        self.type_expr(&scope, body, Position::Type);
                    }
                    Some(TypeBody::Cases(cases)) => {
                        for case in cases {
                            for (_, field) in case.fields().unwrap_or_default() {
                                self.type_expr(&scope, field, Position::Type);
                            }
                        }
                    }
                    Some(TypeBody::Intrinsic(_)) | None => {}
                }
                if let Some(given) = view.given() {
                    for (_, bound) in given.entries() {
                        self.type_expr(&scope, bound, Position::Interface);
                    }
                }
                for member in view.members() {
                    self.callable(&scope, member);
                }
            }
            DeclView::Interface(view) => {
                let scope = self.owner_scope(index);
                if let Some(given) = view.given() {
                    for (_, bound) in given.entries() {
                        self.type_expr(&scope, bound, Position::Interface);
                    }
                }
                for member in view.members() {
                    self.callable(&scope, member);
                }
            }
            DeclView::Callable(view) => self.callable(&Scope::default(), view),
            DeclView::Let(view) => {
                if let Some(written) = view.written_type() {
                    self.type_expr(&Scope::default(), written, Position::Type);
                }
            }
            DeclView::Test(view) => {
                if let Some((_, written)) = view.takes() {
                    self.type_expr(&Scope::default(), written, Position::Type);
                }
                if let Some(row) = view.performs() {
                    self.row(&Scope::default(), row);
                }
            }
            DeclView::Case(_) | DeclView::Assoc(_) => {}
        }
    }

    /// The signature, `given` bounds, and `performs` row of a callable seen
    /// from `outer`.
    fn callable(&mut self, outer: &Scope, view: Callable<'a>) {
        let scope = outer.with_params(&view.params());
        for (_, written) in view.takes() {
            self.type_expr(&scope, written, Position::Type);
        }
        for (_, written, _) in view.labelled() {
            self.type_expr(&scope, written, Position::Type);
        }
        if let Some((_, result)) = view.result() {
            self.type_expr(&scope, result, Position::Type);
        }
        if let Some(given) = view.given() {
            for (_, bound) in given.entries() {
                self.type_expr(&scope, bound, Position::Interface);
            }
        }
        if let Some(row) = view.performs() {
            self.row(&scope, row);
        }
    }

    fn impl_block(&mut self, block: ImplData<'a>) {
        let mut scope = self.owner_scope(block.owner);
        let in_type = matches!(
            self.graph.decls.get(block.owner).map(|owner| owner.view),
            Some(DeclView::Type(_))
        );
        let mut interface = None;
        if let Some(target) = block.view.target() {
            let position = if in_type {
                Position::Interface
            } else {
                Position::Receiver
            };
            let entity = self.type_expr(&scope, target, position);
            if in_type
                && let Some(Entity::Decl(index)) = entity
                && self
                    .graph
                    .decls
                    .get(index)
                    .is_some_and(|decl| decl.kind() == DeclarationKind::Interface)
            {
                interface = Some(index);
            }
        }
        if !in_type {
            interface = Some(block.owner);
            scope.self_type = Some(None);
        }
        scope.associated = Vec::new();
        for choice in block.view.choices() {
            if let Some(chosen) = choice.chosen() {
                self.type_expr(&scope, chosen, Position::Type);
            }
        }
        let declared = interface
            .map(|index| self.associated_of(index))
            .unwrap_or_default();
        let mut associated = Vec::new();
        for choice in block.view.choices() {
            if let Some(name) = choice.name() {
                let decl = declared
                    .iter()
                    .find(|(declared, _, _)| *declared == name.text())
                    .and_then(|(_, decl, _)| *decl);
                associated.push((name.text().to_owned(), decl, name.span()));
            }
        }
        for entry in declared {
            if !associated.iter().any(|(name, _, _)| *name == entry.0) {
                associated.push(entry);
            }
        }
        scope.associated = associated;
        for member in block.view.members() {
            self.callable(&scope, member);
        }
    }

    fn row(&mut self, scope: &Scope, row: Row<'a>) {
        for path in row.paths() {
            self.path(scope, path, Position::Row);
        }
    }

    /// Resolves a type expression and every type inside it; returns what its
    /// head path denotes.
    fn type_expr(
        &mut self,
        scope: &Scope,
        written: TypeExpr<'a>,
        position: Position,
    ) -> Option<Entity> {
        let mut head = None;
        let mut stack = vec![(written, position)];
        let mut first = true;
        while let Some((current, position)) = stack.pop() {
            let entity = match current.form() {
                Some(TypeForm::Path {
                    path,
                    positional,
                    labelled,
                }) => {
                    stack.extend(
                        positional.into_iter().rev().map(|ty| (ty, Position::Type)),
                    );
                    stack.extend(
                        labelled
                            .into_iter()
                            .rev()
                            .map(|(_, ty)| (ty, Position::Type)),
                    );
                    self.path(scope, path, position)
                }
                Some(TypeForm::Tuple(components)) => {
                    stack.extend(
                        components.into_iter().rev().map(|ty| (ty, Position::Type)),
                    );
                    None
                }
                Some(TypeForm::Record(fields)) => {
                    stack.extend(
                        fields.into_iter().rev().map(|(_, ty)| (ty, Position::Type)),
                    );
                    None
                }
                Some(TypeForm::Fn { inputs, result }) => {
                    stack.push((result, Position::Type));
                    stack.extend(
                        inputs
                            .labelled()
                            .into_iter()
                            .map(|(_, ty)| (ty, Position::Type)),
                    );
                    stack.extend(
                        inputs
                            .positional()
                            .into_iter()
                            .map(|ty| (ty, Position::Type)),
                    );
                    None
                }
                Some(TypeForm::Proc {
                    inputs,
                    result,
                    row,
                }) => {
                    if let Some(row) = row {
                        self.row(scope, row);
                    }
                    stack.push((result, Position::Type));
                    stack.extend(
                        inputs
                            .labelled()
                            .into_iter()
                            .map(|(_, ty)| (ty, Position::Type)),
                    );
                    stack.extend(
                        inputs
                            .positional()
                            .into_iter()
                            .map(|ty| (ty, Position::Type)),
                    );
                    None
                }
                Some(TypeForm::Intent { result, row }) => {
                    if let Some(row) = row {
                        self.row(scope, row);
                    }
                    stack.push((result, Position::Type));
                    None
                }
                Some(TypeForm::Atom(_)) | None => None,
            };
            if std::mem::replace(&mut first, false) {
                head = entity;
            }
        }
        head
    }

    /// The first name of a path, looked up innermost first: the type
    /// parameters, the associated names, `Self`, the module scope, and the
    /// prelude.
    fn lookup(&self, scope: &Scope, name: &str) -> Option<Entity> {
        if let Some((_, span)) = scope.params.iter().find(|(param, _)| param == name) {
            return Some(Entity::Param(*span));
        }
        if let Some((_, decl, span)) =
            scope.associated.iter().find(|(assoc, _, _)| assoc == name)
        {
            return Some(Entity::Associated(*decl, *span));
        }
        if name == SELF {
            return scope.self_type.map(Entity::SelfType);
        }
        if let Some(binding) = self
            .graph
            .modules
            .get(self.module)
            .and_then(|module| module.scope.get(name))
        {
            return Some(match *binding {
                Binding::Decl(index) => Entity::Decl(index),
                Binding::Import(index) => {
                    match self.graph.imports.get(index).map(|import| import.target) {
                        Some(ImportTarget::Module(module)) => Entity::Module(module),
                        Some(ImportTarget::Decl(decl)) => Entity::Decl(decl),
                        Some(ImportTarget::Unresolved) | None => Entity::Failed,
                    }
                }
            });
        }
        if name == ANY {
            return Some(Entity::Any);
        }
        self.graph
            .prelude
            .get(name)
            .map(|decl| decl.map_or(Entity::Failed, Entity::Decl))
    }

    /// Resolves one path in `position`; returns what it denotes, or `None`
    /// when a fault was reported.
    fn path(
        &mut self,
        scope: &Scope,
        path: Path<'a>,
        position: Position,
    ) -> Option<Entity> {
        let span = path.node().span();
        let segments = path.segments();
        let (first, rest) = segments.split_first()?;
        let Some(mut entity) = self.lookup(scope, first.text()) else {
            let message = if first.text() == SELF {
                "`Self` is visible only inside a `type`, an `interface`, and an `impl`"
                    .to_owned()
            } else {
                format!("`{}` names nothing here", first.text())
            };
            let diagnostic = at(
                DiagnosticCode::NameUnknownSymbol,
                self.source_id(),
                first.span(),
                message,
            );
            self.report(diagnostic);
            return None;
        };
        for part in rest {
            entity = self.step(entity, *part, span)?;
        }
        self.check_kind(entity, position, span)?;
        let identity = match entity {
            Entity::Decl(index) | Entity::Associated(Some(index), _) => self
                .graph
                .decls
                .get(index)
                .map(|decl| decl.identity.clone()),
            _ => None,
        };
        if let Some(identity) = identity {
            self.references
                .push((self.source_id().to_owned(), span, identity));
        }
        Some(entity)
    }

    /// The entity `part` names inside `entity`, the prefix of a path.
    fn step(
        &mut self,
        entity: Entity,
        part: Ident<'a>,
        span: ByteSpan,
    ) -> Option<Entity> {
        let source_id = self.source_id().to_owned();
        let unknown = |message: String| {
            at(
                DiagnosticCode::NameUnknownSymbol,
                &source_id,
                part.span(),
                message,
            )
        };
        match entity {
            Entity::Failed => None,
            Entity::Param(declared) => {
                let diagnostic = unknown(format!(
                    "a type parameter has no associated type path; `{}` names nothing",
                    part.text()
                ))
                .with_related(declared, "the type parameter");
                self.report(diagnostic);
                None
            }
            Entity::SelfType(owner) => {
                let mut diagnostic = unknown(format!(
                    "a type has no associated type path; `{}` names nothing",
                    part.text()
                ));
                if let Some(decl) = owner.and_then(|owner| self.graph.decls.get(owner))
                {
                    diagnostic = diagnostic.with_related(decl.name_span, "the type");
                }
                self.report(diagnostic);
                None
            }
            Entity::Associated(_, declared) => {
                let diagnostic = unknown(format!(
                    "a type has no associated type path; `{}` names nothing",
                    part.text()
                ))
                .with_related(declared, "the associated type");
                self.report(diagnostic);
                None
            }
            Entity::Module(module) => {
                let Some(index) = top_level(self.graph, module, part.text()) else {
                    let diagnostic =
                        unknown(format!("the module declares no `{}`", part.text()));
                    self.report(diagnostic);
                    return None;
                };
                let decl = self.graph.decls.get(index)?;
                if decl.access == Access::Private && decl.module != self.module {
                    let diagnostic = at(
                        DiagnosticCode::NamePrivateAccess,
                        &source_id,
                        span,
                        format!("`{}` is private to its module", part.text()),
                    )
                    .with_related_source(
                        self.graph.source_id(decl.module),
                        decl.name_span,
                        "the private declaration",
                    );
                    self.report(diagnostic);
                }
                Some(Entity::Decl(index))
            }
            Entity::Any => {
                let diagnostic = unknown(format!(
                    "the predeclared interface `Any` has no member `{}`",
                    part.text()
                ));
                self.report(diagnostic);
                None
            }
            Entity::Decl(index) => {
                let decl = self.graph.decls.get(index)?;
                let member = decl
                    .members
                    .iter()
                    .find(|member| member.name == part.text());
                let diagnostic = match member {
                    Some(member) if member.kind != MemberKind::AssociatedType => at(
                        DiagnosticCode::NameWrongEntityKind,
                        &source_id,
                        span,
                        format!(
                            "`{}` is {} of `{}`, not a type",
                            part.text(),
                            member.kind.noun(),
                            decl.name
                        ),
                    ),
                    _ => unknown(format!(
                        "`{}` has no type named `{}`; an associated type has no path",
                        decl.name,
                        part.text()
                    ))
                    .with_related_source(
                        self.graph.source_id(decl.module),
                        decl.name_span,
                        "the declaration",
                    ),
                };
                self.report(diagnostic);
                None
            }
        }
    }

    /// Reports `entity` when it is not of the kind `position` requires.
    fn check_kind(
        &mut self,
        entity: Entity,
        position: Position,
        span: ByteSpan,
    ) -> Option<()> {
        let found = match entity {
            Entity::Failed => return None,
            Entity::Decl(index) => self.graph.decls.get(index).map_or(
                Found::Other("a declaration"),
                |decl| match decl.kind() {
                    DeclarationKind::Type => Found::Type,
                    DeclarationKind::Interface => Found::Interface,
                    DeclarationKind::AssociatedType => Found::Type,
                    DeclarationKind::Module => Found::Other("a module"),
                    DeclarationKind::Case => Found::Other("a case"),
                    DeclarationKind::Fn => Found::Other("a function"),
                    DeclarationKind::Proc => Found::Other("a procedure"),
                    DeclarationKind::Value => Found::Other("a module value"),
                    DeclarationKind::Test => Found::Other("a test"),
                },
            ),
            Entity::Module(_) => Found::Other("a module"),
            Entity::Any => Found::Interface,
            Entity::Param(_) => Found::Parameter,
            Entity::SelfType(_) | Entity::Associated(..) => Found::Type,
        };
        let fits = match position {
            Position::Row => true,
            Position::Type => {
                matches!(found, Found::Type | Found::Interface | Found::Parameter)
            }
            Position::Interface => found == Found::Interface,
            Position::Receiver => matches!(found, Found::Type | Found::Parameter),
        };
        if fits {
            return Some(());
        }
        let required = match position {
            Position::Type | Position::Row => "a type",
            Position::Interface => "an interface",
            Position::Receiver => "a type, not an interface",
        };
        let diagnostic = at(
            DiagnosticCode::NameWrongEntityKind,
            self.source_id(),
            span,
            format!("this names {}; {required} is required here", found.noun()),
        );
        self.report(diagnostic);
        None
    }
}

/// The kind of entity a path found, as a position judges it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Found {
    Type,
    Interface,
    Parameter,
    Other(&'static str),
}

impl Found {
    const fn noun(self) -> &'static str {
        match self {
            Self::Type => "a type",
            Self::Interface => "an interface",
            Self::Parameter => "a type parameter",
            Self::Other(noun) => noun,
        }
    }
}
