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
//! A member bound with `external` exists only in the embedded library, whose
//! loader checks each binding against its registry row
//! (`crate::stdlib::check_externals`): a `proc` member bound with
//! `external @host` has no body and no `performs`, so its complete row is
//! `{owner}`, and it lowers to `FunctionBody::Host`; the narrowing members
//! `restrict`, `sub`, and `limit` are bound with `external @compiler` to the
//! rows `grant.*` at the grant type `Self`. Both bindings are the
//! implementation of a sealed grant only (`docs/spec/03-effects.md`,
//! "Operation interfaces"): each is the default function of its member, which
//! dispatch on an interface value selects, and for an `impl` the member is
//! abstract, so an implementation supplies it (`@type.missingAbstractMember`
//! otherwise) and supplying it is no `@type.defaultOverride`.
//!
//! A contract `proc` member named as a value has its `Proc` type at the
//! substitution its receiver selects; the closure it becomes awaits the
//! member's plan when it runs, so applying the closure builds a plan of the
//! same row.

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::external::CompilerIntrinsic;
use vibra_ir::external::host::{host_row, test_world_row};
use vibra_ir::{
    BodyContext, CheckedFunction, Expr as IrExpr, FnType, FunctionBody, HostBinding,
    IntentType, LibraryTypes, Node, Row, SignatureRecord, SourceOrigin, Type,
};
use vibra_syntax::source::{Callable, CallableKind};

use super::values::proc_type;
use super::{Hook, rows};
use crate::check::declare::{FnData, InterfaceMemberData, Program};
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

/// The type of the contract member `member` of `interface` named as a
/// value, whose application has the signature `signature` ([`applied`]):
/// `Proc` with the row of the plan its application builds for a `proc`,
/// `Fn` for a `fn`. Naming it builds no plan and performs nothing.
pub(crate) fn named_as_value(
    checker: &BodyChecker<'_, '_>,
    interface: usize,
    member: &str,
    signature: FnType,
) -> Type {
    let is_proc = checker
        .program()
        .interfaces
        .get(interface)
        .and_then(|data| data.member(member))
        .is_some_and(|declared| declared.row.is_some());
    match &signature.result {
        Type::Intent(plan) if is_proc => {
            let IntentType { result, effects } = plan.as_ref().clone();
            proc_type(
                FnType {
                    result,
                    ..signature
                },
                effects,
            )
        }
        _ => Type::function(signature),
    }
}

/// The body of the closure of type `ty` a contract member named as a value
/// becomes, given `call`, the member's application to the closure's own
/// arguments: a procedure value awaits the plan `call` builds when it runs,
/// and a function value calls the member in tail position.
pub(crate) fn closure_body(
    ty: &Type,
    call: Node,
    tail: impl FnOnce(&mut Node),
) -> Node {
    if let Type::Proc(_) = ty {
        let origin = call.origin.clone();
        return Node::new(
            IrExpr::Await {
                plan: Box::new(call),
            },
            origin,
        );
    }
    let mut call = call;
    tail(&mut call);
    call
}

/// The signature of the callable value of type `ty`: its parameters, labels,
/// and result, the result of the run for a procedure value.
pub(crate) fn value_signature(ty: &Type) -> Option<FnType> {
    match ty {
        Type::Fn(callable) => Some(callable.as_ref().clone()),
        Type::Proc(callable) => Some(FnType {
            parameters: callable.parameters.clone(),
            labels: callable.labels.clone(),
            result: callable.result.clone(),
        }),
        _ => None,
    }
}

/// Whether the interface member `member` is bound with `external`, so that
/// it has a function holding its binding although it has no body.
pub(crate) fn bound(member: &Callable<'_>) -> bool {
    member.external().is_some()
}

/// The `@compiler` row the interface member `member` binds, if any.
pub(crate) fn bound_intrinsic(member: &Callable<'_>) -> Option<CompilerIntrinsic> {
    let provider = member.external()?.tag()?;
    if provider.text() != "@compiler" {
        return None;
    }
    CompilerIntrinsic::from_symbol(&member.symbol()?.value()?)
}

/// Whether the default function `default` of an interface member is a
/// binding, the implementation of a sealed grant only: for an `impl` such a
/// member is abstract.
pub(crate) fn sealed(program: &Program<'_>, default: Option<usize>) -> bool {
    default
        .and_then(|function| program.functions.get(function))
        .is_some_and(|data| data.contract.is_some() && bound(&data.view))
}

/// The type arguments of the `@compiler` row `intrinsic` that the function
/// `data` binds: its binders, and for a narrowing row bound in an interface
/// the grant type, `Self`.
pub(crate) fn type_arguments(
    data: &FnData<'_>,
    intrinsic: CompilerIntrinsic,
) -> Vec<Type> {
    let narrowing = matches!(
        intrinsic,
        CompilerIntrinsic::GrantRestrict
            | CompilerIntrinsic::GrantSub
            | CompilerIntrinsic::GrantLimit
    );
    if narrowing && data.contract.is_some() {
        return vec![Type::SelfType];
    }
    (0..data.binders.len()).map(Type::Binder).collect()
}

/// The function `data` lowered when it is the default function of an
/// interface member bound with `external @host`: its body is the binding,
/// its signature record `record`, its labelled defaults `defaults`, and
/// its context the one of its `Proc` type. `None` for any other function.
pub(crate) fn host_function(
    data: &FnData<'_>,
    record: SignatureRecord,
    defaults: Vec<(String, vibra_ir::Node)>,
    origin: SourceOrigin,
) -> Option<CheckedFunction> {
    if data.contract.is_none() || data.view.external()?.tag()?.text() != "@host" {
        return None;
    }
    let symbol = data.view.symbol()?.value()?;
    let binding = match (host_row(&symbol), test_world_row(&symbol)) {
        (Some(row), _) => HostBinding::Operation(row.op),
        (None, Some(row)) => HostBinding::TestWorld(row.op),
        (None, None) => return None,
    };
    let slot_count = data.signature.as_ref().map_or(0, FnType::slot_count);
    Some(CheckedFunction {
        name: data.qualified.clone(),
        signature: record,
        defaults,
        body: FunctionBody::Host(binding),
        entry_guard: None,
        exit_guard: None,
        slot_count,
        origin,
        context: BodyContext::Pure,
    })
}
/// `library` with each narrowing type the grant rows name bound to the
/// declaration of that name in its module, where that module is in scope
/// (`docs/spec/03-effects.md`, "Narrowing").
pub(crate) fn bind_narrowing(
    program: &Program<'_>,
    mut library: LibraryTypes,
    in_scope: &dyn Fn(usize) -> bool,
) -> LibraryTypes {
    for narrowing in crate::stdlib::NARROWING_TYPES {
        let id = program
            .types
            .iter()
            .filter(|data| in_scope(data.module) && data.name == narrowing.name())
            .find(|data| {
                program.modules.get(data.module).is_some_and(|module| {
                    module.embedded && module.path == narrowing.module()
                })
            })
            .and_then(|data| data.id.clone());
        if let Some(id) = id {
            library = library.with_narrowing(narrowing, id);
        }
    }
    library
}
