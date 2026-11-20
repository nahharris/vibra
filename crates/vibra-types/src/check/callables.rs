//! Callables: what the checker does to the lowered nodes of labelled
//! defaults, generic anonymous callables, and tail calls.
//!
//! `docs/spec/02-type-system.md`, "Constant expressions and constant
//! patterns": a labelled default is a constant expression, a literal, an
//! atom, `void`, a construction, tuple, record, or list of constants, or the
//! name of a module value whose initializer is one; a function call never is.
//! [`fold`] reads the value of such an expression from its checked node.
//!
//! "Generics": a generic anonymous callable bound by `let` stays generic, and
//! anywhere else it is instantiated from its written expected type. The
//! checked IR has no generic closure, so a generic anonymous callable is
//! checked once over its own type parameters and each use is a closure at
//! closed type arguments: [`substitute`] replaces its own binders in every
//! type of its lowered node.
//!
//! `docs/spec/06-runtime.md`, "Tail calls": the operand of `return`, and the
//! final value of a `do`, `if`, `match`, or `let` in tail position, are in
//! tail position. [`mark_tail`] turns each call there into a tail call.
//!
//! Every walk here uses an explicit work list, so no host stack grows with
//! the depth of the program.

use std::collections::BTreeMap;
use std::sync::Arc;

use vibra_ir::{
    CallTarget, Construction, Expr as IrExpr, NamedTypes, Node, Type, Value,
};

/// Why a default has no constant value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NotConstant {
    /// The expression is not a constant expression.
    Expression,
    /// It is one, but holds a dict, whose canonical key order this checker
    /// does not compute.
    Dict,
    /// It names a module value whose constant is not known yet.
    Pending,
}

enum Fold<'n> {
    Visit(&'n Node),
    /// Builds the value of a construction from the values of its operands,
    /// the last ones on the value stack.
    Build(&'n Construction),
}

/// The value of the constant expression `node` lowers, reading a module
/// value through `constants`, the constant of each global index known so far.
pub(crate) fn fold(
    node: &Node,
    constants: &BTreeMap<usize, Value>,
) -> Result<Value, NotConstant> {
    let mut pending = vec![Fold::Visit(node)];
    let mut values: Vec<Value> = Vec::new();
    while let Some(item) = pending.pop() {
        match item {
            Fold::Visit(node) => match &node.expr {
                IrExpr::Literal(constant) => values.push(constant.value().clone()),
                IrExpr::WidenAtom(inner) => pending.push(Fold::Visit(inner)),
                IrExpr::Global(index) => match constants.get(index) {
                    Some(value) => values.push(value.clone()),
                    None => return Err(NotConstant::Pending),
                },
                IrExpr::Construct { construction, .. } => {
                    pending.push(Fold::Build(construction));
                    let operands: Vec<&Node> = match construction {
                        Construction::Tuple(nodes) | Construction::List(nodes) => {
                            nodes.iter().collect()
                        }
                        Construction::Record(fields)
                        | Construction::Case { fields, .. } => {
                            fields.iter().map(|(_, node)| node).collect()
                        }
                        Construction::Wrapper(inner) => vec![&**inner],
                        Construction::Dict(_) => return Err(NotConstant::Dict),
                    };
                    pending.extend(operands.into_iter().rev().map(Fold::Visit));
                }
                _ => return Err(NotConstant::Expression),
            },
            Fold::Build(construction) => {
                let count = match construction {
                    Construction::Tuple(nodes) | Construction::List(nodes) => {
                        nodes.len()
                    }
                    Construction::Record(fields)
                    | Construction::Case { fields, .. } => fields.len(),
                    Construction::Wrapper(_) => 1,
                    Construction::Dict(_) => return Err(NotConstant::Dict),
                };
                let start = values
                    .len()
                    .checked_sub(count)
                    .ok_or(NotConstant::Expression)?;
                let operands = values.split_off(start);
                let names = |fields: &[(String, Node)], operands: Vec<Value>| {
                    fields
                        .iter()
                        .map(|(name, _)| name.clone())
                        .zip(operands)
                        .collect::<Vec<_>>()
                };
                values.push(match construction {
                    Construction::Tuple(_) => Value::Tuple(operands),
                    Construction::List(_) => Value::List(operands),
                    Construction::Record(fields) => {
                        Value::Record(names(fields, operands))
                    }
                    Construction::Case { case, fields } => Value::Variant {
                        case: case.clone(),
                        fields: names(fields, operands),
                    },
                    Construction::Wrapper(_) => Value::Wrapper(Box::new(
                        operands.into_iter().next().ok_or(NotConstant::Expression)?,
                    )),
                    Construction::Dict(_) => return Err(NotConstant::Dict),
                });
            }
        }
    }
    values.pop().ok_or(NotConstant::Expression)
}

/// `named` with every type replaced by `go`.
fn map_named(named: &NamedTypes, go: impl Fn(&Type) -> Type) -> NamedTypes {
    NamedTypes::new(
        named
            .entries()
            .iter()
            .map(|(name, ty)| (name.clone(), go(ty))),
    )
    .unwrap_or_else(|_| named.clone())
}

/// Replaces each binder of every type in `node`, closure bodies included, by
/// the type at its position in `binders`; a binder past the end is kept.
pub(crate) fn substitute(node: &mut Node, binders: &[Type]) {
    let go = |ty: &Type| ty.instantiate(binders);
    let replace = |ty: &mut Type| *ty = ty.instantiate(binders);
    let mut pending: Vec<&mut Node> = vec![node];
    while let Some(Node { expr, .. }) = pending.pop() {
        match expr {
            IrExpr::Literal(_) | IrExpr::Global(_) => {}
            IrExpr::Intrinsic {
                type_arguments,
                arguments,
                ..
            } => {
                type_arguments.iter_mut().for_each(replace);
                pending.extend(arguments.iter_mut());
            }
            IrExpr::Sequence(nodes) => pending.extend(nodes.iter_mut()),
            IrExpr::Variable { value_type, .. }
            | IrExpr::Captured { value_type, .. }
            | IrExpr::Default(value_type) => replace(value_type),
            IrExpr::Function { type_arguments, .. } => {
                type_arguments.iter_mut().for_each(replace);
            }
            IrExpr::Closure { closure, captures } => {
                let body = Arc::make_mut(closure);
                replace(&mut body.value_type);
                body.captures.iter_mut().for_each(replace);
                pending.push(&mut body.body);
                pending.extend(captures.iter_mut());
            }
            IrExpr::Let { value, body, .. } => {
                pending.push(value);
                pending.push(body);
            }
            IrExpr::LetElse {
                value,
                fallback,
                body,
                ..
            } => {
                pending.push(value);
                pending.push(fallback);
                pending.push(body);
            }
            IrExpr::Match {
                subject,
                arms,
                value_type,
            } => {
                replace(value_type);
                pending.push(subject);
                pending.extend(arms.iter_mut().map(|arm| &mut arm.body));
            }
            IrExpr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                pending.push(condition);
                pending.push(then_branch);
                if let Some(otherwise) = else_branch {
                    pending.push(otherwise);
                }
            }
            IrExpr::Construct {
                value_type,
                construction,
            } => {
                replace(value_type);
                match construction {
                    Construction::Tuple(nodes) | Construction::List(nodes) => {
                        pending.extend(nodes.iter_mut());
                    }
                    Construction::Record(fields)
                    | Construction::Case { fields, .. } => {
                        pending.extend(fields.iter_mut().map(|(_, node)| node));
                    }
                    Construction::Wrapper(inner) => pending.push(inner),
                    Construction::Dict(entries) => {
                        for (key, value) in entries {
                            pending.push(key);
                            pending.push(value);
                        }
                    }
                }
            }
            IrExpr::Field { record: inner, .. }
            | IrExpr::Return(inner)
            | IrExpr::Try(inner)
            | IrExpr::WidenAtom(inner) => pending.push(inner),
            IrExpr::WidenToInterface { value, value_type } => {
                replace(value_type);
                pending.push(value);
            }
            IrExpr::Lookup { collection, key } => {
                pending.push(collection);
                pending.push(key);
            }
            IrExpr::Call { target, arguments }
            | IrExpr::TailCall { target, arguments } => {
                match target {
                    CallTarget::Direct { type_arguments, .. } => {
                        type_arguments.iter_mut().for_each(replace);
                    }
                    CallTarget::Value(callee) => pending.push(callee),
                    CallTarget::Contract(call) => {
                        replace(&mut call.receiver);
                        call.arguments.iter_mut().for_each(replace);
                        call.member_arguments.iter_mut().for_each(replace);
                        call.choices = map_named(&call.choices, go);
                    }
                }
                pending.extend(arguments.positional.iter_mut());
                pending.extend(arguments.labelled.iter_mut().map(|(_, node)| node));
            }
        }
    }
}

/// Turns every call in tail position of `node`, which is in tail position
/// itself, into a tail call: the node, and the final value of each `let`,
/// sequence, `if`, and `match` in tail position.
pub(crate) fn mark_tail(node: &mut Node) {
    let mut pending: Vec<&mut Node> = vec![node];
    while let Some(Node { expr, .. }) = pending.pop() {
        match expr {
            IrExpr::Call { .. } => {
                let call = std::mem::replace(expr, IrExpr::Global(0));
                if let IrExpr::Call { target, arguments } = call {
                    *expr = IrExpr::TailCall { target, arguments };
                }
            }
            IrExpr::Let { body, .. } | IrExpr::LetElse { body, .. } => {
                pending.push(body)
            }
            IrExpr::Sequence(nodes) => pending.extend(nodes.last_mut()),
            IrExpr::If {
                then_branch,
                else_branch: Some(otherwise),
                ..
            } => {
                pending.push(then_branch);
                pending.push(otherwise);
            }
            IrExpr::Match { arms, .. } => {
                pending.extend(arms.iter_mut().map(|arm| &mut arm.body));
            }
            _ => {}
        }
    }
}
