//! The static types the interpreter needs at run time: the type of a node,
//! which a widening holds and an evaluation's result is observed at, and
//! the matching that selects an implementation by its key.
//!
//! The checked IR has passed its shape check, so every node has one type;
//! a node whose type cannot be worked out is an invalid checked program.

use vibra_ir::{
    CallTarget, CheckedModuleSet, ContractCall, Expr, Library, Node, Substitution,
    Type, TypeBody,
};

use crate::machine::Halt;

fn invalid(message: impl Into<String>) -> Halt {
    Halt::Invalid(message.into())
}

/// The type of a callable's application: its result, or the plan a
/// procedure's application builds.
fn applied(callee: &Type) -> Result<Type, Halt> {
    match callee {
        Type::Fn(signature) => Ok(signature.result.clone()),
        Type::Proc(signature) => Ok(Type::Intent(Box::new(vibra_ir::IntentType {
            result: signature.result.clone(),
            effects: signature.effects.clone(),
        }))),
        other => Err(invalid(format!("a call of {other}"))),
    }
}

/// The type of a function at `type_arguments`.
pub(crate) fn function_type(
    set: &CheckedModuleSet,
    function: usize,
    type_arguments: &[Type],
) -> Result<Type, Halt> {
    set.functions()
        .get(function)
        .map(|function| function.signature.type_info().instantiate(type_arguments))
        .ok_or_else(|| invalid(format!("no function {function}")))
}

/// The type of an interface member at a contract call's substitution.
pub(crate) fn member_type(
    set: &CheckedModuleSet,
    call: &ContractCall,
) -> Result<Type, Halt> {
    let member = set
        .interfaces()
        .get(&call.interface)
        .and_then(|interface| interface.member(&call.member))
        .ok_or_else(|| invalid(format!("no member `{}`", call.member)))?;
    let binders = call
        .arguments
        .iter()
        .chain(&call.member_arguments)
        .cloned()
        .collect::<Vec<_>>();
    Ok(member.signature.type_info().substitute(&Substitution {
        binders: &binders,
        self_type: Some(&call.receiver),
        associated: Some(&call.choices),
    }))
}

/// The type of a call's callee.
pub(crate) fn callee_type(
    set: &CheckedModuleSet,
    target: &CallTarget,
) -> Result<Type, Halt> {
    match target {
        CallTarget::Direct {
            function,
            type_arguments,
        } => function_type(set, *function, type_arguments),
        CallTarget::Value(callee) => type_of(set, callee),
        CallTarget::Contract(call) => member_type(set, call),
    }
}

/// The type of the field `field` of a record of type `record`.
pub(crate) fn field_type(
    set: &CheckedModuleSet,
    record: &Type,
    field: &str,
) -> Result<Type, Halt> {
    let found = match record {
        Type::Record(fields) => fields.get(field).cloned(),
        Type::Declared(id, arguments) => {
            match set.types().definition(id).map(|d| d.body()) {
                Some(TypeBody::Record(fields)) => {
                    fields.iter().find_map(|(name, ty)| {
                        (name == field).then(|| ty.instantiate(arguments))
                    })
                }
                _ => None,
            }
        }
        _ => None,
    };
    found.ok_or_else(|| invalid(format!("{record} has no field `{field}`")))
}

/// The type of the value of a construction of `value_type`: a `Result`
/// when the type has an invariant with a consequence.
pub(crate) fn construction_type(
    set: &CheckedModuleSet,
    value_type: &Type,
) -> Result<Type, Halt> {
    if let Type::Declared(id, _) = value_type
        && let Some(invariant) = set.types().definition(id).and_then(|d| d.invariant())
        && let Some(consequence) = &invariant.consequence
    {
        return library_type(
            set,
            Library::Result,
            vec![value_type.clone(), consequence.value_type().clone()],
        );
    }
    Ok(value_type.clone())
}

pub(crate) fn library_type(
    set: &CheckedModuleSet,
    library: Library,
    arguments: Vec<Type>,
) -> Result<Type, Halt> {
    set.library()
        .declared(library, arguments)
        .map_err(|error| invalid(error.to_string()))
}

/// What is left to work out of a node's type once an operand's type is
/// known.
enum Pending<'n> {
    /// The operand was the `then` branch of an `if` with this `else`.
    Else(&'n Node),
    /// The operand was a record whose field this is.
    Field(&'n str),
    /// The operand was the collection of a lookup.
    Lookup,
    /// The operand was the carrier of a `try`.
    Try,
    /// The operand was the callee of a call of a value.
    Applied,
}

/// The static type of `node`, open in the binders of its activation.
pub(crate) fn type_of(set: &CheckedModuleSet, node: &Node) -> Result<Type, Halt> {
    type_of_expr(set, &node.expr)
}

/// The static type of `expr`, open in the binders of its activation. The
/// operands the type depends on are followed with a work list.
pub(crate) fn type_of_expr(set: &CheckedModuleSet, expr: &Expr) -> Result<Type, Halt> {
    let mut pending = Vec::new();
    let mut expr = expr;
    loop {
        let mut ty = loop {
            match expr {
                Expr::Sequence(statements) => {
                    expr = &statements
                        .last()
                        .ok_or_else(|| invalid("an empty sequence"))?
                        .expr;
                }
                Expr::Let { body, .. } | Expr::LetElse { body, .. } => {
                    expr = &body.expr
                }
                Expr::If {
                    then_branch,
                    else_branch: Some(else_branch),
                    ..
                } => {
                    pending.push(Pending::Else(else_branch));
                    expr = &then_branch.expr;
                }
                Expr::Field { record, field } => {
                    pending.push(Pending::Field(field));
                    expr = &record.expr;
                }
                Expr::Lookup { collection, .. } => {
                    pending.push(Pending::Lookup);
                    expr = &collection.expr;
                }
                Expr::Try(value) => {
                    pending.push(Pending::Try);
                    expr = &value.expr;
                }
                Expr::Call {
                    target: CallTarget::Value(callee),
                    ..
                }
                | Expr::TailCall {
                    target: CallTarget::Value(callee),
                    ..
                } => {
                    pending.push(Pending::Applied);
                    expr = &callee.expr;
                }
                other => break own_type(set, other)?,
            }
        };
        loop {
            match pending.pop() {
                None => return Ok(ty),
                Some(Pending::Else(else_branch)) => {
                    if ty == Type::Never {
                        expr = &else_branch.expr;
                        break;
                    }
                }
                Some(Pending::Field(field)) => ty = field_type(set, &ty, field)?,
                Some(Pending::Lookup) => ty = lookup_type(set, &ty)?,
                Some(Pending::Try) => ty = try_type(set, &ty)?,
                Some(Pending::Applied) => ty = applied(&ty)?,
            }
        }
    }
}

fn lookup_type(set: &CheckedModuleSet, collection: &Type) -> Result<Type, Halt> {
    let library = set.library();
    let item = if let Type::List(element) = collection {
        element.as_ref().clone()
    } else if library.arguments_of(Library::Str, collection).is_some() {
        Type::Char
    } else if library.arguments_of(Library::Bytes, collection).is_some() {
        Type::U8
    } else if let Some([_, value]) = library.arguments_of(Library::Dict, collection) {
        value.clone()
    } else {
        return Err(invalid(format!("a lookup on {collection}")));
    };
    library_type(set, Library::Option, vec![item])
}

fn try_type(set: &CheckedModuleSet, carrier: &Type) -> Result<Type, Halt> {
    let library = set.library();
    library
        .arguments_of(Library::Option, carrier)
        .or_else(|| library.arguments_of(Library::Result, carrier))
        .and_then(<[Type]>::first)
        .cloned()
        .ok_or_else(|| invalid(format!("a try on {carrier}")))
}

/// The type of a node whose type does not depend on an operand's.
fn own_type(set: &CheckedModuleSet, expr: &Expr) -> Result<Type, Halt> {
    let library = set.library();
    Ok(match expr {
        Expr::Literal(constant) => constant.value_type().clone(),
        Expr::Intrinsic {
            intrinsic,
            type_arguments,
            ..
        } => intrinsic
            .row()
            .result
            .instantiate(library, type_arguments)
            .map_err(|error| invalid(error.to_string()))?,
        Expr::Variable { value_type, .. } | Expr::Captured { value_type, .. } => {
            value_type.clone()
        }
        Expr::Global(index) => set
            .globals()
            .get(*index)
            .map(|global| global.value_type.clone())
            .ok_or_else(|| invalid(format!("no module value {index}")))?,
        Expr::Function {
            function,
            type_arguments,
        } => function_type(set, *function, type_arguments)?,
        Expr::Closure { closure, .. } => closure.value_type.clone(),
        Expr::Match { value_type, .. } => value_type.clone(),
        Expr::If { .. } => Type::Void,
        Expr::Construct { value_type, .. } => construction_type(set, value_type)?,
        Expr::Call { target, .. } | Expr::TailCall { target, .. } => {
            applied(&callee_type(set, target)?)?
        }
        Expr::Return(_) => Type::Never,
        Expr::WidenToInterface { value_type, .. } => value_type.clone(),
        Expr::WidenAtom(_) => Type::Atom,
        Expr::Default(value_type) => value_type.clone(),
        Expr::Sequence(_)
        | Expr::Let { .. }
        | Expr::LetElse { .. }
        | Expr::Field { .. }
        | Expr::Lookup { .. }
        | Expr::Try(_) => {
            return Err(invalid("an unreachable node form"));
        }
    })
}

/// Whether two types have the same outermost form.
fn same_head(left: &Type, right: &Type) -> bool {
    match (left, right) {
        (Type::AtomSingleton(left), Type::AtomSingleton(right))
        | (Type::Associated(left), Type::Associated(right)) => left == right,
        (Type::List(_), Type::List(_)) => true,
        (Type::Tuple(left), Type::Tuple(right)) => left.len() == right.len(),
        (Type::Record(left), Type::Record(right)) => left.names().eq(right.names()),
        (Type::Fn(left), Type::Fn(right)) => {
            left.parameters.len() == right.parameters.len()
                && left.labels.names().eq(right.labels.names())
        }
        (Type::Proc(left), Type::Proc(right)) => {
            left.parameters.len() == right.parameters.len()
                && left.labels.names().eq(right.labels.names())
                && left.effects == right.effects
        }
        (Type::Intent(left), Type::Intent(right)) => left.effects == right.effects,
        (Type::Declared(left, a), Type::Declared(right, b)) => {
            left == right && a.len() == b.len()
        }
        (Type::Interface(left, a, x), Type::Interface(right, b, y)) => {
            left == right && a.len() == b.len() && x.names().eq(y.names())
        }
        (left, right) => left.components().is_empty() && left == right,
    }
}

/// Matches the open `pattern`, whose binder references are the variables
/// of `assignment`, against the closed `actual`, extending `assignment`.
pub(crate) fn match_type(
    pattern: &Type,
    actual: &Type,
    assignment: &mut [Option<Type>],
) -> bool {
    if let Type::Binder(index) = pattern
        && let Some(slot) = assignment.get_mut(*index)
    {
        return match slot {
            Some(bound) => bound == actual,
            None => {
                *slot = Some(actual.clone());
                true
            }
        };
    }
    same_head(pattern, actual)
        && pattern
            .components()
            .into_iter()
            .zip(actual.components())
            .all(|(pattern, actual)| match_type(pattern, actual, assignment))
}

/// Whether `Self` occurs in `ty`.
pub(crate) fn mentions_self(ty: &Type) -> bool {
    let mut found = false;
    ty.visit(&mut |inner| found |= *inner == Type::SelfType);
    found
}
