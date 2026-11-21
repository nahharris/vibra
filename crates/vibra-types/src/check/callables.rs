//! Callables: what the checker does to the lowered nodes of labelled
//! defaults, generic anonymous callables, and tail calls.
//!
//! `docs/spec/02-type-system.md`, "Constant expressions and constant
//! patterns": a labelled default is a constant expression, a literal, an
//! atom, `void`, a construction, tuple, record, list, or dict of constants,
//! or the name of a module value whose initializer is one; a function call
//! never is. [`fold`] reads the value of such an expression from its checked
//! node, a dict's entries in canonical key order.
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

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::Arc;

use vibra_ir::{
    CallTarget, Construction, Expr as IrExpr, Library, NamedTypes, Node, Type, TypeId,
    Value,
};

/// Why a default has no constant value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NotConstant {
    /// The expression is not a constant expression.
    Expression,
    /// It holds a dict whose canonical key order is a declared type's own
    /// `Ordered` implementation, which only running that implementation
    /// could apply: no constant expression.
    Dict,
    /// It names a module value whose constant is not known yet.
    Pending,
}

enum Fold<'n> {
    Visit(&'n Node),
    /// Builds the value of a construction of the given type from the values
    /// of its operands, the last ones on the value stack.
    Build(&'n Construction, &'n Type),
}

/// The value of the constant expression `node` lowers, reading a module
/// value through `constants`, the constant of each global index known so far.
/// `library` names the library type an identity plays, which fixes the key
/// order of a dict.
pub(crate) fn fold(
    node: &Node,
    constants: &BTreeMap<usize, Value>,
    library: &dyn Fn(&TypeId) -> Option<Library>,
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
                IrExpr::Construct {
                    construction,
                    value_type,
                } => {
                    pending.push(Fold::Build(construction, value_type));
                    let operands: Vec<&Node> = match construction {
                        Construction::Tuple(nodes) | Construction::List(nodes) => {
                            nodes.iter().collect()
                        }
                        Construction::Record(fields)
                        | Construction::Case { fields, .. } => {
                            fields.iter().map(|(_, node)| node).collect()
                        }
                        Construction::Wrapper(inner) => vec![&**inner],
                        Construction::Dict(entries) => entries
                            .iter()
                            .flat_map(|(key, value)| [key, value])
                            .collect(),
                    };
                    pending.extend(operands.into_iter().rev().map(Fold::Visit));
                }
                _ => return Err(NotConstant::Expression),
            },
            Fold::Build(construction, value_type) => {
                let count = match construction {
                    Construction::Tuple(nodes) | Construction::List(nodes) => {
                        nodes.len()
                    }
                    Construction::Record(fields)
                    | Construction::Case { fields, .. } => fields.len(),
                    Construction::Wrapper(_) => 1,
                    Construction::Dict(entries) => entries.len() * 2,
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
                    Construction::Dict(_) => {
                        let key = dict_key(value_type, library)
                            .ok_or(NotConstant::Expression)?;
                        if ordered_by_implementation(key, library) {
                            return Err(NotConstant::Dict);
                        }
                        Value::Dict(dict_entries(operands, key, library)?)
                    }
                });
            }
        }
    }
    values.pop().ok_or(NotConstant::Expression)
}

/// Whether the canonical order of `key` uses a declared type's own `Ordered`
/// implementation: it is, or holds, a declared type other than the library's
/// `Bool`, `Str`, and `Bytes`, whose orders the type chapter fixes.
fn ordered_by_implementation(
    key: &Type,
    library: &dyn Fn(&TypeId) -> Option<Library>,
) -> bool {
    let mut found = false;
    key.visit(&mut |ty| {
        if let Type::Declared(id, _) = ty {
            found |= !matches!(
                library(id),
                Some(Library::Bool | Library::Str | Library::Bytes)
            );
        }
    });
    found
}

/// The key type of `ty`, a `Dict(K V)`.
fn dict_key<'t>(
    ty: &'t Type,
    library: &dyn Fn(&TypeId) -> Option<Library>,
) -> Option<&'t Type> {
    match ty {
        Type::Declared(id, arguments) if library(id) == Some(Library::Dict) => {
            match arguments.as_slice() {
                [key, _] => Some(key),
                _ => None,
            }
        }
        _ => None,
    }
}

/// The entries of a dict literal whose keys and values, in written order,
/// are `operands`, in canonical key order (`docs/spec/02-type-system.md`,
/// "Dict keys, equality, and order"): each key is placed by binary search,
/// and a later duplicate key replaces the earlier value and keeps its place,
/// as the evaluation of the literal does.
pub(crate) fn dict_entries(
    operands: Vec<Value>,
    key: &Type,
    library: &dyn Fn(&TypeId) -> Option<Library>,
) -> Result<Vec<(Value, Value)>, NotConstant> {
    let mut entries: Vec<(Value, Value)> = Vec::new();
    let mut operands = operands.into_iter();
    while let (Some(entry_key), Some(entry_value)) = (operands.next(), operands.next())
    {
        let (mut low, mut high) = (0, entries.len());
        let mut found = None;
        while low < high {
            let middle = low + (high - low) / 2;
            let probe = entries.get(middle).map(|(probe, _)| probe);
            let order = match probe {
                Some(probe) => canonical_order(&entry_key, probe, key, library)?,
                None => return Err(NotConstant::Expression),
            };
            match order {
                Ordering::Less => high = middle,
                Ordering::Greater => low = middle + 1,
                Ordering::Equal => {
                    found = Some(middle);
                    break;
                }
            }
        }
        match found.and_then(|at| entries.get_mut(at)) {
            Some(entry) => entry.1 = entry_value,
            None => entries.insert(low, (entry_key, entry_value)),
        }
    }
    Ok(entries)
}

/// The canonical order of two constant keys of type `key`: `false` before
/// `true`, numeric order for integers, scalar value for `Char`, by scalar
/// for `Str`, by byte for `Bytes`, by the UTF-8 bytes of the spelling for
/// atoms, and component-wise for tuples and, in canonical field order,
/// records. A key type ordered by its own `Ordered` implementation is
/// [`NotConstant::Dict`].
fn canonical_order(
    left: &Value,
    right: &Value,
    key: &Type,
    library: &dyn Fn(&TypeId) -> Option<Library>,
) -> Result<Ordering, NotConstant> {
    let mut pending = vec![(left, right, key)];
    while let Some((left, right, key)) = pending.pop() {
        let order = match (left, right, key) {
            (Value::I8(l), Value::I8(r), _) => l.cmp(r),
            (Value::I16(l), Value::I16(r), _) => l.cmp(r),
            (Value::I32(l), Value::I32(r), _) => l.cmp(r),
            (Value::I64(l), Value::I64(r), _) => l.cmp(r),
            (Value::U8(l), Value::U8(r), _) => l.cmp(r),
            (Value::U16(l), Value::U16(r), _) => l.cmp(r),
            (Value::U32(l), Value::U32(r), _) => l.cmp(r),
            (Value::U64(l), Value::U64(r), _) => l.cmp(r),
            (Value::Char(l), Value::Char(r), _) => l.cmp(r),
            (Value::Atom(l), Value::Atom(r), _) => l.as_bytes().cmp(r.as_bytes()),
            // UTF-8 byte order is scalar order.
            (Value::Str(l), Value::Str(r), _) => l.as_bytes().cmp(r.as_bytes()),
            (Value::Bytes(l), Value::Bytes(r), _) => l.cmp(r),
            (
                Value::Variant { case: l, .. },
                Value::Variant { case: r, .. },
                Type::Declared(id, _),
            ) if library(id) == Some(Library::Bool) => {
                (l == "true").cmp(&(r == "true"))
            }
            (Value::Tuple(l), Value::Tuple(r), Type::Tuple(types))
                if l.len() == types.len() && r.len() == types.len() =>
            {
                // The first component is compared first: pushed last.
                for ((l, r), ty) in l.iter().zip(r).zip(types).rev() {
                    pending.push((l, r, ty));
                }
                continue;
            }
            (Value::Record(l), Value::Record(r), Type::Record(fields)) => {
                let mut components = Vec::new();
                for (name, ty) in fields.entries() {
                    match (field(l, name), field(r, name)) {
                        (Some(l), Some(r)) => components.push((l, r, ty)),
                        _ => return Err(NotConstant::Expression),
                    }
                }
                pending.extend(components.into_iter().rev());
                continue;
            }
            (_, _, Type::Declared(..)) => return Err(NotConstant::Dict),
            _ => return Err(NotConstant::Expression),
        };
        if order != Ordering::Equal {
            return Ok(order);
        }
    }
    Ok(Ordering::Equal)
}

/// The value of the field `name` among `fields`.
fn field<'v>(fields: &'v [(String, Value)], name: &str) -> Option<&'v Value> {
    fields
        .iter()
        .find_map(|(field, value)| (field == name).then_some(value))
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
                // A default is a closed constant at its label's type with
                // every open form `Never`, so it takes the instantiated
                // label's type the same way.
                if let Type::Fn(signature) = &body.value_type {
                    for (label, default) in &mut body.defaults {
                        if let Some(ty) = signature.labels.get(label)
                            && let Ok(retyped) = vibra_ir::Constant::new(
                                ty.at_never(),
                                default.value().clone(),
                            )
                        {
                            *default = retyped;
                        }
                    }
                }
                body.captures.iter_mut().for_each(replace);
                pending.extend(body.nodes_mut());
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
