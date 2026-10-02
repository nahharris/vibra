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
use vibra_ir::external::{CompilerIntrinsic, NumericType};
use vibra_ir::{
    CallTarget, ClosedContract, Expr, FunctionSignature, Implements, SourceOrigin, Type,
};
use vibra_syntax::{
    Application, Attribute, Declaration, DefintDeclaration, DeftypeBody,
    FunctionDeclaration, TypeExpr, TypeMember,
};

use crate::nominal::{
    ContractMember, DeclaredInterface, Implementation, Scope, TypeNames,
};
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
                // A bound is one name, so it cannot apply a generic interface;
                // a parameter typed as its interface value takes that role.
                Some(interface)
                    if types
                        .interface(interface)
                        .is_some_and(|declared| !declared.parameters.is_empty()) =>
                {
                    valid = false;
                    diagnostics.push(
                        Diagnostic::new(
                            DiagnosticCode::TypeTypeArgumentMismatch,
                            binding.span(),
                            format!(
                                "`{}` is a generic interface and cannot bound a parameter; take its interface value, such as `({} item)`",
                                binding.bound().value(),
                                binding.bound().value()
                            ),
                        )
                        .with_source_id(source_id),
                    );
                }
                Some(interface) => {
                    bounds.insert(binding.name().value().to_owned(), interface);
                }
                None => {
                    valid = false;
                    // A type found where an interface is required is the wrong
                    // kind of entity, not an unknown name.
                    let (code, message) = if types
                        .resolve(source_id, binding.bound())
                        .is_ok()
                    {
                        (
                            DiagnosticCode::NameWrongEntityKind,
                            format!(
                                "`{}` is a type, not an interface; a bound names an interface",
                                binding.bound().value()
                            ),
                        )
                    } else {
                        (
                            DiagnosticCode::NameUnknownSymbol,
                            format!(
                                "`{}` does not name a visible interface",
                                binding.bound().value()
                            ),
                        )
                    };
                    diagnostics.push(
                        Diagnostic::new(code, binding.span(), message)
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
        // A member's own `where:` bounds hold in its body and at each call.
        let Some(bounds) =
            generic_bounds(types, source_id, method.attributes().items(), diagnostics)
        else {
            continue;
        };
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
        let defaults =
            labelled_defaults(source_id, method.attributes().items(), &signature);
        members.push(ContractMember {
            name: method.name().value().to_owned(),
            span: method.span(),
            signature,
            generics: crate::nominal::generic_names(method.attributes().items()),
            bounds,
            default: !method.expressions().is_empty(),
            defaults,
            receiver: position,
        });
    }
    types.set_contract(index, members);
}

/// Whether `value` uses `self` where an interface value cannot stand for it:
/// as an operand of a function type, which another receiver's value could
/// then be passed to, or inside a dict key, which must be a concrete type.
fn self_escapes(value: &Type) -> bool {
    match value {
        Type::Function(signature) => {
            signature.parameters().iter().any(mentions_self)
                || signature
                    .labelled()
                    .iter()
                    .any(|slot| mentions_self(&slot.value_type()))
                || signature.variadic().is_some_and(mentions_self)
                || self_escapes(&signature.result())
        }
        Type::Dict(key, value) => mentions_self(key) || self_escapes(value),
        _ => value.components().iter().any(self_escapes),
    }
}

/// Rejects a member call through an interface value when the member's result
/// would let two receivers be mixed (`docs/spec/02-type-system.md`,
/// "Interfaces and methods").
fn reject_escaping_self(
    environment: &mut CheckEnvironment<'_>,
    span: ByteSpan,
    contract: &ContractMember,
    receiver: &Type,
) -> bool {
    if !self_escapes(&contract.signature.result()) {
        return false;
    }
    mismatch(
        environment.diagnostics,
        environment.source_id,
        span,
        self_type(),
        receiver.clone(),
        "a member whose result takes `self` as a function operand or a dict key cannot be called through an interface value, which erases that type",
    );
    true
}

/// Whether `expression` calls a contract member that is selected by its
/// destination, having no `self` operand.
pub(crate) fn selected_by_destination(
    environment: &CheckEnvironment<'_>,
    expression: &vibra_syntax::Expression,
) -> bool {
    let vibra_syntax::ExpressionKind::Application(application) = expression.kind()
    else {
        return false;
    };
    let vibra_syntax::ExpressionKind::Name(name) = application.callee().kind() else {
        return false;
    };
    environment
        .types
        .contract_member(environment.source_id, name)
        .and_then(|(interface, member)| {
            environment
                .types
                .interface(interface)?
                .members
                .get(member)
                .map(|contract| contract.receiver.is_none())
        })
        .unwrap_or(false)
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
                    let intrinsic = match value.body() {
                        DeftypeBody::Intrinsic(atom) => Some(atom.value().to_owned()),
                        DeftypeBody::Type(_) => None,
                    };
                    (value.members(), Owner::Type(value.span(), intrinsic))
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
            continue;
        }
        // Across `from` and `try-from` a receiver converts from each source
        // one way: totally or partially, never both.
        let Some(total) = conversion_contract(types, plan.interface) else {
            continue;
        };
        let other = plans.iter().take(later).find(|earlier| {
            conversion_contract(types, earlier.interface) == Some(!total)
                && crate::union::unifiable(&target(earlier), &target(plan))
        });
        if let Some(other) = other {
            overlapping.insert(later);
            diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::TypeRedundantConversion,
                    plan.span,
                    "this receiver already converts from this source through the other conversion contract",
                )
                .with_source_id(plan.source_id.clone())
                .with_related_source(
                    other.source_id.clone(),
                    other.span,
                    "the other conversion is here",
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
    /// A `deftype`, identified by its span, with its registry atom when it
    /// declares an intrinsic type.
    Type(ByteSpan, Option<String>),
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
        Owner::Type(owner_span, intrinsic) => {
            let index = types.declared().iter().position(|declared| {
                declared.span == *owner_span && declared.source_id == source_id
            })?;
            let declared = types.get(index)?;
            // A type the toolchain represents directly is implemented at that
            // representation, which is the type its values have.
            let receiver = match intrinsic {
                Some(atom) => {
                    crate::standard::builtin_self_type(atom, &declared.parameters)?
                }
                None => types.representation(index).unwrap_or_else(|| {
                    crate::nominal::declared_self_type(
                        &declared.id,
                        &declared.parameters,
                    )
                }),
            };
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
                    | TypeExpr::Union(_, _)
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
            if matches!(receiver, Type::Interface(_, _) | Type::Any) {
                wrong_kind(
                    diagnostics,
                    "an `impl` inside a `defint` targets a concrete type, not an interface",
                );
                return None;
            }
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

/// The default of each labelled parameter among `attributes`: its written
/// spelling and the value it denotes at the parameter's type in `signature`.
/// A default that does not check has no value; the function's own check
/// reports it.
fn labelled_defaults(
    source_id: &str,
    attributes: &[Attribute],
    signature: &FunctionSignature,
) -> BTreeMap<String, (String, Option<vibra_ir::Value>)> {
    attributes
        .iter()
        .filter_map(|attribute| match attribute {
            Attribute::Labelled(parameters) => Some(parameters),
            _ => None,
        })
        .flatten()
        .map(|parameter| {
            let name = parameter.name().value();
            let value = signature
                .labelled()
                .iter()
                .find(|labelled| labelled.name() == name)
                .and_then(|labelled| {
                    crate::check_default(
                        source_id,
                        parameter.span(),
                        parameter.default(),
                        &labelled.value_type(),
                        &mut Vec::new(),
                    )
                });
            (
                name.to_owned(),
                (parameter.default().raw().to_owned(), value),
            )
        })
        .collect()
}

/// Reports a written member whose signature is not the contract's with `self`
/// and the interface arguments substituted, or whose labelled defaults are
/// not the contract's.
pub(crate) fn check_member_signature(
    types: &TypeNames,
    plan: &ImplPlan,
    name: &str,
    written: &FunctionSignature,
    attributes: &[Attribute],
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
        // Two spellings of one value, such as `1` and `1i32` at `i32`, are
        // the same default.
        let defaults = labelled_defaults(&plan.source_id, attributes, written);
        let Some((label, (default, _))) =
            contract.defaults.iter().find(|(label, (_, value))| {
                defaults
                    .get(*label)
                    .is_none_or(|(_, written)| written != value)
            })
        else {
            return true;
        };
        diagnostics.push(
            Diagnostic::new(
                DiagnosticCode::TypeMismatch,
                span,
                format!(
                    "an implementation member must keep its contract's labelled defaults: `{label}` defaults to `{default}` in `{name}`"
                ),
            )
            .with_source_id(&plan.source_id),
        );
        return false;
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
        arguments: plan.arguments.clone(),
        receiver: plan.receiver.clone(),
        source_id: plan.source_id.clone(),
        span: plan.span,
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
    tail_position: bool,
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
    if !contract.signature.labelled().is_empty()
        || application.type_arguments().is_some()
    {
        unavailable(
            environment,
            "labelled operands and written type arguments on a contract member are outside the M3 profile",
        );
        return None;
    }
    // A default member is one function for every receiver.
    if contract.default
        && let Some(position) = contract.receiver
    {
        return check_default_call(
            environment,
            application,
            interface,
            &declared,
            &contract,
            position,
            expected,
            tail_position,
        );
    }
    if !contract.generics.is_empty() {
        unavailable(
            environment,
            "an abstract contract member with its own generic parameters is outside the M3 profile",
        );
        return None;
    }
    // A destination-dispatched member, a generic interface, and a variadic
    // member select among implementations.
    let Some(position) = contract.receiver.filter(|_| {
        declared.parameters.is_empty() && contract.signature.variadic().is_none()
    }) else {
        return check_selected_call(
            environment,
            application,
            interface,
            &declared,
            &contract,
            expected,
            tail_position,
        );
    };
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
        // An interface value dispatches from the type it holds. Its concrete
        // type is erased, so no other operand can be required to share it.
        Type::Interface(id, _) if *id == declared.id => {
            let shared = contract.signature.parameters().iter().enumerate().find(
                |(index, parameter)| *index != position && mentions_self(parameter),
            );
            if let Some((index, _)) = shared {
                mismatch(
                    environment.diagnostics,
                    environment.source_id,
                    operands.get(index)?.value().span(),
                    self_type(),
                    receiver.clone(),
                    "a member with another `self` operand cannot be called through an interface value, which erases the type they must share",
                );
                return None;
            }
            if reject_escaping_self(environment, span, &contract, &receiver) {
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
        Some(function) => crate::direct_call(
            environment,
            function,
            arguments,
            result,
            origin,
            tail_position,
        ),
        None => Expr::Call {
            target: CallTarget::Contract {
                interface: declared.id.clone(),
                member: contract.name.clone(),
                receiver: position,
                arguments: Vec::new(),
                destination: None,
                signature: Box::new(signature),
                closed: closed_contract(environment.types, interface, &contract.name),
            },
            arguments,
            result,
            tail: tail_position,
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
                    bounds: std::iter::once((SELF.to_owned(), index))
                        .chain(member.bounds.clone())
                        .collect(),
                    impl_member: None,
                    implements: Some(Implements {
                        interface: interface.id.clone(),
                        member: member.name.clone(),
                        receiver: self_type(),
                        arguments: interface
                            .parameters
                            .iter()
                            .map(|parameter| Type::Param(parameter.clone()))
                            .collect(),
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
                method.attributes().items(),
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
                    arguments: plan.arguments.clone(),
                }),
            });
        }
        register(types, plan, &written, &defaults);
    }
    types.set_default_members(defaults);
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
    if is_iter(types, interface) && member == "next" {
        return Some(ClosedContract::IterNext);
    }
    key_contract(types, interface)
        .filter(|(name, _)| *name == member)
        .map(|(_, closed)| closed)
}

/// Whether `interface` is the `iter` of `@std.iter`, which plays `@iter`.
pub(crate) fn is_iter(types: &TypeNames, interface: usize) -> bool {
    types.interface(interface).is_some_and(|declared| {
        declared.id == crate::stdlib::stdlib_type_id(&["iter"], "iter")
    })
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

/// The `ordered` interface a dict keyed by `key` orders its keys through:
/// `None` for a key of the closed registry alone, whose canonical key order is
/// its `compare`.
pub(crate) fn key_order(types: &TypeNames, key: &Type) -> Option<vibra_ir::TypeId> {
    if crate::nominal::dict_key(key) == crate::nominal::KeyVerdict::Admissible {
        return None;
    }
    let id = crate::stdlib::stdlib_type_id(&["core"], "ordered");
    types.interface_index_of(&id).map(|_| id)
}

/// One implementation a contract call may select.
struct Candidate {
    /// The member's signature at this implementation.
    signature: FunctionSignature,
    target: CandidateTarget,
    /// The applied interface, for diagnostics.
    spelling: String,
    /// The interface's type arguments at this implementation.
    arguments: Vec<Type>,
}

enum CandidateTarget {
    /// A written or default member, by its function.
    Function(usize),
    /// A closed toolchain conformance, by its registry operation.
    Primitive(CompilerIntrinsic),
    /// The implementation for the type the receiver holds at run time: a
    /// builtin constructor type through the closed registry, an interface
    /// value, or the `self` of a default member.
    Dispatch,
}

/// Checks a contract call that selects among implementations: a
/// destination-dispatched member, whose receiver is the written expected
/// type, or a member of a generic interface, which one receiver may implement
/// at several arguments (`docs/spec/02-type-system.md`, "Interfaces and
/// methods"). Selection uses the written operand types and the written
/// expected type, and never picks an order between two that remain.
fn check_selected_call(
    environment: &mut CheckEnvironment<'_>,
    application: &Application,
    interface: usize,
    declared: &DeclaredInterface,
    contract: &ContractMember,
    expected: Option<Type>,
    tail_position: bool,
) -> Option<Expr> {
    let span = application.span();
    let operands = application.arguments();
    let fixed = contract.signature.parameters().len();
    let variadic = contract.signature.variadic().is_some();
    if operands.iter().any(|operand| operand.label().is_some())
        || operands.len() < fixed
        || (!variadic && operands.len() != fixed)
    {
        call_contract_error(
            environment,
            span,
            format!(
                "`{}.{}` takes {}{fixed} unlabelled operands",
                declared.name,
                contract.name,
                if variadic { "at least " } else { "exactly " },
            ),
        );
        return None;
    }
    let mut receiver_value = None;
    let mut dispatched = None;
    let receiver = match contract.receiver {
        Some(position) => {
            let value =
                check_expression(environment, operands.get(position)?.value(), None)?;
            let receiver = value.result_type();
            // The `self` of a default member and an interface value dispatch
            // at run time, at the interface's own or the value's arguments.
            dispatched = match &receiver {
                Type::Param(name)
                    if environment.bounds.get(name) == Some(&interface) =>
                {
                    Some(
                        declared
                            .parameters
                            .iter()
                            .map(|parameter| Type::Param(parameter.clone()))
                            .collect::<Vec<_>>(),
                    )
                }
                Type::Interface(id, arguments) if *id == declared.id => {
                    let shared =
                        contract.signature.parameters().iter().enumerate().find(
                            |(index, parameter)| {
                                *index != position && mentions_self(parameter)
                            },
                        );
                    if let Some((index, _)) = shared {
                        mismatch(
                            environment.diagnostics,
                            environment.source_id,
                            operands.get(index)?.value().span(),
                            self_type(),
                            receiver.clone(),
                            "a member with another `self` operand cannot be called through an interface value, which erases the type they must share",
                        );
                        return None;
                    }
                    if reject_escaping_self(environment, span, contract, &receiver) {
                        return None;
                    }
                    Some(arguments.clone())
                }
                Type::Param(_) | Type::Interface(_, _) | Type::Any => {
                    unsatisfied(
                        environment,
                        operands.get(position)?.value().span(),
                        &receiver,
                        &declared.name,
                    );
                    return None;
                }
                _ => None,
            };
            receiver_value = Some((position, value));
            receiver
        }
        None => destination(
            environment,
            interface,
            declared,
            contract,
            expected.as_ref(),
            span,
        )?,
    };
    // A destination that is a parameter bounded by this interface, such as
    // the `self` of a default member, is instantiated at run time, and the
    // implementation is selected from it there.
    if contract.receiver.is_none()
        && declared.parameters.is_empty()
        && matches!(&receiver, Type::Param(name)
            if environment.bounds.get(name) == Some(&interface))
    {
        dispatched = Some(Vec::new());
    }
    let candidates = match dispatched {
        Some(arguments) => vec![candidate(
            declared,
            contract,
            &receiver,
            &arguments,
            CandidateTarget::Dispatch,
        )],
        None => candidates(environment.types, interface, declared, contract, &receiver),
    };
    if candidates.is_empty() {
        unsatisfied(environment, span, &receiver, &declared.name);
        return None;
    }
    let position = receiver_value.as_ref().map(|(position, _)| *position);
    // An unsuffixed literal has no type of its own, so it cannot choose
    // among conversion sources: which one fits would depend on its value.
    if candidates.len() > 1
        && conversion_contract(environment.types, interface).is_some()
        && operands
            .iter()
            .any(|operand| crate::unsuffixed_literal(operand.value()))
    {
        let mut diagnostic = Diagnostic::new(
            DiagnosticCode::TypeAmbiguousImplementation,
            span,
            format!(
                "an unsuffixed literal fits more than one source of `{}` for {receiver}; write its suffix",
                declared.name
            ),
        )
        .with_source_id(environment.source_id);
        for candidate in &candidates {
            diagnostic = diagnostic
                .with_note(format!("`{}` is a candidate", candidate.spelling));
        }
        environment.diagnostics.push(diagnostic);
        return None;
    }
    let fitting = candidates
        .iter()
        .filter(|candidate| {
            fits(
                environment,
                application,
                candidate,
                position,
                expected.as_ref(),
            )
        })
        .collect::<Vec<_>>();
    let chosen = match (fitting.as_slice(), candidates.as_slice()) {
        ([chosen], _) => *chosen,
        ([], [chosen]) => chosen,
        ([], _) => {
            let mut diagnostic = Diagnostic::new(
                DiagnosticCode::TypeArgumentMismatch,
                span,
                format!(
                    "no implementation of `{}` for {receiver} takes these operands",
                    declared.name
                ),
            )
            .with_source_id(environment.source_id);
            for candidate in &candidates {
                diagnostic = diagnostic.with_note(format!(
                    "{receiver} implements `{}`",
                    candidate.spelling
                ));
            }
            environment.diagnostics.push(diagnostic);
            return None;
        }
        _ => {
            let mut diagnostic = Diagnostic::new(
                DiagnosticCode::TypeAmbiguousImplementation,
                span,
                format!(
                    "more than one implementation of `{}` for {receiver} takes these operands",
                    declared.name
                ),
            )
            .with_source_id(environment.source_id);
            for candidate in &fitting {
                diagnostic = diagnostic
                    .with_note(format!("`{}` is a candidate", candidate.spelling));
            }
            environment.diagnostics.push(diagnostic);
            return None;
        }
    };
    let origin = SourceOrigin::new(environment.source_id, span);
    let mut arguments = Vec::with_capacity(operands.len());
    for (index, (operand, parameter)) in operands
        .iter()
        .zip(chosen.signature.parameters())
        .enumerate()
    {
        match &receiver_value {
            Some((position, value)) if *position == index => {
                arguments.push(value.clone());
            }
            _ => arguments.push(check_operand(
                environment,
                operand.value(),
                Some(parameter.clone()),
            )?),
        }
    }
    if let Some(tail) = chosen.signature.variadic() {
        let Type::Array(element) = tail else {
            crate::unavailable(
                environment.diagnostics,
                environment.source_id,
                span,
                "a dict variadic tail on a contract member is outside the M3 profile",
            );
            return None;
        };
        let mut items = Vec::new();
        for operand in operands.iter().skip(fixed) {
            items.push(check_operand(
                environment,
                operand.value(),
                Some(element.as_ref().clone()),
            )?);
        }
        arguments.push(crate::pack_tail(tail, items, None, origin.clone()));
    }
    let result = chosen.signature.result();
    crate::ensure_expected(environment, span, expected.clone(), result.clone());
    if expected
        .as_ref()
        .is_some_and(|expected| !crate::types_match(expected, &result))
    {
        return None;
    }
    Some(match chosen.target {
        CandidateTarget::Function(function) => crate::direct_call(
            environment,
            function,
            arguments,
            result,
            origin,
            tail_position,
        ),
        CandidateTarget::Primitive(intrinsic) => {
            Expr::external_with_result(intrinsic, arguments, result, origin)
        }
        CandidateTarget::Dispatch => Expr::Call {
            target: CallTarget::Contract {
                interface: declared.id.clone(),
                member: contract.name.clone(),
                receiver: contract.receiver.unwrap_or(0),
                arguments: chosen.arguments.clone(),
                destination: contract.receiver.is_none().then(|| receiver.clone()),
                signature: Box::new(chosen.signature.clone()),
                closed: closed_contract(environment.types, interface, &contract.name),
            },
            arguments,
            result,
            tail: tail_position,
            origin,
        },
    })
}

/// Checks a contract member named as a value when calling it selects among
/// implementations: a destination-dispatched member, a member of a generic
/// interface, or a variadic member. The written expected `fn` type stands
/// for the operands and the expected type of a call: its receiver operand,
/// or its result for a destination, fixes the receiver, and the one
/// implementation whose signature is that type is the value.
fn check_selected_value(
    environment: &mut CheckEnvironment<'_>,
    span: ByteSpan,
    interface: usize,
    declared: &DeclaredInterface,
    contract: &ContractMember,
    expected: Option<&Type>,
) -> Option<Expr> {
    let Some(Type::Function(written)) = expected else {
        environment.diagnostics.push(
            Diagnostic::new(
                DiagnosticCode::TypeAmbiguousInference,
                span,
                format!(
                    "`{}.{}` as a value needs a written expected `fn` type to fix its implementation",
                    declared.name, contract.name
                ),
            )
            .with_source_id(environment.source_id),
        );
        return None;
    };
    let mut dispatched = None;
    let receiver = match contract.receiver {
        Some(position) => {
            let Some(receiver) = written.parameters().get(position).cloned() else {
                crate::ensure_expected(
                    environment,
                    span,
                    expected.cloned(),
                    Type::Function(Box::new(contract.signature.clone())),
                );
                return None;
            };
            dispatched = match &receiver {
                Type::Param(name)
                    if environment.bounds.get(name) == Some(&interface) =>
                {
                    Some(
                        declared
                            .parameters
                            .iter()
                            .map(|parameter| Type::Param(parameter.clone()))
                            .collect::<Vec<_>>(),
                    )
                }
                Type::Interface(id, arguments) if *id == declared.id => {
                    let shared =
                        contract.signature.parameters().iter().enumerate().any(
                            |(index, parameter)| {
                                index != position && mentions_self(parameter)
                            },
                        );
                    if shared {
                        mismatch(
                            environment.diagnostics,
                            environment.source_id,
                            span,
                            self_type(),
                            receiver.clone(),
                            "a member with another `self` operand cannot be called through an interface value, which erases the type they must share",
                        );
                        return None;
                    }
                    if reject_escaping_self(environment, span, contract, &receiver) {
                        return None;
                    }
                    Some(arguments.clone())
                }
                Type::Param(_) | Type::Interface(_, _) | Type::Any => {
                    unsatisfied(environment, span, &receiver, &declared.name);
                    return None;
                }
                _ => None,
            };
            receiver
        }
        None => destination(
            environment,
            interface,
            declared,
            contract,
            Some(&written.result()),
            span,
        )?,
    };
    if contract.receiver.is_none()
        && declared.parameters.is_empty()
        && matches!(&receiver, Type::Param(name)
            if environment.bounds.get(name) == Some(&interface))
    {
        dispatched = Some(Vec::new());
    }
    let candidates = match dispatched {
        Some(arguments) => vec![candidate(
            declared,
            contract,
            &receiver,
            &arguments,
            CandidateTarget::Dispatch,
        )],
        None => candidates(environment.types, interface, declared, contract, &receiver),
    };
    if candidates.is_empty() {
        unsatisfied(environment, span, &receiver, &declared.name);
        return None;
    }
    let fitting = candidates
        .iter()
        .filter(|candidate| candidate.signature.same_shape(written))
        .collect::<Vec<_>>();
    let chosen = match fitting.as_slice() {
        [chosen] => *chosen,
        [] => {
            let mut diagnostic = Diagnostic::new(
                DiagnosticCode::TypeMismatch,
                span,
                format!(
                    "no implementation of `{}` for {receiver} has the written type {}",
                    declared.name,
                    Type::Function(written.clone())
                ),
            )
            .with_source_id(environment.source_id);
            for candidate in &candidates {
                diagnostic = diagnostic.with_note(format!(
                    "`{}` has type {}",
                    candidate.spelling,
                    Type::Function(Box::new(candidate.signature.clone()))
                ));
            }
            environment.diagnostics.push(diagnostic);
            return None;
        }
        _ => {
            let mut diagnostic = Diagnostic::new(
                DiagnosticCode::TypeAmbiguousImplementation,
                span,
                format!(
                    "more than one implementation of `{}` for {receiver} has the written type",
                    declared.name
                ),
            )
            .with_source_id(environment.source_id);
            for candidate in &fitting {
                diagnostic = diagnostic
                    .with_note(format!("`{}` is a candidate", candidate.spelling));
            }
            environment.diagnostics.push(diagnostic);
            return None;
        }
    };
    let signature = chosen.signature.clone();
    let origin = SourceOrigin::new(environment.source_id, span);
    // The closure's slots are the fixed parameters and then the tail, which
    // arrives already packed.
    let slots = signature.slot_types();
    let arguments = slots
        .iter()
        .enumerate()
        .map(|(slot, value_type)| {
            Expr::variable(slot, value_type.clone(), origin.clone())
        })
        .collect::<Vec<_>>();
    let result = signature.result();
    let body = match chosen.target {
        // The closure body's call is in the tail position of its activation.
        CandidateTarget::Function(function) => crate::direct_call(
            environment,
            function,
            arguments,
            result,
            origin.clone(),
            true,
        ),
        CandidateTarget::Primitive(intrinsic) => {
            Expr::external_with_result(intrinsic, arguments, result, origin.clone())
        }
        CandidateTarget::Dispatch => Expr::Call {
            target: CallTarget::Contract {
                interface: declared.id.clone(),
                member: contract.name.clone(),
                receiver: contract.receiver.unwrap_or(0),
                arguments: chosen.arguments.clone(),
                destination: contract.receiver.is_none().then(|| receiver.clone()),
                signature: Box::new(signature.clone()),
                closed: closed_contract(environment.types, interface, &contract.name),
            },
            arguments,
            result,
            tail: true,
            origin: origin.clone(),
        },
    };
    let parameters = signature.parameters().to_vec();
    Some(Expr::closure(
        signature,
        parameters,
        Vec::new(),
        body,
        slots.len(),
        origin,
    ))
}

/// One candidate: `contract`'s signature with `self` as `receiver` and the
/// interface's parameters as `arguments`.
fn candidate(
    declared: &DeclaredInterface,
    contract: &ContractMember,
    receiver: &Type,
    arguments: &[Type],
    target: CandidateTarget,
) -> Candidate {
    let mut substitution = BTreeMap::from([(SELF.to_owned(), receiver.clone())]);
    substitution.extend(
        declared
            .parameters
            .iter()
            .cloned()
            .zip(arguments.iter().cloned()),
    );
    Candidate {
        arguments: arguments.to_vec(),
        signature: contract.signature.substitute(&substitution),
        target,
        spelling: if arguments.is_empty() {
            declared.name.clone()
        } else {
            format!(
                "({}{})",
                declared.name,
                arguments
                    .iter()
                    .map(|argument| format!(" {argument}"))
                    .collect::<String>()
            )
        },
    }
}

/// The receiver of a destination-dispatched member: `self` from unifying the
/// member's written result type with the written expected type.
fn destination(
    environment: &mut CheckEnvironment<'_>,
    interface: usize,
    declared: &DeclaredInterface,
    contract: &ContractMember,
    expected: Option<&Type>,
    span: ByteSpan,
) -> Option<Type> {
    let Some(expected) = expected else {
        let mut diagnostic = Diagnostic::new(
            DiagnosticCode::TypeAmbiguousDestination,
            span,
            format!(
                "`{}.{}` selects its implementation from a written expected type, and none reaches this call",
                declared.name, contract.name
            ),
        )
        .with_source_id(environment.source_id);
        let mut receivers = environment
            .types
            .implementations()
            .iter()
            .filter(|implementation| implementation.interface == interface)
            .map(|implementation| implementation.receiver.to_string())
            .collect::<BTreeSet<_>>();
        if conversion_contract(environment.types, interface).is_some() {
            receivers.insert("each builtin integer type".to_owned());
        }
        for receiver in receivers {
            diagnostic = diagnostic
                .with_note(format!("{receiver} implements `{}`", declared.name));
        }
        environment
            .diagnostics
            .push(diagnostic.with_note("write the destination with `as`"));
        return None;
    };
    let mut names = vec![SELF.to_owned()];
    names.extend(declared.parameters.iter().cloned());
    let mut instantiation = crate::infer::Instantiation::new(&names);
    let written = contract.signature.result();
    let opened = instantiation.open(&written);
    let receiver = instantiation
        .unify(&opened, expected)
        .then(|| instantiation.resolved(&instantiation.open(&self_type())))
        .flatten();
    if receiver.is_none() {
        mismatch(
            environment.diagnostics,
            environment.source_id,
            span,
            expected.clone(),
            written,
            "the written expected type does not fix the destination of this member",
        );
    }
    receiver
}

/// Every implementation of `interface` whose receiver covers `receiver`, with
/// the member's signature at that implementation.
fn candidates(
    types: &TypeNames,
    interface: usize,
    declared: &DeclaredInterface,
    contract: &ContractMember,
    receiver: &Type,
) -> Vec<Candidate> {
    let spell = |arguments: &[Type]| {
        if arguments.is_empty() {
            declared.name.clone()
        } else {
            format!(
                "({}{})",
                declared.name,
                arguments
                    .iter()
                    .map(|argument| format!(" {argument}"))
                    .collect::<String>()
            )
        }
    };
    let signature = |arguments: &[Type]| {
        let mut substitution = BTreeMap::from([(SELF.to_owned(), receiver.clone())]);
        substitution.extend(
            declared
                .parameters
                .iter()
                .cloned()
                .zip(arguments.iter().cloned()),
        );
        contract.signature.substitute(&substitution)
    };
    let mut found = Vec::new();
    for implementation in types.implementations() {
        if implementation.interface != interface
            || !covers(&implementation.receiver, receiver)
        {
            continue;
        }
        let (Some(arguments), Some(function)) = (
            instantiate_arguments(implementation, receiver),
            implementation.members.get(&contract.name).copied(),
        ) else {
            continue;
        };
        found.push(Candidate {
            signature: signature(&arguments),
            target: CandidateTarget::Function(function),
            spelling: spell(&arguments),
            arguments,
        });
    }
    // The builtin constructor types iterate through the closed registry.
    if is_iter(types, interface)
        && contract.name == "next"
        && let Some(item) = types.closed_iter_item(receiver)
    {
        found.push(candidate(
            declared,
            contract,
            receiver,
            &[item],
            CandidateTarget::Dispatch,
        ));
    }
    // The builtin integer types convert through the closed registry: a
    // conversion that cannot fail is a `from`, every other one a `try-from`.
    if let Some(total) = conversion_contract(types, interface)
        && let Some(target) = NumericType::ALL
            .into_iter()
            .find(|numeric| numeric.is_integer() && numeric.to_type() == *receiver)
    {
        for source in NumericType::ALL {
            if source == target
                || !source.is_integer()
                || CompilerIntrinsic::conversion_is_total(source, target) != total
            {
                continue;
            }
            let arguments = [source.to_type()];
            found.push(Candidate {
                signature: signature(&arguments),
                target: CandidateTarget::Primitive(CompilerIntrinsic::Convert(
                    source, target,
                )),
                spelling: spell(&arguments),
                arguments: arguments.to_vec(),
            });
        }
    }
    found
}

/// Every argument list at which `value` conforms to the generic interface
/// `interface`: through a written implementation or, for `iter`, the closed
/// registry of the builtin constructor types.
pub(crate) fn conformances(
    types: &TypeNames,
    bounds: &BTreeMap<String, usize>,
    interface: usize,
    value: &Type,
) -> Vec<Vec<Type>> {
    // The `self` of a default member conforms at the interface's own
    // parameters.
    if let Type::Param(name) = value {
        return match types.interface(interface) {
            Some(declared) if bounds.get(name) == Some(&interface) => {
                vec![
                    declared
                        .parameters
                        .iter()
                        .map(|parameter| Type::Param(parameter.clone()))
                        .collect(),
                ]
            }
            _ => Vec::new(),
        };
    }
    let mut found = types
        .implementations()
        .iter()
        .filter(|implementation| {
            implementation.interface == interface
                && covers(&implementation.receiver, value)
        })
        .filter_map(|implementation| instantiate_arguments(implementation, value))
        .collect::<Vec<_>>();
    if is_iter(types, interface)
        && let Some(item) = types.closed_iter_item(value)
    {
        found.push(vec![item]);
    }
    found
}

/// The interface arguments of `implementation` at `receiver`: its own when
/// its receiver is closed, and those its receiver's generic names take when
/// it covers `receiver` generically.
fn instantiate_arguments(
    implementation: &Implementation,
    receiver: &Type,
) -> Option<Vec<Type>> {
    let mut names = BTreeSet::new();
    parameter_names(&implementation.receiver, &mut names);
    if names.is_empty() {
        return Some(implementation.arguments.clone());
    }
    let names = names.into_iter().collect::<Vec<_>>();
    let mut instantiation = crate::infer::Instantiation::new(&names);
    let opened = instantiation.open(&implementation.receiver);
    if !instantiation.unify(&opened, receiver) {
        return None;
    }
    implementation
        .arguments
        .iter()
        .map(|argument| instantiation.resolved(&instantiation.open(argument)))
        .collect()
}

/// Whether the operands of `application` check against `candidate`. The
/// attempt leaves no diagnostic and no binding behind.
fn fits(
    environment: &mut CheckEnvironment<'_>,
    application: &Application,
    candidate: &Candidate,
    receiver: Option<usize>,
    expected: Option<&Type>,
) -> bool {
    if expected.is_some_and(|expected| {
        !crate::types_match(expected, &candidate.signature.result())
    }) {
        return false;
    }
    let diagnostics = environment.diagnostics.len();
    let bindings = environment.bindings.len();
    let element = match candidate.signature.variadic() {
        Some(Type::Array(element)) => Some(element.as_ref().clone()),
        _ => None,
    };
    let fits = {
        let mut scope = environment.scoped();
        application
            .arguments()
            .iter()
            .enumerate()
            .all(|(index, operand)| {
                if receiver == Some(index) {
                    return true;
                }
                candidate
                    .signature
                    .parameters()
                    .get(index)
                    .cloned()
                    .or_else(|| element.clone())
                    .is_some_and(|parameter| {
                        check_operand(&mut scope, operand.value(), Some(parameter))
                            .is_some()
                    })
            })
    };
    environment.diagnostics.truncate(diagnostics);
    environment.bindings.truncate(bindings);
    fits
}

/// Whether `interface` is a conversion contract of `@std.core`: `Some(true)`
/// for `from`, whose conversions cannot fail, and `Some(false)` for
/// `try-from`.
fn conversion_contract(types: &TypeNames, interface: usize) -> Option<bool> {
    let id = &types.interface(interface)?.id;
    [("from", true), ("try-from", false)]
        .into_iter()
        .find(|(name, _)| *id == crate::stdlib::stdlib_type_id(&["core"], name))
        .map(|(_, total)| total)
}

/// Checks a call of a default contract member. A default is never
/// redeclared, so the call is a direct call of the one default function,
/// with `self` as the receiver's type, the interface's parameters as the
/// arguments at which the receiver conforms, and the member's own generic
/// parameters inferred from the operands and the written expected type.
#[allow(clippy::too_many_arguments)]
fn check_default_call(
    environment: &mut CheckEnvironment<'_>,
    application: &Application,
    interface: usize,
    declared: &DeclaredInterface,
    contract: &ContractMember,
    position: usize,
    expected: Option<Type>,
    tail_position: bool,
) -> Option<Expr> {
    let span = application.span();
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
    let function = environment
        .types
        .default_member(interface, &contract.name)?;
    let receiver_operand = operands.get(position)?.value();
    let receiver_value = check_expression(environment, receiver_operand, None)?;
    let receiver = receiver_value.result_type();
    let own = || {
        declared
            .parameters
            .iter()
            .map(|parameter| Type::Param(parameter.clone()))
            .collect::<Vec<_>>()
    };
    // An interface value erases its concrete type, so no other operand can be
    // required to share it, exactly as for an abstract member.
    if matches!(&receiver, Type::Interface(id, _) if *id == declared.id) {
        let shared =
            contract.signature.parameters().iter().enumerate().find(
                |(index, parameter)| *index != position && mentions_self(parameter),
            );
        if let Some((index, _)) = shared {
            mismatch(
                environment.diagnostics,
                environment.source_id,
                operands.get(index)?.value().span(),
                self_type(),
                receiver.clone(),
                "a member with another `self` operand cannot be called through an interface value, which erases the type they must share",
            );
            return None;
        }
        if reject_escaping_self(environment, span, contract, &receiver) {
            return None;
        }
    }
    let arguments = match &receiver {
        Type::Interface(id, arguments) if *id == declared.id => Some(arguments.clone()),
        Type::Interface(_, _) | Type::Any => None,
        _ if declared.parameters.is_empty() => {
            let scope =
                Scope::new(environment.self_type.as_ref(), &environment.generics)
                    .with_bounds(&environment.bounds);
            environment
                .types
                .satisfies(scope, interface, &receiver)
                .then(Vec::new)
        }
        Type::Param(name) => {
            (environment.bounds.get(name) == Some(&interface)).then(own)
        }
        _ => match conformances(
            environment.types,
            &environment.bounds,
            interface,
            &receiver,
        )
        .as_slice()
        {
            [arguments] => Some(arguments.clone()),
            [] => None,
            _ => {
                environment.diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCode::TypeAmbiguousImplementation,
                        span,
                        format!(
                            "{receiver} implements `{}` at more than one argument list; write the interface value with `as`",
                            declared.name
                        ),
                    )
                    .with_source_id(environment.source_id),
                );
                return None;
            }
        },
    };
    let Some(arguments) = arguments else {
        unsatisfied(
            environment,
            receiver_operand.span(),
            &receiver,
            &declared.name,
        );
        return None;
    };
    let mut substitution = BTreeMap::from([(SELF.to_owned(), receiver.clone())]);
    substitution.extend(
        declared
            .parameters
            .iter()
            .cloned()
            .zip(arguments.iter().cloned()),
    );
    let signature = contract.signature.substitute(&substitution);
    let mut instantiation = crate::infer::Instantiation::new(&contract.generics);
    let opened = instantiation.open_signature(&signature);
    if let Some(expected) = &expected {
        // A written expected type may fix a generic the operands leave open.
        let _ = instantiation.unify(&opened.result(), expected);
    }
    let mut checked = Vec::with_capacity(operands.len());
    for (index, (operand, pattern)) in
        operands.iter().zip(opened.parameters()).enumerate()
    {
        if index == position {
            checked.push(receiver_value.clone());
        } else {
            checked.push(crate::check_inferred_operand(
                environment,
                &mut instantiation,
                operand.value(),
                pattern,
                false,
            )?);
        }
    }
    let unbound = instantiation.unbound();
    let Some(Type::Function(instantiated)) = instantiation
        .resolved(&Type::Function(Box::new(opened)))
        .filter(|_| unbound.is_empty())
    else {
        crate::ambiguous_generic(environment, span, &unbound);
        return None;
    };
    // The member's own bounded parameters, at the arguments this call fixed.
    if !contract.bounds.is_empty() {
        let fixed = contract
            .generics
            .iter()
            .filter_map(|name| {
                let variable = instantiation.open(&Type::Param(name.clone()));
                Some((name.clone(), instantiation.resolved(&variable)?))
            })
            .collect::<BTreeMap<_, _>>();
        if !check_bounds(environment, span, &contract.bounds, &fixed) {
            return None;
        }
    }
    let result = instantiated.result();
    crate::ensure_expected(environment, span, expected.clone(), result.clone());
    if expected
        .as_ref()
        .is_some_and(|expected| !crate::types_match(expected, &result))
    {
        return None;
    }
    // The default is one function for every receiver. The call names the
    // interface's arguments, which the default's own signature may not
    // mention, so its body dispatches at them.
    let _ = function;
    Some(Expr::Call {
        target: CallTarget::Contract {
            interface: declared.id.clone(),
            member: contract.name.clone(),
            receiver: position,
            arguments,
            destination: None,
            signature: instantiated,
            closed: None,
        },
        arguments: checked,
        result,
        tail: tail_position,
        origin: SourceOrigin::new(environment.source_id, span),
    })
}

/// An abstract contract member named as a function value
/// (`docs/spec/02-type-system.md`, "Interfaces and methods").
///
/// The written expected `fn` type fixes the receiver. The value is a closure
/// that performs the contract call, so it selects the implementation exactly
/// as a call written at that receiver would. `None` means the name is not
/// such a member and the caller resolves it as an ordinary function.
pub(crate) fn check_contract_value(
    environment: &mut CheckEnvironment<'_>,
    span: ByteSpan,
    interface: usize,
    member: usize,
    expected: Option<&Type>,
) -> Option<Option<Expr>> {
    let declared = environment.types.interface(interface)?.clone();
    let contract = declared.members.get(member)?.clone();
    // A default member with a receiver is one function for every receiver.
    if contract.default && contract.receiver.is_some() {
        return None;
    }
    // The forms a contract call cannot take yet are not values either.
    if !contract.generics.is_empty() || !contract.signature.labelled().is_empty() {
        crate::unavailable(
            environment.diagnostics,
            environment.source_id,
            span,
            "a contract member with its own generic parameters or with labelled parameters is not a function value in the M3 profile",
        );
        return Some(None);
    }
    let position = contract.receiver.filter(|_| {
        declared.parameters.is_empty() && contract.signature.variadic().is_none()
    });
    let Some(position) = position else {
        return Some(check_selected_value(
            environment,
            span,
            interface,
            &declared,
            &contract,
            expected,
        ));
    };
    let Some(Type::Function(written)) = expected else {
        environment.diagnostics.push(
            Diagnostic::new(
                DiagnosticCode::TypeAmbiguousInference,
                span,
                format!(
                    "`{}.{}` as a value needs a written expected `fn` type to fix its receiver",
                    declared.name, contract.name
                ),
            )
            .with_source_id(environment.source_id),
        );
        return Some(None);
    };
    let Some(receiver) = written.parameters().get(position).cloned() else {
        crate::ensure_expected(
            environment,
            span,
            expected.cloned(),
            Type::Function(Box::new(contract.signature.clone())),
        );
        return Some(None);
    };
    let signature = contract
        .signature
        .substitute(&BTreeMap::from([(SELF.to_owned(), receiver.clone())]));
    let actual = Type::Function(Box::new(signature.clone()));
    crate::ensure_expected(environment, span, expected.cloned(), actual.clone());
    if !expected.is_some_and(|expected| crate::types_match(expected, &actual)) {
        return Some(None);
    }
    let holds = match &receiver {
        Type::Interface(id, _) if *id == declared.id => {
            let shared = contract.signature.parameters().iter().enumerate().any(
                |(index, parameter)| index != position && mentions_self(parameter),
            );
            if shared {
                mismatch(
                    environment.diagnostics,
                    environment.source_id,
                    span,
                    self_type(),
                    receiver.clone(),
                    "a member with another `self` operand cannot be called through an interface value, which erases the type they must share",
                );
                return Some(None);
            }
            if reject_escaping_self(environment, span, &contract, &receiver) {
                return Some(None);
            }
            true
        }
        Type::Interface(_, _) | Type::Any => false,
        _ => {
            let scope =
                Scope::new(environment.self_type.as_ref(), &environment.generics)
                    .with_bounds(&environment.bounds);
            environment.types.satisfies(scope, interface, &receiver)
        }
    };
    if !holds {
        unsatisfied(environment, span, &receiver, &declared.name);
        return Some(None);
    }
    let origin = SourceOrigin::new(environment.source_id, span);
    let parameters = signature.parameters().to_vec();
    let arguments = parameters
        .iter()
        .enumerate()
        .map(|(slot, value_type)| {
            Expr::variable(slot, value_type.clone(), origin.clone())
        })
        .collect::<Vec<_>>();
    let body = Expr::Call {
        target: CallTarget::Contract {
            interface: declared.id.clone(),
            member: contract.name.clone(),
            receiver: position,
            arguments: Vec::new(),
            destination: None,
            signature: Box::new(signature.clone()),
            closed: closed_contract(environment.types, interface, &contract.name),
        },
        arguments,
        result: signature.result(),
        tail: true,
        origin: origin.clone(),
    };
    let slot_count = parameters.len();
    Some(Some(Expr::closure(
        signature,
        parameters,
        Vec::new(),
        body,
        slot_count,
        origin,
    )))
}
