//! Grant types: a value of an operation interface type, and every type that
//! holds one, is neither equatable nor a dict key.
//!
//! `docs/spec/03-effects.md`, "Capability values": the type of a grant is
//! the operation interface it permits, and a grant has no encoding, so it
//! "cannot be written to a file, compared, or used as a dict key". The
//! runtime chapter, "What has no encoding", rejects a type statically known
//! to contain one wherever an encoding is required: an operand of
//! `assert.equal` (`@type.functionNotEquatable`, as for a callable), a dict
//! key (`@type.invalidDictKey`), and a labelled default, which is a constant
//! and so has an encoding (`@type.notConstant`, even for `Option.none()`).
//! The checked IR keeps the same rule for constants and keys (`vibra-ir`,
//! `constant.rs`), and no checked program reaches its refusal.
//!
//! The rest of what keeps a grant sealed needs no rule here: an interface
//! value type has no constructor, viewing a value as an interface makes no
//! grant, and a brand is a declared type with a private representation.

use std::collections::BTreeSet;

use vibra_diagnostics::DiagnosticCode;
use vibra_ir::{Type, TypeId};

use crate::check::declare::{Program, TypeKind};
use crate::check::effects::rows;

/// Whether `ty` is, or holds, an operation interface value type: through a
/// record, a tuple, a list, `Option`, a wrapper, or the representation of
/// any declared type, and through the type arguments of a declared type.
///
/// The walk keeps an explicit work list, and reads each declared type's
/// representation once, so a recursive type ends the walk. A callable type
/// is not entered: it is not equatable and no dict key on its own account.
pub(crate) fn contains_grant(program: &Program<'_>, ty: &Type) -> bool {
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
            Type::Fn(_) | Type::Proc(_) | Type::Intent(_) => {}
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

/// Why `key`, which holds no callable, cannot key a dict because it holds
/// a grant, when it cannot: `@type.invalidDictKey` at the key type.
/// [`Program::key_fault`] asks it before it looks for an `Ordered`
/// implementation, so a declared type over a grant is refused even when it
/// implements `Ordered`.
pub(crate) fn check_dict_key(
    program: &Program<'_>,
    key: &Type,
) -> Option<DiagnosticCode> {
    contains_grant(program, key).then_some(DiagnosticCode::TypeInvalidDictKey)
}

/// Whether a labelled default of type `ty` holds a grant, so that it is
/// `@type.notConstant` at the default whatever its value: a constant has an
/// encoding, and a type statically known to contain a grant has none.
pub(crate) fn check_constant(program: &Program<'_>, ty: &Type) -> bool {
    contains_grant(program, ty)
}

/// Whether an operand of type `operand` of `assert.equal` holds a grant, so
/// that it is `@type.functionNotEquatable` as an operand that holds a
/// callable is.
pub(crate) fn check_equatable(program: &Program<'_>, operand: &Type) -> bool {
    contains_grant(program, operand)
}
