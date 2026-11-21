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
//! `fn`'s are, and only the type they denote differs. A `proc` has the type
//! `Proc({…} R {Row})` with its written row, resolved by [`super::rows`],
//! empty when no `performs` or `performs {}` is written; it is never given a
//! `Fn` type. Applying one builds a plan of type `Intent(R {Row})` and
//! enters nothing, so a bare `requires` of the callee is
//! an obligation of the application, where the plan is built. `await plan`
//! is valid in the body of a `proc` and in the body of a test, at any depth
//! of `if`, `match`, `do`, and `let`, and not in a guard, a labelled
//! default, or an anonymous callable written there; its operand MUST be a
//! plan.
//!
//! A test with no `takes` runs in a procedure context with its written
//! row, which MUST resolve as a `proc`'s does. It is never given a `Proc`
//! type: it is not applicable, and nothing builds a plan of it.
//!
//! A `proc` is not a certified function (`docs/spec/02-type-system.md`,
//! "Predicates"): its application builds a plan, so it is never read
//! through. Running a plan is the call (`GUARD-7`): after `await f(args)`,
//! whose operand is directly the application of a named `proc`, the callee's
//! proven bare `ensures` holds of the awaited result, under the conditions
//! of a call. A plan that reaches `await` any other way gives no fact, and
//! building a plan gives none.
//!
//! A `proc` named as a value has its `Proc` type, and applying a value of a
//! `Proc` type builds a plan of its row. Each `await` is recorded with its
//! plan's type and the call it runs, and [`super::analysis`] checks the rows
//! they perform against the ceiling. Step 21 lifts a test that takes a
//! world.

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};
use vibra_ir::{
    BodyContext, Expr as IrExpr, FnType, IntentType, Node, ProcType, Row, Type,
};
use vibra_syntax::source::{Callable, CallableKind, Expr};

use super::rows;
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
/// row, and a `proc` whose row resolves ([`super::rows::resolve`]), whose
/// type is then [`callable_type`]. A `proc` whose row does not resolve has
/// that row's fault; a `fn` that writes a row, which the reader rejects, is
/// not supported.
pub(crate) fn header(
    program: &Program<'_>,
    data: &FnData<'_>,
    _scope: Scope<'_>,
) -> Option<Result<Header, NoType>> {
    let row = data.view.performs();
    match data.view.kind() {
        CallableKind::Fn => row.is_some().then_some(Err(NoType::Unsupported)),
        CallableKind::Proc => rows::resolve(program, data.module, row, None)
            .err()
            .map(Err),
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
/// for a `fn`. The ceiling of a member of an interface or of an
/// implementation is the member's complete row: the interface and the
/// written row. A `proc` with a header has a row that resolves, so its
/// faults are reported where the header is read and not here.
fn row(program: &Program<'_>, data: &FnData<'_>) -> Option<Row> {
    if data.view.kind() != CallableKind::Proc {
        return None;
    }
    let owner = data.contract.or(data.implements);
    Some(
        rows::resolve(program, data.module, data.view.performs(), owner)
            .unwrap_or_default(),
    )
}

/// Whether applying the callable `data` builds a plan: it is a `proc`. Such
/// a callable is never certified, so no predicate reads its body, and its
/// `ensures` is never a certified result (`GUARD-7`): the contract review
/// asks this before it treats a callable as a certified function.
pub(crate) fn builds_plan(data: &FnData<'_>) -> bool {
    data.view.kind() == CallableKind::Proc
}

/// The type the callable `data` of `program`, whose header is `signature`,
/// denotes: the `Proc` type of a `proc`, with its row, the `Fn` type of a
/// `fn`.
pub(crate) fn callable_type(
    program: &Program<'_>,
    data: &FnData<'_>,
    signature: FnType,
) -> Type {
    match row(program, data) {
        Some(effects) => Type::Proc(Box::new(ProcType {
            parameters: signature.parameters,
            labels: signature.labels,
            result: signature.result,
            effects,
        })),
        None => Type::function(signature),
    }
}

/// The type of an application of the callable `data` of `program` whose
/// header gives the result `result`: for a `proc` the plan it builds,
/// `Intent(result {Row})` with the `proc`'s row; for a `fn` its result.
pub(crate) fn applied(program: &Program<'_>, data: &FnData<'_>, result: Type) -> Type {
    match row(program, data) {
        Some(effects) => Type::Intent(Box::new(IntentType { result, effects })),
        None => result,
    }
}

/// The callee of an application whose callee is the value `callee` of the
/// `Proc` type `signature`, written at `_callee_span`: a callable value
/// whose application builds a plan of `signature`'s row
/// ([`applied_value`]).
pub(crate) fn apply(
    _checker: &mut BodyChecker<'_, '_>,
    callee: Option<Node>,
    signature: &ProcType,
    _callee_span: ByteSpan,
) -> Option<Callee> {
    let function = FnType {
        parameters: signature.parameters.clone(),
        labels: signature.labels.clone(),
        result: signature.result.clone(),
    };
    Some(Callee::Value(
        callee.map(Box::new),
        Box::new(function),
        Some(signature.effects.clone()),
    ))
}

/// The type of an application of a callable value whose signature gives
/// `result`: the plan `Intent(result {row})` when the value is a procedure
/// of the row `row`, and `result` itself when it is a function.
pub(crate) fn applied_value(result: Type, row: Option<&Row>) -> Type {
    match row {
        Some(effects) => Type::Intent(Box::new(IntentType {
            result,
            effects: effects.clone(),
        })),
        None => result,
    }
}

/// The type of a callable value of the signature `signature`: `Proc` with
/// the row `row` when it has one, `Fn` otherwise.
pub(crate) fn value_type(signature: &FnType, row: Option<&Row>) -> Type {
    match row {
        Some(effects) => Type::Proc(Box::new(ProcType {
            parameters: signature.parameters.clone(),
            labels: signature.labels.clone(),
            result: signature.result.clone(),
            effects: effects.clone(),
        })),
        None => Type::function(signature.clone()),
    }
}

/// The type of the callable `data` of `program`, whose header is
/// `signature`, named as a value: its [`callable_type`]. Naming a `proc`
/// builds no plan and performs nothing.
pub(crate) fn named_as_value(
    program: &Program<'_>,
    data: &FnData<'_>,
    signature: FnType,
) -> Type {
    callable_type(program, data, signature)
}

/// Whether a call in tail position of a callable whose result is `result`
/// becomes a tail call: never when the call builds a plan, an `Intent`,
/// since a tail call of a procedure would return a plan nothing runs.
pub(crate) fn marks_tail(result: &Type) -> bool {
    !matches!(result, Type::Intent(_))
}

/// The step of `await operand`, written at `span` where `ctx` uses its
/// value.
///
/// The position is valid in the body of a `proc` itself, and in the body of
/// a test itself, which runs in a procedure context the test runner owns:
/// the expression is neither in a guard or a labelled default, where
/// nothing leaves the callable, nor in an anonymous callable. In any other
/// position the `await` is reported and its operand is not checked (Q4).
pub(crate) fn await_expr<'a>(
    checker: &mut BodyChecker<'_, 'a>,
    operand: Expr<'a>,
    span: ByteSpan,
    _ctx: &Ctx,
) -> Step<'a> {
    let procedure = matches!(checker.context, BodyContext::Procedure(_));
    let valid = checker.in_test_body()
        || (procedure && checker.in_own_body())
        || super::values::in_anonymous_body(checker);
    if !valid {
        checker.report(
            DiagnosticCode::EffectInvalidAwait,
            span,
            "`await` is valid only in the body of a `proc` or of a test",
        );
        return Step::Done(Out::error());
    }
    Step::Child(Task::Await(span), Task::Expr(operand, Ctx::Value(None)))
}

/// The `await` at `span` whose operand arrived as `input`: a plan yields
/// its result, and the `await` is recorded with the plan's type and the
/// call it runs ([`super::analysis::hop`]); any other
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
    let hop = node
        .as_ref()
        .map(|operand| super::analysis::hop(checker, operand));
    super::values::record(
        checker,
        super::analysis::Await {
            span,
            plan: plan.clone(),
            hop,
        },
    );
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
    if !builds_plan(data) {
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
pub(crate) fn lower(program: &Program<'_>, data: &FnData<'_>) -> BodyContext {
    row(program, data).map_or(BodyContext::Pure, BodyContext::Procedure)
}

/// The fault of the header of the test `test`, with no source, when its
/// body is not checked; `None` when it is. A test with no `takes` whose
/// row resolves, written or not, is checked. A row that does not resolve
/// has its fault; a test that takes a world is `@tool.unavailable` over
/// the test until step 21.
pub(crate) fn test_header(
    program: &Program<'_>,
    test: &TestData<'_>,
) -> Option<Diagnostic> {
    if test.view.takes().is_some() {
        return Some(Diagnostic::new(
            DiagnosticCode::ToolUnavailable,
            test.view.construct().span(),
            "a test that takes a world is not supported by this checker yet",
        ));
    }
    match rows::resolve(program, test.module, test.view.performs(), None) {
        Ok(_) => None,
        Err(NoType::Fault(diagnostic)) => Some(*diagnostic),
        Err(NoType::Unsupported | NoType::Unresolved) => Some(Diagnostic::new(
            DiagnosticCode::ToolUnavailable,
            test.view.construct().span(),
            "a test whose row names an interface this checker does not support is not supported by this checker yet",
        )),
    }
}

/// The context the lowered body of the test `test` of `program` runs in: a
/// procedure context with its row, which resolves for every test
/// [`test_header`] checks. A test is not applicable, so it has no `Proc`
/// type.
pub(crate) fn test_context(program: &Program<'_>, test: &TestData<'_>) -> BodyContext {
    BodyContext::Procedure(
        rows::resolve(program, test.module, test.view.performs(), None)
            .unwrap_or_default(),
    )
}
