//! Frontend-neutral semantic relations over lowered Vibra types.
//!
//! This module deliberately depends only on semantic IR types. Source readers
//! (including the legacy YAML reader) must lower into [`TypeRef`] before using
//! these relations.

use crate::lower::{
    CapabilityType, LiteralType, PolicyGroup, PolicyScope, PolicyType, TypeAlias, TypeRef,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub(crate) fn substitute_self(ty: &TypeRef, self_ty: &TypeRef) -> TypeRef {
    substitute(ty, &HashMap::new(), Some(self_ty))
}

pub(crate) fn substitute_type(ty: &TypeRef, substitutions: &HashMap<String, TypeRef>) -> TypeRef {
    substitute(ty, substitutions, None)
}

pub(crate) fn substitute(
    ty: &TypeRef,
    substitutions: &HashMap<String, TypeRef>,
    self_ty: Option<&TypeRef>,
) -> TypeRef {
    let recurse = |ty| substitute(ty, substitutions, self_ty);
    match ty {
        TypeRef::Generic(name) => substitutions
            .get(name)
            .map(|replacement| match self_ty {
                Some(self_ty) => substitute_self(replacement, self_ty),
                None => replacement.clone(),
            })
            .unwrap_or_else(|| ty.clone()),
        TypeRef::SelfType => self_ty.cloned().unwrap_or_else(|| ty.clone()),
        TypeRef::Mutable(inner) => TypeRef::Mutable(Box::new(recurse(inner))),
        TypeRef::Reference { inner, mutable } => TypeRef::Reference {
            inner: Box::new(recurse(inner)),
            mutable: *mutable,
        },
        TypeRef::JoinHandle(inner) => TypeRef::JoinHandle(Box::new(recurse(inner))),
        TypeRef::Union(items) => TypeRef::Union(items.iter().map(recurse).collect()),
        TypeRef::Enum(fields) => TypeRef::Enum(
            fields
                .iter()
                .map(|(name, ty)| (name.clone(), recurse(ty)))
                .collect(),
        ),
        TypeRef::Record(fields) => TypeRef::Record(
            fields
                .iter()
                .map(|(name, ty)| (name.clone(), recurse(ty)))
                .collect(),
        ),
        TypeRef::Tuple(items) => TypeRef::Tuple(items.iter().map(recurse).collect()),
        TypeRef::Array(inner) => TypeRef::Array(Box::new(recurse(inner))),
        TypeRef::Map { key, value } => TypeRef::Map {
            key: Box::new(recurse(key)),
            value: Box::new(recurse(value)),
        },
        TypeRef::Interface(fields) => TypeRef::Interface(
            fields
                .iter()
                .map(|(name, ty)| (name.clone(), recurse(ty)))
                .collect(),
        ),
        TypeRef::Intersect(items) => TypeRef::Intersect(items.iter().map(recurse).collect()),
        TypeRef::FnType { args, return_type } => TypeRef::FnType {
            args: Box::new(recurse(args)),
            return_type: Box::new(recurse(return_type)),
        },
        TypeRef::Instantiated { base, type_args } => TypeRef::Instantiated {
            base: base.clone(),
            type_args: type_args.iter().map(recurse).collect(),
        },
        TypeRef::Newtype { name, inner } => TypeRef::Newtype {
            name: name.clone(),
            inner: Box::new(recurse(inner)),
        },
        _ => ty.clone(),
    }
}

pub(crate) fn normalize_type_ref(ty: &TypeRef, aliases: &HashMap<String, TypeAlias>) -> TypeRef {
    match ty {
        TypeRef::Named(name) => match aliases.get(name) {
            Some(alias)
                if alias.type_params.is_empty()
                    && !matches!(alias.body, TypeRef::Newtype { .. }) =>
            {
                normalize_type_ref(&alias.body, aliases)
            }
            _ => ty.clone(),
        },
        TypeRef::Instantiated { base, type_args } => {
            let args: Vec<_> = type_args
                .iter()
                .map(|arg| normalize_type_ref(arg, aliases))
                .collect();
            match aliases.get(base) {
                Some(alias)
                    if !matches!(alias.body, TypeRef::Newtype { .. })
                        && alias.type_params.len() == args.len() =>
                {
                    let substitutions = alias
                        .type_params
                        .iter()
                        .cloned()
                        .zip(args.iter().cloned())
                        .collect();
                    normalize_type_ref(&substitute_type(&alias.body, &substitutions), aliases)
                }
                _ => TypeRef::Instantiated {
                    base: base.clone(),
                    type_args: args,
                },
            }
        }
        TypeRef::Mutable(inner) => TypeRef::Mutable(Box::new(normalize_type_ref(inner, aliases))),
        TypeRef::Reference { inner, mutable } => TypeRef::Reference {
            inner: Box::new(normalize_type_ref(inner, aliases)),
            mutable: *mutable,
        },
        TypeRef::JoinHandle(inner) => {
            TypeRef::JoinHandle(Box::new(normalize_type_ref(inner, aliases)))
        }
        TypeRef::Union(items) => TypeRef::Union(
            items
                .iter()
                .map(|item| normalize_type_ref(item, aliases))
                .collect(),
        ),
        TypeRef::Enum(fields) => TypeRef::Enum(
            fields
                .iter()
                .map(|(name, ty)| (name.clone(), normalize_type_ref(ty, aliases)))
                .collect(),
        ),
        TypeRef::Record(fields) => TypeRef::Record(
            fields
                .iter()
                .map(|(name, ty)| (name.clone(), normalize_type_ref(ty, aliases)))
                .collect(),
        ),
        TypeRef::Tuple(items) => TypeRef::Tuple(
            items
                .iter()
                .map(|item| normalize_type_ref(item, aliases))
                .collect(),
        ),
        TypeRef::Array(inner) => TypeRef::Array(Box::new(normalize_type_ref(inner, aliases))),
        TypeRef::Map { key, value } => TypeRef::Map {
            key: Box::new(normalize_type_ref(key, aliases)),
            value: Box::new(normalize_type_ref(value, aliases)),
        },
        TypeRef::Interface(fields) => TypeRef::Interface(
            fields
                .iter()
                .map(|(name, ty)| (name.clone(), normalize_type_ref(ty, aliases)))
                .collect(),
        ),
        TypeRef::Intersect(items) => TypeRef::Intersect(
            items
                .iter()
                .map(|item| normalize_type_ref(item, aliases))
                .collect(),
        ),
        TypeRef::FnType { args, return_type } => TypeRef::FnType {
            args: Box::new(normalize_type_ref(args, aliases)),
            return_type: Box::new(normalize_type_ref(return_type, aliases)),
        },
        TypeRef::Newtype { name, inner } => TypeRef::Newtype {
            name: name.clone(),
            inner: Box::new(normalize_type_ref(inner, aliases)),
        },
        _ => ty.clone(),
    }
}

pub(crate) fn unify_types(
    expected: &TypeRef,
    actual: &TypeRef,
    aliases: &HashMap<String, TypeAlias>,
    bindings: &mut HashMap<String, TypeRef>,
) -> bool {
    let expected = normalize_type_ref(expected, aliases);
    let actual = normalize_type_ref(actual, aliases);
    if expected == actual {
        return true;
    }
    if let TypeRef::Generic(name) = &expected {
        if let Some(bound) = bindings.get(name).cloned() {
            return unify_types(&bound, &actual, aliases, bindings);
        }
        bindings.insert(name.clone(), actual);
        return true;
    }
    if let TypeRef::Generic(name) = &actual {
        if let Some(bound) = bindings.get(name).cloned() {
            return unify_types(&expected, &bound, aliases, bindings);
        }
        bindings.insert(name.clone(), expected);
        return true;
    }
    match (&expected, &actual) {
        (TypeRef::Policy(_), TypeRef::Policy(_)) => {
            policy_type_is_subset(&actual, &expected, aliases)
        }
        (TypeRef::Literal(left), TypeRef::Literal(right)) => left == right,
        (expected, TypeRef::Literal(actual)) => literal_fits_primitive(actual, expected),
        (TypeRef::Literal(_), _) => false,
        (
            TypeRef::Instantiated {
                base: left,
                type_args: left_args,
            },
            TypeRef::Instantiated {
                base: right,
                type_args: right_args,
            },
        ) => {
            bare_name(left) == bare_name(right)
                && left_args.len() == right_args.len()
                && left_args
                    .iter()
                    .zip(right_args)
                    .all(|(left, right)| unify_types(left, right, aliases, bindings))
        }
        (TypeRef::Union(options), actual) => options
            .iter()
            .any(|option| unify_types(option, actual, aliases, bindings)),
        (TypeRef::Record(expected), TypeRef::Record(actual))
        | (TypeRef::Interface(expected), TypeRef::Record(actual))
        | (TypeRef::Interface(expected), TypeRef::Interface(actual)) => {
            expected.iter().all(|(name, ty)| {
                actual
                    .get(name)
                    .is_some_and(|actual| unify_types(ty, actual, aliases, bindings))
            })
        }
        (TypeRef::Tuple(left), TypeRef::Tuple(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| unify_types(left, right, aliases, bindings))
        }
        (TypeRef::Array(left), TypeRef::Array(right)) => {
            unify_types(left, right, aliases, bindings)
        }
        (
            TypeRef::Map {
                key: left_key,
                value: left_value,
            },
            TypeRef::Map {
                key: right_key,
                value: right_value,
            },
        ) => {
            unify_types(left_key, right_key, aliases, bindings)
                && unify_types(left_value, right_value, aliases, bindings)
        }
        (TypeRef::Intersect(parts), actual) => parts
            .iter()
            .all(|part| unify_types(part, actual, aliases, bindings)),
        (
            TypeRef::FnType {
                args: left_args,
                return_type: left_return,
            },
            TypeRef::FnType {
                args: right_args,
                return_type: right_return,
            },
        ) => {
            unify_types(left_args, right_args, aliases, bindings)
                && unify_types(left_return, right_return, aliases, bindings)
        }
        (left, right) if is_numeric_type(left) && is_numeric_type(right) => true,
        (TypeRef::Named(left), TypeRef::Named(right)) => bare_name(left) == bare_name(right),
        (TypeRef::Enum(left), TypeRef::Enum(right)) => left == right,
        _ => false,
    }
}

pub(crate) fn literal_fits_primitive(literal: &LiteralType, primitive: &TypeRef) -> bool {
    match (literal, primitive) {
        (LiteralType::Bool(_), TypeRef::Bool) | (LiteralType::Str(_), TypeRef::Str) => true,
        (LiteralType::Int(_), primitive) if is_numeric_type(primitive) => true,
        (LiteralType::Float(_), TypeRef::Float32 | TypeRef::Float64) => true,
        _ => false,
    }
}

pub(crate) fn is_numeric_type(ty: &TypeRef) -> bool {
    matches!(
        ty,
        TypeRef::Int8
            | TypeRef::Int16
            | TypeRef::Int32
            | TypeRef::Int64
            | TypeRef::UInt8
            | TypeRef::UInt16
            | TypeRef::UInt32
            | TypeRef::UInt64
            | TypeRef::Float32
            | TypeRef::Float64
    )
}

pub(crate) fn type_compatible(
    expected: &TypeRef,
    actual: &TypeRef,
    aliases: &HashMap<String, TypeAlias>,
) -> bool {
    match (expected, actual) {
        (TypeRef::Mutable(expected), TypeRef::Mutable(actual)) => {
            return type_compatible(expected, actual, aliases);
        }
        (
            TypeRef::Reference {
                inner: expected,
                mutable: expected_mutable,
            },
            TypeRef::Reference {
                inner: actual,
                mutable: actual_mutable,
            },
        ) => {
            return expected_mutable == actual_mutable
                && type_compatible(expected, actual, aliases);
        }
        (expected, TypeRef::Mutable(actual)) => {
            return type_compatible(expected, actual, aliases);
        }
        _ => {}
    }
    if policy_body(expected, aliases).is_some() && policy_body(actual, aliases).is_some() {
        return policy_type_is_subset(expected, actual, aliases);
    }
    unify_types(expected, actual, aliases, &mut HashMap::new())
}

pub(crate) fn newtype_inner<'a>(
    ty: &'a TypeRef,
    aliases: &'a HashMap<String, TypeAlias>,
) -> Option<&'a TypeRef> {
    match ty {
        TypeRef::Named(name) => aliases.get(name).and_then(newtype_alias_inner),
        TypeRef::Instantiated { base, .. } => aliases.get(base).and_then(newtype_alias_inner),
        TypeRef::Newtype { inner, .. } => Some(inner),
        _ => None,
    }
}

fn newtype_alias_inner(alias: &TypeAlias) -> Option<&TypeRef> {
    match &alias.body {
        TypeRef::Newtype { inner, .. } => Some(inner),
        _ => None,
    }
}

pub(crate) fn crosses_newtype_boundary(
    expected: &TypeRef,
    actual: &TypeRef,
    aliases: &HashMap<String, TypeAlias>,
) -> bool {
    match (
        newtype_inner(expected, aliases),
        newtype_inner(actual, aliases),
    ) {
        (Some(inner), None) => type_compatible(inner, actual, aliases),
        (None, Some(inner)) => type_compatible(expected, inner, aliases),
        _ => false,
    }
}

pub(crate) fn valid_cast_path(
    source: &TypeRef,
    target: &TypeRef,
    aliases: &HashMap<String, TypeAlias>,
) -> bool {
    if policy_body(target, aliases).is_some()
        || capability_body(target, aliases).is_some()
        || matches!(resolve_alias_type(target, aliases), TypeRef::HostHandle(_))
    {
        return false;
    }
    newtype_inner(target, aliases).is_some_and(|inner| type_compatible(inner, source, aliases))
        || newtype_inner(source, aliases)
            .is_some_and(|inner| type_compatible(target, inner, aliases))
}

pub(crate) fn resolve_alias_type<'a>(
    ty: &'a TypeRef,
    aliases: &'a HashMap<String, TypeAlias>,
) -> &'a TypeRef {
    match ty {
        TypeRef::Named(name) => aliases.get(name).map(|alias| &alias.body).unwrap_or(ty),
        _ => ty,
    }
}

pub(crate) fn policy_body<'a>(
    ty: &'a TypeRef,
    aliases: &'a HashMap<String, TypeAlias>,
) -> Option<&'a PolicyType> {
    match resolve_alias_type(ty, aliases) {
        TypeRef::Policy(policy) => Some(policy),
        _ => None,
    }
}

pub(crate) fn capability_body<'a>(
    ty: &'a TypeRef,
    aliases: &'a HashMap<String, TypeAlias>,
) -> Option<&'a CapabilityType> {
    match resolve_alias_type(ty, aliases) {
        TypeRef::Capability(capability) => Some(capability),
        _ => None,
    }
}

pub(crate) fn policy_type_is_subset(
    source: &TypeRef,
    target: &TypeRef,
    aliases: &HashMap<String, TypeAlias>,
) -> bool {
    let Some(source) = policy_body(source, aliases) else {
        return false;
    };
    if let Some(target) = capability_body(target, aliases) {
        let Some(source_groups) = source.domains.get(&target.domain) else {
            return false;
        };
        return target.groups.is_empty()
            || target.groups.iter().all(|target_group| {
                source_groups
                    .iter()
                    .any(|source_group| policy_group_covers(source_group, target_group))
            });
    }
    let Some(target) = policy_body(target, aliases) else {
        return false;
    };
    target.domains.iter().all(|(domain, target_groups)| {
        source.domains.get(domain).is_some_and(|source_groups| {
            target_groups.iter().all(|target_group| {
                source_groups
                    .iter()
                    .any(|source_group| policy_group_covers(source_group, target_group))
            })
        })
    })
}

fn policy_group_covers(source: &PolicyGroup, target: &PolicyGroup) -> bool {
    source
        .scopes
        .iter()
        .any(|scope| matches!(scope, PolicyScope::Any))
        || target.scopes.iter().all(|target_scope| {
            source
                .scopes
                .iter()
                .any(|source_scope| policy_scope_covers(source_scope, target_scope))
        })
}

fn policy_scope_covers(source: &PolicyScope, target: &PolicyScope) -> bool {
    match (source, target) {
        (PolicyScope::Any, _) => true,
        (PolicyScope::Dir(source), PolicyScope::Dir(target))
        | (PolicyScope::Dir(source), PolicyScope::File(target)) => {
            path_scope_covers(source, target)
        }
        (PolicyScope::File(source), PolicyScope::File(target))
        | (PolicyScope::Exact(source), PolicyScope::Exact(target)) => source == target,
        (PolicyScope::Prefix(source), PolicyScope::Exact(target))
        | (PolicyScope::Prefix(source), PolicyScope::Prefix(target)) => target.starts_with(source),
        _ => false,
    }
}

fn path_scope_covers(source: &str, target: &str) -> bool {
    normalize_policy_path(target).starts_with(normalize_policy_path(source))
}

fn normalize_policy_path(path: &str) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in Path::new(path).components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn bare_name(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lower::{CapabilityDomain, PolicyRequirement};
    use std::collections::BTreeMap;

    fn alias(name: &str, body: TypeRef) -> TypeAlias {
        TypeAlias {
            alias: String::new(),
            name: name.into(),
            type_params: Vec::new(),
            type_param_bounds: Vec::new(),
            body,
            doc: None,
        }
    }

    #[test]
    fn substitutions_cover_typed_mutability_references_and_handles() {
        let ty = TypeRef::Reference {
            mutable: true,
            inner: Box::new(TypeRef::JoinHandle(Box::new(TypeRef::Generic("t".into())))),
        };
        let substitutions = HashMap::from([("t".into(), TypeRef::SelfType)]);
        assert_eq!(
            substitute(&ty, &substitutions, Some(&TypeRef::Int64)),
            TypeRef::Reference {
                mutable: true,
                inner: Box::new(TypeRef::JoinHandle(Box::new(TypeRef::Int64))),
            }
        );
    }

    #[test]
    fn aliases_literals_newtypes_and_casts_share_one_relation() {
        let aliases = HashMap::from([
            ("count".into(), alias("count", TypeRef::Int64)),
            (
                "user-id".into(),
                alias(
                    "user-id",
                    TypeRef::Newtype {
                        name: "user-id".into(),
                        inner: Box::new(TypeRef::Int64),
                    },
                ),
            ),
        ]);
        assert!(type_compatible(
            &TypeRef::Named("count".into()),
            &TypeRef::Literal(LiteralType::Int(1)),
            &aliases
        ));
        assert!(crosses_newtype_boundary(
            &TypeRef::Named("user-id".into()),
            &TypeRef::Int64,
            &aliases
        ));
        assert!(valid_cast_path(
            &TypeRef::Int64,
            &TypeRef::Named("user-id".into()),
            &aliases
        ));
    }

    #[test]
    fn policy_subset_handles_aliases_and_normalized_paths() {
        let group = |scope| PolicyGroup {
            requirement: PolicyRequirement::Mandatory,
            scopes: vec![scope],
        };
        let source = TypeRef::Policy(PolicyType {
            domains: BTreeMap::from([(
                CapabilityDomain::FsRead,
                vec![group(PolicyScope::Dir("workspace/./src".into()))],
            )]),
        });
        let target = TypeRef::Policy(PolicyType {
            domains: BTreeMap::from([(
                CapabilityDomain::FsRead,
                vec![group(PolicyScope::File("workspace/src/lib.vibra".into()))],
            )]),
        });
        let aliases = HashMap::from([("read-policy".into(), alias("read-policy", source))]);
        assert!(policy_type_is_subset(
            &TypeRef::Named("read-policy".into()),
            &target,
            &aliases
        ));
        assert!(!policy_type_is_subset(
            &target,
            &TypeRef::Named("read-policy".into()),
            &aliases
        ));
    }
}
