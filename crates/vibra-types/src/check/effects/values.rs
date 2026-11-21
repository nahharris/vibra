//! Procedures as values: the written `Proc` and `Intent` types, and the
//! anonymous `proc`.
//!
//! `docs/spec/02-type-system.md`, "Callable types" and "Functions as
//! values", and `docs/spec/03-effects.md`, "Effect ceilings". Step 10 fills
//! this file: until then a written `Proc` or `Intent` type, an anonymous
//! `proc`, and an anonymous callable with a row are `@tool.unavailable`.

use vibra_diagnostics::{ByteSpan, DiagnosticCode};
use vibra_ir::Type;
use vibra_syntax::source::{Callable, CallableKind, TypeForm};

use crate::check::declare::{NoType, Program, Scope, repeated_label};
use crate::check::expr::{BodyChecker, ClosureMode, Ctx, Out, Step};

/// The checked type of the written `Proc` or `Intent` type `form`, read in
/// `module` in `scope`. [`Program::written`] hands it every such form.
///
/// A `Proc` type whose inputs repeat a label is the fault the `Fn` type
/// has; any other is not supported yet.
pub(crate) fn written_type(
    _program: &Program<'_>,
    _module: usize,
    form: TypeForm<'_>,
    _scope: Scope<'_>,
) -> Result<Type, NoType> {
    match form {
        TypeForm::Proc { inputs, .. } => {
            Err(repeated_label(&inputs).unwrap_or(NoType::Unsupported))
        }
        _ => Err(NoType::Unsupported),
    }
}

/// The step of the anonymous callable `view`, written at `span` where
/// `ctx` uses its value, when it is a `proc` or has a row; `None` for an
/// anonymous `fn` with no row, which the shared checker starts.
pub(crate) fn anonymous<'a>(
    checker: &mut BodyChecker<'_, 'a>,
    view: &Callable<'a>,
    span: ByteSpan,
    _ctx: &Ctx,
    _mode: ClosureMode,
) -> Option<Step<'a>> {
    let what = if view.kind() != CallableKind::Fn {
        "an anonymous `proc`"
    } else if view.performs().is_some() {
        "an anonymous callable with a row"
    } else {
        return None;
    };
    checker.report(
        DiagnosticCode::ToolUnavailable,
        span,
        format!("{what} is not supported by this checker yet"),
    );
    Some(Step::Done(Out::error()))
}
