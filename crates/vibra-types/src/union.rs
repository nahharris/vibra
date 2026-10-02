//! Union membership, overlap, and widening (M3 Step 6).
//!
//! `docs/spec/02-type-system.md`, "Overlap and non-unifiability" and
//! "Type ascription and widening": union members are concrete and pairwise
//! non-unifiable, and a value widens to a union, or an atom singleton to
//! `atom`, once, at a written expected type.

use std::collections::BTreeMap;

use vibra_ir::{Type, TypeBody};

use crate::nominal::{LowerError, TypeNames};
use crate::pattern::instantiated_body;

/// The member types of a union type, in discriminant order: canonical order
/// for an anonymous union, declaration order for a declared one. `None` for
/// any other type.
pub(crate) fn members(types: &TypeNames, value_type: &Type) -> Option<Vec<Type>> {
    match value_type {
        Type::Union(members) => Some(members.clone()),
        Type::Declared(_) | Type::Applied(_, _) => {
            match instantiated_body(types, value_type)? {
                TypeBody::Union(members) => Some(members),
                _ => None,
            }
        }
        _ => None,
    }
}

/// The discriminant of `member` in `union`, when `member` is exactly one of
/// its member types.
pub(crate) fn discriminant(
    types: &TypeNames,
    union: &Type,
    member: &Type,
) -> Option<usize> {
    members(types, union)?
        .iter()
        .position(|candidate| candidate.same_shape(member))
}

/// Checks a written member list: every member concrete and no two members
/// unifiable under any substitution of the generic names in scope.
pub(crate) fn check_members(
    types: &TypeNames,
    members: &[Type],
) -> Result<(), LowerError> {
    for member in members {
        let declared_union = matches!(member, Type::Declared(_) | Type::Applied(_, _))
            && matches!(instantiated_body(types, member), Some(TypeBody::Union(_)));
        if matches!(
            member,
            Type::Param(_) | Type::Union(_) | Type::Interface(_, _) | Type::Any
        ) || declared_union
        {
            return Err(LowerError::UnionNotConcrete(member.clone()));
        }
    }
    for (index, left) in members.iter().enumerate() {
        for right in members.iter().skip(index + 1) {
            if unifiable(left, right) {
                return Err(LowerError::UnionOverlap(Box::new((
                    left.clone(),
                    right.clone(),
                ))));
            }
        }
    }
    Ok(())
}

/// Whether some substitution of the generic names makes both types equal.
/// Unification is bound-agnostic: a declared bound never separates them.
pub(crate) fn unifiable(left: &Type, right: &Type) -> bool {
    let mut bindings = BTreeMap::new();
    unify(left, right, &mut bindings)
}

/// Whether `value` names a generic parameter anywhere.
pub(crate) fn mentions_param(value: &Type) -> bool {
    matches!(value, Type::Param(_)) || value.components().iter().any(mentions_param)
}

fn occurs(name: &str, value: &Type, bindings: &BTreeMap<String, Type>) -> bool {
    match resolve(value, bindings) {
        Type::Param(other) => other == name,
        other => other
            .components()
            .iter()
            .any(|component| occurs(name, component, bindings)),
    }
}

fn resolve<'a>(value: &'a Type, bindings: &'a BTreeMap<String, Type>) -> &'a Type {
    let mut current = value;
    while let Type::Param(name) = current {
        match bindings.get(name) {
            Some(bound) => current = bound,
            None => break,
        }
    }
    current
}

fn unify(left: &Type, right: &Type, bindings: &mut BTreeMap<String, Type>) -> bool {
    let left = resolve(left, bindings).clone();
    let right = resolve(right, bindings).clone();
    match (&left, &right) {
        (Type::Param(left_name), Type::Param(right_name))
            if left_name == right_name =>
        {
            true
        }
        (Type::Param(name), other) | (other, Type::Param(name)) => {
            // A name never equals a type that contains it.
            if occurs(name, other, bindings) {
                return false;
            }
            bindings.insert(name.clone(), other.clone());
            true
        }
        (
            Type::Applied(left_id, left_arguments),
            Type::Applied(right_id, right_arguments),
        )
        | (
            Type::Interface(left_id, left_arguments),
            Type::Interface(right_id, right_arguments),
        ) => {
            left_id == right_id && unify_all(left_arguments, right_arguments, bindings)
        }
        (Type::Tuple(left), Type::Tuple(right))
        | (Type::Union(left), Type::Union(right)) => unify_all(left, right, bindings),
        (Type::Record(left), Type::Record(right))
        | (Type::Enum(left), Type::Enum(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|((left_name, _), (right_name, _))| left_name == right_name)
                && left
                    .iter()
                    .zip(right)
                    .all(|((_, left), (_, right))| unify(left, right, bindings))
        }
        (Type::Array(left), Type::Array(right)) => unify(left, right, bindings),
        (Type::Dict(left_key, left_value), Type::Dict(right_key, right_value)) => {
            unify(left_key, right_key, bindings)
                && unify(left_value, right_value, bindings)
        }
        (Type::Function(left), Type::Function(right)) => {
            let left_slots = left.slot_types();
            let right_slots = right.slot_types();
            left.labelled().len() == right.labelled().len()
                && left
                    .labelled()
                    .iter()
                    .zip(right.labelled())
                    .all(|(left, right)| left.name() == right.name())
                && left.variadic().is_some() == right.variadic().is_some()
                && unify_all(&left_slots, &right_slots, bindings)
                && unify(&left.result(), &right.result(), bindings)
        }
        _ => left.same_shape(&right),
    }
}

fn unify_all(
    left: &[Type],
    right: &[Type],
    bindings: &mut BTreeMap<String, Type>,
) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| unify(left, right, bindings))
}
