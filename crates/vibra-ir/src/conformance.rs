//! Conformance evidence (`docs/spec/02-type-system.md`, "Dispatch",
//! "Bounds", "Widening", "Dict keys, equality, and order").
//!
//! One selection serves every place the checked IR relies on a type
//! implementing an interface: a widening, a contract call, a generic
//! instantiation, and a dict key. A goal is a receiver, an interface, and the
//! interface's positional arguments. It holds by one of four kinds of
//! evidence, tried in this order:
//!
//! 1. the bound of a type parameter in scope, whose interface and positional
//!    arguments are the goal's;
//! 2. `Self` inside a member of the interface that declares it;
//! 3. the one implementation whose key matches the goal, with each bound of
//!    its binders a further goal at the types the match fixed;
//! 4. the structural `Equatable` and `Ordered` conformance of an atom, an
//!    atom singleton, and an anonymous tuple or record, with each component a
//!    further goal.
//!
//! `Encodable` is decided before these: a type with no callable, plan, or
//! grant in it, through the representation of every declared type in it,
//! conforms structurally, with each open form in it a further goal, and
//! an open form on its own conforms only through a bound.
//!
//! The search keeps its goals on a work list and stops after
//! [`GOAL_LIMIT`] of them, so no host stack and no loop grows with what a
//! checker built.

use crate::expr::Tables;
use crate::{
    Binder, Implementation, IrError, Library, NamedTypes, Substitution, Type, TypeBody,
    TypeId,
};

/// The most goals one conformance question may visit.
pub(crate) const GOAL_LIMIT: usize = 4096;

/// The evidence a body is checked under: the binders of its signature, with
/// their bounds, and the interface whose member it is, when it is the
/// default body of one.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Evidence<'a> {
    pub(crate) binders: &'a [Binder],
    pub(crate) self_interface: Option<&'a TypeId>,
}

/// A goal, and the choices its evidence must give, when they are fixed.
struct Goal {
    receiver: Type,
    interface: TypeId,
    arguments: Vec<Type>,
    required: Option<NamedTypes>,
}

/// Whether `required` holds every choice it names with the same type in
/// `provided`.
fn covers(provided: &NamedTypes, required: &NamedTypes) -> bool {
    required
        .entries()
        .iter()
        .all(|(name, ty)| provided.get(name) == Some(ty))
}

/// Matches the open `pattern`, whose binder references are the variables of
/// `assignment`, against `actual`, extending `assignment`. A binder reference
/// in `actual` is a type parameter in scope, which matches only itself.
pub(crate) fn match_open(
    pattern: &Type,
    actual: &Type,
    assignment: &mut [Option<Type>],
) -> bool {
    let mut pending = vec![(pattern, actual)];
    while let Some((pattern, actual)) = pending.pop() {
        if let Type::Binder(index) = pattern
            && let Some(slot) = assignment.get_mut(*index)
        {
            match slot {
                Some(bound) if bound != actual => return false,
                Some(_) => {}
                None => *slot = Some(actual.clone()),
            }
            continue;
        }
        if !crate::module_set::same_head(pattern, actual) {
            return false;
        }
        pending.extend(pattern.components().into_iter().zip(actual.components()));
    }
    true
}

/// The binder assignment by which `implementation`'s key matches the goal,
/// with every binder the key leaves open still `None`.
fn key_match(
    implementation: &Implementation,
    receiver: &Type,
    interface: &TypeId,
    arguments: &[Type],
) -> Option<Vec<Option<Type>>> {
    let key = implementation.contract();
    if key.interface != *interface || key.arguments.len() != arguments.len() {
        return None;
    }
    let mut assignment = vec![None; implementation.binders().len()];
    let matched = match_open(implementation.receiver(), receiver, &mut assignment)
        && key
            .arguments
            .iter()
            .zip(arguments)
            .all(|(pattern, actual)| match_open(pattern, actual, &mut assignment));
    matched.then_some(assignment)
}

/// `ty` under `assignment`, each binder it mentions being assigned.
fn under(ty: &Type, assignment: &[Option<Type>]) -> Type {
    let filled = assignment
        .iter()
        .map(|slot| slot.clone().unwrap_or(Type::Void))
        .collect::<Vec<_>>();
    ty.instantiate(&filled)
}

/// Fixes the binders the labels of the bound of binder `at` name from
/// `provided`, the choices of the evidence for that bound. `false` when a
/// label disagrees with them.
fn fix_labels(
    implementation: &Implementation,
    at: usize,
    provided: &NamedTypes,
    assignment: &mut [Option<Type>],
) -> bool {
    let Some(bound) = implementation.binders().get(at).and_then(Binder::bound) else {
        return false;
    };
    bound.choices.entries().iter().all(|(name, pattern)| {
        provided
            .get(name)
            .is_some_and(|actual| match_open(pattern, actual, assignment))
    })
}

/// The implementation in `implementations` whose key matches the goal, with
/// the type of each of its binders (`docs/spec/02-type-system.md`,
/// "Bounds"). The key fixes some binders; a label of the bound of a fixed
/// binder fixes the binder it names, from the choices of the evidence for
/// that bound. `evidence` gives those choices where something other than an
/// implementation shows the bound, such as a bound in scope; otherwise the
/// implementation the bound's goal selects gives them, itself completed
/// the same way. The nested selections are kept on a work list, so no host
/// stack grows with them.
///
/// # Errors
///
/// A selected implementation leaves a binder open, or the selections visit
/// more goals than one conformance question may (4096).
pub fn select_implementation<'i>(
    implementations: &'i [Implementation],
    receiver: &Type,
    interface: &TypeId,
    arguments: &[Type],
    mut evidence: impl FnMut(&Type, &TypeId, &[Type]) -> Option<NamedTypes>,
) -> Result<Option<(&'i Implementation, Vec<Type>)>, IrError> {
    let find = |receiver: &Type, interface: &TypeId, arguments: &[Type]| {
        implementations.iter().find_map(|implementation| {
            key_match(implementation, receiver, interface, arguments)
                .map(|assignment| (implementation, assignment))
        })
    };
    let Some(first) = find(receiver, interface, arguments) else {
        return Ok(None);
    };
    // Each frame: an implementation, its assignment so far, and the binder
    // whose bound the frame above it is the evidence for.
    let mut frames = vec![(first.0, first.1, 0_usize)];
    let mut visited = 0_usize;
    while let Some((implementation, assignment, waiting)) = frames.last_mut() {
        visited = visited.saturating_add(1);
        if visited > GOAL_LIMIT {
            return Err(IrError::InvalidExpression(format!(
                "selecting the implementation of `{}` for {receiver} needs more than {GOAL_LIMIT} goals",
                interface.path()
            )));
        }
        let implementation: &'i Implementation = implementation;
        if let Some(at) = implementation.record().open_bound(assignment) {
            let Some(bound) = implementation.binders().get(at).and_then(Binder::bound)
            else {
                return Ok(None);
            };
            let Some(bounded) = assignment.get(at).cloned().flatten() else {
                return Ok(None);
            };
            let bound_arguments = bound
                .contract
                .arguments
                .iter()
                .map(|argument| under(argument, assignment))
                .collect::<Vec<_>>();
            if let Some(provided) =
                evidence(&bounded, &bound.contract.interface, &bound_arguments)
            {
                if !fix_labels(implementation, at, &provided, assignment) {
                    return Ok(None);
                }
                continue;
            }
            let Some(next) =
                find(&bounded, &bound.contract.interface, &bound_arguments)
            else {
                return Ok(None);
            };
            *waiting = at;
            frames.push((next.0, next.1, 0));
            continue;
        }
        let Some((done, assignment, _)) = frames.pop() else {
            break;
        };
        let binders = assignment
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| {
                IrError::InvalidType(format!(
                    "an implementation of `{}` for {} has a binder neither its key nor a bound fixes",
                    done.contract().interface.path(),
                    done.receiver()
                ))
            })?;
        let Some((parent, parent_assignment, at)) = frames.last_mut() else {
            return Ok(Some((done, binders)));
        };
        let provided = NamedTypes::new(
            done.choices()
                .entries()
                .iter()
                .map(|(name, ty)| (name.clone(), ty.instantiate(&binders))),
        )?;
        if !fix_labels(parent, *at, &provided, parent_assignment) {
            return Ok(None);
        }
    }
    Ok(None)
}

/// Whether `ty` is a type that conforms to `Equatable` and `Ordered`
/// structurally when its components do.
const fn is_structural(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Atom | Type::AtomSingleton(_) | Type::Tuple(_) | Type::Record(_)
    )
}

impl Tables<'_> {
    /// The choices by which `receiver` implements `interface` at
    /// `arguments` under `evidence`; `None` when nothing shows that it does.
    /// The choices of a bound are the labels it fixes, which may be fewer
    /// than the interface's associated types.
    ///
    /// # Errors
    ///
    /// The search visits more than [`GOAL_LIMIT`] goals, or the matching
    /// implementation leaves one of its binders open.
    pub(crate) fn conformance(
        &self,
        evidence: Evidence<'_>,
        receiver: &Type,
        interface: &TypeId,
        arguments: &[Type],
    ) -> Result<Option<NamedTypes>, IrError> {
        let Some((choices, mut pending)) =
            self.select_evidence(evidence, receiver, interface, arguments)?
        else {
            return Ok(None);
        };
        let mut visited = 1_usize;
        while let Some(goal) = pending.pop() {
            visited = visited.saturating_add(1);
            if visited > GOAL_LIMIT {
                return Err(IrError::InvalidExpression(format!(
                    "the evidence that {receiver} implements `{}` needs more than {GOAL_LIMIT} goals",
                    interface.path()
                )));
            }
            let Some((provided, more)) = self.select_evidence(
                evidence,
                &goal.receiver,
                &goal.interface,
                &goal.arguments,
            )?
            else {
                return Ok(None);
            };
            if let Some(required) = &goal.required
                && !covers(&provided, required)
            {
                return Ok(None);
            }
            pending.extend(more);
        }
        Ok(Some(choices))
    }

    /// Whether `receiver` implements `interface` at `arguments` with exactly
    /// the choices `choices`.
    pub(crate) fn conforms_with(
        &self,
        evidence: Evidence<'_>,
        receiver: &Type,
        interface: &TypeId,
        arguments: &[Type],
        choices: &NamedTypes,
    ) -> Result<bool, IrError> {
        Ok(self
            .conformance(evidence, receiver, interface, arguments)?
            .is_some_and(|provided| provided == *choices))
    }

    /// Whether `key` may key a dict: it implements `Ordered`, or, when no
    /// interface plays `@ordered`, it is ordered structurally throughout.
    pub(crate) fn is_dict_key(
        &self,
        evidence: Evidence<'_>,
        key: &Type,
    ) -> Result<bool, IrError> {
        if let Some(ordered) = self.library.get(Library::Ordered) {
            return Ok(self.conformance(evidence, key, ordered, &[])?.is_some());
        }
        let mut pending = vec![key];
        while let Some(ty) = pending.pop() {
            match ty {
                Type::Atom | Type::AtomSingleton(_) => {}
                Type::Tuple(_) | Type::Record(_) => pending.extend(ty.components()),
                _ => return Ok(false),
            }
        }
        Ok(true)
    }

    /// The choices of the evidence the scope gives for one goal: the bound
    /// of a type parameter in scope, or `Self` inside a member of the
    /// interface that declares it.
    fn scoped_evidence(
        &self,
        evidence: Evidence<'_>,
        receiver: &Type,
        interface: &TypeId,
        arguments: &[Type],
    ) -> Option<NamedTypes> {
        if let Type::Binder(index) = receiver
            && let Some(bound) = evidence.binders.get(*index).and_then(Binder::bound)
            && bound.contract.interface == *interface
            && bound.contract.arguments == arguments
        {
            return Some(bound.choices.clone());
        }
        if *receiver == Type::SelfType
            && evidence.self_interface == Some(interface)
            && let Some(definition) = self.interfaces.get(interface)
        {
            let parameters = (0..definition.parameters().len())
                .map(Type::Binder)
                .collect::<Vec<_>>();
            if parameters == arguments {
                return NamedTypes::new(
                    definition
                        .associated()
                        .iter()
                        .map(|name| (name.clone(), Type::Associated(name.clone()))),
                )
                .ok();
            }
        }
        None
    }

    /// The evidence for one goal: the choices it gives and the goals it
    /// rests on.
    fn select_evidence(
        &self,
        evidence: Evidence<'_>,
        receiver: &Type,
        interface: &TypeId,
        arguments: &[Type],
    ) -> Result<Option<(NamedTypes, Vec<Goal>)>, IrError> {
        if self.library.get(Library::Encodable) == Some(interface)
            && arguments.is_empty()
            && !matches!(
                receiver,
                Type::Binder(_) | Type::SelfType | Type::Associated(_) | Type::Never
            )
        {
            return Ok(self
                .is_encodable(receiver)
                .then(|| (NamedTypes::empty(), open_goals(receiver, interface))));
        }
        if receiver.is_interface_value() || *receiver == Type::Never {
            return Ok(None);
        }
        if let Some(choices) =
            self.scoped_evidence(evidence, receiver, interface, arguments)
        {
            return Ok(Some((choices, Vec::new())));
        }
        let scoped = |receiver: &Type, interface: &TypeId, arguments: &[Type]| {
            self.scoped_evidence(evidence, receiver, interface, arguments)
        };
        if let Some((implementation, binders)) = select_implementation(
            self.implementations,
            receiver,
            interface,
            arguments,
            scoped,
        )? {
            let choices = NamedTypes::new(
                implementation
                    .choices()
                    .entries()
                    .iter()
                    .map(|(name, ty)| (name.clone(), ty.instantiate(&binders))),
            )?;
            let mut goals = Vec::new();
            for (binder, actual) in implementation.binders().iter().zip(&binders) {
                if let Some(bound) = binder.bound() {
                    goals.push(bound_goal(
                        bound,
                        actual,
                        &Substitution {
                            binders: &binders,
                            ..Substitution::default()
                        },
                    )?);
                }
            }
            return Ok(Some((choices, goals)));
        }
        let structural = [Library::Equatable, Library::Ordered]
            .into_iter()
            .any(|library| self.library.get(library) == Some(interface));
        if structural && arguments.is_empty() && is_structural(receiver) {
            let goals = receiver
                .components()
                .into_iter()
                .map(|component| Goal {
                    receiver: component.clone(),
                    interface: interface.clone(),
                    arguments: Vec::new(),
                    required: None,
                })
                .collect();
            return Ok(Some((NamedTypes::empty(), goals)));
        }
        Ok(None)
    }

    /// Whether `ty` conforms to `Encodable` on its own account, before the
    /// open forms in it are asked: no callable or plan type and no operation
    /// interface value type occurs in it, through the type arguments and the
    /// representation of every declared type in it. The walk keeps a work
    /// list and reads each declared type once, so a recursive type ends it.
    fn is_encodable(&self, ty: &Type) -> bool {
        let mut seen = std::collections::BTreeSet::new();
        let mut pending = vec![ty.clone()];
        while let Some(ty) = pending.pop() {
            match &ty {
                Type::Interface(id, _, _)
                    if self
                        .interfaces
                        .get(id)
                        .is_some_and(crate::effects::is_operation_interface) =>
                {
                    return false;
                }
                Type::Fn(_) | Type::Proc(_) | Type::Intent(_) => return false,
                Type::Declared(id, _) if seen.insert(id.clone()) => {
                    if let Some(definition) = self.types.get(id) {
                        pending.extend(body_types(definition.body()));
                    }
                }
                _ => {}
            }
            pending.extend(ty.components().into_iter().cloned());
        }
        true
    }

    /// Checks that every application of a declared type in `ty` satisfies
    /// the bounds of the type's parameters under `evidence`
    /// (`docs/spec/02-type-system.md`, "Generics").
    ///
    /// # Errors
    ///
    /// An argument does not satisfy its parameter's bound, or the search
    /// for the evidence visits more than [`GOAL_LIMIT`] goals.
    pub(crate) fn check_applications(
        &self,
        evidence: Evidence<'_>,
        ty: &Type,
    ) -> Result<(), IrError> {
        let mut applications = Vec::new();
        ty.visit(&mut |inner| {
            if let Type::Declared(id, arguments) = inner
                && let Some(definition) = self.types.get(id)
                && definition
                    .parameters()
                    .iter()
                    .any(|binder| binder.bound().is_some())
            {
                applications.push((definition, arguments.clone()));
            }
        });
        for (definition, arguments) in applications {
            self.check_bounds(
                evidence,
                definition.parameters(),
                &Substitution {
                    binders: &arguments,
                    ..Substitution::default()
                },
            )?;
        }
        Ok(())
    }

    /// Checks that each bound of `binders` holds at its type in
    /// `substitution`, whose `binders` give one type for each of them.
    pub(crate) fn check_bounds(
        &self,
        evidence: Evidence<'_>,
        binders: &[Binder],
        substitution: &Substitution<'_>,
    ) -> Result<(), IrError> {
        for (binder, actual) in binders.iter().zip(substitution.binders) {
            let Some(bound) = binder.bound() else {
                continue;
            };
            let goal = bound_goal(bound, actual, substitution)?;
            let holds = self
                .conformance(
                    evidence,
                    &goal.receiver,
                    &goal.interface,
                    &goal.arguments,
                )?
                .is_some_and(|provided| {
                    goal.required
                        .as_ref()
                        .is_none_or(|required| covers(&provided, required))
                });
            if !holds {
                return Err(IrError::InvalidExpression(format!(
                    "{actual} does not satisfy the bound `{}` of `{}`",
                    bound.contract.interface.path(),
                    binder.name()
                )));
            }
        }
        Ok(())
    }
}

/// The goal a bound sets for the type `actual` of its binder, under
/// `substitution`.
fn bound_goal(
    bound: &crate::Bound,
    actual: &Type,
    substitution: &Substitution<'_>,
) -> Result<Goal, IrError> {
    Ok(Goal {
        receiver: actual.clone(),
        interface: bound.contract.interface.clone(),
        arguments: bound
            .contract
            .arguments
            .iter()
            .map(|argument| argument.substitute(substitution))
            .collect(),
        required: Some(NamedTypes::new(
            bound
                .choices
                .entries()
                .iter()
                .map(|(name, ty)| (name.clone(), ty.substitute(substitution))),
        )?),
    })
}

/// The goals of a structural `Encodable`: each distinct open form in `ty`,
/// which conforms only through a bound.
fn open_goals(ty: &Type, interface: &TypeId) -> Vec<Goal> {
    let mut goals: Vec<Goal> = Vec::new();
    ty.visit(&mut |inner| {
        if matches!(
            inner,
            Type::Binder(_) | Type::SelfType | Type::Associated(_)
        ) && !goals.iter().any(|goal| goal.receiver == *inner)
        {
            goals.push(Goal {
                receiver: inner.clone(),
                interface: interface.clone(),
                arguments: Vec::new(),
                required: None,
            });
        }
    });
    goals
}

/// The types a declared type's body is made of. A type parameter in it is a
/// [`Type::Binder`], which holds nothing: the arguments it stands for are
/// walked where the type is applied.
fn body_types(body: &TypeBody) -> Vec<Type> {
    match body {
        TypeBody::Record(fields) => fields.iter().map(|(_, ty)| ty.clone()).collect(),
        TypeBody::Tuple(components) => components.clone(),
        TypeBody::Wrapper(inner) => vec![inner.clone()],
        TypeBody::Variant(cases) => cases
            .iter()
            .flat_map(|case| case.fields.iter().map(|(_, ty)| ty.clone()))
            .collect(),
    }
}
