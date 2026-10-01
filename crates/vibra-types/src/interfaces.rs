//! Interfaces, implementations, and contract calls (M3 Step 11).
//!
//! `docs/spec/02-type-system.md`, "Interfaces and methods": a `defint`
//! declares contract members over `self`; an `impl` block nested in a
//! `deftype` targets an interface, and one nested in a `defint` targets a
//! concrete type. A contract member is called through its interface path and
//! selects the implementation from its receiver: statically when the receiver
//! type is known, and at run time through a [`CallTarget::Contract`] when the
//! receiver is a generic bounded by the interface.

use std::collections::{BTreeMap, BTreeSet};

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::{
    CallTarget, ClosedContract, Expr, FunctionSignature, Implements, SourceOrigin, Type,
};
use vibra_syntax::{
    Application, Attribute, Declaration, DefintDeclaration, FunctionDeclaration,
    TypeExpr, TypeMember,
};

use crate::nominal::{ContractMember, Implementation, Scope, TypeNames};
use crate::{
    CheckEnvironment, call_contract_error, check_expression, check_operand, mismatch,
};

/// The receiver placeholder a contract signature is written over.
pub(crate) const SELF: &str = "self";

fn self_type() -> Type {
    Type::Param(SELF.to_owned())
}

/// The interface bounds of a `where:` clause: every generic name bound by an
/// interface other than `any`, with that interface. `None` after reporting
/// a bound that names no visible interface.
pub(crate) fn generic_bounds(
    types: &TypeNames,
    source_id: &str,
    attributes: &[Attribute],
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<BTreeMap<String, usize>> {
    let mut bounds = BTreeMap::new();
    let mut valid = true;
    for attribute in attributes {
        let Attribute::Where(bindings) = attribute else {
            continue;
        };
        for binding in bindings {
            if binding.bound().value() == "any" {
                continue;
            }
            match types.resolve_interface(source_id, binding.bound()) {
                Some(interface) => {
                    bounds.insert(binding.name().value().to_owned(), interface);
                }
                None => {
                    valid = false;
                    diagnostics.push(
                        Diagnostic::new(
                            DiagnosticCode::NameUnknownSymbol,
                            binding.span(),
                            format!(
                                "`{}` does not name a visible interface",
                                binding.bound().value()
                            ),
                        )
                        .with_source_id(source_id),
                    );
                }
            }
        }
    }
    valid.then_some(bounds)
}

/// Lowers the contract of the interface at `index`, reporting a member that
/// neither a receiver nor a destination can select.
pub(crate) fn lower_contract(
    types: &mut TypeNames,
    source_id: &str,
    index: usize,
    declaration: &DefintDeclaration,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let parameters = types
        .interface(index)
        .map(|interface| interface.parameters.clone())
        .unwrap_or_default();
    let receiver = self_type();
    let mut members = Vec::new();
    for member in declaration.members() {
        let TypeMember::Method(method) = member else {
            continue;
        };
        let mut generics = parameters.clone();
        generics.extend(crate::nominal::generic_names(method.attributes().items()));
        let Some(signature) = crate::check_signature(
            source_id,
            method,
            diagnostics,
            types,
            Scope::new(Some(&receiver), &generics),
        ) else {
            continue;
        };
        let position = signature
            .parameters()
            .iter()
            .position(|parameter| *parameter == receiver);
        if position.is_none() && !mentions_self(&signature.result()) {
            diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::TypeUndispatchableContractMember,
                    method.span(),
                    "a contract member needs a fixed positional `self` parameter or `self` in its result",
                )
                .with_source_id(source_id),
            );
            continue;
        }
        members.push(ContractMember {
            name: method.name().value().to_owned(),
            span: method.span(),
            signature,
            generics: crate::nominal::generic_names(method.attributes().items()),
            default: !method.expressions().is_empty(),
            receiver: position,
        });
    }
    types.set_contract(index, members);
}

fn mentions_self(value: &Type) -> bool {
    matches!(value, Type::Param(name) if name == SELF)
        || value.components().iter().any(mentions_self)
}

/// One `impl` block, located and checked for placement and completeness.
#[derive(Clone, Debug)]
pub(crate) struct ImplPlan {
    pub(crate) interface: usize,
    pub(crate) arguments: Vec<Type>,
    pub(crate) receiver: Type,
    pub(crate) receiver_parameters: Vec<String>,
    pub(crate) source_id: String,
    pub(crate) span: ByteSpan,
    pub(crate) module_index: usize,
    pub(crate) declaration_index: usize,
    /// The block's index among its owner's members.
    pub(crate) member_index: usize,
    /// Each written member's name and index within the block.
    pub(crate) written: Vec<(String, usize)>,
}

/// One module of a checking run, as the planner sees it.
pub(crate) struct PlanModule<'a> {
    pub(crate) source_id: &'a str,
    pub(crate) declarations: &'a [Declaration],
}

/// Locates every `impl` block of `modules`, checking its target slot, its
/// completeness against the contract, and overlap with the other blocks of
/// its interface and receiver.
pub(crate) fn plan_implementations(
    types: &TypeNames,
    modules: &[PlanModule<'_>],
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<ImplPlan> {
    let mut plans: Vec<ImplPlan> = Vec::new();
    for (module_index, module) in modules.iter().enumerate() {
        let source_id = module.source_id;
        for (declaration_index, declaration) in module.declarations.iter().enumerate() {
            let (members, owner) = match declaration {
                Declaration::Deftype(value) => {
                    (value.members(), Owner::Type(value.span()))
                }
                Declaration::Defint(value) => {
                    (value.members(), Owner::Interface(value.span()))
                }
                _ => continue,
            };
            for (member_index, member) in members.iter().enumerate() {
                let TypeMember::Implementation(block) = member else {
                    continue;
                };
                let Some((interface, arguments, receiver, receiver_parameters)) =
                    resolve_block(
                        types,
                        source_id,
                        &owner,
                        block.target(),
                        block.span(),
                        diagnostics,
                    )
                else {
                    continue;
                };
                let Some(written) = check_completeness(
                    types,
                    source_id,
                    interface,
                    block.members(),
                    block.span(),
                    diagnostics,
                ) else {
                    continue;
                };
                plans.push(ImplPlan {
                    interface,
                    arguments,
                    receiver,
                    receiver_parameters,
                    source_id: source_id.to_owned(),
                    span: block.span(),
                    module_index,
                    declaration_index,
                    member_index,
                    written,
                });
            }
        }
    }
    let mut overlapping = BTreeSet::new();
    for (later, plan) in plans.iter().enumerate() {
        let earlier = plans.iter().take(later).find(|earlier| {
            earlier.interface == plan.interface
                && crate::union::unifiable(&target(earlier), &target(plan))
        });
        if let Some(earlier) = earlier {
            overlapping.insert(later);
            diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::TypeOverlappingImplementation,
                    plan.span,
                    "this implementation overlaps another of the same interface on the same receiver",
                )
                .with_source_id(plan.source_id.clone())
                .with_related_source(
                    earlier.source_id.clone(),
                    earlier.span,
                    "the other implementation is here",
                ),
            );
        }
    }
    plans
        .into_iter()
        .enumerate()
        .filter(|(index, _)| !overlapping.contains(index))
        .map(|(_, plan)| plan)
        .collect()
}

enum Owner {
    /// A `deftype`, identified by its span.
    Type(ByteSpan),
    /// A `defint`, identified by its span.
    Interface(ByteSpan),
}

/// The interface, its arguments, and the receiver of one block.
fn resolve_block(
    types: &TypeNames,
    source_id: &str,
    owner: &Owner,
    target: &TypeExpr,
    span: ByteSpan,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<(usize, Vec<Type>, Type, Vec<String>)> {
    let wrong_kind = |diagnostics: &mut Vec<Diagnostic>, message: &str| {
        diagnostics.push(
            Diagnostic::new(DiagnosticCode::NameWrongEntityKind, span, message)
                .with_source_id(source_id),
        );
    };
    match owner {
        Owner::Type(owner_span) => {
            let declared = types.declared().iter().find(|declared| {
                declared.span == *owner_span && declared.source_id == source_id
            })?;
            let receiver =
                crate::nominal::declared_self_type(&declared.id, &declared.parameters);
            let parameters = declared.parameters.clone();
            let (head, written) = match target {
                TypeExpr::Name(name) => (name, &[][..]),
                TypeExpr::Applied { head, arguments } => (head, arguments.as_slice()),
                _ => {
                    wrong_kind(
                        diagnostics,
                        "an `impl` inside a `deftype` targets an interface",
                    );
                    return None;
                }
            };
            let Some(interface) = types.resolve_interface(source_id, head) else {
                if types.resolve(source_id, head).is_ok() {
                    wrong_kind(
                        diagnostics,
                        "an `impl` inside a `deftype` targets an interface, not a type",
                    );
                } else {
                    diagnostics.push(
                        Diagnostic::new(
                            DiagnosticCode::NameUnknownSymbol,
                            span,
                            format!(
                                "`{}` does not name a visible interface",
                                head.value()
                            ),
                        )
                        .with_source_id(source_id),
                    );
                }
                return None;
            };
            let arguments = written
                .iter()
                .map(|argument| {
                    types.lower_or_report(
                        source_id,
                        Scope::new(Some(&receiver), &parameters),
                        argument,
                        span,
                        diagnostics,
                    )
                })
                .collect::<Option<Vec<_>>>()?;
            check_arity(types, source_id, interface, &arguments, span, diagnostics)?;
            Some((interface, arguments, receiver, parameters))
        }
        Owner::Interface(owner_span) => {
            let interface = types.interfaces().iter().position(|interface| {
                interface.span == *owner_span && interface.source_id == source_id
            })?;
            if matches!(
                target,
                TypeExpr::Record(_)
                    | TypeExpr::Enum(_)
                    | TypeExpr::Tuple(_)
                    | TypeExpr::Union(_)
            ) {
                wrong_kind(
                    diagnostics,
                    "an anonymous type has no owner to carry an implementation",
                );
                return None;
            }
            if let TypeExpr::Name(name) = target
                && types.resolve_interface(source_id, name).is_some()
            {
                wrong_kind(
                    diagnostics,
                    "an `impl` inside a `defint` targets a concrete type, not an interface",
                );
                return None;
            }
            let receiver = types.lower_or_report(
                source_id,
                Scope::NONE,
                target,
                span,
                diagnostics,
            )?;
            if let Type::Declared(id) | Type::Applied(id, _) = &receiver
                && types
                    .index_of(id)
                    .and_then(|index| types.get(index))
                    .is_some_and(|declared| declared.source_id == source_id)
            {
                diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCode::TypeRedundantImplementation,
                        span,
                        "a type declared in this module implements the interface inside its own `deftype`",
                    )
                    .with_source_id(source_id),
                );
                return None;
            }
            check_arity(types, source_id, interface, &[], span, diagnostics)?;
            Some((interface, Vec::new(), receiver, Vec::new()))
        }
    }
}

fn check_arity(
    types: &TypeNames,
    source_id: &str,
    interface: usize,
    arguments: &[Type],
    span: ByteSpan,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<()> {
    let expected = types.interface(interface)?.parameters.len();
    if expected == arguments.len() {
        return Some(());
    }
    diagnostics.push(
        Diagnostic::new(
            DiagnosticCode::TypeTypeArgumentMismatch,
            span,
            format!(
                "the interface takes {expected} type arguments, not {}",
                arguments.len()
            ),
        )
        .with_source_id(source_id),
    );
    None
}

/// Every abstract member written once and no default member redeclared.
fn check_completeness(
    types: &TypeNames,
    source_id: &str,
    interface: usize,
    written: &[FunctionDeclaration],
    span: ByteSpan,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Vec<(String, usize)>> {
    let contract = &types.interface(interface)?.members;
    let mut valid = true;
    let mut seen = BTreeSet::new();
    let mut members = Vec::new();
    for (index, member) in written.iter().enumerate() {
        let name = member.name().value();
        let report = |diagnostics: &mut Vec<Diagnostic>, code, message: String| {
            diagnostics.push(
                Diagnostic::new(code, member.span(), message).with_source_id(source_id),
            );
        };
        if !seen.insert(name.to_owned()) {
            valid = false;
            report(
                diagnostics,
                DiagnosticCode::NameMemberCollision,
                format!("the implementation writes `{name}` more than once"),
            );
            continue;
        }
        match contract.iter().find(|candidate| candidate.name == name) {
            None => {
                valid = false;
                report(
                    diagnostics,
                    DiagnosticCode::NameUnknownSymbol,
                    format!("the contract has no member `{name}`"),
                );
            }
            Some(candidate) if candidate.default => {
                valid = false;
                report(
                    diagnostics,
                    DiagnosticCode::TypeDefaultOverride,
                    format!("`{name}` is a default member and cannot be redeclared"),
                );
            }
            Some(_) => members.push((name.to_owned(), index)),
        }
    }
    for missing in contract
        .iter()
        .filter(|candidate| !candidate.default && !seen.contains(&candidate.name))
    {
        valid = false;
        diagnostics.push(
            Diagnostic::new(
                DiagnosticCode::TypeMissingAbstractMember,
                span,
                format!(
                    "the implementation does not write the abstract member `{}`",
                    missing.name
                ),
            )
            .with_source_id(source_id),
        );
    }
    valid.then_some(members)
}

/// The substitution that instantiates a contract signature for one
/// implementation: `self` and the interface's own parameters.
pub(crate) fn contract_substitution(
    types: &TypeNames,
    interface: usize,
    receiver: &Type,
    arguments: &[Type],
) -> BTreeMap<String, Type> {
    let mut substitution = BTreeMap::from([(SELF.to_owned(), receiver.clone())]);
    if let Some(interface) = types.interface(interface) {
        substitution.extend(
            interface
                .parameters
                .iter()
                .cloned()
                .zip(arguments.iter().cloned()),
        );
    }
    substitution
}

/// Reports a written member whose signature is not the contract's with `self`
/// and the interface arguments substituted.
pub(crate) fn check_member_signature(
    types: &TypeNames,
    plan: &ImplPlan,
    name: &str,
    written: &FunctionSignature,
    span: ByteSpan,
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    let Some(contract) = types.interface(plan.interface).and_then(|interface| {
        interface.members.iter().find(|member| member.name == name)
    }) else {
        return false;
    };
    let expected = contract.signature.substitute(&contract_substitution(
        types,
        plan.interface,
        &plan.receiver,
        &plan.arguments,
    ));
    if expected.same_shape(written) {
        return true;
    }
    mismatch(
        diagnostics,
        &plan.source_id,
        span,
        Type::Function(Box::new(expected)),
        Type::Function(Box::new(written.clone())),
        "an implementation member must have its contract signature",
    );
    false
}

/// Records every planned block with the headers that implement its members:
/// `written` maps a written member's name to its header, and `defaults` maps
/// an interface and member name to its default header.
pub(crate) fn register(
    types: &mut TypeNames,
    plan: &ImplPlan,
    written: &BTreeMap<String, usize>,
    defaults: &BTreeMap<(usize, String), usize>,
) {
    let Some(contract) = types
        .interface(plan.interface)
        .map(|interface| interface.members.clone())
    else {
        return;
    };
    let mut members = BTreeMap::new();
    for member in contract {
        let header = if member.default {
            defaults
                .get(&(plan.interface, member.name.clone()))
                .copied()
        } else {
            written.get(&member.name).copied()
        };
        if let Some(header) = header {
            members.insert(member.name, header);
        }
    }
    types.add_implementation(Implementation {
        interface: plan.interface,
        receiver: plan.receiver.clone(),
        members,
    });
}

/// Checks `(interface.member operand…)`: the receiver operand selects the
/// implementation, statically for a known type and at run time for a
/// generic bounded by the interface.
pub(crate) fn check_contract_call(
    environment: &mut CheckEnvironment<'_>,
    application: &Application,
    interface: usize,
    member: usize,
    expected: Option<Type>,
) -> Option<Expr> {
    let declared = environment.types.interface(interface)?.clone();
    let contract = declared.members.get(member)?.clone();
    let span = application.span();
    let unavailable = |environment: &mut CheckEnvironment<'_>, message: &str| {
        crate::unavailable(
            environment.diagnostics,
            environment.source_id,
            span,
            message,
        );
    };
    let Some(position) = contract.receiver else {
        unavailable(
            environment,
            "destination-dispatched contract members are called from M3 Step 13",
        );
        return None;
    };
    if !declared.parameters.is_empty()
        || !contract.generics.is_empty()
        || !contract.signature.labelled().is_empty()
        || contract.signature.variadic().is_some()
        || application.type_arguments().is_some()
    {
        unavailable(
            environment,
            "contract calls with generic interfaces, member generics, labelled operands, or variadic tails arrive with Steps 13 and 14",
        );
        return None;
    }
    let operands = application.arguments();
    if operands.len() != contract.signature.parameters().len()
        || operands.iter().any(|operand| operand.label().is_some())
    {
        call_contract_error(
            environment,
            span,
            format!(
                "`{}.{}` takes exactly {} unlabelled operands",
                declared.name,
                contract.name,
                contract.signature.parameters().len()
            ),
        );
        return None;
    }
    let receiver_value =
        check_expression(environment, operands.get(position)?.value(), None)?;
    let receiver = receiver_value.result_type();
    let dispatch = match &receiver {
        Type::Param(name) => {
            if environment.bounds.get(name) != Some(&interface) {
                unsatisfied(
                    environment,
                    operands.get(position)?.value().span(),
                    &receiver,
                    &declared.name,
                );
                return None;
            }
            None
        }
        _ => {
            let candidates = environment
                .types
                .implementations()
                .iter()
                .filter(|implementation| {
                    implementation.interface == interface
                        && covers(&implementation.receiver, &receiver)
                })
                .collect::<Vec<_>>();
            match candidates.as_slice() {
                [implementation] => Some(*implementation.members.get(&contract.name)?),
                // A closed key type conforms through the toolchain.
                [] if closed_key(environment, interface, &receiver) => None,
                [] => {
                    unsatisfied(
                        environment,
                        operands.get(position)?.value().span(),
                        &receiver,
                        &declared.name,
                    );
                    return None;
                }
                _ => {
                    environment.diagnostics.push(
                        Diagnostic::new(
                            DiagnosticCode::TypeAmbiguousImplementation,
                            span,
                            format!("more than one implementation of `{}` matches {receiver}", declared.name),
                        )
                        .with_source_id(environment.source_id),
                    );
                    return None;
                }
            }
        }
    };
    let signature = contract
        .signature
        .substitute(&BTreeMap::from([(SELF.to_owned(), receiver.clone())]));
    let mut arguments = Vec::with_capacity(operands.len());
    for (index, (operand, parameter)) in
        operands.iter().zip(signature.parameters()).enumerate()
    {
        if index == position {
            arguments.push(receiver_value.clone());
        } else {
            arguments.push(check_operand(
                environment,
                operand.value(),
                Some(parameter.clone()),
            )?);
        }
    }
    let result = signature.result();
    crate::ensure_expected(environment, span, expected.clone(), result.clone());
    if expected
        .as_ref()
        .is_some_and(|expected| !crate::types_match(expected, &result))
    {
        return None;
    }
    let origin = SourceOrigin::new(environment.source_id, span);
    Some(match dispatch {
        Some(function) => Expr::call(function, arguments, result, origin),
        None => Expr::Call {
            target: CallTarget::Contract {
                interface: declared.id.clone(),
                member: contract.name.clone(),
                receiver: position,
                signature: Box::new(signature),
                closed: closed_contract(environment.types, interface, &contract.name),
            },
            arguments,
            result,
            tail: false,
            origin,
        },
    })
}

fn unsatisfied(
    environment: &mut CheckEnvironment<'_>,
    span: ByteSpan,
    receiver: &Type,
    interface: &str,
) {
    environment.diagnostics.push(
        Diagnostic::new(
            DiagnosticCode::TypeUnsatisfiedBound,
            span,
            format!("{receiver} does not implement `{interface}`"),
        )
        .with_source_id(environment.source_id),
    );
}

/// Reports every type argument of a generic application that does not
/// implement its parameter's interface bound.
pub(crate) fn check_bounds(
    environment: &mut CheckEnvironment<'_>,
    span: ByteSpan,
    bounds: &BTreeMap<String, usize>,
    arguments: &BTreeMap<String, Type>,
) -> bool {
    let mut satisfied = true;
    for (name, interface) in bounds {
        let Some(argument) = arguments.get(name) else {
            continue;
        };
        let scope = Scope::new(environment.self_type.as_ref(), &environment.generics)
            .with_bounds(&environment.bounds);
        let holds = environment.types.satisfies(scope, *interface, argument);
        if !holds {
            satisfied = false;
            let interface = environment
                .types
                .interface(*interface)
                .map(|interface| interface.name.clone())
                .unwrap_or_default();
            unsatisfied(environment, span, argument, &interface);
        }
    }
    satisfied
}

/// Lowers every contract of `modules`, then appends one function header per
/// interface default and per member written in an `impl` block, and records
/// each block as an implementation. `name` gives a header's program name from
/// its module index and the member's span, when the caller has one.
pub(crate) fn materialize(
    types: &mut TypeNames,
    modules: &[PlanModule<'_>],
    functions: &mut Vec<crate::FunctionHeader>,
    name: &dyn Fn(usize, ByteSpan) -> Option<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for (index, declaration) in types.standard_contracts() {
        let Some(source_id) = types
            .interface(index)
            .map(|interface| interface.source_id.clone())
        else {
            continue;
        };
        lower_contract(types, &source_id, index, declaration, diagnostics);
    }
    for module in modules {
        for declaration in module.declarations {
            let Declaration::Defint(value) = declaration else {
                continue;
            };
            let Some(index) = types.interfaces().iter().position(|interface| {
                interface.span == value.span()
                    && interface.source_id == module.source_id
            }) else {
                continue;
            };
            lower_contract(types, module.source_id, index, value, diagnostics);
        }
    }
    let mut defaults = BTreeMap::new();
    for (module_index, module) in modules.iter().enumerate() {
        for (declaration_index, declaration) in module.declarations.iter().enumerate() {
            let Declaration::Defint(value) = declaration else {
                continue;
            };
            let Some(index) = types.interfaces().iter().position(|interface| {
                interface.span == value.span()
                    && interface.source_id == module.source_id
            }) else {
                continue;
            };
            let Some(interface) = types.interface(index).cloned() else {
                continue;
            };
            for member in interface.members.iter().filter(|member| member.default) {
                let Some(member_index) = value.members().iter().position(|written| {
                    matches!(written, TypeMember::Method(method) if method.span() == member.span)
                }) else {
                    continue;
                };
                let mut type_parameters = vec![SELF.to_owned()];
                type_parameters.extend(interface.parameters.iter().cloned());
                type_parameters.extend(member.generics.iter().cloned());
                defaults.insert((index, member.name.clone()), functions.len());
                functions.push(crate::FunctionHeader {
                    declaration_index,
                    module_index,
                    source_id: module.source_id.to_owned(),
                    name: name(module_index, member.span).unwrap_or_else(|| {
                        format!("{}.{}", interface.name, member.name)
                    }),
                    signature: member.signature.clone(),
                    external: None,
                    external_declared: false,
                    test: None,
                    test_assertion: None,
                    member_index: Some(member_index),
                    self_type: Some(self_type()),
                    type_parameters,
                    bounds: BTreeMap::from([(SELF.to_owned(), index)]),
                    impl_member: None,
                    implements: Some(Implements {
                        interface: interface.id.clone(),
                        member: member.name.clone(),
                        receiver: self_type(),
                    }),
                });
            }
        }
    }
    let plans = plan_implementations(types, modules, diagnostics);
    for plan in &plans {
        let Some(module) = modules.get(plan.module_index) else {
            continue;
        };
        let (owner, members) = match module.declarations.get(plan.declaration_index) {
            Some(Declaration::Deftype(value)) => {
                (value.name().value(), value.members())
            }
            Some(Declaration::Defint(value)) => (value.name().value(), value.members()),
            _ => continue,
        };
        let Some(TypeMember::Implementation(block)) = members.get(plan.member_index)
        else {
            continue;
        };
        let Some(interface) = types.interface(plan.interface).cloned() else {
            continue;
        };
        let mut written = BTreeMap::new();
        for (member_name, position) in &plan.written {
            let Some(method) = block.members().get(*position) else {
                continue;
            };
            let Some(bounds) = generic_bounds(
                types,
                &plan.source_id,
                method.attributes().items(),
                diagnostics,
            ) else {
                continue;
            };
            // A block nested in a bounded `deftype` sees the type's bounds.
            let mut bounds = bounds;
            if let Type::Applied(id, _) = &plan.receiver
                && let Some(owner) =
                    types.index_of(id).and_then(|index| types.get(index))
            {
                bounds.extend(owner.bounds.clone());
            }
            let mut type_parameters = plan.receiver_parameters.clone();
            type_parameters
                .extend(crate::nominal::generic_names(method.attributes().items()));
            let Some(signature) = crate::check_signature(
                &plan.source_id,
                method,
                diagnostics,
                types,
                Scope::new(Some(&plan.receiver), &type_parameters).with_bounds(&bounds),
            ) else {
                continue;
            };
            if !check_member_signature(
                types,
                plan,
                member_name,
                &signature,
                method.span(),
                diagnostics,
            ) {
                continue;
            }
            written.insert(member_name.clone(), functions.len());
            functions.push(crate::FunctionHeader {
                declaration_index: plan.declaration_index,
                module_index: plan.module_index,
                source_id: plan.source_id.clone(),
                name: name(plan.module_index, method.span()).unwrap_or_else(|| {
                    format!("{owner}.impl-{}.{member_name}", plan.member_index)
                }),
                signature,
                external: None,
                external_declared: false,
                test: None,
                test_assertion: None,
                member_index: Some(plan.member_index),
                self_type: Some(plan.receiver.clone()),
                type_parameters,
                bounds,
                impl_member: Some(*position),
                implements: Some(Implements {
                    interface: interface.id.clone(),
                    member: member_name.clone(),
                    receiver: plan.receiver.clone(),
                }),
            });
        }
        register(types, plan, &written, &defaults);
    }
    types.set_bounds_ready();
    check_header_bounds(types, modules, functions, diagnostics);
}

/// Checks the bounds of every applied declared type in the signatures and
/// type bodies of `modules`, which lowered before any implementation was
/// known; lowering checks them itself from here on.
fn check_header_bounds(
    types: &TypeNames,
    modules: &[PlanModule<'_>],
    functions: &[crate::FunctionHeader],
    diagnostics: &mut Vec<Diagnostic>,
) {
    for header in functions {
        let Some(function) = modules
            .get(header.module_index)
            .and_then(|module| crate::header_function(module.declarations, header))
        else {
            continue;
        };
        let scope = Scope::new(header.self_type.as_ref(), &header.type_parameters)
            .with_bounds(&header.bounds);
        // Body checking lowers each positional parameter again, at its own
        // span, so only the other slots are checked here.
        let mut slots = header
            .signature
            .slot_types()
            .into_iter()
            .skip(header.signature.parameters().len())
            .collect::<Vec<_>>();
        slots.push(header.signature.result());
        for slot in slots {
            for error in types.unsatisfied_bounds(scope, &slot) {
                crate::nominal::report_lower_error(
                    diagnostics,
                    &header.source_id,
                    function.span(),
                    &error,
                );
            }
        }
    }
    for declared in types.declared() {
        if !modules
            .iter()
            .any(|module| module.source_id == declared.source_id)
        {
            continue;
        }
        let Some(body) = &declared.body else {
            continue;
        };
        let scope =
            Scope::new(None, &declared.parameters).with_bounds(&declared.bounds);
        for (_, slot) in body.slots() {
            for error in types.unsatisfied_bounds(scope, &slot) {
                crate::nominal::report_lower_error(
                    diagnostics,
                    &declared.source_id,
                    declared.span,
                    &error,
                );
            }
        }
    }
}

/// The type argument each of `parameters` took when `generic` was
/// instantiated as `instantiated`.
pub(crate) fn type_arguments(
    parameters: &[String],
    generic: &FunctionSignature,
    instantiated: &FunctionSignature,
) -> BTreeMap<String, Type> {
    let mut instantiation = crate::infer::Instantiation::new(parameters);
    let opened = instantiation.open(&Type::Function(Box::new(generic.clone())));
    instantiation.unify(&opened, &Type::Function(Box::new(instantiated.clone())));
    parameters
        .iter()
        .filter_map(|name| {
            let variable = instantiation.open(&Type::Param(name.clone()));
            Some((name.clone(), instantiation.resolved(&variable)?))
        })
        .collect()
}

/// A block's receiver and interface arguments as one type, so overlap is
/// decided by a single substitution across all of them.
fn target(plan: &ImplPlan) -> Type {
    Type::Tuple(
        std::iter::once(plan.receiver.clone())
            .chain(plan.arguments.iter().cloned())
            .collect(),
    )
}

/// Whether the implementation receiver `pattern`, whose generic names stand
/// for any type, covers `actual`, whose generic names are the caller's own
/// and so stand only for themselves.
pub(crate) fn covers(pattern: &Type, actual: &Type) -> bool {
    let mut names = BTreeSet::new();
    parameter_names(actual, &mut names);
    let rigid = names
        .into_iter()
        .map(|name| {
            let opaque = Type::Declared(vibra_ir::TypeId::new(
                format!("?rigid:{name}"),
                name.clone(),
            ));
            (name, opaque)
        })
        .collect();
    crate::union::unifiable(pattern, &actual.substitute(&rigid))
}

fn parameter_names(value: &Type, names: &mut BTreeSet<String>) {
    if let Type::Param(name) = value {
        names.insert(name.clone());
    }
    for component in value.components() {
        parameter_names(&component, names);
    }
}

/// The `@std.core` key contract `interface` is, with its one member.
pub(crate) fn key_contract(
    types: &TypeNames,
    interface: usize,
) -> Option<(&'static str, ClosedContract)> {
    let id = &types.interface(interface)?.id;
    [
        ("ordered", "compare", ClosedContract::KeyCompare),
        ("equatable", "equal", ClosedContract::KeyEqual),
    ]
    .into_iter()
    .find(|(name, _, _)| *id == crate::stdlib::stdlib_type_id(&["core"], name))
    .map(|(_, member, closed)| (member, closed))
}

/// The toolchain conformance a call of `member` of `interface` falls back to.
fn closed_contract(
    types: &TypeNames,
    interface: usize,
    member: &str,
) -> Option<ClosedContract> {
    key_contract(types, interface)
        .filter(|(name, _)| *name == member)
        .map(|(_, closed)| closed)
}

/// Whether `receiver` conforms to the key contract `interface` through the
/// closed toolchain registry in the scope of `environment`.
fn closed_key(
    environment: &CheckEnvironment<'_>,
    interface: usize,
    receiver: &Type,
) -> bool {
    let scope = Scope::new(environment.self_type.as_ref(), &environment.generics)
        .with_bounds(&environment.bounds);
    environment.types.closed_key(scope, interface, receiver)
}

/// The `ordered` interface a map keyed by `key` orders its keys through:
/// `None` for a key of the closed registry alone, whose canonical key order is
/// its `compare`.
pub(crate) fn key_order(types: &TypeNames, key: &Type) -> Option<vibra_ir::TypeId> {
    if crate::nominal::map_key(key) == crate::nominal::KeyVerdict::Admissible {
        return None;
    }
    let id = crate::stdlib::stdlib_type_id(&["core"], "ordered");
    types.interface_index_of(&id).map(|_| id)
}
