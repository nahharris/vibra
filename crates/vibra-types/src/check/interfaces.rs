//! Interfaces, bounds, and implementations.
//!
//! `docs/spec/02-type-system.md`, "Interfaces" (Implementations,
//! Substitution, Associated types, Overlap), "Bounds", and "Conversion".
//! An interface's shape (its identity, parameters, and associated types) is
//! read before any type body, since a type may hold an interface value; its
//! members after every type, since a member's signature names types. Each
//! `impl`, nested in a type or in an interface, is checked against its
//! interface under its substitution, and the keys of one interface are
//! decided pairwise non-unifiable from the keys alone, so no verdict depends
//! on an implementation written anywhere else.

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::external::compiler::CompilerIntrinsic;
use vibra_ir::{FnType, NamedTypes, Substitution, Type, TypeId};
use vibra_syntax::source::{
    Callable, CallableKind, Given, Ident, Impl, Path, TypeExpr, TypeForm,
};

use super::declare::{
    BoundData, BoundGoal, Entity, FnData, GoalHead, Host, ImplData,
    InterfaceMemberData, NoType, Program, Scope, TypeKind, unguarded,
};

/// An interface as applied: its index, its positional arguments, and the
/// choices its labels fix, in canonical name order.
pub(crate) type Applied = (usize, Vec<Type>, Vec<(String, Type)>);

/// The parts of an implementation its host decides: where it is written,
/// its key, and the type parameters that key is open in.
struct ImplHead<'a> {
    module: usize,
    owner: Option<usize>,
    /// The name of the type or interface it is nested in.
    host: String,
    interface: usize,
    binders: Vec<String>,
    /// Where each binder is declared.
    binder_spans: Vec<ByteSpan>,
    bounds: Vec<Option<BoundData>>,
    receiver: Type,
    arguments: Vec<Type>,
    head: ByteSpan,
    block: Impl<'a>,
}

impl<'a> Program<'a> {
    /// The entity a written path names, without reporting.
    pub(crate) fn path_entity(&self, module: usize, path: &Path<'_>) -> Option<Entity> {
        let segments = path.segments();
        let (first, rest) = segments.split_first()?;
        let mut entity = self.lookup(module, first.text())?;
        for part in rest {
            entity = match entity {
                Entity::Module(target) => {
                    self.modules.get(target)?.scope.get(part.text()).copied()?
                }
                _ => return None,
            };
        }
        Some(entity)
    }

    /// The interface `index` applied as written: its positional arguments
    /// and the choices its labels fix, in canonical name order. A wrong
    /// argument count, a label that names no associated type, and a repeated
    /// label are `@type.typeArgumentMismatch`; so is an interface value type
    /// (`value`) that leaves an associated type open.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn interface_application(
        &self,
        module: usize,
        index: usize,
        written: TypeExpr<'_>,
        positional: &[TypeExpr<'_>],
        labelled: &[(Ident<'_>, TypeExpr<'_>)],
        scope: Scope<'_>,
        value: bool,
    ) -> Result<Applied, NoType> {
        let data = self.interfaces.get(index).ok_or(NoType::Unresolved)?;
        if data.id.is_none() {
            return Err(NoType::Unsupported);
        }
        let fault = |span: ByteSpan, message: String| {
            Err(NoType::Fault(Box::new(Diagnostic::new(
                DiagnosticCode::TypeTypeArgumentMismatch,
                span,
                message,
            ))))
        };
        if positional.len() != data.params.len() {
            return fault(
                written.span(),
                format!(
                    "`{}` takes {} positional type argument(s), but {} are written",
                    data.name,
                    data.params.len(),
                    positional.len()
                ),
            );
        }
        let mut seen: Vec<&str> = Vec::new();
        for (label, _) in labelled {
            if !data.associated.iter().any(|(name, _)| name == label.text()) {
                return fault(
                    label.span(),
                    format!(
                        "`{}` has no associated type `{}`",
                        data.name,
                        label.text()
                    ),
                );
            }
            if seen.contains(&label.text()) {
                return fault(
                    label.span(),
                    format!("the associated type `{}` is fixed twice", label.text()),
                );
            }
            seen.push(label.text());
        }
        if value && seen.len() != data.associated.len() {
            let open = data
                .associated
                .iter()
                .filter(|(name, _)| !seen.contains(&name.as_str()))
                .map(|(name, _)| format!("`{name}`"))
                .collect::<Vec<_>>()
                .join(", ");
            return fault(
                written.span(),
                format!(
                    "an interface value type fixes every associated type, and this one leaves {open} open"
                ),
            );
        }
        let arguments = positional
            .iter()
            .map(|argument| self.written(module, *argument, scope))
            .collect::<Result<Vec<_>, _>>()?;
        self.bound_goal(
            module,
            index,
            &arguments,
            positional.iter().map(TypeExpr::span).collect(),
            written.span(),
        );
        let mut choices = labelled
            .iter()
            .map(|(label, ty)| {
                Ok((label.text().to_owned(), self.written(module, *ty, scope)?))
            })
            .collect::<Result<Vec<_>, NoType>>()?;
        choices.sort_by(|left, right| left.0.cmp(&right.0));
        Ok((index, arguments, choices))
    }

    /// Records that `arguments`, written at `spans` in the application
    /// `application` of the interface `interface`, must satisfy the
    /// interface's bounds. The bounds may not be read yet, so every
    /// application with an argument is recorded.
    fn bound_goal(
        &self,
        module: usize,
        interface: usize,
        arguments: &[Type],
        spans: Vec<ByteSpan>,
        application: ByteSpan,
    ) {
        self.record_goal(
            module,
            GoalHead::Interface(interface),
            arguments,
            spans,
            application,
        );
    }

    /// Records that the written application `application` of `head` at
    /// `arguments`, each written at its span of `spans`, must satisfy what
    /// its head requires of them.
    pub(crate) fn record_goal(
        &self,
        module: usize,
        head: GoalHead,
        arguments: &[Type],
        spans: Vec<ByteSpan>,
        application: ByteSpan,
    ) {
        if arguments.is_empty() {
            return;
        }
        let goal = BoundGoal {
            module,
            head,
            arguments: arguments.to_vec(),
            spans,
            application,
        };
        let mut goals = self.goals.borrow_mut();
        if !goals.contains(&goal) {
            goals.push(goal);
        }
    }

    /// The bound written `written`: `None` for `Any`. A bound that is not an
    /// interface is `@name.wrongEntityKind` at the bound.
    fn read_bound(
        &self,
        module: usize,
        written: TypeExpr<'_>,
        scope: Scope<'_>,
    ) -> Result<Option<BoundData>, NoType> {
        let not_interface = || {
            Err(NoType::Fault(Box::new(Diagnostic::new(
                DiagnosticCode::NameWrongEntityKind,
                written.span(),
                "a bound is an interface, and this is not one",
            ))))
        };
        let Some(TypeForm::Path {
            path,
            positional,
            labelled,
        }) = written.form()
        else {
            return not_interface();
        };
        if path.dotted() == "Any"
            && positional.is_empty()
            && labelled.is_empty()
            && self.lookup(module, "Any").is_none()
        {
            return Ok(None);
        }
        match self.path_entity(module, &path) {
            Some(Entity::Interface(index)) => {
                let (interface, arguments, choices) = self.interface_application(
                    module,
                    index,
                    written,
                    &positional,
                    &labelled,
                    scope,
                    false,
                )?;
                Ok(Some(BoundData {
                    interface,
                    arguments,
                    choices,
                    span: written.span(),
                }))
            }
            Some(_) => not_interface(),
            None => Err(NoType::Unresolved),
        }
    }

    /// The bound of each of `binders` that the `given` section writes, for
    /// the parameters from `own_start` on, which are the header's own.
    pub(crate) fn read_bounds(
        &self,
        module: usize,
        given: Option<Given<'_>>,
        binders: &[String],
        own_start: usize,
        scope: Scope<'_>,
    ) -> Result<Vec<Option<BoundData>>, NoType> {
        let mut bounds = vec![None; binders.len()];
        let Some(given) = given else {
            return Ok(bounds);
        };
        for (key, written) in given.entries() {
            let own = binders.get(own_start..).unwrap_or_default();
            // A key that is no parameter of the header is resolution's.
            let Some(at) = own.iter().position(|name| name == key.text()) else {
                continue;
            };
            if let Some(bound) = self.read_bound(module, written, scope)?
                && let Some(slot) = bounds.get_mut(own_start.saturating_add(at))
            {
                *slot = Some(bound);
            }
        }
        Ok(bounds)
    }

    /// Reads every interface's identity, parameters, and associated types.
    /// An associated type with the name of a parameter is
    /// `@name.genericRedeclaration` at the associated type.
    pub(crate) fn interface_shapes(&mut self) {
        let mut faults = Vec::new();
        for index in 0..self.interfaces.len() {
            let Some(data) = self.interfaces.get(index) else {
                continue;
            };
            let view = data.view;
            let params = view
                .params()
                .iter()
                .map(|param| param.text().to_owned())
                .collect::<Vec<_>>();
            let mut associated = Vec::new();
            for assoc in view.assoc_types() {
                let Some(name) = assoc.name() else { continue };
                if let Some(param) = view
                    .params()
                    .iter()
                    .find(|param| param.text() == name.text())
                {
                    faults.push((
                        data.module,
                        Diagnostic::new(
                            DiagnosticCode::NameGenericRedeclaration,
                            name.span(),
                            format!(
                                "the associated type `{}` has the name of a type parameter of `{}`",
                                name.text(),
                                data.name
                            ),
                        )
                        .with_related(param.span(), "the type parameter"),
                    ));
                }
                associated.push((name.text().to_owned(), name.span()));
            }
            let path = self
                .modules
                .get(data.module)
                .map(|module| module.path.trim_start_matches('@').to_owned());
            let name = data.name.clone();
            let id = path.and_then(|path| {
                self.interface_identity(index)
                    .and_then(|seed| TypeId::new(format!("{path}.{name}"), seed).ok())
            });
            if let Some(data) = self.interfaces.get_mut(index) {
                data.id = id;
                data.bounds = vec![None; params.len()];
                data.params = params;
                data.associated = associated;
            }
        }
        for (module, diagnostic) in faults {
            self.fault(module, diagnostic);
        }
    }

    /// Reads the bounds and the members of every interface. A member's
    /// written result type holds `Self` only in positions of the `Self` map,
    /// or it is `@type.unsupportedSelfPosition`; a member with no receiver
    /// slot whose result does not hold `Self` is
    /// `@type.undispatchableContractMember`. A default member becomes a
    /// function, checked once over the interface's open parameters.
    pub(crate) fn interface_headers(&mut self) {
        for index in 0..self.interfaces.len() {
            let Some(data) = self.interfaces.get(index) else {
                continue;
            };
            if data.id.is_none() {
                continue;
            }
            let module = data.module;
            let view = data.view;
            let interface_name = data.name.clone();
            let params = data.params.clone();
            let own_associated = data.own_associated();
            let header_scope = Scope {
                self_type: None,
                binders: &params,
                associated: &[],
            };
            let bounds = match self.read_bounds(
                module,
                view.given(),
                &params,
                0,
                header_scope,
            ) {
                Ok(bounds) => bounds,
                Err(why) => {
                    let what = format!("the bounds of `{interface_name}`");
                    self.no_type_at(module, view.construct().span(), &what, why);
                    vec![None; params.len()]
                }
            };
            let mut members = Vec::new();
            for member in view.members() {
                let Some(name) = member.name() else { continue };
                let what = format!("the member `{interface_name}.{}`", name.text());
                if member.performs().is_some() || member.kind() == CallableKind::Proc {
                    self.unsupported(module, member.construct().span(), &what);
                    continue;
                }
                let mut binders = params.clone();
                binders.extend(
                    member.params().iter().map(|param| param.text().to_owned()),
                );
                let scope = Scope {
                    self_type: Some(&Type::SelfType),
                    binders: &binders,
                    associated: &own_associated,
                };
                let header = self
                    .read_bounds(module, member.given(), &binders, params.len(), scope)
                    .and_then(|own| {
                        Ok((own, self.callable_type(module, &member, scope)?))
                    });
                let (own_bounds, signature) = match header {
                    Ok((own, signature)) => {
                        // An associated type a member-local bound leaves open
                        // is one more member-local parameter.
                        let (all, own) = self.open_choices(&binders, own);
                        binders = all;
                        (own, signature)
                    }
                    Err(why) => {
                        self.no_type_at(module, member.construct().span(), &what, why);
                        continue;
                    }
                };
                let receiver = signature
                    .parameters
                    .iter()
                    .position(|parameter| *parameter == Type::SelfType);
                if let Some((_, written)) = member.result()
                    && !self_map_holds(self, &signature.result)
                {
                    self.fault(
                        module,
                        Diagnostic::new(
                            DiagnosticCode::TypeUnsupportedSelfPosition,
                            written.span(),
                            "`Self` stands in a result type only as the result itself, a component of a tuple or record, or an argument of `Option`, `Result`, or `List`",
                        ),
                    );
                    continue;
                }
                if receiver.is_none() && !mentions_self(&signature.result) {
                    self.fault(
                        module,
                        Diagnostic::new(
                            DiagnosticCode::TypeUndispatchableContractMember,
                            name.span(),
                            format!(
                                "`{}` has no parameter written `Self` and its result does not hold `Self`, so no call can select an implementation",
                                name.text()
                            ),
                        ),
                    );
                    continue;
                }
                let mut all_bounds = bounds.clone();
                all_bounds.extend(own_bounds.into_iter().skip(params.len()));
                let default = member.body().map(|_| {
                    self.default_function(
                        index,
                        &member,
                        binders.clone(),
                        all_bounds.clone(),
                        signature.clone(),
                    )
                });
                members.push(InterfaceMemberData {
                    name: name.text().to_owned(),
                    span: name.span(),
                    signature,
                    binders,
                    bounds: all_bounds,
                    receiver,
                    default,
                });
            }
            if let Some(data) = self.interfaces.get_mut(index) {
                data.bounds = bounds;
                data.members = members;
            }
        }
    }

    /// The function holding the default body of `member` of the interface
    /// `index`.
    fn default_function(
        &mut self,
        index: usize,
        member: &Callable<'a>,
        binders: Vec<String>,
        bounds: Vec<Option<BoundData>>,
        signature: FnType,
    ) -> usize {
        let fn_index = self.functions.len();
        let Some(data) = self.interfaces.get(index) else {
            return fn_index;
        };
        let module = data.module;
        let interface_name = data.name.clone();
        let associated = data.own_associated();
        let name = member
            .name()
            .map(|name| name.text().to_owned())
            .unwrap_or_default();
        let (path, embedded) = self
            .modules
            .get(module)
            .map(|data| (data.path.clone(), data.embedded))
            .unwrap_or_default();
        let identity = self.identity_of(&path, embedded, &interface_name, "@fn", &name);
        self.functions.push(FnData {
            module,
            owner: None,
            implements: None,
            contract: Some(index),
            name: name.clone(),
            view: *member,
            identity,
            qualified: format!(
                "{}.{interface_name}.{name}",
                path.trim_start_matches('@')
            ),
            binders,
            bounds,
            self_type: Some(Type::SelfType),
            associated,
            signature: Some(signature),
            external: None,
        });
        fn_index
    }

    /// Reports why a written type of the declaration `what` has no checked
    /// type.
    pub(crate) fn no_type_at(
        &mut self,
        module: usize,
        span: ByteSpan,
        what: &str,
        why: NoType,
    ) {
        match why {
            NoType::Unsupported => self.unsupported(module, span, what),
            NoType::Unresolved => {}
            NoType::Fault(diagnostic) => self.fault(module, *diagnostic),
        }
    }

    /// Records every implementation, nested in a type or in an interface.
    pub(crate) fn implementations(&mut self, pending: Vec<(Host, Impl<'a>)>) {
        for (host, block) in pending {
            let head = match host {
                Host::Type(owner) => self.type_impl(owner, block),
                Host::Interface(interface) => self.interface_impl(interface, block),
            };
            if let Some(head) = head {
                self.impl_body(head);
            }
        }
    }

    /// The head of an `impl` nested in the type `owner`, whose target MUST
    /// be an interface applied positionally.
    fn type_impl(&mut self, owner: usize, block: Impl<'a>) -> Option<ImplHead<'a>> {
        let data = self.types.get(owner)?;
        let module = data.module;
        let head = impl_head(block);
        let host = data.name.clone();
        let Some(receiver) = data.self_type() else {
            let what = format!("an implementation in `{host}`");
            self.unsupported(module, head, &what);
            return None;
        };
        let binders = data.params.clone();
        let binder_spans = data
            .view
            .params()
            .iter()
            .map(|param| param.span())
            .collect();
        let target = block.target()?;
        let Some(TypeForm::Path {
            path,
            positional,
            labelled,
        }) = target.form()
        else {
            self.fault(
                module,
                Diagnostic::new(
                    DiagnosticCode::NameWrongEntityKind,
                    target.span(),
                    "an implementation nested in a type names an interface",
                ),
            );
            return None;
        };
        let interface = match self.path_entity(module, &path) {
            Some(Entity::Interface(interface)) => interface,
            Some(_) => {
                self.fault(
                    module,
                    Diagnostic::new(
                        DiagnosticCode::NameWrongEntityKind,
                        target.span(),
                        format!(
                            "`{}` is not an interface; an implementation nested in a type names the interface it implements",
                            path.dotted()
                        ),
                    ),
                );
                return None;
            }
            None => return None,
        };
        let idata = self.interfaces.get(interface)?;
        if idata.id.is_none() {
            let what = format!("an implementation in `{host}`");
            self.unsupported(module, head, &what);
            return None;
        }
        if let Some((label, _)) = labelled.first() {
            self.fault(
                module,
                Diagnostic::new(
                    DiagnosticCode::TypeTypeArgumentMismatch,
                    label.span(),
                    "an implementation's target admits no label: its key is positional, and an associated type is chosen inside the `impl`",
                ),
            );
            return None;
        }
        if positional.len() != idata.params.len() {
            let message = format!(
                "`{}` takes {} positional type argument(s), but {} are written",
                idata.name,
                idata.params.len(),
                positional.len()
            );
            self.fault(
                module,
                Diagnostic::new(
                    DiagnosticCode::TypeTypeArgumentMismatch,
                    target.span(),
                    message,
                ),
            );
            return None;
        }
        let scope = Scope {
            self_type: Some(&receiver),
            binders: &binders,
            associated: &[],
        };
        let mut arguments = Vec::new();
        for argument in positional.iter().copied() {
            match self.written(module, argument, scope) {
                Ok(ty) => arguments.push(ty),
                Err(why) => {
                    let what = format!("an implementation in `{host}`");
                    self.no_type_at(module, head, &what, why);
                    return None;
                }
            }
        }
        self.bound_goal(
            module,
            interface,
            &arguments,
            positional.iter().map(TypeExpr::span).collect(),
            target.span(),
        );
        // An implementation declares no type parameter: it passes the
        // type's through, bounded as the type bounds them.
        let bounds = self
            .types
            .get(owner)
            .map_or_else(|| vec![None; binders.len()], |data| data.bounds.clone());
        // An associated type the type's bound leaves open is one more
        // parameter of the implementation, which selecting it determines.
        let (binders, bounds) = self.open_choices(&binders, bounds);
        let mut binder_spans: Vec<ByteSpan> = binder_spans;
        binder_spans.resize(binders.len(), head);
        Some(ImplHead {
            module,
            owner: Some(owner),
            host,
            interface,
            binders,
            binder_spans,
            bounds,
            receiver,
            arguments,
            head,
            block,
        })
    }

    /// The head of an `impl` nested in the interface `interface`, whose
    /// target MUST be a closed declared or builtin type of another module.
    fn interface_impl(
        &mut self,
        interface: usize,
        block: Impl<'a>,
    ) -> Option<ImplHead<'a>> {
        let idata = self.interfaces.get(interface)?;
        let module = idata.module;
        let host = idata.name.clone();
        let head = impl_head(block);
        let whole = block.node().span();
        if idata.id.is_none() {
            let what = format!("an implementation in `{host}`");
            self.unsupported(module, head, &what);
            return None;
        }
        let binders = idata.params.clone();
        let binder_spans = idata
            .view
            .params()
            .iter()
            .map(|param| param.span())
            .collect();
        let bounds = idata.bounds.clone();
        let target = block.target()?;
        let wrong_kind = |message: &str| {
            Diagnostic::new(
                DiagnosticCode::NameWrongEntityKind,
                whole,
                message.to_owned(),
            )
        };
        let entity = match target.form() {
            Some(TypeForm::Path { path, .. }) => {
                if path.dotted() == "Any" && self.lookup(module, "Any").is_none() {
                    None
                } else {
                    match self.path_entity(module, &path) {
                        Some(entity) => Some(entity),
                        None => return None,
                    }
                }
            }
            _ => {
                let diagnostic = wrong_kind(
                    "an implementation is for a declared or builtin type, never for an anonymous type",
                );
                self.fault(module, diagnostic);
                return None;
            }
        };
        match entity {
            Some(Entity::Type(index)) => {
                let target_data = self.types.get(index)?;
                if target_data.kind == TypeKind::Scalar(Type::Never) {
                    let diagnostic = wrong_kind(
                        "`Never` satisfies only `Any` and has no implementation",
                    );
                    self.fault(module, diagnostic);
                    return None;
                }
                if target_data.module == module {
                    let message = format!(
                        "`{}` is declared in this module, so its implementations are nested in it",
                        target_data.name
                    );
                    self.fault(
                        module,
                        Diagnostic::new(
                            DiagnosticCode::TypeRedundantImplementation,
                            head,
                            message,
                        ),
                    );
                    return None;
                }
            }
            Some(_) | None => {
                let diagnostic = wrong_kind(
                    "an implementation nested in an interface is for a declared or builtin type, not for an interface value type",
                );
                self.fault(module, diagnostic);
                return None;
            }
        }
        let receiver = match self.written(module, target, Scope::default()) {
            Ok(receiver) => receiver,
            Err(why) => {
                let what = format!("an implementation in `{host}`");
                self.no_type_at(module, head, &what, why);
                return None;
            }
        };
        let arguments = (0..binders.len()).map(Type::Binder).collect();
        Some(ImplHead {
            module,
            owner: None,
            host,
            interface,
            binders,
            binder_spans,
            bounds,
            receiver,
            arguments,
            head,
            block,
        })
    }

    /// Checks an implementation's choices and members against its interface
    /// under its substitution, and records it.
    #[allow(clippy::too_many_lines)]
    fn impl_body(&mut self, head: ImplHead<'a>) {
        let ImplHead {
            module,
            owner,
            host,
            interface,
            binders,
            binder_spans,
            bounds,
            receiver,
            arguments,
            head,
            block,
        } = head;
        let Some(idata) = self.interfaces.get(interface) else {
            return;
        };
        let interface_name = idata.name.clone();
        let interface_module = idata.module;
        let declared_associated = idata.associated.clone();
        let declared_members = idata.members.clone();
        let written_members = idata
            .view
            .members()
            .iter()
            .filter_map(|member| member.name().map(|name| name.text().to_owned()))
            .collect::<Vec<_>>();
        let mut faults: Vec<Diagnostic> = Vec::new();
        let mut usable = true;
        for (name, _) in &declared_associated {
            if let Some(at) = binders.iter().position(|binder| binder == name) {
                let mut diagnostic = Diagnostic::new(
                    DiagnosticCode::NameGenericRedeclaration,
                    head,
                    format!(
                        "this implementation brings the associated type `{name}` into a scope where a type parameter has that name"
                    ),
                );
                if let Some(span) = binder_spans.get(at) {
                    diagnostic =
                        self.related(diagnostic, module, *span, "the type parameter");
                }
                faults.push(diagnostic);
                usable = false;
            }
        }
        let choice_scope = Scope {
            self_type: Some(&receiver),
            binders: &binders,
            associated: &[],
        };
        let mut chosen: Vec<(String, Type, ByteSpan)> = Vec::new();
        for choice in block.choices() {
            let Some(name) = choice.name() else {
                usable = false;
                continue;
            };
            if !declared_associated
                .iter()
                .any(|(declared, _)| declared == name.text())
            {
                faults.push(Diagnostic::new(
                    DiagnosticCode::NameUnknownSymbol,
                    name.span(),
                    format!(
                        "`{interface_name}` has no associated type `{}`",
                        name.text()
                    ),
                ));
                usable = false;
                continue;
            }
            if let Some((_, _, first)) =
                chosen.iter().find(|(chosen, _, _)| chosen == name.text())
            {
                let diagnostic = Diagnostic::new(
                    DiagnosticCode::NameRedeclaration,
                    name.span(),
                    format!("the associated type `{}` is chosen twice", name.text()),
                )
                .with_related(*first, "the first choice");
                faults.push(diagnostic);
                usable = false;
                continue;
            }
            let Some(written) = choice.chosen() else {
                usable = false;
                continue;
            };
            let names = declared_associated
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>();
            if let Some(span) =
                associated_reference(self, module, written, &names, &binders)
            {
                faults.push(Diagnostic::new(
                    DiagnosticCode::NameUnknownSymbol,
                    span,
                    "a choice is read before the associated names of its own implementation are in scope, so it cannot name one",
                ));
                usable = false;
                continue;
            }
            match self.written(module, written, choice_scope) {
                Ok(ty) => chosen.push((name.text().to_owned(), ty, name.span())),
                Err(why) => {
                    let what = format!("a choice in `{host}`");
                    self.no_type_at(module, written.span(), &what, why);
                    usable = false;
                }
            }
        }
        for (name, span) in &declared_associated {
            if !chosen.iter().any(|(chosen, _, _)| chosen == name) {
                let diagnostic = Diagnostic::new(
                    DiagnosticCode::TypeMissingAbstractMember,
                    head,
                    format!(
                        "this implementation of `{interface_name}` does not choose `{name}`"
                    ),
                );
                faults.push(self.related(
                    diagnostic,
                    interface_module,
                    *span,
                    "the associated type",
                ));
                usable = false;
            }
        }
        let mut choices = chosen
            .iter()
            .map(|(name, ty, _)| (name.clone(), ty.clone()))
            .collect::<Vec<_>>();
        choices.sort_by(|left, right| left.0.cmp(&right.0));
        let named_choices =
            NamedTypes::new(choices.clone()).unwrap_or_else(|_| NamedTypes::empty());
        let dotted = self
            .modules
            .get(module)
            .map(|data| data.path.trim_start_matches('@').to_owned())
            .unwrap_or_default();
        let key = if arguments.is_empty() {
            interface_name.clone()
        } else {
            format!(
                "{interface_name}({})",
                arguments
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" ")
            )
        };
        let choices_complete = declared_associated
            .iter()
            .all(|(name, _)| chosen.iter().any(|(chosen, _, _)| chosen == name));
        let mut members: Vec<(String, usize)> = Vec::new();
        let mut seen: Vec<(String, ByteSpan)> = Vec::new();
        for member in block.members() {
            let Some(name) = member.name() else {
                usable = false;
                continue;
            };
            if let Some((_, first)) = seen.iter().find(|(seen, _)| seen == name.text())
            {
                let diagnostic = Diagnostic::new(
                    DiagnosticCode::NameMemberCollision,
                    name.span(),
                    format!(
                        "`{}` is supplied twice in this implementation",
                        name.text()
                    ),
                )
                .with_related(*first, "the first");
                faults.push(diagnostic);
                usable = false;
                continue;
            }
            seen.push((name.text().to_owned(), name.span()));
            let Some(declared) = declared_members
                .iter()
                .find(|declared| declared.name == name.text())
            else {
                if !written_members.iter().any(|written| written == name.text()) {
                    faults.push(Diagnostic::new(
                        DiagnosticCode::NameUnknownSymbol,
                        name.span(),
                        format!("`{interface_name}` has no member `{}`", name.text()),
                    ));
                }
                usable = false;
                continue;
            };
            if let Some(default) = declared.default {
                let span = self
                    .functions
                    .get(default)
                    .and_then(|data| data.view.name())
                    .map_or(declared.span, |name| name.span());
                let diagnostic = Diagnostic::new(
                    DiagnosticCode::TypeDefaultOverride,
                    name.span(),
                    format!(
                        "`{interface_name}.{}` is a default member, so an implementation does not supply it again",
                        name.text()
                    ),
                );
                faults.push(self.related(
                    diagnostic,
                    interface_module,
                    span,
                    "the default",
                ));
                usable = false;
                continue;
            }
            let what = format!(
                "the member `{}` of an implementation in `{host}`",
                name.text()
            );
            if !unguarded(&member) || member.kind() == CallableKind::Proc {
                self.unsupported(module, member.construct().span(), &what);
                usable = false;
                continue;
            }
            let declared_own = declared.binders.len().saturating_sub(arguments.len());
            let mut member_binders = binders.clone();
            member_binders
                .extend(member.params().iter().map(|param| param.text().to_owned()));
            let scope = Scope {
                self_type: Some(&receiver),
                binders: &member_binders,
                associated: &choices,
            };
            let header = self
                .read_bounds(
                    module,
                    member.given(),
                    &member_binders,
                    binders.len(),
                    scope,
                )
                .and_then(|own| Ok((own, self.callable_type(module, &member, scope)?)));
            let (own_bounds, signature) = match header {
                Ok((own, signature)) => {
                    let (all, own) = self.open_choices(&member_binders, own);
                    member_binders = all;
                    (own, signature)
                }
                Err(why) => {
                    self.no_type_at(module, member.construct().span(), &what, why);
                    usable = false;
                    continue;
                }
            };
            let own = member_binders.len().saturating_sub(binders.len());
            let mut substitution_binders = arguments.clone();
            substitution_binders.extend(
                (0..declared_own)
                    .map(|at| Type::Binder(binders.len().saturating_add(at))),
            );
            let expected =
                Type::function(declared.signature.clone()).substitute(&Substitution {
                    binders: &substitution_binders,
                    self_type: Some(&receiver),
                    associated: Some(&named_choices),
                });
            let differing = if own == declared_own {
                differing_type(&member, &signature, &expected)
            } else {
                Some(name.span())
            };
            // With a choice missing or faulty the substitution is not known,
            // and that fault is already reported.
            if let Some(span) = differing.filter(|_| choices_complete) {
                let diagnostic = Diagnostic::new(
                    DiagnosticCode::TypeMismatch,
                    span,
                    format!(
                        "this member's signature differs from `{interface_name}.{}` under the implementation's substitution, which is `{expected}`",
                        name.text()
                    ),
                );
                faults.push(self.related(
                    diagnostic,
                    interface_module,
                    declared.span,
                    "the contract member",
                ));
                usable = false;
                continue;
            }
            let substitution = Substitution {
                binders: &substitution_binders,
                self_type: Some(&receiver),
                associated: Some(&named_choices),
            };
            let differing_bound = (0..own).find_map(|at| {
                let contract = declared
                    .bounds
                    .get(arguments.len().saturating_add(at))
                    .cloned()
                    .flatten()
                    .map(|bound| substituted_bound(bound, &substitution));
                let written = own_bounds
                    .get(binders.len().saturating_add(at))
                    .cloned()
                    .flatten();
                let same = match (&contract, &written) {
                    (None, None) => true,
                    (Some(contract), Some(written)) => {
                        contract.interface == written.interface
                            && contract.arguments == written.arguments
                            && contract.choices == written.choices
                    }
                    _ => false,
                };
                let at_span = written
                    .as_ref()
                    .map(|bound| bound.span)
                    .or_else(|| member.params().get(at).map(|param| param.span()));
                (!same).then_some((at_span, contract))
            });
            if let Some((span, contract)) = differing_bound.filter(|_| choices_complete)
            {
                let diagnostic = Diagnostic::new(
                    DiagnosticCode::TypeMismatch,
                    span.unwrap_or_else(|| name.span()),
                    format!(
                        "this member's type parameters are bounded otherwise than those of `{interface_name}.{}` under the implementation's substitution",
                        name.text()
                    ),
                );
                let (related, note) = match contract {
                    Some(bound) => (bound.span, "the contract's bound"),
                    None => (declared.span, "the contract member"),
                };
                faults.push(self.related(diagnostic, interface_module, related, note));
                usable = false;
                continue;
            }
            let external = member
                .symbol()
                .and_then(|symbol| symbol.value())
                .and_then(|symbol| CompilerIntrinsic::from_symbol(&symbol));
            if external.is_none() && member.body().is_none() {
                usable = false;
                continue;
            }
            let mut all_bounds = bounds.clone();
            all_bounds.extend(own_bounds.into_iter().skip(binders.len()));
            let fn_index = self.functions.len();
            self.functions.push(FnData {
                module,
                owner,
                implements: Some(interface),
                contract: None,
                name: name.text().to_owned(),
                view: member,
                identity: String::new(),
                qualified: format!("{dotted}.{host}.{key}.{}", name.text()),
                binders: member_binders,
                bounds: all_bounds,
                self_type: Some(receiver.clone()),
                associated: choices.clone(),
                signature: Some(signature),
                external,
            });
            members.push((name.text().to_owned(), fn_index));
        }
        for declared in &declared_members {
            if declared.default.is_none()
                && !members.iter().any(|(name, _)| *name == declared.name)
            {
                if seen.iter().any(|(name, _)| *name == declared.name) {
                    continue;
                }
                let diagnostic = Diagnostic::new(
                    DiagnosticCode::TypeMissingAbstractMember,
                    head,
                    format!(
                        "this implementation of `{interface_name}` does not supply `{}`",
                        declared.name
                    ),
                );
                faults.push(self.related(
                    diagnostic,
                    interface_module,
                    declared.span,
                    "the contract member",
                ));
                usable = false;
            }
        }
        let written_count = declared_members.len();
        if written_count < written_members.len() {
            // A member of the interface this checker skipped: its
            // implementations cannot be checked against it.
            usable = false;
        }
        for diagnostic in faults {
            self.fault(module, diagnostic);
        }
        if !usable {
            for (_, function) in &members {
                if let Some(data) = self.functions.get_mut(*function) {
                    data.signature = None;
                }
            }
            members.clear();
        }
        self.impls.push(ImplData {
            module,
            interface,
            binders,
            bounds,
            receiver,
            arguments,
            choices,
            members,
            head,
            usable,
        });
    }

    /// `diagnostic` related to `span` of `module`.
    pub(crate) fn related(
        &self,
        diagnostic: Diagnostic,
        module: usize,
        span: ByteSpan,
        message: &str,
    ) -> Diagnostic {
        match self.modules.get(module) {
            Some(data) => {
                diagnostic.with_related_source(data.source_id.as_ref(), span, message)
            }
            None => diagnostic.with_related(span, message),
        }
    }

    /// Decides overlap: two keys of one interface whose receivers and
    /// positional arguments unify, and two sources of one receiver's `From`
    /// and `TryFrom` implementations that unify. The later of each pair is
    /// reported, related to the earlier, and is selected by nothing.
    pub(crate) fn overlaps(&mut self) {
        let from = self.prelude_interface("From");
        let try_from = self.prelude_interface("TryFrom");
        let mut faults = Vec::new();
        for later in 0..self.impls.len() {
            let Some(late) = self.impls.get(later) else {
                continue;
            };
            for earlier in 0..later {
                let Some(early) = self.impls.get(earlier) else {
                    continue;
                };
                let conversion = late.interface != early.interface
                    && [from, try_from].contains(&Some(late.interface))
                    && [from, try_from].contains(&Some(early.interface));
                if late.interface != early.interface && !conversion {
                    continue;
                }
                let key = |data: &ImplData| {
                    let mut key = vec![data.receiver.clone()];
                    if conversion {
                        key.extend(data.arguments.first().cloned());
                    } else {
                        key.extend(data.arguments.iter().cloned());
                    }
                    key
                };
                if !unifiable(
                    &key(early),
                    early.binders.len(),
                    &key(late),
                    late.binders.len(),
                ) {
                    continue;
                }
                let (code, message) = if conversion {
                    (
                        DiagnosticCode::TypeRedundantConversion,
                        "this conversion's source unifies with the source of another `From` or `TryFrom` of the same receiver",
                    )
                } else {
                    (
                        DiagnosticCode::TypeOverlappingImplementation,
                        "this implementation's key unifies with the key of another implementation of the same interface",
                    )
                };
                let diagnostic = Diagnostic::new(code, late.head, message);
                let diagnostic = self.related(
                    diagnostic,
                    early.module,
                    early.head,
                    "the earlier implementation",
                );
                faults.push((later, late.module, diagnostic));
                break;
            }
        }
        for (later, module, diagnostic) in faults {
            if let Some(data) = self.impls.get_mut(later) {
                data.usable = false;
            }
            self.fault(module, diagnostic);
        }
    }

    /// The interface a prelude name denotes.
    pub(crate) fn prelude_interface(&self, name: &str) -> Option<usize> {
        match self.prelude.get(name) {
            Some(Entity::Interface(index)) => Some(*index),
            _ => None,
        }
    }
}

/// The `impl` keyword through the end of its target.
fn impl_head(block: Impl<'_>) -> ByteSpan {
    let start = block.node().span().start();
    let end = block
        .target()
        .map_or(start.saturating_add("impl".len()), |target| {
            target.span().end()
        });
    ByteSpan::new(start, end)
}

/// The span of the first written type of `member` whose checked type
/// differs from `expected`, a callable type: a parameter, then the result.
fn differing_type(
    member: &Callable<'_>,
    signature: &FnType,
    expected: &Type,
) -> Option<ByteSpan> {
    let Type::Fn(expected) = expected else {
        return member.name().map(|name| name.span());
    };
    let takes = member.takes();
    let names = |labels: &vibra_ir::NamedTypes| {
        labels
            .entries()
            .iter()
            .map(|(label, _)| label.clone())
            .collect::<Vec<_>>()
    };
    if takes.len() != expected.parameters.len()
        || names(&signature.labels) != names(&expected.labels)
    {
        return member.name().map(|name| name.span());
    }
    for ((_, written), (actual, wanted)) in takes
        .iter()
        .zip(signature.parameters.iter().zip(&expected.parameters))
    {
        if actual != wanted {
            return Some(written.span());
        }
    }
    for (label, written, _) in member.labelled() {
        if signature.labels.get(label.text()) != expected.labels.get(label.text()) {
            return Some(written.span());
        }
    }
    if signature.result != expected.result {
        return member
            .result()
            .map(|(_, written)| written.span())
            .or_else(|| member.name().map(|name| name.span()));
    }
    None
}

/// `bound` with its positional arguments and choices under `substitution`.
fn substituted_bound(bound: BoundData, substitution: &Substitution<'_>) -> BoundData {
    BoundData {
        arguments: bound
            .arguments
            .iter()
            .map(|ty| ty.substitute(substitution))
            .collect(),
        choices: bound
            .choices
            .iter()
            .map(|(name, ty)| (name.clone(), ty.substitute(substitution)))
            .collect(),
        ..bound
    }
}

/// The span of a bare name in `written` that names one of the associated
/// types `names` and nothing else in scope.
fn associated_reference(
    program: &Program<'_>,
    module: usize,
    written: TypeExpr<'_>,
    names: &[&str],
    binders: &[String],
) -> Option<ByteSpan> {
    let mut pending = vec![written];
    while let Some(written) = pending.pop() {
        match written.form() {
            Some(TypeForm::Path {
                path,
                positional,
                labelled,
            }) => {
                let segments = path.segments();
                if let [only] = segments.as_slice()
                    && names.contains(&only.text())
                    && !binders.iter().any(|binder| binder == only.text())
                    && program.lookup(module, only.text()).is_none()
                {
                    return Some(only.span());
                }
                pending.extend(positional);
                pending.extend(labelled.into_iter().map(|(_, ty)| ty));
            }
            Some(TypeForm::Tuple(components)) => pending.extend(components),
            Some(TypeForm::Record(fields)) => {
                pending.extend(fields.into_iter().map(|(_, ty)| ty))
            }
            Some(
                TypeForm::Fn { inputs, result } | TypeForm::Proc { inputs, result, .. },
            ) => {
                pending.extend(inputs.positional());
                pending.extend(inputs.labelled().into_iter().map(|(_, ty)| ty));
                pending.push(result);
            }
            Some(TypeForm::Intent { result, .. }) => pending.push(result),
            Some(TypeForm::Atom(_)) | None => {}
        }
    }
    None
}

/// Whether `ty` holds `Self`.
pub(crate) fn mentions_self(ty: &Type) -> bool {
    let mut found = false;
    ty.visit(&mut |inner| found |= *inner == Type::SelfType);
    found
}

/// Whether every `Self` in the result type `ty` stands in a position of the
/// `Self` map: the result itself, a component of a tuple or record, or an
/// argument of `Option`, `Result`, or `List`.
fn self_map_holds(program: &Program<'_>, ty: &Type) -> bool {
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        match ty {
            Type::SelfType => {}
            Type::Tuple(_) | Type::Record(_) | Type::List(_) => {
                pending.extend(ty.components())
            }
            Type::Declared(id, arguments)
                if matches!(
                    program.library_of(id),
                    Some(vibra_ir::Library::Option | vibra_ir::Library::Result)
                ) =>
            {
                pending.extend(arguments);
            }
            other if mentions_self(other) => return false,
            _ => {}
        }
    }
    true
}

/// Whether two types have the same outermost form: the same scalar, the
/// same declared type or interface, or the same shape of tuple, record,
/// list, or callable.
pub(crate) fn same_head(left: &Type, right: &Type) -> bool {
    match (left, right) {
        (Type::Declared(a, x), Type::Declared(b, y)) => a == b && x.len() == y.len(),
        (Type::Interface(a, x, p), Type::Interface(b, y, q)) => {
            a == b && x.len() == y.len() && p.names().eq(q.names())
        }
        (Type::Tuple(x), Type::Tuple(y)) => x.len() == y.len(),
        (Type::Record(x), Type::Record(y)) => x.names().eq(y.names()),
        (Type::List(_), Type::List(_)) => true,
        (Type::Fn(x), Type::Fn(y)) => {
            x.parameters.len() == y.parameters.len()
                && x.labels.names().eq(y.labels.names())
        }
        _ => {
            left.components().is_empty()
                && right.components().is_empty()
                && left == right
        }
    }
}

/// Whether some substitution of both sides' type parameters makes the two
/// lists of types equal: the left's parameters are binders `0..left_count`,
/// the right's are binders `0..right_count` of their own.
pub(crate) fn unifiable(
    left: &[Type],
    left_count: usize,
    right: &[Type],
    right_count: usize,
) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let shift = (0..right_count)
        .map(|at| Type::Binder(left_count.saturating_add(at)))
        .collect::<Vec<_>>();
    let mut assignment: Vec<Option<Type>> =
        vec![None; left_count.saturating_add(right_count)];
    let resolve = |ty: &Type, assignment: &[Option<Type>]| {
        let mut current = ty.clone();
        for _ in 0..=assignment.len() {
            match &current {
                Type::Binder(at) => match assignment.get(*at) {
                    Some(Some(bound)) => current = bound.clone(),
                    _ => break,
                },
                _ => break,
            }
        }
        current
    };
    let mut pending = left
        .iter()
        .cloned()
        .zip(right.iter().map(|ty| ty.instantiate(&shift)))
        .collect::<Vec<_>>();
    let mut steps = 0_usize;
    while let Some((a, b)) = pending.pop() {
        steps = steps.saturating_add(1);
        if steps > 1 << 16 {
            return true;
        }
        let a = resolve(&a, &assignment);
        let b = resolve(&b, &assignment);
        if a == b {
            continue;
        }
        match (&a, &b) {
            (Type::Binder(at), _) => {
                if let Some(slot) = assignment.get_mut(*at) {
                    *slot = Some(b.clone());
                }
            }
            (_, Type::Binder(at)) => {
                if let Some(slot) = assignment.get_mut(*at) {
                    *slot = Some(a.clone());
                }
            }
            _ if same_head(&a, &b) => {
                pending.extend(
                    a.components()
                        .into_iter()
                        .cloned()
                        .zip(b.components().into_iter().cloned()),
                );
            }
            _ => return false,
        }
    }
    true
}
