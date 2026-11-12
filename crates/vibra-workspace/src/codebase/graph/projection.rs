//! Projection of accepted resolver and checked-IR facts into graph candidates.

use std::collections::{BTreeMap, BTreeSet};

use vibra_ir::{CheckedFunction, CheckedGlobal, TypeDefinition};
use vibra_resolve::{DeclarationId, EntityKind, ResolvedSnapshot};
use vibra_syntax::Declaration;
use vibra_types::{
    IndexedImplementation, ResolvedCheckResult, contract_implementation_is_eligible,
};

use super::{
    CheckedGraphInput, EdgeKind, EntityKey, GraphEncodingError, ObjectKey, ObjectKind,
    ObjectOrigin,
    encoding::{
        self, Candidate, CandidateReference, CandidateTarget, DefinitionNode,
        IrProjection, PendingReference,
    },
};

struct Scope<'a> {
    checked: &'a ResolvedCheckResult,
    source_ids: &'a BTreeSet<String>,
    functions: Vec<String>,
    globals: Vec<String>,
}

impl<'a> Scope<'a> {
    fn new(checked: &'a ResolvedCheckResult) -> Self {
        let (functions, globals) = checked.modules().map_or_else(
            || (Vec::new(), Vec::new()),
            |modules| {
                (
                    modules
                        .functions()
                        .iter()
                        .map(|function| function.name().to_owned())
                        .collect(),
                    modules
                        .globals()
                        .iter()
                        .map(|global| global.name().to_owned())
                        .collect(),
                )
            },
        );
        Self {
            checked,
            source_ids: checked.source_ids(),
            functions,
            globals,
        }
    }

    fn function(&self, identity: &str) -> Option<&CheckedFunction> {
        self.checked
            .modules()?
            .functions()
            .iter()
            .find(|value| value.name() == identity)
    }

    fn global(&self, identity: &str) -> Option<&CheckedGlobal> {
        self.checked
            .modules()?
            .globals()
            .iter()
            .find(|value| value.name() == identity)
    }

    fn type_definition(&self, identity: &str) -> Option<&TypeDefinition> {
        self.checked
            .modules()?
            .types()
            .iter()
            .find(|value| value.id().id() == identity)
    }
}

/// Collects one candidate per checked declaration, test, and explicit impl.
pub(super) fn collect_candidates(
    input: &CheckedGraphInput<'_>,
    compiler_registry: &str,
    host_registry: &str,
) -> Result<Vec<Candidate>, GraphEncodingError> {
    let resolved = input.resolved;
    let mut scopes = vec![Scope::new(
        input
            .checked
            .checked()
            .ok_or(GraphEncodingError::UncheckedInput)?,
    )];
    if let Some(tests) = input.tests {
        scopes.push(Scope::new(
            tests.checked().ok_or(GraphEncodingError::UncheckedInput)?,
        ));
    }

    let mut selected_sources = BTreeSet::new();
    for scope in &scopes {
        selected_sources.extend(scope.source_ids.iter().cloned());
    }
    let mut candidates = Vec::<Candidate>::new();
    let mut owner_for_identity = BTreeMap::<String, (ObjectKey, Vec<String>)>::new();
    let declaration_ids = resolved
        .declarations()
        .iter()
        .map(|declaration| (declaration.id().canonical(), declaration.id().clone()))
        .collect::<BTreeMap<_, _>>();
    let mut impl_candidates = Vec::<(IndexedImplementation, ObjectKey)>::new();
    let bool_identities = boolean_identities(resolved);

    for module in resolved
        .modules()
        .iter()
        .filter(|module| selected_sources.contains(module.source_id()))
    {
        let Some(ast) = module.ast() else { continue };
        let scope = scope_for(&scopes, module.source_id()).ok_or_else(|| {
            GraphEncodingError::MissingResolution(format!(
                "accepted module {} has no checker scope",
                module.source_id()
            ))
        })?;
        for declaration in ast.declarations() {
            let Some(resolved_declaration) =
                resolved.declarations().iter().find(|candidate| {
                    candidate.source_id() == module.source_id()
                        && candidate.span() == declaration.span()
                })
            else {
                if matches!(declaration, Declaration::Import(_)) {
                    continue;
                }
                return Err(GraphEncodingError::MissingResolution(format!(
                    "declaration at {}:{} has no resolver identity",
                    module.source_id(),
                    declaration.span().start()
                )));
            };
            if matches!(declaration, Declaration::Import(_)) {
                continue;
            }

            let identity = resolved_declaration.id().canonical();
            let candidate = match declaration {
                Declaration::Def(value) => {
                    let key = declaration_key(resolved_declaration.id());
                    let checked = scope
                        .global(&identity)
                        .ok_or_else(|| missing_checked(&identity))?;
                    let mut references = Vec::new();
                    let binders = BTreeMap::new();
                    let mut next_binder = 0;
                    let mut projector = projection(
                        scope,
                        &identity,
                        &binders,
                        &bool_identities,
                        &mut references,
                        &mut next_binder,
                        Vec::new(),
                    );
                    let ty = projector.type_node(&checked.value_type())?;
                    let initializer_start = projector.references.len();
                    let initializer = projector.expression(checked.initializer())?;
                    let initializer_references = projector
                        .references
                        .get(initializer_start..)
                        .ok_or_else(|| {
                            GraphEncodingError::InconsistentCheckedFacts(
                                "initializer projection reference range moved backwards"
                                    .to_owned(),
                            )
                        })?;
                    let initializer_targets = initializer_references
                        .iter()
                        .filter(|reference| {
                            matches!(
                                reference.kind,
                                EdgeKind::ValueReference
                                    | EdgeKind::Call
                                    | EdgeKind::Contract
                            )
                        })
                        .map(|reference| PendingReference {
                            identity: reference.identity.clone(),
                            kind: EdgeKind::Initializer,
                            part: reference.part.clone(),
                            dispatch: None,
                        })
                        .collect::<Vec<_>>();
                    projector.references.extend(initializer_targets);
                    drop(projector);
                    let mut attrs_binders = BTreeMap::new();
                    let mut attrs_next = 0;
                    let attrs = attributes(
                        value.attributes().items(),
                        module.source_id(),
                        scope.checked,
                        &mut attrs_binders,
                        &mut attrs_next,
                    )?;
                    let form = encoding::tagged(
                        "def",
                        [
                            ("attributes".to_owned(), attrs),
                            ("type".to_owned(), ty),
                            ("initializer".to_owned(), initializer),
                        ],
                    );
                    let mut candidate =
                        Candidate::new(key.clone(), ObjectKind::Def, &form)?;
                    attach_references(&mut candidate, references);
                    candidate.origins.push(origin(
                        module.source_id(),
                        declaration.span(),
                        Vec::new(),
                    ));
                    add_owned_declarations(
                        &mut candidate,
                        &mut owner_for_identity,
                        resolved,
                        resolved_declaration.id(),
                        module.source_id(),
                        &[],
                    )?;
                    candidate
                }
                Declaration::Defn(value) => {
                    let key = declaration_key(resolved_declaration.id());
                    let (form, references, nested_origins, _) = function_form(
                        value,
                        &identity,
                        scope,
                        &bool_identities,
                        module.source_id(),
                        (BTreeMap::new(), Vec::new(), 0),
                        scope.checked,
                        Vec::new(),
                        false,
                    )?;
                    let mut candidate =
                        Candidate::new(key.clone(), ObjectKind::Defn, &form)?;
                    attach_references(&mut candidate, references);
                    candidate.origins.push(origin(
                        module.source_id(),
                        declaration.span(),
                        Vec::new(),
                    ));
                    candidate.origins.extend(nested_origins);
                    add_owned_declarations(
                        &mut candidate,
                        &mut owner_for_identity,
                        resolved,
                        resolved_declaration.id(),
                        module.source_id(),
                        &[],
                    )?;
                    candidate
                }
                Declaration::Deftype(value) => {
                    let key = declaration_key(resolved_declaration.id());
                    let (form, references, nested_origins) = type_declaration_form(
                        value,
                        resolved_declaration.id(),
                        scope,
                        &bool_identities,
                        module.source_id(),
                        scope.checked,
                    )?;
                    let mut candidate =
                        Candidate::new(key.clone(), ObjectKind::Deftype, &form)?;
                    attach_references(&mut candidate, references);
                    candidate.origins.push(origin(
                        module.source_id(),
                        declaration.span(),
                        Vec::new(),
                    ));
                    candidate.origins.extend(nested_origins);
                    add_owned_declarations(
                        &mut candidate,
                        &mut owner_for_identity,
                        resolved,
                        resolved_declaration.id(),
                        module.source_id(),
                        &[],
                    )?;
                    let type_generics = generic_binders(
                        value.attributes().items(),
                        module.source_id(),
                        scope.checked,
                    )?;
                    add_explicit_impls(
                        &mut candidates,
                        &mut owner_for_identity,
                        &mut impl_candidates,
                        value.members(),
                        resolved,
                        scope,
                        &scopes,
                        &bool_identities,
                        module.source_id(),
                        Some(type_generics),
                    )?;
                    candidate
                }
                Declaration::Defint(value) => {
                    let key = declaration_key(resolved_declaration.id());
                    let (form, references, nested_origins) = interface_form(
                        value,
                        resolved_declaration.id(),
                        scope,
                        &bool_identities,
                        module.source_id(),
                        scope.checked,
                    )?;
                    let mut candidate =
                        Candidate::new(key.clone(), ObjectKind::Defint, &form)?;
                    attach_references(&mut candidate, references);
                    candidate.origins.push(origin(
                        module.source_id(),
                        declaration.span(),
                        Vec::new(),
                    ));
                    candidate.origins.extend(nested_origins);
                    add_owned_declarations(
                        &mut candidate,
                        &mut owner_for_identity,
                        resolved,
                        resolved_declaration.id(),
                        module.source_id(),
                        &[],
                    )?;
                    let type_generics = generic_binders(
                        value.attributes().items(),
                        module.source_id(),
                        scope.checked,
                    )?;
                    add_explicit_impls(
                        &mut candidates,
                        &mut owner_for_identity,
                        &mut impl_candidates,
                        value.members(),
                        resolved,
                        scope,
                        &scopes,
                        &bool_identities,
                        module.source_id(),
                        Some(type_generics),
                    )?;
                    candidate
                }
                Declaration::Deffect(value) => {
                    let key = declaration_key(resolved_declaration.id());
                    let (form, references, nested_origins) = effect_form(
                        value,
                        resolved_declaration.id(),
                        scope,
                        &bool_identities,
                        module.source_id(),
                        scope.checked,
                    )?;
                    let mut candidate =
                        Candidate::new(key.clone(), ObjectKind::Deffect, &form)?;
                    attach_references(&mut candidate, references);
                    candidate.origins.push(origin(
                        module.source_id(),
                        declaration.span(),
                        Vec::new(),
                    ));
                    candidate.origins.extend(nested_origins);
                    add_owned_declarations(
                        &mut candidate,
                        &mut owner_for_identity,
                        resolved,
                        resolved_declaration.id(),
                        module.source_id(),
                        &[],
                    )?;
                    candidate
                }
                Declaration::Test(value) => {
                    let test = input
                        .tests
                        .and_then(|tests| {
                            tests.tests().iter().find(|test| {
                                test.source_id() == module.source_id()
                                    && test.span() == value.span()
                            })
                        })
                        .ok_or_else(|| missing_checked(&identity))?;
                    if test.identity().canonical() != identity {
                        return Err(GraphEncodingError::InconsistentCheckedFacts(
                            format!(
                                "test scope identity {} disagrees with resolver identity {identity}",
                                test.identity().canonical(),
                            ),
                        ));
                    }
                    let module_id = canonical_module(resolved_declaration.id());
                    let key = ObjectKey::Test {
                        module: module_id.clone(),
                        name: test.name().to_owned(),
                    };
                    let function = test
                        .function_index()
                        .and_then(|index| {
                            scope.checked.modules()?.functions().get(index)
                        })
                        .ok_or_else(|| missing_checked(&identity))?;
                    let (body, references, _) = function_body(
                        function,
                        scope,
                        &identity,
                        &bool_identities,
                        &BTreeMap::new(),
                        0,
                        Vec::new(),
                    )?;
                    let form = encoding::tagged(
                        "test",
                        [
                            ("name".to_owned(), encoding::string(test.name())),
                            ("body".to_owned(), body),
                        ],
                    );
                    let mut candidate =
                        Candidate::new(key.clone(), ObjectKind::Test, &form)?;
                    attach_references(&mut candidate, references);
                    candidate.origins.push(origin(
                        module.source_id(),
                        declaration.span(),
                        Vec::new(),
                    ));
                    let test_key = EntityKey::Test {
                        module: module_id,
                        name: test.name().to_owned(),
                    };
                    candidate.entities = vec![(test_key, Vec::new())];
                    candidate
                }
                Declaration::Import(_) => continue,
            };
            let key = candidate.key.clone();
            if let Some(existing) =
                candidates.iter().find(|existing| existing.key == key)
            {
                if existing.definition != candidate.definition {
                    return Err(GraphEncodingError::InconsistentCheckedFacts(format!(
                        "two checked scopes disagree on {}",
                        key.canonical()
                    )));
                }
                continue;
            }
            owner_for_identity.insert(identity, (key, Vec::new()));
            candidates.push(candidate);
        }
    }

    add_trusted_assertions(
        &scopes,
        resolved,
        input.stdlib,
        &bool_identities,
        &mut candidates,
        &mut owner_for_identity,
    )?;

    // Every nested declaration not already mapped by a top-level owner belongs
    // to the longest preceding declaration path in the same module.
    for declaration in resolved
        .declarations()
        .iter()
        .filter(|declaration| selected_sources.contains(declaration.source_id()))
    {
        let identity = declaration.id().canonical();
        if owner_for_identity.contains_key(&identity) {
            continue;
        }
        let Some((owner, part)) = owner_for_identity_for_nested(
            declaration,
            &owner_for_identity,
            &declaration_ids,
        ) else {
            continue;
        };
        owner_for_identity.insert(identity.clone(), (owner.clone(), part.clone()));
        if let Some(candidate) = candidates
            .iter_mut()
            .find(|candidate| candidate.key == owner)
        {
            candidate.entities.push((
                EntityKey::Declaration {
                    identity,
                    entity_kind: declaration.id().kind().as_str().to_owned(),
                },
                part.clone(),
            ));
            candidate.origins.push(origin(
                declaration.source_id(),
                declaration.span(),
                part,
            ));
        }
    }

    // Resolver references catch checked namespace/bounds uses which have no
    // runtime IR node. Typed IR already contributes ordinary body references.
    for reference in resolved
        .references()
        .iter()
        .filter(|reference| selected_sources.contains(reference.source_id()))
    {
        let from = reference.from().canonical();
        let Some((owner, part)) = owner_for_identity.get(&from).cloned() else {
            continue;
        };
        let Some(target) = reference.target() else {
            // Some vocabulary is intentionally resolved only by the checker:
            // constructors, builtin members, and prelude names can lower to
            // checked IR without a resolver target. Accepted IR projection
            // and checked header facts below are authoritative for these.
            continue;
        };
        let target_id = target.canonical();
        let target_owner = owner_for_identity
            .get(&target_id)
            .map(|(key, _)| key.clone())
            .ok_or_else(|| GraphEncodingError::MissingResolution(target_id.clone()))?;
        if let Some(candidate) = candidates
            .iter_mut()
            .find(|candidate| candidate.key == owner)
        {
            candidate.references.push(CandidateReference {
                part,
                kind: EdgeKind::ValueReference,
                target: CandidateTarget::Object(target_owner),
                dispatch: None,
            });
        } else {
            return Err(GraphEncodingError::IncompleteProjection(format!(
                "reference owner {} is not an encoded object",
                owner.canonical()
            )));
        }
    }

    let contract_dependencies = candidates
        .iter()
        .flat_map(|candidate| {
            candidate.references.iter().filter_map(move |reference| {
                if reference.kind != EdgeKind::Contract {
                    return None;
                }
                let dispatch = reference.dispatch.as_ref()?;
                Some((
                    candidate.key.clone(),
                    reference.part.clone(),
                    dispatch.clone(),
                ))
            })
        })
        .collect::<Vec<_>>();

    // Convert every typed IR reference to its owner object or pinned registry.
    for candidate in &mut candidates {
        for reference in &mut candidate.references {
            let CandidateTarget::Object(ObjectKey::Declaration { identity, .. }) =
                &reference.target
            else {
                continue;
            };
            let identity = identity.clone();
            if let Some((target_owner, _)) = owner_for_identity.get(&identity) {
                reference.target = CandidateTarget::Object(target_owner.clone());
            } else if identity.starts_with("@compiler:") {
                reference.target = CandidateTarget::TrustedExternal {
                    identity,
                    registry: compiler_registry.to_owned(),
                };
            } else if identity.starts_with("@host:") {
                reference.target = CandidateTarget::TrustedExternal {
                    identity,
                    registry: host_registry.to_owned(),
                };
            } else {
                return Err(GraphEncodingError::MissingResolution(identity));
            }
        }
    }

    // Contract dispatch pins the implementation objects admitted by the
    // checker's receiver and applied-interface rules for this exact call.
    for (owner, part, dispatch) in contract_dependencies {
        for (implementation, implementation_key) in &impl_candidates {
            if contract_implementation_is_eligible(
                implementation,
                &dispatch.target,
                &dispatch.actual_operand_types,
            ) && let Some(candidate) = candidates
                .iter_mut()
                .find(|candidate| candidate.key == owner)
            {
                candidate.references.push(CandidateReference {
                    part: part.clone(),
                    kind: EdgeKind::Implementation,
                    target: CandidateTarget::Object(implementation_key.clone()),
                    dispatch: None,
                });
            }
        }
    }

    for candidate in &mut candidates {
        candidate.origins.sort_by(|left, right| {
            left.source_id
                .as_bytes()
                .cmp(right.source_id.as_bytes())
                .then(left.span.start().cmp(&right.span.start()))
                .then(left.span.end().cmp(&right.span.end()))
                .then(left.part.cmp(&right.part))
        });
        candidate.origins.dedup();
        candidate.entities.sort_by(|left, right| {
            left.0
                .canonical()
                .as_bytes()
                .cmp(right.0.canonical().as_bytes())
        });
        candidate.entities.dedup_by(|left, right| left.0 == right.0);
    }
    Ok(candidates)
}

fn scope_for<'a>(scopes: &'a [Scope<'_>], source_id: &str) -> Option<&'a Scope<'a>> {
    scopes
        .iter()
        // The ordinary checked result is first. If a source is present there,
        // use its stable module indices; test-only sources fall through to the
        // non-running test check result.
        .find(|scope| scope.source_ids.contains(source_id))
}

fn add_trusted_assertions(
    scopes: &[Scope<'_>],
    resolved: &ResolvedSnapshot,
    stdlib: &vibra_types::Stdlib,
    booleans: &BTreeMap<bool, String>,
    candidates: &mut Vec<Candidate>,
    owners: &mut BTreeMap<String, (ObjectKey, Vec<String>)>,
) -> Result<(), GraphEncodingError> {
    let registry = stdlib
        .module("std.assert")
        .filter(|module| module.is_test_registry())
        .ok_or_else(|| {
            GraphEncodingError::InconsistentCheckedFacts(
                "verified assertion catalogue has no admitted @std.assert test registry"
                    .to_owned(),
            )
        })?;
    let package = stdlib.package();
    let resolved_registry = resolved
        .modules()
        .iter()
        .find(|module| module.source_id() == registry.source_id())
        .filter(|module| {
            module.package() == package
                && module.unit() == "std"
                && module.segments() == ["assert"]
                && module.bytes() == registry.bytes()
        })
        .ok_or_else(|| {
            GraphEncodingError::InconsistentCheckedFacts(
                "resolved @std.assert module differs from the verified test registry"
                    .to_owned(),
            )
        })?;
    let mut seen = BTreeSet::new();
    for scope in scopes {
        let Some(modules) = scope.checked.modules() else {
            continue;
        };
        for function in modules.functions() {
            let Some(assertion) = function.test_assertion() else {
                continue;
            };
            let manifest_member = format!("assert.{}", assertion.member());
            if !stdlib
                .assertion_members()
                .iter()
                .any(|member| member == &manifest_member)
            {
                return Err(GraphEncodingError::InconsistentCheckedFacts(format!(
                    "checked assertion {} is absent from the verified manifest catalogue",
                    assertion.member()
                )));
            }
            let identity = format!(
                "@{}@{}/std.assert.{}",
                package.name(),
                package.version(),
                assertion.member()
            );
            if function.name() != identity {
                return Err(GraphEncodingError::InconsistentCheckedFacts(format!(
                    "checked assertion member {} has unexpected identity {}",
                    assertion.member(),
                    function.name()
                )));
            }
            if function.origin().source_id() != registry.source_id() {
                return Err(GraphEncodingError::InconsistentCheckedFacts(format!(
                    "checked assertion {identity} has origin outside the admitted registry"
                )));
            }
            if resolved_registry.source_id() != function.origin().source_id() {
                return Err(GraphEncodingError::InconsistentCheckedFacts(format!(
                    "checked assertion {identity} is not owned by the resolved registry module"
                )));
            }
            if function.signature() != &assertion.signature() {
                return Err(GraphEncodingError::InconsistentCheckedFacts(format!(
                    "checked assertion {identity} disagrees with its closed catalogue signature"
                )));
            }
            if !seen.insert(identity.clone()) {
                continue;
            }

            let type_parameters = assertion.type_parameters();
            let type_binders = type_parameters
                .iter()
                .enumerate()
                .map(|(index, name)| {
                    u64::try_from(index)
                        .map(|index| (name.clone(), index))
                        .map_err(|_| {
                            GraphEncodingError::MalformedInput(
                                "assertion type-parameter count exceeds u64".to_owned(),
                            )
                        })
                })
                .collect::<Result<BTreeMap<_, _>, _>>()?;
            let generic_binders = type_parameters
                .iter()
                .enumerate()
                .map(|(index, _)| {
                    u64::try_from(index)
                        .map(|index| DefinitionNode::Binder {
                            kind: "type".to_owned(),
                            index,
                        })
                        .map_err(|_| {
                            GraphEncodingError::MalformedInput(
                                "assertion type-parameter count exceeds u64".to_owned(),
                            )
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let mut references = Vec::new();
            let mut next_binder =
                u64::try_from(type_parameters.len()).map_err(|_| {
                    GraphEncodingError::MalformedInput(
                        "assertion type-parameter count exceeds u64".to_owned(),
                    )
                })?;
            let mut projector = IrProjection {
                functions: &scope.functions,
                globals: &scope.globals,
                type_binders: &type_binders,
                role_type_identities: scope.checked.role_type_identities(),
                owner_identity: identity.clone(),
                booleans,
                references: &mut references,
                next_binder: &mut next_binder,
                slots: BTreeMap::new(),
                captures: BTreeMap::new(),
                part: Vec::new(),
            };
            let signature = projector.signature_node(function.signature())?;
            let form = encoding::tagged(
                "trusted-assertion",
                [
                    ("member".to_owned(), encoding::string(assertion.member())),
                    (
                        "type-parameters".to_owned(),
                        encoding::array(generic_binders),
                    ),
                    ("signature".to_owned(), signature),
                ],
            );
            drop(projector);
            let key = ObjectKey::Declaration {
                identity: identity.clone(),
                entity_kind: EntityKind::Function.as_str().to_owned(),
            };
            let mut candidate = Candidate::new(key.clone(), ObjectKind::Defn, &form)?;
            attach_references(&mut candidate, references);
            candidate.origins.push(origin(
                function.origin().source_id(),
                function.origin().span(),
                Vec::new(),
            ));
            owners.insert(identity, (key, Vec::new()));
            candidates.push(candidate);
        }
    }
    Ok(())
}

fn declaration_key(id: &DeclarationId) -> ObjectKey {
    ObjectKey::Declaration {
        identity: id.canonical(),
        entity_kind: id.kind().as_str().to_owned(),
    }
}

fn canonical_module(id: &DeclarationId) -> String {
    let mut value = format!(
        "@{}@{}/{}",
        id.package().name(),
        id.package().version(),
        id.unit()
    );
    if !id.module().is_empty() {
        value.push('.');
        value.push_str(&id.module().join("."));
    }
    value
}

fn origin(
    source_id: &str,
    span: vibra_diagnostics::ByteSpan,
    part: Vec<String>,
) -> ObjectOrigin {
    ObjectOrigin {
        source_id: source_id.to_owned(),
        span,
        part,
    }
}

fn missing_checked(identity: &str) -> GraphEncodingError {
    GraphEncodingError::MissingResolution(format!(
        "accepted declaration {identity} has no checked projection"
    ))
}

fn projection<'a>(
    scope: &'a Scope<'_>,
    owner_identity: &str,
    type_binders: &'a BTreeMap<String, u64>,
    booleans: &'a BTreeMap<bool, String>,
    references: &'a mut Vec<PendingReference>,
    next_binder: &'a mut u64,
    part: Vec<String>,
) -> IrProjection<'a> {
    IrProjection {
        functions: &scope.functions,
        globals: &scope.globals,
        type_binders,
        role_type_identities: scope.checked.role_type_identities(),
        owner_identity: owner_identity.to_owned(),
        booleans,
        references,
        next_binder,
        slots: BTreeMap::new(),
        captures: BTreeMap::new(),
        part,
    }
}

fn boolean_identities(resolved: &ResolvedSnapshot) -> BTreeMap<bool, String> {
    let mut values = BTreeMap::new();
    for declaration in resolved.declarations() {
        let id = declaration.id();
        if id.kind() != EntityKind::Variant
            || id.package().name() != "vibra-stdlib"
            || id.unit() != "std"
            || id.module() != ["bool"]
            || id.path().len() != 2
            || id.path().first().map(String::as_str) != Some("bool")
        {
            continue;
        }
        match id.name() {
            "true" => {
                values.insert(true, declaration.id().canonical());
            }
            "false" => {
                values.insert(false, declaration.id().canonical());
            }
            _ => {}
        }
    }
    values
}

fn attach_references(candidate: &mut Candidate, references: Vec<PendingReference>) {
    candidate
        .references
        .extend(references.into_iter().map(|reference| CandidateReference {
            part: reference.part,
            kind: reference.kind,
            target: CandidateTarget::Object(ObjectKey::Declaration {
                identity: reference.identity,
                entity_kind: "value".to_owned(),
            }),
            dispatch: reference.dispatch,
        }));
}

mod forms;
use forms::{
    add_explicit_impls, add_owned_declarations, attributes, effect_form, function_body,
    function_form, generic_binders, interface_form, owner_for_identity_for_nested,
    type_declaration_form,
};
