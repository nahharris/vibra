//! Named procedures, the plans their application builds, `await`, and the
//! headers of tests.
//!
//! `docs/spec/01-source-language.md`, "`fn` and `proc`" and "`return`,
//! `try`, `await`"; `docs/spec/03-effects.md`, "Cold plans and performed
//! rows"; `docs/spec/04-programs-and-packages.md`, "Tests". Step 05 fills
//! the hooks of a `proc` and of `await`, step 07 and step 21 those of a
//! test, and step 08 reads a non-empty row through [`super::rows`]. Until
//! then a `proc`, a member with a row, and a test that takes a world or
//! performs are `@tool.unavailable` over the declaration, and every `await`
//! is `@effect.invalidAwait`.

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::{BodyContext, FnType, Node, ProcType, Type};
use vibra_syntax::source::{Callable, CallableKind, Declaration, Expr};

use crate::check::declare::{
    BoundData, Entity, FnData, NoType, Program, Scope, TestData,
};
use crate::check::expr::{BodyChecker, Callee, Ctx, Out, Step};

/// The header of a callable: its signature and the bound of each of its
/// binders.
pub(crate) type Header = (FnType, Vec<Option<BoundData>>);

/// Declares the name of the module-level `proc` `callable` of `module`.
pub(crate) fn declare<'a>(
    program: &mut Program<'a>,
    module: usize,
    callable: Callable<'a>,
) {
    if let Some(name) = callable.name() {
        let span = callable.construct().span();
        program.declare(module, name.text(), Entity::Other(module, span));
    }
}

/// Reads the header of every module-level `proc`, once the header of every
/// `fn` is read.
pub(crate) fn module_headers(program: &mut Program<'_>) {
    for module in 0..program.modules.len() {
        let Some(view) = program.modules.get(module).and_then(|data| data.view) else {
            continue;
        };
        for declaration in view.declarations() {
            match declaration {
                Declaration::Proc(callable) => {
                    let what = format!(
                        "the procedure `{}`",
                        callable.name().map(|name| name.text()).unwrap_or_default()
                    );
                    program.unsupported(module, callable.construct().span(), &what);
                }
                Declaration::Test(_)
                | Declaration::Interface(_)
                | Declaration::Use(_)
                | Declaration::Type(_)
                | Declaration::Fn(_)
                | Declaration::Let(_) => {}
            }
        }
    }
}

/// The header of the callable `data` read in `scope`, when it is a `proc`
/// or has a row; `None` for a `fn` with no row, whose header the shared
/// checker reads.
pub(crate) fn header(
    _program: &Program<'_>,
    data: &FnData<'_>,
    _scope: Scope<'_>,
) -> Option<Result<Header, NoType>> {
    (data.view.performs().is_some() || data.view.kind() == CallableKind::Proc)
        .then_some(Err(NoType::Unsupported))
}

/// How a diagnostic or a note names the callable `data`.
pub(crate) fn describe(data: &FnData<'_>) -> String {
    match data.view.kind() {
        CallableKind::Proc => format!("the procedure `{}`", data.qualified),
        CallableKind::Fn => format!("the function `{}`", data.qualified),
    }
}

/// The callee of an application whose callee is the value `callee` of the
/// `Proc` type `signature`, written at `callee_span`.
pub(crate) fn apply(
    checker: &mut BodyChecker<'_, '_>,
    _callee: Option<Node>,
    signature: &ProcType,
    callee_span: ByteSpan,
) -> Option<Callee> {
    let ty = Type::Proc(Box::new(signature.clone()));
    checker.report(
        DiagnosticCode::TypeNotApplicable,
        callee_span,
        format!("a value of type `{ty}` cannot be applied"),
    );
    None
}

/// The step of `await operand`, written at `span` where `ctx` uses its
/// value.
pub(crate) fn await_expr<'a>(
    checker: &mut BodyChecker<'_, 'a>,
    _operand: Expr<'a>,
    span: ByteSpan,
    _ctx: &Ctx,
) -> Step<'a> {
    checker.report(
        DiagnosticCode::EffectInvalidAwait,
        span,
        "`await` is valid only in a `proc`",
    );
    Step::Done(Out::error())
}

/// The context the lowered body of the callable `data` runs in.
pub(crate) fn lower(_program: &Program<'_>, _data: &FnData<'_>) -> BodyContext {
    BodyContext::Pure
}

/// The `@tool.unavailable` of the test `test`, with no source, when its
/// header is not supported; `None` when its body is checked.
pub(crate) fn test_header(test: &TestData<'_>) -> Option<Diagnostic> {
    if test.view.takes().is_none() && test.view.performs().is_none() {
        return None;
    }
    Some(Diagnostic::new(
        DiagnosticCode::ToolUnavailable,
        test.view.construct().span(),
        "a test that takes a world or performs operations is not supported \
         by this checker yet",
    ))
}

/// The context the lowered body of the test `test` runs in.
pub(crate) fn test_context(_test: &TestData<'_>) -> BodyContext {
    BodyContext::Pure
}
