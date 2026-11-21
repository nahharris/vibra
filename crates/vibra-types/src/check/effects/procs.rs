//! Named procedures, the plans their application builds, `await`, and the
//! headers of tests.
//!
//! `docs/spec/01-source-language.md`, "`fn` and `proc`" and "`return`,
//! `try`, `await`"; `docs/spec/02-type-system.md`, "Application" and
//! "Guards and callable types"; `docs/spec/03-effects.md`, "Cold plans and
//! performed rows"; `docs/spec/04-programs-and-packages.md`, "Tests".
//!
//! A module-level `proc` and a `proc` member of a type are functions of the
//! program, as a `fn` is: their headers, type parameters, `given`, labelled
//! parameters, parameter patterns, and guards are read and checked the way a
//! `fn`'s are, and only the type they denote differs. A `proc` with no
//! `performs`, or with `performs {}`, has the type `Proc({…} R {})`; it is
//! never given a `Fn` type. Applying one builds a plan of type
//! `Intent(R {})` and enters nothing, so a bare `requires` of the callee is
//! an obligation of the application, where the plan is built. `await plan`
//! is valid in the body of a `proc` only, at any depth of `if`, `match`,
//! `do`, and `let`, and not in a guard, a labelled default, or an anonymous
//! callable written there; its operand MUST be a plan.
//!
//! Step 08 reads a non-empty row through [`super::rows`]; until then a
//! `proc` with one is `@tool.unavailable` at its `performs` section. Step 10
//! lifts a `proc` named as a value and the application of a procedure
//! value; steps 07 and 21 fill the hooks of a test.

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::{
    BodyContext, Expr as IrExpr, FnType, IntentType, Node, ProcType, Row, Type,
};
use vibra_syntax::SyntaxKind;
use vibra_syntax::source::{Callable, CallableKind, Expr};

use crate::check::declare::{
    BoundData, Entity, FnData, NoType, Program, Scope, TestData,
};
use crate::check::expr::{BodyChecker, Callee, Ctx, Out, Step, Task};

/// The header of a callable: its signature and the bound of each of its
/// binders.
pub(crate) type Header = (FnType, Vec<Option<BoundData>>);

/// Declares the module-level `proc` `callable` of `module`: a function of
/// the program, as a module-level `fn` is.
pub(crate) fn declare<'a>(
    program: &mut Program<'a>,
    module: usize,
    callable: Callable<'a>,
) {
    let Some(name) = callable.name() else {
        return;
    };
    let Some(data) = program.modules.get(module) else {
        return;
    };
    let identity =
        program.identity_of(&data.path, data.embedded, "", "@proc", name.text());
    let dotted = data.path.strip_prefix('@').unwrap_or(&data.path);
    let qualified = format!("{dotted}.{}", name.text());
    let index = program.functions.len();
    program.functions.push(FnData {
        module,
        owner: None,
        implements: None,
        contract: None,
        name: name.text().to_owned(),
        view: callable,
        identity,
        qualified,
        binders: callable
            .params()
            .iter()
            .map(|param| param.text().to_owned())
            .collect(),
        bounds: Vec::new(),
        self_type: None,
        associated: Vec::new(),
        signature: None,
        external: None,
    });
    program.declare(module, name.text(), Entity::Fn(index));
}

/// The kind atom of the identity anchor of the callable `view`
/// (`docs/spec/02-type-system.md`, "Declared types").
pub(crate) fn identity_kind(view: &Callable<'_>) -> &'static str {
    match view.kind() {
        CallableKind::Proc => "@proc",
        CallableKind::Fn => "@fn",
    }
}

/// The header of the callable `data` read in `scope`, when this hook reads
/// it; `None` when the shared checker reads it as a `fn`'s: a `fn` with no
/// row, and a `proc` with an empty row, whose type is then
/// [`callable_type`].
///
/// A `proc` whose row is not empty is `@tool.unavailable` at its `performs`
/// section until step 08; a `fn` that writes a row, which the reader
/// rejects, is not supported.
pub(crate) fn header(
    _program: &Program<'_>,
    data: &FnData<'_>,
    _scope: Scope<'_>,
) -> Option<Result<Header, NoType>> {
    let row = data.view.performs();
    match data.view.kind() {
        CallableKind::Fn => row.is_some().then_some(Err(NoType::Unsupported)),
        CallableKind::Proc => {
            let row = row.filter(|row| !row.paths().is_empty())?;
            let section = data
                .view
                .node()
                .children()
                .iter()
                .find(|child| child.kind() == SyntaxKind::Performs)
                .map_or(row.node().span(), |section| {
                    let start = section
                        .children()
                        .first()
                        .map_or(section.span().start(), |keyword| {
                            keyword.span().start()
                        });
                    ByteSpan::new(start, row.node().span().end())
                });
            Some(Err(NoType::Fault(Box::new(Diagnostic::new(
                DiagnosticCode::ToolUnavailable,
                section,
                "a `proc` that performs operations is not supported by this checker yet",
            )))))
        }
    }
}

/// How a diagnostic or a note names the callable `data`.
pub(crate) fn describe(data: &FnData<'_>) -> String {
    match data.view.kind() {
        CallableKind::Proc => format!("the procedure `{}`", data.qualified),
        CallableKind::Fn => format!("the function `{}`", data.qualified),
    }
}

/// The row of the callable `data`: its ceiling when it is a `proc`, `None`
/// for a `fn`. A `proc` with a header has an empty row until step 08.
fn row(data: &FnData<'_>) -> Option<Row> {
    (data.view.kind() == CallableKind::Proc).then(Row::empty)
}

/// The type the callable `data`, whose header is `signature`, denotes: the
/// `Proc` type of a `proc`, the `Fn` type of a `fn`.
pub(crate) fn callable_type(data: &FnData<'_>, signature: FnType) -> Type {
    match row(data) {
        Some(effects) => Type::Proc(Box::new(ProcType {
            parameters: signature.parameters,
            labels: signature.labels,
            result: signature.result,
            effects,
        })),
        None => Type::function(signature),
    }
}

/// The type of an application of the callable `data` whose header gives
/// the result `result`: for a `proc` the plan it builds, `Intent(result
/// {Row})`, of the kind `@procPlan`; for a `fn` its result.
pub(crate) fn applied(data: &FnData<'_>, result: Type) -> Type {
    match row(data) {
        Some(effects) => Type::Intent(Box::new(IntentType { result, effects })),
        None => result,
    }
}

/// The callee of an application whose callee is the value `callee` of the
/// `Proc` type `signature`, written at `callee_span`. A procedure value is
/// not supported until step 10, and nothing at this step makes one.
pub(crate) fn apply(
    checker: &mut BodyChecker<'_, '_>,
    _callee: Option<Node>,
    _signature: &ProcType,
    callee_span: ByteSpan,
) -> Option<Callee> {
    checker.report(
        DiagnosticCode::ToolUnavailable,
        callee_span,
        "an application of a procedure value is not supported by this checker yet",
    );
    None
}

/// Whether the callable `data`, named as a value at `span`, is a `proc`:
/// it is then reported, since a procedure named as a value is not
/// supported until step 10.
pub(crate) fn named_as_value(
    checker: &mut BodyChecker<'_, '_>,
    data: &FnData<'_>,
    span: ByteSpan,
) -> bool {
    if data.view.kind() != CallableKind::Proc {
        return false;
    }
    checker.report(
        DiagnosticCode::ToolUnavailable,
        span,
        "a procedure named as a value is not supported by this checker yet",
    );
    true
}

/// The step of `await operand`, written at `span` where `ctx` uses its
/// value.
///
/// The position is valid in the body of a `proc` itself: the checker's
/// context is a procedure context, and the expression is neither in a guard
/// or a labelled default, where nothing leaves the callable, nor in an
/// anonymous callable. In any other position the `await` is reported and
/// its operand is not checked (Q4). An `await` in a test's own body is
/// valid and not supported until step 07.
pub(crate) fn await_expr<'a>(
    checker: &mut BodyChecker<'_, 'a>,
    operand: Expr<'a>,
    span: ByteSpan,
    _ctx: &Ctx,
) -> Step<'a> {
    if checker.in_test_body() {
        checker.report(
            DiagnosticCode::ToolUnavailable,
            span,
            "a test that awaits is not supported by this checker yet",
        );
        return Step::Done(Out::error());
    }
    let procedure = matches!(checker.context, BodyContext::Procedure(_));
    if !procedure || !checker.in_own_body() {
        checker.report(
            DiagnosticCode::EffectInvalidAwait,
            span,
            "`await` is valid only in the body of a `proc`",
        );
        return Step::Done(Out::error());
    }
    Step::Child(Task::Await(span), Task::Expr(operand, Ctx::Value(None)))
}

/// The `await` at `span` whose operand arrived as `input`: a plan yields
/// its result, and the `await` is recorded with the plan's type; any other
/// operand is `@effect.invalidAwait` at the `await`, never
/// `@type.mismatch` (Q4).
pub(crate) fn finish_await<'a>(
    checker: &mut BodyChecker<'_, 'a>,
    span: ByteSpan,
    input: Option<Out>,
) -> Step<'a> {
    let Out { ty, node } = input.unwrap_or_else(Out::error);
    let plan = match ty {
        Some(Type::Intent(ref plan)) => plan.as_ref().clone(),
        Some(found) => {
            checker.report(
                DiagnosticCode::EffectInvalidAwait,
                span,
                format!(
                    "`await` runs a plan, an `Intent`, and this operand is `{found}`"
                ),
            );
            return Step::Done(Out::error());
        }
        None => return Step::Done(Out::error()),
    };
    checker.awaits.push((span, plan.clone()));
    let node = node.map(|plan| {
        checker.node(
            IrExpr::Await {
                plan: Box::new(plan),
            },
            span,
        )
    });
    Step::Done(Out {
        ty: Some(plan.result),
        node,
    })
}

/// The body of the stand-in for the function `data` that was not lowered,
/// given `call`, the call of itself with its own arguments in tail
/// position: a `proc` awaits the plan the call builds, since a tail call of
/// a procedure builds a plan and returns nothing it could run.
pub(crate) fn stub(data: &FnData<'_>, call: Node) -> Node {
    if row(data).is_none() {
        return call;
    }
    let mut call = call;
    call.expr = match std::mem::replace(&mut call.expr, IrExpr::Global(0)) {
        IrExpr::TailCall { target, arguments } => IrExpr::Call { target, arguments },
        other => other,
    };
    let origin = call.origin.clone();
    Node::new(
        IrExpr::Await {
            plan: Box::new(call),
        },
        origin,
    )
}

/// The context the body of the callable `data` runs in: a `proc`'s is a
/// procedure context with its ceiling, which equals the row of its type.
pub(crate) fn lower(_program: &Program<'_>, data: &FnData<'_>) -> BodyContext {
    row(data).map_or(BodyContext::Pure, BodyContext::Procedure)
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
