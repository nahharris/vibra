//! Canonical encoding of checked declarations and their owned parts.

use vibra_ir::{CheckedFunction, TypeBody, TypeDefinition};
use vibra_resolve::{DeclarationId, ResolvedDeclaration, ResolvedSnapshot};
use vibra_syntax::{Attribute, DeftypeBody, FunctionDeclaration, Literal, TypeMember};
use vibra_types::{CheckedNamespaceRole, IndexedImplementation};

use super::*;

pub(super) type GenericBinderSet = (BTreeMap<String, u64>, Vec<DefinitionNode>, u64);

pub(super) fn attributes(
    items: &[Attribute],
    source_id: &str,
    checked: &ResolvedCheckResult,
    binders: &mut BTreeMap<String, u64>,
    next: &mut u64,
) -> Result<DefinitionNode, GraphEncodingError> {
    let mut items = items.iter().collect::<Vec<_>>();
    items.sort_by_key(|attribute| attribute.canonical_order());
    let mut nodes = Vec::new();
    for item in items {
        match item {
            Attribute::Where(generics) => {
                for generic in generics {
                    let index = *next;
                    *next = next.checked_add(1).ok_or_else(|| {
                        GraphEncodingError::MalformedInput(
                            "binder count exceeds u64".to_owned(),
                        )
                    })?;
                    binders.insert(generic.name().value().to_owned(), index);
                    let bound = checked
                        .namespace_facts()
                        .iter()
                        .find(|fact| {
                            fact.source_id() == source_id
                                && fact.written() == generic.bound().value()
                                && fact.role() == CheckedNamespaceRole::Interface
                        })
                        .map(|fact| fact.identity().to_owned());
                    let bound = bound.map_or_else(
                        || {
                            if generic.bound().value() == "any" {
                                encoding::atom("any")
                            } else {
                                DefinitionNode::Void
                            }
                        },
                        DefinitionNode::Reference,
                    );
                    if matches!(bound, DefinitionNode::Void) {
                        return Err(GraphEncodingError::IncompleteProjection(format!(
                            "generic bound `{}` in {source_id} has no checked interface identity",
                            generic.bound().value()
                        )));
                    }
                    nodes.push(encoding::tagged(
                        "generic",
                        [
                            (
                                "binder".to_owned(),
                                DefinitionNode::Binder {
                                    kind: "type".to_owned(),
                                    index,
                                },
                            ),
                            ("bound".to_owned(), bound),
                        ],
                    ));
                }
            }
            Attribute::Visibility(value) => nodes.push(encoding::tagged(
                "visibility",
                [("value".to_owned(), encoding::atom(value.value()))],
            )),
            Attribute::Role(value) => nodes.push(encoding::tagged(
                "role",
                [("value".to_owned(), encoding::atom(value.value()))],
            )),
            Attribute::External(value) => nodes.push(encoding::tagged(
                "external",
                [("value".to_owned(), encoding::atom(value.value()))],
            )),
            Attribute::Symbol(value) => nodes.push(encoding::tagged(
                "symbol",
                [("value".to_owned(), literal_node(value)?)],
            )),
            Attribute::Native(value) => nodes.push(encoding::tagged(
                "native",
                [("value".to_owned(), literal_node(value)?)],
            )),
            Attribute::Labelled(_)
            | Attribute::Variadic(_)
            | Attribute::Effects(_)
            | Attribute::Doc(_) => {}
        }
    }
    Ok(encoding::array(nodes))
}

fn literal_node(value: &Literal) -> Result<DefinitionNode, GraphEncodingError> {
    Ok(match value {
        Literal::String(value) => encoding::string(value.value()),
        Literal::Character(value) => DefinitionNode::Character(value.value()),
        Literal::Boolean(value) => {
            encoding::atom(if value.value() { "true" } else { "false" })
        }
        Literal::Void(_) => DefinitionNode::Void,
        Literal::Integer(value) => DefinitionNode::Integer {
            ty: value
                .suffix()
                .map_or("default", |suffix| suffix.as_str())
                .to_owned(),
            value: format!(
                "{}{}",
                if value.is_negative() { "-" } else { "" },
                value.digits()
            ),
        },
        Literal::Float(value) => DefinitionNode::Float {
            ty: value
                .suffix()
                .map_or("default", |suffix| suffix.as_str())
                .to_owned(),
            value: value.body().to_owned(),
        },
    })
}

pub(super) fn generic_binders(
    items: &[Attribute],
    source_id: &str,
    checked: &ResolvedCheckResult,
) -> Result<GenericBinderSet, GraphEncodingError> {
    let mut map = BTreeMap::new();
    let mut next = 0;
    let node = attributes(items, source_id, checked, &mut map, &mut next)?;
    let nodes = match node {
        DefinitionNode::List(values) => values,
        _ => Vec::new(),
    };
    Ok((map, nodes, next))
}

fn generic_references(
    items: &[Attribute],
    source_id: &str,
    checked: &ResolvedCheckResult,
    part: &[String],
) -> Result<Vec<PendingReference>, GraphEncodingError> {
    let mut references = Vec::new();
    for item in items {
        let Attribute::Where(generics) = item else {
            continue;
        };
        for generic in generics {
            let identity = checked
                .namespace_facts()
                .iter()
                .find(|fact| {
                    fact.source_id() == source_id
                        && fact.written() == generic.bound().value()
                        && fact.role() == CheckedNamespaceRole::Interface
                })
                .map(|fact| fact.identity().to_owned());
            let Some(identity) = identity else {
                if generic.bound().value() == "any" {
                    continue;
                }
                return Err(GraphEncodingError::IncompleteProjection(format!(
                    "generic bound `{}` in {source_id} has no checked interface identity",
                    generic.bound().value()
                )));
            };
            references.push(PendingReference {
                identity,
                kind: EdgeKind::Contract,
                part: part.to_vec(),
                dispatch: None,
            });
        }
    }
    Ok(references)
}

pub(super) fn function_body(
    function: &CheckedFunction,
    scope: &Scope<'_>,
    owner_identity: &str,
    booleans: &BTreeMap<bool, String>,
    type_binders: &BTreeMap<String, u64>,
    binder_base: u64,
    part: Vec<String>,
) -> Result<(DefinitionNode, Vec<PendingReference>, u64), GraphEncodingError> {
    let mut references = Vec::new();
    let mut next = binder_base.max(
        type_binders
            .values()
            .max()
            .copied()
            .map_or(0, |value| value.saturating_add(1)),
    );
    let fixed = function.signature().fixed_parameter_count();
    let slots = (0..fixed)
        .map(|slot| (slot, next.saturating_add(slot as u64)))
        .collect::<BTreeMap<_, _>>();
    next = next.saturating_add(fixed as u64);
    let mut projection = projection(
        scope,
        owner_identity,
        type_binders,
        booleans,
        &mut references,
        &mut next,
        part,
    );
    projection.slots = slots;
    let signature = projection.signature_node(function.signature())?;
    let (semantic_body, _) = function.semantic_body();
    let body = projection.expression(semantic_body)?;
    let lowered = if semantic_body != function.body() {
        projection.expression(function.body())?
    } else {
        DefinitionNode::Void
    };
    let native = function
        .native_body()
        .map(|body| projection.expression(body))
        .transpose()?
        .unwrap_or(DefinitionNode::Void);
    let mut implements = DefinitionNode::Void;
    if let Some(implementation) = function.implements() {
        let identity = format!(
            "{}.{}",
            implementation.interface.id(),
            implementation.member
        );
        let part = projection.part.clone();
        projection.references.push(PendingReference {
            identity: implementation.interface.id().to_owned(),
            kind: EdgeKind::Implementation,
            part: part.clone(),
            dispatch: None,
        });
        projection.references.push(PendingReference {
            identity: identity.clone(),
            kind: EdgeKind::Contract,
            part,
            dispatch: None,
        });
        let arguments = implementation
            .arguments
            .iter()
            .map(|value| projection.type_node(value))
            .collect::<Result<Vec<_>, _>>()?;
        implements = encoding::tagged(
            "implements",
            [
                ("contract".to_owned(), DefinitionNode::Reference(identity)),
                (
                    "receiver".to_owned(),
                    projection.type_node(&implementation.receiver)?,
                ),
                ("arguments".to_owned(), encoding::array(arguments)),
            ],
        );
    }
    let owner = if let Some(root) = function.effect_owner() {
        projection.references.push(PendingReference {
            identity: root.id().to_owned(),
            kind: EdgeKind::EffectOwner,
            part: projection.part.clone(),
            dispatch: None,
        });
        DefinitionNode::Reference(root.id().to_owned())
    } else {
        DefinitionNode::Void
    };
    let form = encoding::tagged(
        "checked-function",
        [
            ("signature".to_owned(), signature),
            ("body".to_owned(), body),
            ("lowered-body".to_owned(), lowered),
            ("native-body".to_owned(), native),
            ("implements".to_owned(), implements),
            ("effect-owner".to_owned(), owner),
        ],
    );
    drop(projection);
    Ok((form, references, next))
}

// Keep the immutable checker and source facts explicit at this encoding boundary.
#[allow(clippy::too_many_arguments)]
pub(super) fn function_form(
    declaration: &FunctionDeclaration,
    identity: &str,
    scope: &Scope<'_>,
    booleans: &BTreeMap<bool, String>,
    source_id: &str,
    (mut type_binders, _generic_nodes, mut next): GenericBinderSet,
    checked: &ResolvedCheckResult,
    part: Vec<String>,
    allow_abstract: bool,
) -> Result<
    (
        DefinitionNode,
        Vec<PendingReference>,
        Vec<ObjectOrigin>,
        u64,
    ),
    GraphEncodingError,
> {
    let attr_node = attributes(
        declaration.attributes().items(),
        source_id,
        checked,
        &mut type_binders,
        &mut next,
    )?;
    let (body, mut references, next) = if let Some(function) = scope.function(identity)
    {
        function_body(
            function,
            scope,
            identity,
            booleans,
            &type_binders,
            next,
            part.clone(),
        )?
    } else if allow_abstract && declaration.body().is_empty() {
        let checked_type = checked
            .signature_for_identity(identity)
            .map(vibra_types::IndexedSignature::checked_type)
            .ok_or_else(|| missing_checked(identity))?;
        let vibra_ir::Type::Function(signature) = checked_type else {
            return Err(GraphEncodingError::InconsistentCheckedFacts(format!(
                "abstract member {identity} has a non-function checked type"
            )));
        };
        let mut references = Vec::new();
        let mut projection = projection(
            scope,
            identity,
            &type_binders,
            booleans,
            &mut references,
            &mut next,
            part.clone(),
        );
        let signature = projection.signature_node(signature)?;
        let absent = encoding::tagged(
            "abstract-function",
            [("signature".to_owned(), signature)],
        );
        drop(projection);
        (absent, references, next)
    } else {
        return Err(missing_checked(identity));
    };
    references.extend(generic_references(
        declaration.attributes().items(),
        source_id,
        checked,
        &part,
    )?);
    let form = encoding::tagged(
        "function-part",
        [
            (
                "name".to_owned(),
                encoding::string(declaration.name().value()),
            ),
            ("attributes".to_owned(), attr_node),
            ("body".to_owned(), body),
        ],
    );
    let origins = vec![origin(source_id, declaration.span(), part)];
    Ok((form, references, origins, next))
}

pub(super) fn type_declaration_form(
    declaration: &vibra_syntax::DeftypeDeclaration,
    id: &DeclarationId,
    scope: &Scope<'_>,
    booleans: &BTreeMap<bool, String>,
    source_id: &str,
    checked: &ResolvedCheckResult,
) -> Result<
    (DefinitionNode, Vec<PendingReference>, Vec<ObjectOrigin>),
    GraphEncodingError,
> {
    let (binders, _generics, mut next) =
        generic_binders(declaration.attributes().items(), source_id, checked)?;
    let mut attrs_binders = BTreeMap::new();
    let mut attrs_next = 0;
    let attrs = attributes(
        declaration.attributes().items(),
        source_id,
        checked,
        &mut attrs_binders,
        &mut attrs_next,
    )?;
    let mut references = Vec::new();
    references.extend(generic_references(
        declaration.attributes().items(),
        source_id,
        checked,
        &[],
    )?);
    let body = if let Some(definition) = scope.type_definition(&id.canonical()) {
        let mut body_next = next;
        let mut projection = projection(
            scope,
            &id.canonical(),
            &binders,
            booleans,
            &mut references,
            &mut body_next,
            Vec::new(),
        );
        type_body_node(definition, &mut projection)?
    } else {
        match declaration.body() {
            DeftypeBody::Intrinsic(name) => encoding::tagged(
                "intrinsic-type",
                [("name".to_owned(), encoding::atom(name.value()))],
            ),
            DeftypeBody::Type(_) => return Err(missing_checked(&id.canonical())),
        }
    };
    let mut members = Vec::new();
    let mut origins = Vec::new();
    for member in declaration.members() {
        if let TypeMember::Method(function) = member {
            // Resolver path is owner identity followed by this member name.
            let member_id = format!("{}.{}", id.canonical(), function.name().value());
            let (form, mut method_references, method_origins, method_next) =
                function_form(
                    function,
                    &member_id,
                    scope,
                    booleans,
                    source_id,
                    (binders.clone(), Vec::new(), next),
                    checked,
                    vec![function.name().value().to_owned()],
                    false,
                )?;
            references.append(&mut method_references);
            origins.extend(method_origins);
            members.push(form);
            next = method_next;
        }
    }
    Ok((
        encoding::tagged(
            "deftype",
            [
                ("attributes".to_owned(), attrs),
                ("body".to_owned(), body),
                ("members".to_owned(), encoding::array(members)),
            ],
        ),
        references,
        origins,
    ))
}

fn type_body_node(
    definition: &TypeDefinition,
    projection: &mut IrProjection<'_>,
) -> Result<DefinitionNode, GraphEncodingError> {
    Ok(match definition.body() {
        TypeBody::Record(fields) => encoding::tagged(
            "record",
            [(
                "fields".to_owned(),
                encoding::array(
                    fields
                        .iter()
                        .map(|(name, ty)| {
                            Ok(encoding::tagged(
                                "field",
                                [
                                    ("name".to_owned(), encoding::string(name.clone())),
                                    ("type".to_owned(), projection.type_node(ty)?),
                                ],
                            ))
                        })
                        .collect::<Result<Vec<_>, GraphEncodingError>>()?,
                ),
            )],
        ),
        TypeBody::Enum(fields) => encoding::tagged(
            "enum",
            [(
                "variants".to_owned(),
                encoding::array(
                    fields
                        .iter()
                        .map(|(name, ty)| {
                            Ok(encoding::tagged(
                                "variant",
                                [
                                    ("name".to_owned(), encoding::string(name.clone())),
                                    ("type".to_owned(), projection.type_node(ty)?),
                                ],
                            ))
                        })
                        .collect::<Result<Vec<_>, GraphEncodingError>>()?,
                ),
            )],
        ),
        TypeBody::Wrapper(ty) => encoding::tagged(
            "wrapper",
            [("type".to_owned(), projection.type_node(ty)?)],
        ),
        TypeBody::Tuple(types) => encoding::tagged(
            "tuple",
            [(
                "items".to_owned(),
                encoding::array(
                    types
                        .iter()
                        .map(|ty| projection.type_node(ty))
                        .collect::<Result<Vec<_>, _>>()?,
                ),
            )],
        ),
        TypeBody::Union(types) => encoding::tagged(
            "union",
            [(
                "members".to_owned(),
                encoding::array(
                    types
                        .iter()
                        .map(|ty| projection.type_node(ty))
                        .collect::<Result<Vec<_>, _>>()?,
                ),
            )],
        ),
    })
}

pub(super) fn interface_form(
    declaration: &vibra_syntax::DefintDeclaration,
    id: &DeclarationId,
    scope: &Scope<'_>,
    booleans: &BTreeMap<bool, String>,
    source_id: &str,
    checked: &ResolvedCheckResult,
) -> Result<
    (DefinitionNode, Vec<PendingReference>, Vec<ObjectOrigin>),
    GraphEncodingError,
> {
    let mut binders = BTreeMap::from([("self".to_owned(), 0)]);
    let mut next = 1;
    let attrs = attributes(
        declaration.attributes().items(),
        source_id,
        checked,
        &mut binders,
        &mut next,
    )?;
    let mut members = Vec::new();
    let mut references = Vec::new();
    let mut origins = Vec::new();
    references.extend(generic_references(
        declaration.attributes().items(),
        source_id,
        checked,
        &[],
    )?);
    for member in declaration.members() {
        if let TypeMember::Method(function) = member {
            let identity = format!("{}.{}", id.canonical(), function.name().value());
            let (form, mut refs, mut part_origins, method_next) = function_form(
                function,
                &identity,
                scope,
                booleans,
                source_id,
                (binders.clone(), Vec::new(), next),
                checked,
                vec![function.name().value().to_owned()],
                true,
            )?;
            references.append(&mut refs);
            origins.append(&mut part_origins);
            members.push(form);
            next = method_next;
        }
    }
    Ok((
        encoding::tagged(
            "defint",
            [
                ("attributes".to_owned(), attrs),
                ("members".to_owned(), encoding::array(members)),
            ],
        ),
        references,
        origins,
    ))
}

pub(super) fn effect_form(
    declaration: &vibra_syntax::DeffectDeclaration,
    id: &DeclarationId,
    scope: &Scope<'_>,
    booleans: &BTreeMap<bool, String>,
    source_id: &str,
    checked: &ResolvedCheckResult,
) -> Result<
    (DefinitionNode, Vec<PendingReference>, Vec<ObjectOrigin>),
    GraphEncodingError,
> {
    let (binders, _generics, mut next) =
        generic_binders(declaration.attributes().items(), source_id, checked)?;
    let mut attr_binders = BTreeMap::new();
    let mut attr_next = 0;
    let attrs = attributes(
        declaration.attributes().items(),
        source_id,
        checked,
        &mut attr_binders,
        &mut attr_next,
    )?;
    let mut members = Vec::new();
    let mut references = Vec::new();
    let mut origins = Vec::new();
    references.extend(generic_references(
        declaration.attributes().items(),
        source_id,
        checked,
        &[],
    )?);
    for function in declaration.members() {
        let identity = format!("{}.{}", id.canonical(), function.name().value());
        let (form, mut refs, mut part_origins, method_next) = function_form(
            function,
            &identity,
            scope,
            booleans,
            source_id,
            (binders.clone(), Vec::new(), next),
            checked,
            vec![function.name().value().to_owned()],
            false,
        )?;
        references.append(&mut refs);
        origins.append(&mut part_origins);
        members.push(form);
        next = method_next;
    }
    Ok((
        encoding::tagged(
            "deffect",
            [
                ("attributes".to_owned(), attrs),
                ("members".to_owned(), encoding::array(members)),
            ],
        ),
        references,
        origins,
    ))
}

pub(super) fn add_owned_declarations(
    candidate: &mut Candidate,
    owners: &mut BTreeMap<String, (ObjectKey, Vec<String>)>,
    resolved: &ResolvedSnapshot,
    owner_id: &DeclarationId,
    source_id: &str,
    excluded: &[String],
) -> Result<(), GraphEncodingError> {
    let key = candidate.key.clone();
    let prefix = owner_id.path();
    for declaration in resolved
        .declarations()
        .iter()
        .filter(|value| value.source_id() == source_id)
    {
        let id = declaration.id();
        let Some(suffix) = id.path().get(prefix.len()..) else {
            continue;
        };
        if id == owner_id
            || suffix.is_empty()
            || id.package() != owner_id.package()
            || id.unit() != owner_id.unit()
            || id.module() != owner_id.module()
            || !id.path().starts_with(prefix)
        {
            continue;
        }
        let suffix = suffix.to_vec();
        if suffix.first().is_some_and(|name| name.starts_with("impl-")) {
            continue;
        }
        if !excluded.is_empty() && suffix.iter().any(|piece| excluded.contains(piece)) {
            continue;
        }
        let entity = EntityKey::Declaration {
            identity: id.canonical(),
            entity_kind: id.kind().as_str().to_owned(),
        };
        candidate.entities.push((entity, suffix.clone()));
        candidate
            .origins
            .push(origin(source_id, declaration.span(), suffix.clone()));
        owners.insert(id.canonical(), (key.clone(), suffix));
    }
    Ok(())
}

// Keep the resolver/checker scopes and implementation ownership inputs explicit.
#[allow(clippy::too_many_arguments)]
pub(super) fn add_explicit_impls(
    candidates: &mut Vec<Candidate>,
    owners: &mut BTreeMap<String, (ObjectKey, Vec<String>)>,
    impl_candidates: &mut Vec<(IndexedImplementation, ObjectKey)>,
    members: &[TypeMember],
    resolved: &ResolvedSnapshot,
    scope: &Scope<'_>,
    scopes: &[Scope<'_>],
    booleans: &BTreeMap<bool, String>,
    source_id: &str,
    parent_generics: Option<GenericBinderSet>,
) -> Result<(), GraphEncodingError> {
    for member in members {
        let TypeMember::Implementation(implementation) = member else {
            continue;
        };
        let indexed = scopes
            .iter()
            .flat_map(|scope| scope.checked.implementations().iter())
            .find(|value| {
                value.source_id() == source_id && value.span() == implementation.span()
            })
            .ok_or_else(|| {
                GraphEncodingError::IncompleteProjection(format!(
                    "impl block at {}:{} has no checked implementation fact",
                    source_id,
                    implementation.span().start()
                ))
            })?;
        let (binders, _generics, mut next) =
            parent_generics.clone().unwrap_or_default();
        let receiver =
            encoding::normalized_type_text(indexed.receiver_type(), &binders)?;
        let interface =
            encoding::normalized_type_text(indexed.interface_type(), &binders)?;
        let key = ObjectKey::Implementation {
            receiver: receiver.clone(),
            interface: interface.clone(),
        };
        let mut references = Vec::new();
        let mut projection = projection(
            scope,
            &key.canonical(),
            &binders,
            booleans,
            &mut references,
            &mut next,
            Vec::new(),
        );
        let receiver_node = projection.type_node(indexed.receiver_type())?;
        let interface_node = projection.type_node(indexed.interface_type())?;
        projection.references.push(PendingReference {
            identity: indexed.interface_id().to_owned(),
            kind: EdgeKind::Implementation,
            part: Vec::new(),
            dispatch: None,
        });
        drop(projection);
        let mut forms = Vec::new();
        let mut origins = vec![origin(source_id, implementation.span(), Vec::new())];
        for function in implementation.members() {
            let declaration = resolved
                .declarations()
                .iter()
                .find(|value| {
                    value.source_id() == source_id && value.span() == function.span()
                })
                .ok_or_else(|| {
                    GraphEncodingError::IncompleteProjection(format!(
                        "impl member at {}:{} has no resolver identity",
                        source_id,
                        function.span().start()
                    ))
                })?;
            let contract =
                format!("{}.{}", indexed.interface_id(), function.name().value());
            let part = vec![contract.clone()];
            let mut method_binders = binders.clone();
            let mut method_next = next;
            let attr_node = attributes(
                function.attributes().items(),
                source_id,
                scope.checked,
                &mut method_binders,
                &mut method_next,
            )?;
            references.extend(generic_references(
                function.attributes().items(),
                source_id,
                scope.checked,
                &part,
            )?);
            let implementation_member_identity = declaration.id().canonical();
            let fn_ir = scope
                .function(&implementation_member_identity)
                .ok_or_else(|| missing_checked(&implementation_member_identity))?;
            let (body, method_refs, body_next) = function_body(
                fn_ir,
                scope,
                &implementation_member_identity,
                booleans,
                &method_binders,
                method_next,
                part.clone(),
            )?;
            references.extend(method_refs);
            references.push(PendingReference {
                identity: contract.clone(),
                kind: EdgeKind::Implementation,
                part: part.clone(),
                dispatch: None,
            });
            let method_form = encoding::tagged(
                "implementation-member",
                [
                    (
                        "contract".to_owned(),
                        DefinitionNode::Reference(contract.clone()),
                    ),
                    ("attributes".to_owned(), attr_node),
                    ("body".to_owned(), body),
                ],
            );
            forms.push(method_form);
            origins.push(origin(source_id, function.span(), part.clone()));
            owners.insert(declaration.id().canonical(), (key.clone(), part));
            next = body_next;
        }
        let form = encoding::tagged(
            "implementation",
            [
                ("receiver".to_owned(), receiver_node),
                ("interface".to_owned(), interface_node),
                ("members".to_owned(), encoding::array(forms)),
            ],
        );
        let mut candidate =
            Candidate::new(key.clone(), ObjectKind::Implementation, &form)?;
        attach_references(&mut candidate, references);
        candidate.origins = origins;
        for function in implementation.members() {
            let contract =
                format!("{}.{}", indexed.interface_id(), function.name().value());
            candidate.entities.push((
                EntityKey::ImplementationMember {
                    contract: contract.clone(),
                    receiver: receiver.clone(),
                    interface: interface.clone(),
                },
                vec![contract.clone()],
            ));
        }
        impl_candidates.push((indexed.clone(), key.clone()));
        candidates.push(candidate);
    }
    Ok(())
}

pub(super) fn owner_for_identity_for_nested(
    declaration: &ResolvedDeclaration,
    owners: &BTreeMap<String, (ObjectKey, Vec<String>)>,
    declaration_ids: &BTreeMap<String, DeclarationId>,
) -> Option<(ObjectKey, Vec<String>)> {
    let id = declaration.id();
    owners
        .iter()
        .filter_map(|(identity, (key, _))| {
            let owner_id = declaration_ids.get(identity)?;
            let suffix = id.path().get(owner_id.path().len()..)?;
            (id.package() == owner_id.package()
                && id.unit() == owner_id.unit()
                && id.module() == owner_id.module()
                && !suffix.is_empty()
                && id.path().starts_with(owner_id.path()))
            .then(|| (owner_id.path().len(), key.clone(), suffix.to_vec()))
        })
        .max_by_key(|(length, _, _)| *length)
        .map(|(_, key, part)| (key, part))
}
