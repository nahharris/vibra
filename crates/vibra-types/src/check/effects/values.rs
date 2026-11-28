//! Procedures as values: the written `Proc` and `Intent` types, the
//! anonymous `proc`, and the `await` nodes each procedure body holds.
//!
//! `docs/spec/02-type-system.md`, "Callable types" and "Functions as
//! values"; `docs/spec/01-source-language.md`, "Grammar" (the notes on a
//! row operand); `docs/spec/03-effects.md`, "Effect ceilings" and "Cold
//! plans and performed rows".
//!
//! - `Proc({inputs} R {Row})` and `Intent(R {Row})` are read with their
//!   row, which [`super::rows::resolve`] resolves as a `performs` row is
//!   resolved. An omitted row is the empty row, never an unknown one.
//! - A written `Proc` or `Intent` type whose row is omitted and whose result
//!   operand is a tuple of at least one component, every one an operation
//!   interface, is the warning `@type.suspiciousRow` at the type. A
//!   `returns` type is never reported. The warning is kept once per written
//!   type, however often the type is read.
//! - An anonymous `proc` is a closure of the type `Proc({…} R {Row})` with
//!   its written row as its ceiling. Its body runs in a procedure context of
//!   its own: an `await` in it is valid and contributes to its row, not to
//!   the row of the body it is written in, which building the closure does
//!   not perform. Its captures, guards, and labelled defaults are those of
//!   an anonymous `fn`. [`super::analysis`] checks its row against its
//!   ceiling and reports its unused roots.
//!
//! A row written on an anonymous `fn` is `@syntax.invalidSection`, which
//! the reader reports; the checker reads such a callable as nothing.

use std::cell::RefCell;
use std::collections::BTreeMap;

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::{FnType, IntentType, NamedTypes, ProcType, Row, Type};
use vibra_syntax::source::{Callable, CallableKind, TypeExpr, TypeForm};

use super::analysis::Await;
use super::rows;
use crate::check::declare::{NoType, Program, Scope, repeated_label};
use crate::check::expr::{BodyChecker, ClosureMode, Ctx, Out, Step};

/// The `@type.suspiciousRow` warnings found while written types are read,
/// each once, by its module and the span of its type.
#[derive(Debug, Default)]
pub(crate) struct Warnings(RefCell<BTreeMap<(usize, usize, usize), Diagnostic>>);

impl Warnings {
    /// Keeps `diagnostic`, found in a type written in `module` at `span`,
    /// unless a warning of that type is kept already.
    fn keep(&self, module: usize, span: ByteSpan, diagnostic: Diagnostic) {
        self.0
            .borrow_mut()
            .entry((module, span.start(), span.end()))
            .or_insert(diagnostic);
    }
}

/// The checked type of the written `Proc` or `Intent` type `written`, read
/// in `module` in `scope`. [`Program::written`] hands it every such type.
pub(crate) fn written_type(
    program: &Program<'_>,
    module: usize,
    written: TypeExpr<'_>,
    scope: Scope<'_>,
) -> Result<Type, NoType> {
    match written.form() {
        Some(TypeForm::Proc {
            inputs,
            result,
            row,
        }) => {
            if let Some(fault) = repeated_label(&inputs) {
                return Err(fault);
            }
            let parameters = inputs
                .positional()
                .into_iter()
                .map(|input| program.written_at(module, input, scope))
                .collect::<Result<Vec<_>, _>>()?;
            let labelled = inputs
                .labelled()
                .into_iter()
                .map(|(name, input)| {
                    Ok((
                        name.text().to_owned(),
                        program.written_at(module, input, scope)?,
                    ))
                })
                .collect::<Result<Vec<_>, NoType>>()?;
            let labels = NamedTypes::new(labelled).map_err(|_| NoType::Unresolved)?;
            let result = program.written_at(module, result, scope)?;
            let effects = rows::resolve(program, module, row, None)?;
            if row.is_none() {
                suspicious(program, module, written.span(), &result);
            }
            Ok(Type::Proc(Box::new(ProcType {
                parameters,
                labels,
                result,
                effects,
            })))
        }
        Some(TypeForm::Intent { result, row }) => {
            let result = program.written_at(module, result, scope)?;
            let effects = rows::resolve(program, module, row, None)?;
            if row.is_none() {
                suspicious(program, module, written.span(), &result);
            }
            Ok(Type::Intent(Box::new(IntentType { result, effects })))
        }
        _ => Err(NoType::Unresolved),
    }
}

/// Keeps `@type.suspiciousRow` for the type written in `module` at `span`,
/// whose row is omitted, when its result `result` is a tuple of at least
/// one component, every one an operation interface.
fn suspicious(program: &Program<'_>, module: usize, span: ByteSpan, result: &Type) {
    let Type::Tuple(components) = result else {
        return;
    };
    let roots = components.iter().all(|component| match component {
        Type::Interface(id, ..) => program
            .interfaces
            .iter()
            .position(|data| data.id.as_ref() == Some(id))
            .is_some_and(|index| rows::is_operation_interface(program, index)),
        _ => false,
    });
    if components.is_empty() || !roots {
        return;
    }
    let diagnostic = Diagnostic::new(
        DiagnosticCode::TypeSuspiciousRow,
        span,
        format!(
            "this type has no row, so its result is the tuple `{result}` of grants and \
             its row is empty; write the row as a further operand, after the result"
        ),
    );
    program.callable_warnings.keep(module, span, diagnostic);
}

/// The `@type.suspiciousRow` warnings of the local modules in scope, each
/// with its source.
pub(crate) fn warnings(program: &Program<'_>) -> Vec<Diagnostic> {
    program
        .callable_warnings
        .0
        .borrow()
        .iter()
        .filter_map(|((module, _, _), diagnostic)| {
            let data = program.modules.get(*module)?;
            (data.in_scope && !data.embedded)
                .then(|| diagnostic.clone().with_source_id(data.source_id.as_ref()))
        })
        .collect()
}

/// One anonymous `proc`: where it is written, its ceiling, the entries of
/// its written row, and the `await` nodes of its own body.
#[derive(Clone, Debug)]
pub(crate) struct Anonymous {
    /// The anonymous `proc`, `proc` to `endproc`.
    pub(crate) span: ByteSpan,
    /// The number of anonymous callables around its body, itself included.
    depth: usize,
    /// Its ceiling: its written row.
    pub(crate) ceiling: Row,
    /// Each entry of its written row: where it is written, the interface it
    /// names, and how it is written.
    pub(crate) written: Vec<(ByteSpan, usize, String)>,
    /// The `await` nodes of its own body, not of a callable written in it.
    pub(crate) awaits: Vec<Await>,
}

/// The `await` nodes of one checked body: those of the body itself, and
/// those of each anonymous `proc` written in it.
#[derive(Clone, Debug, Default)]
pub(crate) struct Awaits {
    /// The `await` nodes of the body itself.
    pub(crate) own: Vec<Await>,
    /// Each anonymous `proc` of the body that was checked, in the order its
    /// checking ended.
    pub(crate) anonymous: Vec<Anonymous>,
    /// The anonymous `proc`s being checked, innermost last.
    open: Vec<Anonymous>,
}

impl Awaits {
    /// Forgets every `await`, for a new body.
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }
}

/// Whether the expression being checked is in the own body of an anonymous
/// `proc`: not in its guards or labelled defaults, and not in another
/// anonymous callable written in it.
pub(crate) fn in_anonymous_body(checker: &BodyChecker<'_, '_>) -> bool {
    let Some(depth) = checker.body_depth() else {
        return false;
    };
    checker
        .awaits
        .open
        .last()
        .is_some_and(|open| open.depth == depth)
}

/// Records the checked `await` `awaited`: into the anonymous `proc` whose
/// own body holds it, or into the body being checked.
pub(crate) fn record(checker: &mut BodyChecker<'_, '_>, awaited: Await) {
    let inner = in_anonymous_body(checker);
    match checker.awaits.open.last_mut() {
        Some(open) if inner => open.awaits.push(awaited),
        _ => checker.awaits.own.push(awaited),
    }
}

/// The step of the anonymous callable `view`, written at `span` where
/// `ctx` uses its value, before the shared checker starts it; `None` when
/// the shared checker starts it.
///
/// An anonymous `proc` whose row resolves is opened, so that its body runs
/// in a procedure context of its own; one whose row does not has the row's
/// fault. An anonymous `fn` that writes a row has the reader's
/// `@syntax.invalidSection` and is read as nothing.
pub(crate) fn anonymous<'a>(
    checker: &mut BodyChecker<'_, 'a>,
    view: &Callable<'a>,
    span: ByteSpan,
    _ctx: &Ctx,
    _mode: ClosureMode,
) -> Option<Step<'a>> {
    // An anonymous callable that did not start leaves nothing open: only
    // the callables around this one are.
    let around = checker.depth();
    checker.awaits.open.retain(|open| open.depth <= around);
    let written = view.performs();
    if view.kind() == CallableKind::Fn {
        return written.is_some().then(|| Step::Done(Out::error()));
    }
    let program = checker.program();
    let module = checker.module_index();
    let ceiling = match rows::resolve(program, module, written, None) {
        Ok(ceiling) => ceiling,
        Err(NoType::Fault(diagnostic)) => {
            checker.push_diagnostic(*diagnostic);
            return Some(Step::Done(Out::error()));
        }
        Err(NoType::Unsupported | NoType::Unresolved) => {
            return Some(Step::Done(Out::error()));
        }
    };
    let entries = super::analysis::entries(program, module, written);
    checker.awaits.open.push(Anonymous {
        span,
        depth: around.saturating_add(1),
        ceiling,
        written: entries,
        awaits: Vec::new(),
    });
    None
}

/// The type of the anonymous callable of type `signature` written at
/// `span`, whose checking ends: the `Proc` type with its row for an
/// anonymous `proc`, which is closed, and the `Fn` type of an anonymous
/// `fn`.
pub(crate) fn closure_type(
    checker: &mut BodyChecker<'_, '_>,
    span: ByteSpan,
    signature: &FnType,
) -> Type {
    let depth = checker.depth().saturating_add(1);
    let open = checker
        .awaits
        .open
        .last()
        .is_some_and(|open| open.depth == depth && open.span == span);
    if !open {
        return Type::function(signature.clone());
    }
    let Some(closed) = checker.awaits.open.pop() else {
        return Type::function(signature.clone());
    };
    let ty = proc_type(signature.clone(), closed.ceiling.clone());
    checker.awaits.anonymous.push(closed);
    ty
}

/// The type the anonymous callable of type `signature`, written at `span`
/// and being checked, has once it is done, for the instantiation of its own
/// type parameters from an expected type.
pub(crate) fn pattern(
    checker: &BodyChecker<'_, '_>,
    span: ByteSpan,
    signature: &FnType,
) -> Type {
    match checker.awaits.open.last() {
        Some(open) if open.span == span && open.depth == checker.depth() + 1 => {
            proc_type(signature.clone(), open.ceiling.clone())
        }
        _ => Type::function(signature.clone()),
    }
}

/// The type of an application of a generic anonymous callable of type
/// `value_type` whose signature gives `result`: the plan of an anonymous
/// `proc`, the result of an anonymous `fn`.
pub(crate) fn generic_applied(value_type: &Type, result: Type) -> Type {
    match value_type {
        Type::Proc(signature) => Type::Intent(Box::new(IntentType {
            result,
            effects: signature.effects.clone(),
        })),
        _ => result,
    }
}

/// `Proc` over `signature` with the row `effects`.
pub(crate) fn proc_type(signature: FnType, effects: Row) -> Type {
    Type::Proc(Box::new(ProcType {
        parameters: signature.parameters,
        labels: signature.labels,
        result: signature.result,
        effects,
    }))
}
