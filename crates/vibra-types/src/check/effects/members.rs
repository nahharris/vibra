//! Members with rows: a `proc` member of an interface, which makes it an
//! operation interface, and a `proc` member of an implementation, which
//! keeps its contract member's row.
//!
//! `docs/spec/03-effects.md`, "Operation interfaces", and
//! `docs/spec/02-type-system.md`, "Implementations". Step 08 fills this
//! file and step 19 adds the rule of a host-bound member: until then a
//! `proc` member and a member with a row are `@tool.unavailable` over the
//! member.

use vibra_syntax::source::{Callable, CallableKind};

use super::Hook;
use crate::check::declare::Program;

/// Reads the member `member` of an interface of `module`, described as
/// `what`, when it is a `proc` or has a row.
/// [`Program::interface_headers`] hands it every member before it reads
/// the member's header.
pub(crate) fn contract_member(
    program: &mut Program<'_>,
    module: usize,
    member: &Callable<'_>,
    what: &str,
) -> Hook {
    if member.performs().is_none() && member.kind() == CallableKind::Fn {
        return Hook::Continue;
    }
    program.unsupported(module, member.construct().span(), what);
    Hook::Reported
}

/// Reads the member `member` of an implementation in `module`, described as
/// `what`, when it is a `proc` or has a row. The implementation hands it
/// every member that supplies a contract member and has no guard, before it
/// reads the member's header.
pub(crate) fn impl_member(
    program: &mut Program<'_>,
    module: usize,
    member: &Callable<'_>,
    what: &str,
) -> Hook {
    if member.performs().is_none() && member.kind() == CallableKind::Fn {
        return Hook::Continue;
    }
    program.unsupported(module, member.construct().span(), what);
    Hook::Reported
}
