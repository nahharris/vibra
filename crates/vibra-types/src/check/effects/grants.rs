//! Grant types: a value of an operation interface type, and every type that
//! holds one, has no encoding and so is not `Encodable`.
//!
//! `docs/spec/03-effects.md`, "Capability values": the type of a grant is
//! the operation interface it permits, and a grant has no encoding. The
//! runtime chapter, "What has no encoding", rejects a type statically known
//! to contain one wherever an encoding is required: a type that holds a
//! grant is not `Encodable` (the type chapter, "Interfaces"), so it is no
//! operand of `assert.equal`, and a labelled default, which is a constant
//! and so has an encoding, is `@type.notConstant`, even for
//! `Option.none()`. The checked IR keeps the same rule for constants and
//! for `Encodable` evidence (`vibra-ir`, `constant.rs` and
//! `conformance.rs`), and no checked program reaches its refusal.
//!
//! A dict key needs no rule here: a key is decided by its `Ordered`
//! conformance alone, and a declared type that holds a grant may implement
//! `Ordered`. The rest of what keeps a grant sealed needs no rule either:
//! an interface value type has no constructor, viewing a value as an
//! interface makes no grant, and a brand is a declared type with a private
//! representation.

use std::collections::BTreeSet;

use vibra_ir::{Type, TypeId};

use crate::check::declare::{Program, TypeKind};
use crate::check::effects::rows;

/// Whether `ty` is, or holds, an operation interface value type: through a
/// record, a tuple, a list, `Option`, a wrapper, or the representation of
/// any declared type, and through the type arguments of a declared type.
/// A callable type is not entered: it holds no grant on its own account.
pub(crate) fn contains_grant(program: &Program<'_>, ty: &Type) -> bool {
    holds(program, ty, false)
}

/// Whether `ty` conforms to `Encodable` on its own account, before the open
/// forms in it are asked: no callable or plan type and no grant occurs in
/// it, through the type arguments and the representation of every declared
/// type in it. Each open form in it then conforms only through a bound.
pub(crate) fn is_encodable(program: &Program<'_>, ty: &Type) -> bool {
    !holds(program, ty, true)
}

/// Whether a grant, or with `callables` also a callable or plan type,
/// occurs in `ty`, through the type arguments and the representation of
/// every declared type in it.
///
/// The walk keeps an explicit work list, and reads each declared type's
/// representation once, so a recursive type ends the walk.
fn holds(program: &Program<'_>, ty: &Type, callables: bool) -> bool {
    let mut seen: BTreeSet<&TypeId> = BTreeSet::new();
    let mut pending: Vec<&Type> = vec![ty];
    while let Some(ty) = pending.pop() {
        match ty {
            Type::Interface(id, arguments, choices) => {
                if is_operation_interface(program, id) {
                    return true;
                }
                pending.extend(arguments);
                pending.extend(choices.entries().iter().map(|(_, choice)| choice));
            }
            Type::Fn(_) | Type::Proc(_) | Type::Intent(_) => {
                if callables {
                    return true;
                }
            }
            Type::Declared(id, arguments) => {
                pending.extend(arguments);
                if seen.insert(id) {
                    pending.extend(representation(program, id));
                }
            }
            other => pending.extend(other.components()),
        }
    }
    false
}

/// Whether the interface `id` declares a `proc` member.
fn is_operation_interface(program: &Program<'_>, id: &TypeId) -> bool {
    program
        .interfaces
        .iter()
        .position(|data| data.id.as_ref() == Some(id))
        .is_some_and(|index| rows::is_operation_interface(program, index))
}

/// The types the representation of the declared type `id` is made of: its
/// wrapped type, components, fields, or the fields of its cases. A type
/// parameter in it is a [`Type::Binder`], which holds nothing: the
/// arguments it stands for are walked where the type is applied.
fn representation<'p>(program: &'p Program<'_>, id: &TypeId) -> Vec<&'p Type> {
    let Some(data) = program
        .type_index_of(id)
        .and_then(|index| program.types.get(index))
    else {
        return Vec::new();
    };
    match &data.kind {
        TypeKind::Wrapper(inner) => vec![inner],
        TypeKind::Tuple(components) => components.iter().collect(),
        TypeKind::Record(fields) => fields.iter().map(|(_, field)| field).collect(),
        TypeKind::Variant(cases) => cases
            .iter()
            .flat_map(|case| case.fields.iter().flatten())
            .map(|(_, field)| field)
            .collect(),
        TypeKind::Scalar(_)
        | TypeKind::List
        | TypeKind::Pending
        | TypeKind::Faulty
        | TypeKind::Unsupported => Vec::new(),
    }
}

/// Whether a labelled default of type `ty` holds a grant, so that it is
/// `@type.notConstant` at the default whatever its value: a constant has an
/// encoding, and a type statically known to contain a grant has none.
pub(crate) fn check_constant(program: &Program<'_>, ty: &Type) -> bool {
    contains_grant(program, ty)
}
