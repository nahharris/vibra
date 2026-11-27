//! The rules of `docs/spec/03-effects.md` that a checked module set keeps:
//! where an `await` may stand, that every body's row is contained in its
//! ceiling, that a row names operation interfaces only, and that a host
//! binding is its registry row's.
//!
//! The row of a body is recomputed here from the facts the shape check of
//! every body gathers: the row of each plan it awaits, which the plan's
//! `Intent` type carries ("Cold plans and performed rows": "`await plan`
//! contributes the plan's complete row"). Building a plan contributes
//! nothing. The bodies are visited with an explicit work list: each function
//! and module value, then each closure, attributed to the function or module
//! value whose body creates it.

use std::collections::BTreeMap;
use std::fmt;

use crate::external::host::{
    HostErrorType, HostParameter, HostType, Owner, TEST_WORLD_OWNER,
};
use crate::{
    BodyContext, Callable, CheckedFunction, CheckedModuleSet, EntityRef, Facts,
    FunctionBody, HostBinding, InterfaceDefinition, IrError, Library, NamedTypes,
    ProcType, Row, Type, TypeId,
};

/// What an effect failure is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EffectFault {
    /// An `await` in a body that runs in no procedure context: a `fn`, a
    /// `Fn` closure, a module-value initializer.
    AwaitOutsideProcedure,
    /// An `await` of a value that is not a plan.
    NotAPlan(Box<Type>),
    /// An awaited plan whose row holds a root outside the body's ceiling.
    OutsideCeiling {
        /// The root outside the ceiling.
        root: EntityRef,
        /// The ceiling.
        ceiling: Row,
    },
    /// A row entry that is not an operation interface: no interface, or one
    /// with no `proc` member.
    NotARoot(EntityRef),
    /// A function of a `Proc` type whose procedure context states a row
    /// other than its type's.
    ContextRow {
        /// The row of the function's type.
        typed: Row,
        /// The row its context states.
        stated: Row,
    },
    /// A host binding that is not its registry row's: on no member, on
    /// another owner's or another name's member, or with another signature.
    HostMismatch {
        /// The row's wire symbol.
        symbol: &'static str,
        /// What differs.
        message: String,
    },
}

impl fmt::Display for EffectFault {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AwaitOutsideProcedure => formatter
                .write_str("an `await` in a body that runs in no procedure context"),
            Self::NotAPlan(ty) => {
                write!(formatter, "an `await` of {ty}, which is no plan")
            }
            Self::OutsideCeiling { root, ceiling } => write!(
                formatter,
                "an awaited plan performs `@{}`, which is outside the ceiling {ceiling}",
                root.path()
            ),
            Self::NotARoot(root) => write!(
                formatter,
                "the row entry `@{}` is not an operation interface",
                root.path()
            ),
            Self::ContextRow { typed, stated } => write!(
                formatter,
                "a procedure context states the row {stated} on a function whose type has {typed}"
            ),
            Self::HostMismatch { symbol, message } => {
                write!(formatter, "the host binding `{symbol}`: {message}")
            }
        }
    }
}

/// The failure `fault`, in no owner yet.
pub(crate) fn effect(fault: EffectFault) -> IrError {
    IrError::Effect {
        owner: None,
        fault: Box::new(fault),
    }
}

/// Whether `definition` is an operation interface: it declares at least one
/// `proc` member (`docs/spec/03-effects.md`, "Operation interfaces").
pub(crate) fn is_operation_interface(definition: &InterfaceDefinition) -> bool {
    definition
        .members()
        .iter()
        .any(|member| matches!(member.signature.type_info(), Type::Proc(_)))
}

/// Checks that every entry of `row` is an operation interface of the set.
pub(crate) fn check_row(
    interfaces: &BTreeMap<TypeId, InterfaceDefinition>,
    row: &Row,
) -> Result<(), IrError> {
    for root in row.roots() {
        let is_root = interfaces
            .get(&TypeId::from(root.clone()))
            .is_some_and(is_operation_interface);
        if !is_root {
            return Err(effect(EffectFault::NotARoot(root.clone())));
        }
    }
    Ok(())
}

/// The ceiling of a function's body: the row of its `Proc` type, or the row
/// its procedure context states; `None` when it performs nothing.
fn function_ceiling<'f>(
    set: &CheckedModuleSet,
    function: &'f CheckedFunction,
) -> Result<Option<&'f Row>, IrError> {
    match (function.signature.type_info(), &function.context) {
        (Type::Proc(signature), BodyContext::Pure) => Ok(Some(&signature.effects)),
        (Type::Proc(signature), BodyContext::Procedure(stated)) => {
            if *stated == signature.effects {
                Ok(Some(stated))
            } else {
                Err(effect(EffectFault::ContextRow {
                    typed: signature.effects.clone(),
                    stated: stated.clone(),
                }))
            }
        }
        (_, BodyContext::Pure) => Ok(None),
        (_, BodyContext::Procedure(stated)) => {
            check_row(set.interfaces(), stated)?;
            Ok(Some(stated))
        }
    }
}

/// The ceiling of a closure's body: the row of its `Proc` type; a `Fn`
/// closure performs nothing, whatever body encloses it.
fn closure_ceiling(closure_type: &Type) -> Option<&Row> {
    match closure_type {
        Type::Proc(signature) => Some(&signature.effects),
        _ => None,
    }
}

/// Checks that each plan `facts` awaits is awaited in a procedure context
/// whose ceiling holds the plan's row.
fn check_awaits(facts: &Facts, ceiling: Option<&Row>) -> Result<(), IrError> {
    for awaited in &facts.awaited {
        let Some(ceiling) = ceiling else {
            return Err(effect(EffectFault::AwaitOutsideProcedure));
        };
        if let Some(root) = awaited
            .roots()
            .iter()
            .find(|root| !ceiling.roots().contains(root))
        {
            return Err(effect(EffectFault::OutsideCeiling {
                root: root.clone(),
                ceiling: ceiling.clone(),
            }));
        }
    }
    Ok(())
}

/// The closures `facts` creates, each with the name its failures carry.
fn created<'n>(facts: &Facts, owner: &'n str, pending: &mut Vec<(usize, &'n str)>) {
    for (callable, _) in &facts.values {
        if let Callable::Closure(index) = callable {
            pending.push((*index, owner));
        }
    }
}

/// Validates the effect rules of `set`, whose flow is computed.
///
/// # Errors
///
/// The first failure, as an [`IrError::Effect`] that names the function or
/// module value it is in.
pub(crate) fn validate(set: &CheckedModuleSet) -> Result<(), IrError> {
    let flow = set.flow();
    let mut pending = Vec::new();
    for (global, facts) in set.globals().iter().zip(&flow.globals) {
        check_awaits(facts, None).map_err(|error| error.owned_by(&global.name))?;
        created(facts, &global.name, &mut pending);
    }
    for (function, facts) in set.functions().iter().zip(&flow.functions) {
        let ceiling = function_ceiling(set, function)
            .map_err(|error| error.owned_by(&function.name))?;
        check_awaits(facts, ceiling).map_err(|error| error.owned_by(&function.name))?;
        created(facts, &function.name, &mut pending);
    }
    while let Some((index, owner)) = pending.pop() {
        let (closure_type, facts) = flow
            .closures
            .get(index)
            .ok_or_else(|| IrError::InvalidExpression(format!("no closure {index}")))?;
        check_awaits(facts, closure_ceiling(closure_type))
            .map_err(|error| error.owned_by(owner))?;
        created(facts, owner, &mut pending);
    }
    check_host_bindings(set)
}

/// The interface and the member each function is the default body of.
fn members_of(set: &CheckedModuleSet) -> BTreeMap<usize, (&InterfaceDefinition, &str)> {
    let mut members = BTreeMap::new();
    for interface in set.interfaces().values() {
        for member in interface.members() {
            if let Some(default) = member.default {
                members.insert(default, (interface, member.name.as_str()));
            }
        }
    }
    members
}

/// Checks every host binding against its registry row: it is the body of a
/// member of the interface the row names, the member has the row's name,
/// its row is exactly `{owner}`, and its type is the row's signature with
/// `Self` first (`docs/spec/03-effects.md`, "Operation interfaces").
fn check_host_bindings(set: &CheckedModuleSet) -> Result<(), IrError> {
    let members = members_of(set);
    for (index, function) in set.functions().iter().enumerate() {
        let FunctionBody::Host(binding) = &function.body else {
            continue;
        };
        check_host(set, function, *binding, members.get(&index).copied())
            .map_err(|error| error.owned_by(&function.name))?;
    }
    Ok(())
}

fn host_fault(binding: HostBinding, message: String) -> IrError {
    effect(EffectFault::HostMismatch {
        symbol: binding.symbol(),
        message,
    })
}

/// The path of a library declaration without its `@`: `std.fs.FsError`.
fn owner_path(owner: Owner) -> String {
    let module = owner.module.strip_prefix('@').unwrap_or(owner.module);
    format!("{module}.{}", owner.name)
}

fn check_host(
    set: &CheckedModuleSet,
    function: &CheckedFunction,
    binding: HostBinding,
    member: Option<(&InterfaceDefinition, &str)>,
) -> Result<(), IrError> {
    let fault = |message: String| host_fault(binding, message);
    let (owner, name) = match binding {
        HostBinding::Operation(op) => (op.interface().owner(), op.interface().member()),
        HostBinding::TestWorld(op) => (TEST_WORLD_OWNER, op.member()),
    };
    let owner = owner_path(owner);
    let Some((interface, member)) = member else {
        return Err(fault("it is the body of no interface member".to_owned()));
    };
    if interface.id().path() != owner {
        return Err(fault(format!(
            "it is on a member of `@{}`, and the row's owner is `@{owner}`",
            interface.id().path()
        )));
    }
    if member != name {
        return Err(fault(format!(
            "it is on the member `{member}`, and the row's member is `{name}`"
        )));
    }
    let owner_row = Row::new([interface.id().entity().clone()])?;
    let actual = function.signature.type_info();
    if let Type::Proc(signature) = actual
        && signature.effects != owner_row
    {
        return Err(fault(format!(
            "its row is {}, and a host-bound member performs exactly {owner_row}",
            signature.effects
        )));
    }
    let expected = Type::Proc(Box::new(host_signature(set, binding, owner_row)?));
    if *actual != expected {
        return Err(fault(format!(
            "its type is {actual}, and the row's signature is {expected}"
        )));
    }
    Ok(())
}

/// The registry row's signature as a member's type: `Self`, the row's
/// parameters, its result, and the row `{owner}`.
fn host_signature(
    set: &CheckedModuleSet,
    binding: HostBinding,
    effects: Row,
) -> Result<ProcType, IrError> {
    let positional = |parameters: &[HostParameter]| {
        std::iter::once(Ok(Type::SelfType))
            .chain(
                parameters
                    .iter()
                    .map(|parameter| host_type(set, binding, parameter.ty)),
            )
            .collect::<Result<Vec<_>, _>>()
    };
    match binding {
        HostBinding::Operation(op) => {
            let row = op.row();
            let result = set.library().declared(
                Library::Result,
                vec![
                    host_type(set, binding, row.ok)?,
                    error_type(set, binding, row.error)?,
                ],
            )?;
            Ok(ProcType {
                parameters: positional(row.parameters)?,
                labels: NamedTypes::empty(),
                result,
                effects,
            })
        }
        HostBinding::TestWorld(op) => {
            let row = op.row();
            let labels = NamedTypes::new(
                row.labelled
                    .iter()
                    .map(|parameter| {
                        host_type(set, binding, parameter.ty)
                            .map(|ty| (parameter.name.to_owned(), ty))
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            )?;
            Ok(ProcType {
                parameters: positional(&[])?,
                labels,
                result: host_type(set, binding, row.result)?,
                effects,
            })
        }
    }
}

/// The declared type of the set at `path`.
fn declared_at(
    set: &CheckedModuleSet,
    binding: HostBinding,
    path: &str,
) -> Result<Type, IrError> {
    set.types()
        .types()
        .keys()
        .find(|id| id.path() == path)
        .map(|id| Type::Declared(id.clone(), Vec::new()))
        .ok_or_else(|| {
            host_fault(binding, format!("the set declares no type `@{path}`"))
        })
}

/// The interface value type of the set's interface at `path`.
fn interface_at(
    set: &CheckedModuleSet,
    binding: HostBinding,
    path: &str,
) -> Result<Type, IrError> {
    set.interfaces()
        .keys()
        .find(|id| id.path() == path)
        .map(|id| Type::Interface(id.clone(), Vec::new(), NamedTypes::empty()))
        .ok_or_else(|| {
            host_fault(binding, format!("the set declares no interface `@{path}`"))
        })
}

fn error_type(
    set: &CheckedModuleSet,
    binding: HostBinding,
    error: HostErrorType,
) -> Result<Type, IrError> {
    let module = error.module().strip_prefix('@').unwrap_or(error.module());
    declared_at(set, binding, &format!("{module}.{}", error.name()))
}

/// A registry type as a type of the set. The registry's types nest a few
/// levels at most, so this recursion is bounded by the registry, not by a
/// program.
fn host_type(
    set: &CheckedModuleSet,
    binding: HostBinding,
    ty: HostType,
) -> Result<Type, IrError> {
    let library = set.library();
    Ok(match ty {
        HostType::Void => Type::Void,
        HostType::I64 => Type::I64,
        HostType::U64 => Type::U64,
        HostType::Str => library.declared(Library::Str, Vec::new())?,
        HostType::Bytes => library.declared(Library::Bytes, Vec::new())?,
        HostType::RelativePath => declared_at(set, binding, "std.fs.RelativePath")?,
        HostType::FsKind => declared_at(set, binding, "std.fs.FsKind")?,
        HostType::FsMetadata => declared_at(set, binding, "std.fs.FsMetadata")?,
        HostType::List(element) => Type::list(host_type(set, binding, *element)?),
        HostType::Dict(key, value) => library.declared(
            Library::Dict,
            vec![
                host_type(set, binding, *key)?,
                host_type(set, binding, *value)?,
            ],
        )?,
        HostType::Interface(interface) => {
            interface_at(set, binding, &owner_path(interface.owner()))?
        }
        HostType::TestWorld => {
            interface_at(set, binding, &owner_path(TEST_WORLD_OWNER))?
        }
        HostType::Record(fields) => Type::record(
            fields
                .iter()
                .map(|field| {
                    host_type(set, binding, field.ty)
                        .map(|ty| (field.name.to_owned(), ty))
                })
                .collect::<Result<Vec<_>, _>>()?,
        )?,
    })
}
