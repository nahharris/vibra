//! Bare-guard obligations and the patterns constant constructions decide.
//!
//! `docs/spec/02-type-system.md`, "Bare guards": a bare `requires` states an
//! obligation at every call, a bare `ensures` at every `return` of the body,
//! and a bare `where` at every construction. The discharge rules are not
//! applied yet, so each obligation over a certified predicate is reported
//! `@contract.unproved`, at the predicate or the failing application and
//! related to the guarded declaration, and its status is `unknown`. When a
//! tool limit stops the evaluation of a predicate over constant operands,
//! the report is `@tool.limitExceeded` alone, at the predicate.
//!
//! "Guards with `else`": a `let` pattern `Result.ok(value: x)` over a
//! construction of a type whose `where` has an `else` is irrefutable only
//! when the evaluation of that constant construction succeeds.

use vibra_diagnostics::{ByteSpan, DiagnosticCode};
use vibra_ir::{
    Arguments, CallTarget, Constant, Construction, Expr, MatchArm, NamedTypes, Node,
    Pattern, Type, Value,
};

use super::super::ctfe::{Evaluator, Outcome};
use super::super::declare::TypeKind;
use super::{Certified, Context, Site, without_bindings};

/// The name span and module of the function at program index `function`.
fn function_name(
    context: &Context<'_, '_>,
    function: usize,
) -> Option<(usize, ByteSpan, String)> {
    let data = context.program().functions.get(function)?;
    let name = data.view.name()?;
    Some((data.module, name.span(), data.qualified.clone()))
}

/// The name span and module of the type at program index `ty`.
fn type_name(
    context: &Context<'_, '_>,
    ty: usize,
) -> Option<(usize, ByteSpan, String)> {
    let data = context.program().types.get(ty)?;
    let name = data.view.name()?;
    Some((data.module, name.span(), data.name.clone()))
}

/// Reports `@contract.unproved` at `span` of the module `module`, related
/// to the declaration `declaration`.
fn unproved(
    context: &mut Context<'_, '_>,
    module: usize,
    span: ByteSpan,
    declaration: Option<(usize, ByteSpan, String)>,
    what: &str,
) {
    let mut diagnostic = context.diagnostic(
        DiagnosticCode::ContractUnproved,
        module,
        span,
        format!(
            "{what} is not established by the discharge rules; branch on the same \
             predicate, pass a value of a type whose invariant states it, or give \
             the guard an `else`"
        ),
    );
    if let Some((at, name_span, name)) = declaration {
        diagnostic = context.program().related(
            diagnostic,
            at,
            name_span,
            &format!("the guard of `{name}`"),
        );
    }
    context.push(module, diagnostic);
}

/// Reports the obligation of every certified bare `ensures`, at its
/// predicate.
pub(super) fn bare_ensures(context: &mut Context<'_, '_>, certified: &Certified) {
    let inputs = context.inputs;
    for (index, checked) in inputs.functions {
        let Some((_, guard)) = &checked.guards.exit else {
            continue;
        };
        if guard.consequence.is_some()
            || !certified.get(index).is_some_and(|(_, exit)| *exit)
        {
            continue;
        }
        let declaration = function_name(context, *index);
        let Some(module) = declaration.as_ref().map(|(module, _, _)| *module) else {
            continue;
        };
        unproved(
            context,
            module,
            guard.span,
            declaration,
            "the bare `ensures` at each `return` of the body",
        );
    }
}

/// Reports or decides every site.
pub(super) fn sites(
    context: &mut Context<'_, '_>,
    certified: &Certified,
    evaluator: &mut Evaluator<'_>,
) {
    let inputs = context.inputs;
    for (module, site) in inputs.sites {
        match site {
            Site::Requires { function, span } => {
                let bare = inputs
                    .functions
                    .get(function)
                    .and_then(|checked| checked.guards.entry.as_ref())
                    .is_some_and(|guard| guard.consequence.is_none());
                if !bare || !certified.get(function).is_some_and(|(entry, _)| *entry) {
                    continue;
                }
                let declaration = function_name(context, *function);
                unproved(
                    context,
                    *module,
                    *span,
                    declaration,
                    "the bare `requires` at this call",
                );
            }
            Site::Where { ty, span, node } => {
                if !context.certified_predicate(*ty) {
                    continue;
                }
                if let Some(node) = node
                    && let Some(call) = predicate_call(context, *ty, node)
                    && evaluator.evaluate(&call) == Outcome::LimitReached
                {
                    limit_at_predicate(context, *ty);
                    continue;
                }
                let declaration = type_name(context, *ty);
                unproved(
                    context,
                    *module,
                    *span,
                    declaration,
                    "the bare `where` of this construction",
                );
            }
            Site::Pattern {
                ty,
                span,
                pattern,
                value,
            } => {
                if !context.certified_predicate(*ty) {
                    continue;
                }
                match evaluator.evaluate(&matches(pattern, value)) {
                    Outcome::Value(Value::U8(1)) => {}
                    Outcome::LimitReached => limit_at_predicate(context, *ty),
                    Outcome::Value(_) | Outcome::Unevaluated => {
                        let diagnostic = context.diagnostic(
                            DiagnosticCode::PatternRefutableBinding,
                            *module,
                            *span,
                            "this pattern does not cover the value of this constant \
                             construction, which the checker evaluated; add an `else`",
                        );
                        context.push(*module, diagnostic);
                    }
                }
            }
        }
    }
}

/// Reports the limit at the predicate of the type at program index `ty`.
fn limit_at_predicate(context: &mut Context<'_, '_>, ty: usize) {
    let inputs = context.inputs;
    let (Some(data), Some(invariant)) =
        (inputs.program.types.get(ty), inputs.invariants.get(&ty))
    else {
        return;
    };
    context.limit(data.module, invariant.span);
}

/// `match value case pattern 1u8 case _ 0u8 endmatch`, with every binding
/// of `pattern` a `_`.
fn matches(pattern: &Pattern, value: &Node) -> Node {
    let origin = value.origin.clone();
    let flag = |held: u8| match Constant::new(Type::U8, Value::U8(held)) {
        Ok(constant) => Node::new(Expr::Literal(constant), origin.clone()),
        Err(_) => Node::new(Expr::Default(Type::U8), origin.clone()),
    };
    Node::new(
        Expr::Match {
            subject: Box::new(value.clone()),
            arms: vec![
                MatchArm {
                    pattern: without_bindings(pattern),
                    body: flag(1),
                },
                MatchArm {
                    pattern: Pattern::Discard,
                    body: flag(0),
                },
            ],
            value_type: Type::U8,
        },
        origin.clone(),
    )
}

/// The call of the predicate of the type at program index `ty` over the
/// representation the construction `node` assembles.
fn predicate_call(context: &Context<'_, '_>, ty: usize, node: &Node) -> Option<Node> {
    let entry = context.predicate_entry(ty)?;
    let Expr::Construct { construction, .. } = &node.expr else {
        return None;
    };
    let kind = &context.program().types.get(ty)?.kind;
    let representation = match (construction, kind) {
        (Construction::Wrapper(inner), TypeKind::Wrapper(_)) => (**inner).clone(),
        (Construction::Tuple(components), TypeKind::Tuple(types)) => Node::new(
            Expr::Construct {
                value_type: Type::Tuple(types.clone()),
                construction: Construction::Tuple(components.clone()),
            },
            node.origin.clone(),
        ),
        (Construction::Record(fields), TypeKind::Record(types)) => Node::new(
            Expr::Construct {
                value_type: Type::Record(NamedTypes::new(types.clone()).ok()?),
                construction: Construction::Record(fields.clone()),
            },
            node.origin.clone(),
        ),
        _ => return None,
    };
    Some(Node::new(
        Expr::Call {
            target: CallTarget::Direct {
                function: entry,
                type_arguments: Vec::new(),
            },
            arguments: Arguments {
                positional: vec![representation],
                labelled: Vec::new(),
            },
        },
        node.origin.clone(),
    ))
}
