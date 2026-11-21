//! Grant types: a value of an operation interface type, and every type that
//! holds one, is neither equatable nor a dict key.
//!
//! `docs/spec/03-effects.md`, "Capability values". Step 22 fills this file:
//! until then no type the checker accepts holds a grant, so neither hook
//! finds a fault.

use vibra_diagnostics::DiagnosticCode;
use vibra_ir::Type;

use crate::check::declare::Program;

/// Why `key`, which holds no callable, cannot key a dict because it holds
/// a grant, when it cannot. [`Program::key_fault`] asks it before it looks
/// for an `Ordered` implementation.
pub(crate) fn check_dict_key(
    _program: &Program<'_>,
    _key: &Type,
) -> Option<DiagnosticCode> {
    None
}

/// Whether an operand of type `operand` of `assert.equal` holds a grant, so
/// that it is `@type.functionNotEquatable` as an operand that holds a
/// callable is.
pub(crate) fn check_equatable(_program: &Program<'_>, _operand: &Type) -> bool {
    false
}
