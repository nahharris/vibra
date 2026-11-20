//! The patterns constant constructions decide.
//!
//! `docs/spec/02-type-system.md`, "Guards with `else`": a `let` pattern
//! `Result.ok(value: x)` over a construction of a type whose `where` has an
//! `else` is irrefutable only when the evaluation of that constant
//! construction succeeds. When a tool limit stops that evaluation, the
//! report is `@tool.limitExceeded` alone, at the predicate.
use vibra_diagnostics::DiagnosticCode;
use vibra_ir::{
    Arguments, CallTarget, Constant, Construction, Expr, MatchArm, NamedTypes, Node,
    Pattern, Type, Value,
};

use super::super::ctfe::{Evaluator, Outcome};
use super::super::declare::TypeKind;
use super::{Context, Site, without_bindings};

/// Decides every `let` pattern a constant construction of a type whose
/// `where` has an `else` makes irrefutable.
pub(super) fn patterns(context: &mut Context<'_, '_>, evaluator: &mut Evaluator<'_>) {
    let inputs = context.inputs;
    for (module, _, site) in inputs.sites {
        let Site::Pattern {
            ty,
            span,
            pattern,
            value,
        } = site
        else {
            continue;
        };
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
/// Reports the limit at the predicate of the type at program index `ty`.
pub(super) fn limit_at_predicate(context: &mut Context<'_, '_>, ty: usize) {
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
pub(super) fn predicate_call(
    context: &Context<'_, '_>,
    ty: usize,
    node: &Node,
) -> Option<Node> {
    let entry = context.predicate_entry(ty)?;
    let Expr::Construct {
        construction,
        value_type,
    } = &node.expr
    else {
        return None;
    };
    // A generic type's predicate is applied at the construction's type
    // arguments.
    let arguments = match value_type {
        Type::Declared(_, arguments) => arguments.clone(),
        _ => Vec::new(),
    };
    let at = |types: &[Type]| {
        types
            .iter()
            .map(|ty| ty.instantiate(&arguments))
            .collect::<Vec<_>>()
    };
    let kind = &context.program().types.get(ty)?.kind;
    let representation = match (construction, kind) {
        (Construction::Wrapper(inner), TypeKind::Wrapper(_)) => (**inner).clone(),
        (Construction::Tuple(components), TypeKind::Tuple(types)) => Node::new(
            Expr::Construct {
                value_type: Type::Tuple(at(types)),
                construction: Construction::Tuple(components.clone()),
            },
            node.origin.clone(),
        ),
        (Construction::Record(fields), TypeKind::Record(types)) => Node::new(
            Expr::Construct {
                value_type: Type::Record(
                    NamedTypes::new(
                        types
                            .iter()
                            .map(|(name, ty)| {
                                (name.clone(), ty.instantiate(&arguments))
                            })
                            .collect::<Vec<_>>(),
                    )
                    .ok()?,
                ),
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
                type_arguments: arguments,
            },
            arguments: Arguments {
                positional: vec![representation],
                labelled: Vec::new(),
            },
        },
        node.origin.clone(),
    ))
}
