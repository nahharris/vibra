//! Members with rows: a `proc` member of an interface, which makes it an
//! operation interface, and a `proc` member of an implementation, which
//! keeps its contract member's row.
//!
//! `docs/spec/03-effects.md`, "Operation interfaces": the complete row of
//! an operation member is its owning interface plus the member's written
//! `performs`, and writing the owner there is `@effect.duplicateRoot`; a
//! `fn` member of an operation interface is pure. An implementation MUST
//! keep each member's row: under the substitution its member is a `proc`
//! exactly when the contract's is, with the same complete row, or it is
//! `@type.mismatch` (`docs/spec/02-type-system.md`, "Implementations"). A
//! row names no type parameter, so the substitution leaves it as it is.
//!
//! A contract `proc` member has the type `Proc({…} R {Row})`, and an
//! application of it, whatever the receiver, builds a plan of the type
//! `Intent(R {Row})` with the member's complete row ("Cold plans and
//! performed rows": "A plan's type always carries its complete row,
//! including through an interface value").
//!
//! Step 19 adds the rule of a host-bound member: until then a `proc` member
//! bound with `external` is `@tool.unavailable` over the member. Step 10
//! lifts a contract `proc` member named as a value.

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::{FnType, IntentType, ProcType, Row, Type};
use vibra_syntax::source::{Callable, CallableKind};

use super::{Hook, rows};
use crate::check::declare::{InterfaceMemberData, Program};
use crate::check::expr::BodyChecker;

/// Reads the member `member` of the interface `interface` of `module`,
/// described as `what`, before [`Program::interface_headers`] reads its
/// header: a `proc` member's row MUST resolve, without its owner.
pub(crate) fn contract_member(
    program: &mut Program<'_>,
    module: usize,
    interface: usize,
    member: &Callable<'_>,
    what: &str,
) -> Hook {
    match member.kind() {
        CallableKind::Fn if member.performs().is_none() => Hook::Continue,
        CallableKind::Fn => {
            program.unsupported(module, member.construct().span(), what);
            Hook::Reported
        }
        CallableKind::Proc if member.external().is_some() => {
            program.unsupported(module, member.construct().span(), what);
            Hook::Reported
        }
        CallableKind::Proc => {
            match rows::resolve(program, module, member.performs(), Some(interface)) {
                Ok(_) => Hook::Continue,
                Err(why) => {
                    program.no_type_at(module, member.construct().span(), what, why);
                    Hook::Reported
                }
            }
        }
    }
}

/// The complete row of the member `member` of the interface `interface`,
/// whose header [`contract_member`] accepted: its owner and its written
/// row for a `proc`, `None` for a `fn`.
pub(crate) fn contract_row(
    program: &Program<'_>,
    interface: usize,
    member: &Callable<'_>,
) -> Option<Row> {
    if member.kind() != CallableKind::Proc {
        return None;
    }
    let module = program.interfaces.get(interface)?.module;
    rows::resolve(program, module, member.performs(), Some(interface)).ok()
}

/// Reads the member `member` of an implementation of `interface` in
/// `module`, described as `what`, before its header is read. The
/// implementation hands it every member that supplies a contract member
/// and has no guard. A member whose kind or complete row differs from the
/// contract member's is `@type.mismatch`: at the member's `performs`
/// section for a differing row, at its name for a differing kind or a row
/// it omits, related to the contract member.
pub(crate) fn impl_member(
    program: &mut Program<'_>,
    module: usize,
    interface: usize,
    member: &Callable<'_>,
    what: &str,
) -> Hook {
    let (Some(name), Some(data)) = (member.name(), program.interfaces.get(interface))
    else {
        return Hook::Continue;
    };
    let Some(declared) = data.member(name.text()) else {
        return Hook::Continue;
    };
    let contract = declared.row.clone();
    let related = (data.module, declared.span);
    let qualified = format!("{}.{}", data.name, name.text());
    let mismatch = |span: ByteSpan, message: String| {
        Diagnostic::new(DiagnosticCode::TypeMismatch, span, message)
    };
    let diagnostic = match (member.kind(), contract) {
        (CallableKind::Fn, None) if member.performs().is_none() => {
            return Hook::Continue;
        }
        (CallableKind::Fn, None) => {
            program.unsupported(module, member.construct().span(), what);
            return Hook::Reported;
        }
        (CallableKind::Fn, Some(row)) => mismatch(
            name.span(),
            format!(
                "`{qualified}` is a `proc` with the row {row}, so the member that implements it is a `proc` with that row, not a `fn`"
            ),
        ),
        (CallableKind::Proc, None) => mismatch(
            name.span(),
            format!(
                "`{qualified}` is a `fn`, so the member that implements it is a pure `fn`, not a `proc`"
            ),
        ),
        (CallableKind::Proc, Some(_)) if member.external().is_some() => {
            program.unsupported(module, member.construct().span(), what);
            return Hook::Reported;
        }
        (CallableKind::Proc, Some(row)) => {
            match rows::resolve(program, module, member.performs(), Some(interface)) {
                Err(why) => {
                    program.no_type_at(module, member.construct().span(), what, why);
                    return Hook::Reported;
                }
                Ok(written) if written == row => return Hook::Continue,
                Ok(written) => {
                    let span = member.performs().map_or(name.span(), |performs| {
                        rows::section(member.node(), performs)
                    });
                    mismatch(
                        span,
                        format!(
                            "this member performs {written}, and `{qualified}` performs {row}: an implementation keeps its contract member's row"
                        ),
                    )
                }
            }
        }
    };
    let diagnostic =
        program.related(diagnostic, related.0, related.1, "the contract member");
    program.fault(module, diagnostic);
    Hook::Reported
}

/// The kind atom of the identity anchor of the contract member `member`
/// (`docs/spec/02-type-system.md`, "Declared types"). A member reaches its
/// checked interface only when its row resolves, so it has a row exactly
/// when it is a `proc`.
pub(crate) const fn identity_kind(member: &InterfaceMemberData) -> &'static str {
    if member.row.is_some() { "@proc" } else { "@fn" }
}

/// The type of the contract member `member`: `Proc` with its complete row
/// for a `proc`, `Fn` for a `fn`.
pub(crate) fn member_type(member: &InterfaceMemberData) -> Type {
    match &member.row {
        Some(row) => proc_type(member.signature.clone(), row.clone()),
        None => Type::function(member.signature.clone()),
    }
}

/// The signature `signature` of an application of the contract member
/// `member`, the member's own signature under the call's substitution:
/// applying a `proc` member builds a plan, so its result is `Intent(R
/// {Row})` with the member's complete row.
pub(crate) fn applied(member: &InterfaceMemberData, signature: FnType) -> FnType {
    match &member.row {
        Some(row) => FnType {
            result: Type::Intent(Box::new(IntentType {
                result: signature.result,
                effects: row.clone(),
            })),
            ..signature
        },
        None => signature,
    }
}

/// Whether the contract member `member` of `interface`, named as a value at
/// `span`, is a `proc`: it is then reported, since a procedure named as a
/// value is not supported until step 10.
pub(crate) fn named_as_value(
    checker: &mut BodyChecker<'_, '_>,
    interface: usize,
    member: &str,
    span: ByteSpan,
) -> bool {
    let is_proc = checker
        .program()
        .interfaces
        .get(interface)
        .and_then(|data| data.member(member))
        .is_some_and(|declared| declared.row.is_some());
    if is_proc {
        checker.report(
            DiagnosticCode::ToolUnavailable,
            span,
            "a procedure member named as a value is not supported by this checker yet",
        );
    }
    is_proc
}

/// `Proc` over `signature` with the row `effects`.
fn proc_type(signature: FnType, effects: Row) -> Type {
    Type::Proc(Box::new(ProcType {
        parameters: signature.parameters,
        labels: signature.labels,
        result: signature.result,
        effects,
    }))
}
