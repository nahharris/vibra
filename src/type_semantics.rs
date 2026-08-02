//! Frontend-neutral semantic relations over lowered Vibra types.
//!
//! This module deliberately depends only on semantic IR types. Source readers
//! (including the legacy YAML reader) must lower into [`TypeRef`] before using
//! these relations.

use crate::lower::{FunctionTypeParameter, LiteralType, TypeAlias, TypeRef};
use std::collections::{BTreeSet, HashMap};

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
        TypeRef::FnType {
            parameters,
            return_type,
            effects,
        } => TypeRef::FnType {
            parameters: parameters
                .iter()
                .map(|parameter| FunctionTypeParameter {
                    name: parameter.name.clone(),
                    ty: recurse(&parameter.ty),
                    variadic: parameter.variadic,
                })
                .collect(),
            return_type: Box::new(recurse(return_type)),
            effects: effects.clone(),
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

/// Normalize `ty` by inlining non-generic, non-newtype aliases.
///
/// Guards against directly or mutually self-referential aliases (a type
/// alias, not wrapped in `Newtype`, whose body eventually names itself
/// again) by tracking which alias names are currently being expanded on
/// this call stack. Re-entering an alias that is already being expanded
/// stops the inlining for that occurrence and returns it unexpanded,
/// rather than recursing without bound.
pub(crate) fn normalize_type_ref(ty: &TypeRef, aliases: &HashMap<String, TypeAlias>) -> TypeRef {
    normalize_type_ref_guarded(ty, aliases, &mut BTreeSet::new())
}

fn normalize_type_ref_guarded(
    ty: &TypeRef,
    aliases: &HashMap<String, TypeAlias>,
    visiting: &mut BTreeSet<String>,
) -> TypeRef {
    match ty {
        TypeRef::Named(name) => match aliases.get(name) {
            Some(alias)
                if alias.type_params.is_empty()
                    && !matches!(alias.body, TypeRef::Newtype { .. }) =>
            {
                if !visiting.insert(name.clone()) {
                    return ty.clone();
                }
                let normalized = normalize_type_ref_guarded(&alias.body, aliases, visiting);
                visiting.remove(name);
                normalized
            }
            _ => ty.clone(),
        },
        TypeRef::Instantiated { base, type_args } => {
            let args: Vec<_> = type_args
                .iter()
                .map(|arg| normalize_type_ref_guarded(arg, aliases, visiting))
                .collect();
            match aliases.get(base) {
                Some(alias)
                    if !matches!(alias.body, TypeRef::Newtype { .. })
                        && alias.type_params.len() == args.len() =>
                {
                    if !visiting.insert(base.clone()) {
                        return TypeRef::Instantiated {
                            base: base.clone(),
                            type_args: args,
                        };
                    }
                    let substitutions = alias
                        .type_params
                        .iter()
                        .cloned()
                        .zip(args.iter().cloned())
                        .collect();
                    let expanded = substitute_type(&alias.body, &substitutions);
                    let normalized = normalize_type_ref_guarded(&expanded, aliases, visiting);
                    visiting.remove(base);
                    normalized
                }
                _ => TypeRef::Instantiated {
                    base: base.clone(),
                    type_args: args,
                },
            }
        }
        TypeRef::Mutable(inner) => TypeRef::Mutable(Box::new(normalize_type_ref_guarded(
            inner, aliases, visiting,
        ))),
        TypeRef::Reference { inner, mutable } => TypeRef::Reference {
            inner: Box::new(normalize_type_ref_guarded(inner, aliases, visiting)),
            mutable: *mutable,
        },
        TypeRef::JoinHandle(inner) => TypeRef::JoinHandle(Box::new(normalize_type_ref_guarded(
            inner, aliases, visiting,
        ))),
        TypeRef::Union(items) => TypeRef::Union(
            items
                .iter()
                .map(|item| normalize_type_ref_guarded(item, aliases, visiting))
                .collect(),
        ),
        TypeRef::Enum(fields) => TypeRef::Enum(
            fields
                .iter()
                .map(|(name, ty)| {
                    (
                        name.clone(),
                        normalize_type_ref_guarded(ty, aliases, visiting),
                    )
                })
                .collect(),
        ),
        TypeRef::Record(fields) => TypeRef::Record(
            fields
                .iter()
                .map(|(name, ty)| {
                    (
                        name.clone(),
                        normalize_type_ref_guarded(ty, aliases, visiting),
                    )
                })
                .collect(),
        ),
        TypeRef::Tuple(items) => TypeRef::Tuple(
            items
                .iter()
                .map(|item| normalize_type_ref_guarded(item, aliases, visiting))
                .collect(),
        ),
        TypeRef::Array(inner) => TypeRef::Array(Box::new(normalize_type_ref_guarded(
            inner, aliases, visiting,
        ))),
        TypeRef::Map { key, value } => TypeRef::Map {
            key: Box::new(normalize_type_ref_guarded(key, aliases, visiting)),
            value: Box::new(normalize_type_ref_guarded(value, aliases, visiting)),
        },
        TypeRef::Interface(fields) => TypeRef::Interface(
            fields
                .iter()
                .map(|(name, ty)| {
                    (
                        name.clone(),
                        normalize_type_ref_guarded(ty, aliases, visiting),
                    )
                })
                .collect(),
        ),
        TypeRef::Intersect(items) => TypeRef::Intersect(
            items
                .iter()
                .map(|item| normalize_type_ref_guarded(item, aliases, visiting))
                .collect(),
        ),
        TypeRef::FnType {
            parameters,
            return_type,
            effects,
        } => TypeRef::FnType {
            parameters: parameters
                .iter()
                .map(|parameter| FunctionTypeParameter {
                    name: parameter.name.clone(),
                    ty: normalize_type_ref_guarded(&parameter.ty, aliases, visiting),
                    variadic: parameter.variadic,
                })
                .collect(),
            return_type: Box::new(normalize_type_ref_guarded(return_type, aliases, visiting)),
            effects: effects.clone(),
        },
        TypeRef::Newtype { name, inner } => TypeRef::Newtype {
            name: name.clone(),
            inner: Box::new(normalize_type_ref_guarded(inner, aliases, visiting)),
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
    if let TypeRef::HostHandle(expected_access) = &expected {
        if let TypeRef::HostHandle(actual_access) = &actual {
            return host_access_widens(*actual_access, *expected_access);
        }
        if let Some(inner) = newtype_inner(&actual, aliases) {
            if let TypeRef::HostHandle(actual_access) = inner {
                return host_access_widens(*actual_access, *expected_access);
            }
        }
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
                parameters: left_parameters,
                return_type: left_return,
                effects: left_effects,
            },
            TypeRef::FnType {
                parameters: right_parameters,
                return_type: right_return,
                effects: right_effects,
            },
        ) => {
            left_parameters.len() == right_parameters.len()
                && left_parameters
                    .iter()
                    .zip(right_parameters)
                    .all(|(left, right)| {
                        left.name == right.name
                            && left.variadic == right.variadic
                            && unify_types(&left.ty, &right.ty, aliases, bindings)
                    })
                && unify_types(left_return, right_return, aliases, bindings)
                // Rows compare by equality here. Subsumption is directional and
                // belongs to the interface/impl check, not to unification.
                && left_effects == right_effects
        }
        (TypeRef::Named(left), TypeRef::Named(right)) => bare_name(left) == bare_name(right),
        (TypeRef::Enum(left), TypeRef::Enum(right)) => {
            left.len() == right.len()
                && left.iter().all(|(name, ty)| {
                    right
                        .get(name)
                        .is_some_and(|actual| unify_types(ty, actual, aliases, bindings))
                })
        }
        _ => false,
    }
}

pub(crate) fn literal_fits_primitive(literal: &LiteralType, primitive: &TypeRef) -> bool {
    match (literal, primitive) {
        (LiteralType::Atom(_), TypeRef::Atom) => true,
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
    if matches!(resolve_alias_type(target, aliases), TypeRef::HostHandle(_))
        || host_backed_newtype(source, aliases)
        || host_backed_newtype(target, aliases)
    {
        return false;
    }
    newtype_inner(target, aliases).is_some_and(|inner| type_compatible(inner, source, aliases))
        || newtype_inner(source, aliases)
            .is_some_and(|inner| type_compatible(target, inner, aliases))
}

pub(crate) fn host_backed_newtype(ty: &TypeRef, aliases: &HashMap<String, TypeAlias>) -> bool {
    newtype_inner(ty, aliases)
        .is_some_and(|inner| matches!(resolve_alias_type(inner, aliases), TypeRef::HostHandle(_)))
}

/// A nominal endpoint may be passed to a trusted generic stream operation only
/// when its capability is at least as strong as the operation's requirement.
/// The relation is intentionally directional: read-write widens to read/write,
/// never the reverse, and unrelated process handles never widen.
fn host_access_widens(
    actual: crate::lower::HandleAccess,
    expected: crate::lower::HandleAccess,
) -> bool {
    use crate::lower::HandleAccess;
    match (actual, expected) {
        (_, HandleAccess::Any) => true,
        (
            HandleAccess::ReadWrite,
            HandleAccess::Read | HandleAccess::Write | HandleAccess::ReadWrite,
        )
        | (HandleAccess::Read, HandleAccess::Read)
        | (HandleAccess::Write, HandleAccess::Write)
        | (HandleAccess::Process, HandleAccess::Process) => true,
        _ => false,
    }
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

fn bare_name(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn atom_singletons_fit_atom_and_only_equal_singletons_unify() {
        let aliases = HashMap::new();
        let ok = TypeRef::Literal(LiteralType::Atom("ok".into()));
        let error = TypeRef::Literal(LiteralType::Atom("error".into()));
        assert!(type_compatible(&TypeRef::Atom, &ok, &aliases));
        assert!(!type_compatible(&ok, &TypeRef::Atom, &aliases));
        assert!(type_compatible(&ok, &ok, &aliases));
        assert!(!type_compatible(&ok, &error, &aliases));
        assert!(!type_compatible(&TypeRef::Str, &ok, &aliases));
        assert!(!type_compatible(
            &TypeRef::Literal(LiteralType::Str("ok".into())),
            &TypeRef::Str,
            &aliases
        ));
        assert!(!type_compatible(
            &TypeRef::Literal(LiteralType::Int(1)),
            &TypeRef::Int64,
            &aliases
        ));
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
    fn self_referential_non_newtype_aliases_terminate_without_expanding() {
        // A directly self-referential alias body that is not wrapped in
        // `Newtype` must not be inlined without bound: absent the cycle
        // guard, this recurses forever and overflows the stack. This
        // exercises the `Named` alias-expansion branch.
        let named_ty = TypeRef::Named("self-ref".into());
        let aliases = HashMap::from([("self-ref".into(), alias("self-ref", named_ty.clone()))]);
        assert_eq!(normalize_type_ref(&named_ty, &aliases), named_ty);

        // Same hazard through the `Instantiated` branch: a generic alias
        // whose body instantiates itself with its own type parameter.
        let mut list_alias = alias(
            "list",
            TypeRef::Instantiated {
                base: "list".into(),
                type_args: vec![TypeRef::Generic("t".into())],
            },
        );
        list_alias.type_params = vec!["t".into()];
        let instantiated_ty = TypeRef::Instantiated {
            base: "list".into(),
            type_args: vec![TypeRef::Int64],
        };
        let aliases = HashMap::from([("list".into(), list_alias)]);
        assert_eq!(
            normalize_type_ref(&instantiated_ty, &aliases),
            instantiated_ty
        );
    }

    #[test]
    fn host_backed_endpoints_widen_only_to_weaker_stream_capabilities() {
        let mut aliases = HashMap::new();
        let endpoint = TypeRef::Newtype {
            name: "fs.reader".into(),
            inner: Box::new(TypeRef::HostHandle(crate::lower::HandleAccess::Read)),
        };
        aliases.insert(
            "fs.reader".into(),
            TypeAlias {
                alias: "fs".into(),
                name: "reader".into(),
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
                body: endpoint.clone(),
                doc: None,
            },
        );
        assert!(unify_types(
            &TypeRef::HostHandle(crate::lower::HandleAccess::Read),
            &endpoint,
            &aliases,
            &mut HashMap::new()
        ));
        assert!(unify_types(
            &TypeRef::HostHandle(crate::lower::HandleAccess::Read),
            &TypeRef::Named("fs.reader".into()),
            &aliases,
            &mut HashMap::new()
        ));
        assert!(!unify_types(
            &TypeRef::HostHandle(crate::lower::HandleAccess::Write),
            &endpoint,
            &aliases,
            &mut HashMap::new()
        ));
        assert!(!valid_cast_path(
            &TypeRef::HostHandle(crate::lower::HandleAccess::Read),
            &endpoint,
            &aliases
        ));
    }
}
