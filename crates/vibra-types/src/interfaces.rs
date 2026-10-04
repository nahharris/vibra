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
    Application, ApplicationBinding, Attribute, BindingFacts, Declaration,
    DefintDeclaration, DeftypeBody, FunctionDeclaration, TypeExpr, TypeMember,
};

use crate::nominal::{
    ContractMember, DeclaredInterface, Implementation, Scope, TypeNames,
};
use crate::{
    CheckEnvironment, GenericCall, GenericOperands, call_contract_error,
    check_expression, check_generic_operands, mismatch,
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
        let defaults = labelled_defaults(
            types,
            source_id,
            method.attributes().items(),
            &signature,
        );
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
            if receiver == Type::Never {
                wrong_kind(
                    diagnostics,
                    "`never` has no values, so no implementation can be written for it",
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
    types: &TypeNames,
    source_id: &str,
    attributes: &[Attribute],
    signature: &FunctionSignature,
) -> BTreeMap<String, (String, Option<vibra_ir::Constant>)> {
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
                        types,
                        parameter,
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
        let defaults = labelled_defaults(types, &plan.source_id, attributes, written);
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

/// The `types:` list a contract call wrote, split at the member it addresses:
/// the contract's own parameters come first and the member's own after them
/// (`docs/spec/02-type-system.md`, "Generics").
struct WrittenTypes {
    /// The interface's type arguments.
    interface: Vec<Type>,
    /// The member's own type arguments, when the member is generic.
    member: Option<Vec<Type>>,
}

/// Lowers and measures the `types:` list of a contract call. The outer `None`
/// means a type failed to lower or the length is wrong, reported once; the
/// inner `None` means nothing was written.
fn written_types(
    environment: &mut CheckEnvironment<'_>,
    application: &Application,
    declared: &DeclaredInterface,
    contract: &ContractMember,
) -> Option<Option<WrittenTypes>> {
    let Some(mut list) = crate::lower_type_arguments(environment, application)? else {
        return Some(None);
    };
    let interface = declared.parameters.len();
    let total = interface + contract.generics.len();
    if total == 0 {
        crate::type_argument_mismatch(
            environment,
            application,
            "the applied contract member declares no generic parameters".to_owned(),
        );
        return None;
    }
    if list.len() != total {
        crate::type_argument_mismatch(
            environment,
            application,
            format!(
                "`types:` supplies {} type argument{}, but `{}.{}` takes {total}: the interface's, then the member's own",
                list.len(),
                if list.len() == 1 { "" } else { "s" },
                declared.name,
                contract.name
            ),
        );
        return None;
    }
    let member = list.split_off(interface);
    Some(Some(WrittenTypes {
        interface: list,
        member: (!member.is_empty()).then_some(member),
    }))
}

/// Whether the interface arguments `arguments` are the ones `written` names.
/// An argument that is a generic parameter of the receiver's own declaration
/// is fixed there and agrees with anything.
fn matches_written(written: &WrittenTypes, arguments: &[Type]) -> bool {
    written.interface.is_empty()
        || (written.interface.len() == arguments.len()
            && written
                .interface
                .iter()
                .zip(arguments)
                .all(|(written, actual)| {
                    matches!(actual, Type::Param(_))
                        || crate::types_match(written, actual)
                }))
}

/// Reports a written interface type argument list that is not the one the
/// receiver conforms at.
fn agrees(
    environment: &mut CheckEnvironment<'_>,
    application: &Application,
    written: Option<&WrittenTypes>,
    arguments: &[Type],
) -> bool {
    let Some(written) = written.filter(|written| !written.interface.is_empty()) else {
        return true;
    };
    if matches_written(written, arguments) {
        return true;
    }
    crate::type_argument_mismatch(
        environment,
        application,
        format!(
            "`types:` names the interface arguments {}, but the receiver implements it at {}",
            written
                .interface
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" "),
            arguments
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" "),
        ),
    );
    false
}

/// What the operands of one contract call check to.
struct MemberOperands {
    /// The operands in resolved parameter order: the fixed ones, the labelled
    /// ones in declaration order with a default for each one not written, and
    /// the packed variadic tail.
    arguments: Vec<Expr>,
    /// The member's signature at this call: `self`, the interface's
    /// arguments, and the member's own type arguments substituted.
    signature: FunctionSignature,
    /// The member's own type arguments in `where:` order.
    member_types: Vec<Type>,
}

/// Checks the written operands of a call of `contract` with `self` and the
/// interface's parameters replaced by `substitution`: binds labelled operands
/// and a variadic tail by the member's signature, infers the member's own
/// type arguments from the operands, `written`, and `expected`, and checks
/// them against the member's bounds.
///
/// `checked` is a positional operand the caller already checked because it
/// selects the implementation. The member's own generic names are renamed
/// first, so a receiver that spells one of them lexically is never captured.
fn check_member_operands(
    environment: &mut CheckEnvironment<'_>,
    application: &Application,
    contract: &ContractMember,
    substitution: &BTreeMap<String, Type>,
    checked: Option<(usize, &Expr)>,
    written: Option<&[Type]>,
    expected: Option<&Type>,
) -> Option<MemberOperands> {
    let (renamed, _) = member_renaming(contract);
    let signature = member_signature(contract, substitution);
    let facts = BindingFacts::new(
        signature.parameters().len(),
        signature
            .labelled()
            .iter()
            .map(|parameter| parameter.name().to_owned())
            .collect(),
        signature.variadic().map(crate::tail_binding),
    );
    let ordered = match application.ordered_arguments(&facts) {
        Ok(ordered) => ordered,
        Err(error) => {
            call_contract_error(environment, application.span(), error.to_string());
            return None;
        }
    };
    let mut labelled = BTreeMap::new();
    let mut tail = Vec::new();
    for argument in ordered.iter().skip(signature.parameters().len()) {
        if let Some(label) = argument.label() {
            labelled.insert(label.value().to_owned(), *argument);
        } else {
            tail.push(*argument);
        }
    }
    let GenericOperands {
        signature: instantiated,
        arguments: member_types,
        positional,
        labelled: mut written_labelled,
        tail,
    } = check_generic_operands(
        environment,
        application,
        GenericCall {
            parameters: &renamed,
            signature: &signature,
            type_arguments: written,
            expected,
            checked,
        },
        &ordered,
        &mut labelled,
        &tail,
    )?;
    if !crate::check_inferred_dict_keys(
        environment,
        application.span(),
        &Type::Function(Box::new(instantiated.clone())),
    ) {
        return None;
    }
    // The member's own bounded parameters, at the arguments this call fixed.
    if !contract.bounds.is_empty() {
        let fixed = contract
            .generics
            .iter()
            .cloned()
            .zip(member_types.iter().cloned())
            .collect::<BTreeMap<_, _>>();
        if !check_bounds(environment, application.span(), &contract.bounds, &fixed) {
            return None;
        }
    }
    let origin = SourceOrigin::new(environment.source_id, application.span());
    let mut arguments = positional;
    for parameter in instantiated.labelled() {
        if let Some(argument) = written_labelled.remove(parameter.name()) {
            arguments.push(argument);
        } else if let Some(default) = parameter.default() {
            // An implementation keeps its contract's defaults, so the
            // contract's value is the operand whichever one is selected.
            arguments.push(default.to_expr(&origin));
        } else {
            call_contract_error(
                environment,
                application.span(),
                format!("labelled argument `{}` has no default", parameter.name()),
            );
            return None;
        }
    }
    if let Some(tail_type) = instantiated.variadic() {
        let key_order = match tail_type {
            Type::Dict(key, _) => key_order(environment.types, key),
            _ => None,
        };
        arguments.push(crate::pack_tail(tail_type, tail, key_order, origin));
    }
    if application.type_arguments_after_operands()
        || ordered
            .iter()
            .zip(application.arguments())
            .any(|(left, right)| !std::ptr::eq(*left, right))
    {
        environment.diagnostics.push(
            Diagnostic::new(
                DiagnosticCode::StyleArgumentOrder,
                application.span(),
                "application operands are not in canonical declaration order",
            )
            .with_source_id(environment.source_id),
        );
    }
    environment
        .bindings
        .push(ApplicationBinding::new(application.span(), facts));
    Some(MemberOperands {
        arguments,
        signature: instantiated,
        member_types,
    })
}

/// The `index`th unlabelled operand: a fixed parameter binds the unlabelled
/// operands in order, whatever labelled operands are written between them.
fn positional_operand(
    application: &Application,
    index: usize,
) -> Option<&vibra_syntax::CallArgument> {
    application
        .arguments()
        .iter()
        .filter(|operand| operand.label().is_none())
        .nth(index)
}

/// Reports why the operands of `application` bind no parameter of `contract`:
/// a missing fixed operand, an unknown or repeated label, an unexpected
/// operand, or an odd dict tail.
fn report_arity(
    environment: &mut CheckEnvironment<'_>,
    application: &Application,
    contract: &ContractMember,
) {
    let facts = BindingFacts::new(
        contract.signature.parameters().len(),
        contract
            .signature
            .labelled()
            .iter()
            .map(|parameter| parameter.name().to_owned())
            .collect(),
        contract.signature.variadic().map(crate::tail_binding),
    );
    if let Err(error) = application.ordered_arguments(&facts) {
        call_contract_error(environment, application.span(), error.to_string());
    }
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
    let written = written_types(environment, application, &declared, &contract)?;
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
            written.as_ref(),
            expected,
            tail_position,
        );
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
            written.as_ref(),
            expected,
            tail_position,
        );
    };
    let Some(receiver_operand) = positional_operand(application, position) else {
        report_arity(environment, application, &contract);
        return None;
    };
    let receiver_value = check_expression(environment, receiver_operand.value(), None)?;
    let receiver = receiver_value.result_type();
    let dispatch = match &receiver {
        Type::Param(name) => {
            if environment.bounds.get(name) != Some(&interface) {
                unsatisfied(
                    environment,
                    receiver_operand.value().span(),
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
                    positional_operand(application, index)?.value().span(),
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
                        receiver_operand.value().span(),
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
    // An interface that takes no parameters has no `types:` entry of its own.
    let substitution = BTreeMap::from([(SELF.to_owned(), receiver.clone())]);
    let MemberOperands {
        arguments,
        signature,
        member_types,
    } = check_member_operands(
        environment,
        application,
        &contract,
        &substitution,
        Some((position, &receiver_value)),
        written
            .as_ref()
            .and_then(|written| written.member.as_deref()),
        expected.as_ref(),
    )?;
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
        // A generic member's own type arguments reach the implementation
        // through the contract call, which the run time binds, so it stays one
        // even when the receiver type is known.
        Some(function) if contract.generics.is_empty() => crate::direct_call(
            environment,
            function,
            arguments,
            result,
            origin,
            tail_position,
        ),
        _ => Expr::Call {
            target: CallTarget::Contract {
                interface: declared.id.clone(),
                member: contract.name.clone(),
                receiver: position,
                arguments: Vec::new(),
                member_types,
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
                        member_generics: member
                            .generics
                            .iter()
                            .cloned()
                            .map(Some)
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
            let member_generics = interface
                .members
                .iter()
                .find(|member| member.name == *member_name)
                .map(|member| {
                    own_generics(
                        &member.generics,
                        &crate::nominal::generic_names(method.attributes().items()),
                    )
                })
                .unwrap_or_default();
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
                    member_generics,
                }),
            });
        }
        register(types, plan, &written, &defaults);
    }
    types.set_default_members(defaults);
    types.set_bounds_ready();
    check_header_bounds(types, modules, functions, diagnostics);
}

/// The generic parameter an implementation member declares for each of its
/// contract member's own generic parameters, in the contract's order. A name
/// the implementation also declares is the same parameter, as its signature
/// must match the contract's; one the signature never mentions may be named
/// differently, and then takes the next unmatched name in `where:` order.
fn own_generics(contract: &[String], declared: &[String]) -> Vec<Option<String>> {
    let mut unmatched = declared.iter().filter(|name| !contract.contains(name));
    contract
        .iter()
        .map(|name| {
            if declared.contains(name) {
                Some(name.clone())
            } else {
                unmatched.next().cloned()
            }
        })
        .collect()
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
#[derive(Clone)]
struct Candidate {
    /// The member's signature at this implementation, with its own generic
    /// parameters still open.
    signature: FunctionSignature,
    /// `self` and the interface's parameters at this implementation.
    substitution: BTreeMap<String, Type>,
    target: CandidateTarget,
    /// The applied interface, for diagnostics.
    spelling: String,
    /// The interface's type arguments at this implementation.
    arguments: Vec<Type>,
}

#[derive(Clone)]
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
#[allow(clippy::too_many_arguments)]
fn check_selected_call(
    environment: &mut CheckEnvironment<'_>,
    application: &Application,
    interface: usize,
    declared: &DeclaredInterface,
    contract: &ContractMember,
    written: Option<&WrittenTypes>,
    expected: Option<Type>,
    tail_position: bool,
) -> Option<Expr> {
    let span = application.span();
    let operands = application.arguments();
    let mut receiver_value = None;
    let mut dispatched = None;
    let receiver = match contract.receiver {
        Some(position) => {
            let Some(receiver_operand) = positional_operand(application, position)
            else {
                report_arity(environment, application, contract);
                return None;
            };
            let value = check_expression(environment, receiver_operand.value(), None)?;
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
                            positional_operand(application, index)?.value().span(),
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
                        receiver_operand.value().span(),
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
    // A written `types:` list names the interface arguments the call selects.
    let candidates = match written.filter(|written| !written.interface.is_empty()) {
        Some(written) => {
            let matching = candidates
                .iter()
                .filter(|candidate| matches_written(written, &candidate.arguments))
                .cloned()
                .collect::<Vec<_>>();
            if matching.is_empty() {
                let first = candidates.first()?.arguments.clone();
                agrees(environment, application, Some(written), &first);
                return None;
            }
            matching
        }
        None => candidates,
    };
    let member_written = written.and_then(|written| written.member.as_deref());
    let checked = receiver_value
        .as_ref()
        .map(|(position, value)| (*position, value));
    let fitting = candidates
        .iter()
        .filter(|candidate| {
            fits(
                environment,
                application,
                contract,
                candidate,
                checked,
                member_written,
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
    let MemberOperands {
        arguments,
        signature,
        member_types,
    } = check_member_operands(
        environment,
        application,
        contract,
        &chosen.substitution,
        checked,
        member_written,
        expected.as_ref(),
    )?;
    let result = signature.result();
    crate::ensure_expected(environment, span, expected.clone(), result.clone());
    if expected
        .as_ref()
        .is_some_and(|expected| !crate::types_match(expected, &result))
    {
        return None;
    }
    Some(match chosen.target {
        // A generic member's own type arguments reach the implementation
        // through the contract call, which the run time binds.
        CandidateTarget::Function(function) if contract.generics.is_empty() => {
            crate::direct_call(
                environment,
                function,
                arguments,
                result,
                origin,
                tail_position,
            )
        }
        CandidateTarget::Primitive(intrinsic) => {
            Expr::external_with_result(intrinsic, arguments, result, origin)
        }
        CandidateTarget::Function(_) | CandidateTarget::Dispatch => Expr::Call {
            target: CallTarget::Contract {
                interface: declared.id.clone(),
                member: contract.name.clone(),
                receiver: contract.receiver.unwrap_or(0),
                arguments: chosen.arguments.clone(),
                member_types,
                destination: contract.receiver.is_none().then(|| receiver.clone()),
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
    let (renamed, _) = member_renaming(contract);
    let fitting = candidates
        .iter()
        .filter(|candidate| fits_value(&renamed, &candidate.signature, written))
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
    let (signature, member_types) = instantiate_value(
        environment,
        span,
        contract,
        &renamed,
        &chosen.signature,
        written,
    )?;
    let origin = SourceOrigin::new(environment.source_id, span);
    // The closure's slots are the fixed parameters, the labelled ones, and
    // then the tail, which arrives already packed.
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
        CandidateTarget::Function(function) if contract.generics.is_empty() => {
            crate::direct_call(
                environment,
                function,
                arguments,
                result,
                origin.clone(),
                true,
            )
        }
        CandidateTarget::Primitive(intrinsic) => {
            Expr::external_with_result(intrinsic, arguments, result, origin.clone())
        }
        CandidateTarget::Function(_) | CandidateTarget::Dispatch => Expr::Call {
            target: CallTarget::Contract {
                interface: declared.id.clone(),
                member: contract.name.clone(),
                receiver: contract.receiver.unwrap_or(0),
                arguments: chosen.arguments.clone(),
                member_types,
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

/// `self` as `receiver` and the interface's parameters as `arguments`.
fn member_substitution(
    declared: &DeclaredInterface,
    receiver: &Type,
    arguments: &[Type],
) -> BTreeMap<String, Type> {
    let mut substitution = BTreeMap::from([(SELF.to_owned(), receiver.clone())]);
    substitution.extend(
        declared
            .parameters
            .iter()
            .cloned()
            .zip(arguments.iter().cloned()),
    );
    substitution
}

/// The names a contract member's own generic parameters take at one call or
/// value, and the renaming to them. Source names cannot contain `#`, so a
/// receiver or interface argument that spells one of the member's generic
/// names lexically is never captured when the two are substituted together.
fn member_renaming(contract: &ContractMember) -> (Vec<String>, BTreeMap<String, Type>) {
    let renamed = contract
        .generics
        .iter()
        .map(|name| format!("{name}#member"))
        .collect::<Vec<_>>();
    let renaming = contract
        .generics
        .iter()
        .cloned()
        .zip(renamed.iter().map(|name| Type::Param(name.clone())))
        .collect();
    (renamed, renaming)
}

/// `contract`'s signature with `substitution` applied and its own generic
/// parameters left open under their [`member_renaming`] names.
fn member_signature(
    contract: &ContractMember,
    substitution: &BTreeMap<String, Type>,
) -> FunctionSignature {
    contract
        .signature
        .substitute(&member_renaming(contract).1)
        .substitute(substitution)
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
    let substitution = member_substitution(declared, receiver, arguments);
    Candidate {
        arguments: arguments.to_vec(),
        signature: member_signature(contract, &substitution),
        substitution,
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
    // The member's own parameters may appear in its result; the written
    // type leaves them open, as they are fixed from the operands.
    names.extend(contract.generics.iter().cloned());
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
        found.push(candidate(
            declared,
            contract,
            receiver,
            &arguments,
            CandidateTarget::Function(function),
        ));
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
            found.push(candidate(
                declared,
                contract,
                receiver,
                &[source.to_type()],
                CandidateTarget::Primitive(CompilerIntrinsic::Convert(source, target)),
            ));
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
    contract: &ContractMember,
    candidate: &Candidate,
    checked: Option<(usize, &Expr)>,
    written: Option<&[Type]>,
    expected: Option<&Type>,
) -> bool {
    // A member with no generic parameter has its result fixed already.
    if contract.generics.is_empty()
        && expected.is_some_and(|expected| {
            !crate::types_match(expected, &candidate.signature.result())
        })
    {
        return false;
    }
    let diagnostics = environment.diagnostics.len();
    let bindings = environment.bindings.len();
    let operands = {
        let mut scope = environment.scoped();
        check_member_operands(
            &mut scope,
            application,
            contract,
            &candidate.substitution,
            checked,
            written,
            expected,
        )
    };
    environment.diagnostics.truncate(diagnostics);
    environment.bindings.truncate(bindings);
    operands.is_some_and(|operands| {
        expected.is_none_or(|expected| {
            crate::types_match(expected, &operands.signature.result())
        })
    })
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
    written: Option<&WrittenTypes>,
    expected: Option<Type>,
    tail_position: bool,
) -> Option<Expr> {
    let span = application.span();
    let function = environment
        .types
        .default_member(interface, &contract.name)?;
    let Some(receiver_argument) = positional_operand(application, position) else {
        report_arity(environment, application, contract);
        return None;
    };
    let receiver_operand = receiver_argument.value();
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
                positional_operand(application, index)?.value().span(),
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
    if !agrees(environment, application, written, &arguments) {
        return None;
    }
    let substitution = member_substitution(declared, &receiver, &arguments);
    let MemberOperands {
        arguments: checked,
        signature: instantiated,
        member_types,
    } = check_member_operands(
        environment,
        application,
        contract,
        &substitution,
        Some((position, &receiver_value)),
        written.and_then(|written| written.member.as_deref()),
        expected.as_ref(),
    )?;
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
            member_types,
            destination: None,
            signature: Box::new(instantiated),
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
    let (renamed, _) = member_renaming(&contract);
    let substitution = BTreeMap::from([(SELF.to_owned(), receiver.clone())]);
    let Some((signature, member_types)) = instantiate_value(
        environment,
        span,
        &contract,
        &renamed,
        &member_signature(&contract, &substitution),
        written,
    ) else {
        return Some(None);
    };
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
    // The closure's slots are the fixed parameters, the labelled ones, and
    // then the tail, which arrives already packed.
    let slots = signature.slot_types();
    let arguments = slots
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
            member_types,
            destination: None,
            signature: Box::new(signature.clone()),
            closed: closed_contract(environment.types, interface, &contract.name),
        },
        arguments,
        result: signature.result(),
        tail: true,
        origin: origin.clone(),
    };
    let parameters = signature.parameters().to_vec();
    Some(Some(Expr::closure(
        signature,
        parameters,
        Vec::new(),
        body,
        slots.len(),
        origin,
    )))
}

/// Whether a candidate's signature, its own generic parameters renamed to
/// `renamed` and still open, is the written `fn` type of a contract member
/// named as a value.
fn fits_value(
    renamed: &[String],
    candidate: &FunctionSignature,
    written: &FunctionSignature,
) -> bool {
    if renamed.is_empty() {
        return candidate.same_shape(written);
    }
    let mut instantiation = crate::infer::Instantiation::new(renamed);
    let opened = instantiation.open_signature(candidate);
    instantiation.unify(
        &Type::Function(Box::new(opened)),
        &Type::Function(Box::new(written.clone())),
    )
}

/// The signature of a contract member named as a value at the written `fn`
/// type `written`, and the member's own type arguments that type fixes
/// (`docs/spec/02-type-system.md`, "Functions as values"). The written type
/// stands for a call's operands and expected type, exactly as for a generic
/// function named as a value: one that leaves a generic parameter open is
/// `@type.ambiguous-inference`, and each argument is checked against its
/// bound.
fn instantiate_value(
    environment: &mut CheckEnvironment<'_>,
    span: ByteSpan,
    contract: &ContractMember,
    renamed: &[String],
    signature: &FunctionSignature,
    written: &FunctionSignature,
) -> Option<(FunctionSignature, Vec<Type>)> {
    let expected = Type::Function(Box::new(written.clone()));
    if renamed.is_empty() {
        let actual = Type::Function(Box::new(signature.clone()));
        crate::ensure_expected(
            environment,
            span,
            Some(expected.clone()),
            actual.clone(),
        );
        return crate::types_match(&expected, &actual)
            .then(|| (signature.clone(), Vec::new()));
    }
    let mut instantiation = crate::infer::Instantiation::new(renamed);
    let opened = instantiation.open_signature(signature);
    let value_type = Type::Function(Box::new(opened));
    if !instantiation.unify(&value_type, &expected) {
        crate::ensure_expected(
            environment,
            span,
            Some(expected),
            crate::infer::display(&Type::Function(Box::new(signature.clone()))),
        );
        return None;
    }
    let unbound = instantiation.unbound();
    if !unbound.is_empty() {
        crate::ambiguous_generic(environment, span, &unbound);
        return None;
    }
    let Some(Type::Function(instantiated)) = instantiation.resolved(&value_type) else {
        return None;
    };
    let arguments = renamed
        .iter()
        .filter_map(|name| {
            let variable = instantiation.open(&Type::Param(name.clone()));
            instantiation.resolved(&variable)
        })
        .collect::<Vec<_>>();
    // The member's own bounded parameters, at the arguments the type fixed.
    if !contract.bounds.is_empty() {
        let fixed = contract
            .generics
            .iter()
            .cloned()
            .zip(arguments.iter().cloned())
            .collect::<BTreeMap<_, _>>();
        if !check_bounds(environment, span, &contract.bounds, &fixed) {
            return None;
        }
    }
    Some((*instantiated, arguments))
}
