//! Decides which parts of a checked program the emitter cannot lower.
//!
//! Since Step 11 that is nothing a checked program can contain: every node
//! below has a lowering, a type has a class (`types::class_of` is total), and
//! every registry row is a few instructions, a native import, or a native that
//! runs its checked body. The walk remains as the guard that a program the
//! checker could not have produced, such as a hand-built one with a registry row
//! the closed tables do not know, produces an error and never a module that
//! omits part of it.
//!
//! The history of the walk, by step:
//!
//! Step 5b lowers the literals and sequences of Step 5a and, with them: module
//! values; functions with fixed and labelled parameters; reads of a local;
//! `let`, `if`, and `return`; direct calls in non-tail position; and the data
//! forms (a record, a variant, a wrapper, a tuple, their projections, and the
//! widening of a member into a union), over every type that has a lowered value
//! kind. Step 6 adds function values, `lambda` and what it captures, calls of
//! every kind but a contract call (tail or not), omitted labelled operands, and
//! the generic types, so a function type and a generic parameter have value
//! kinds too. Step 7 adds `match` with every pattern kind (and so destructuring
//! in `let`, a parameter, and a `lambda`, and `let-else`, which the checked IR
//! writes as a `match`), `as` narrowing, `try`, and `never`; the array pattern and
//! a wrapper pattern over a text type wait for Step 8b. Every other node is
//! reported, so a program that the emitter cannot
//! lower completely produces an error and never a module that omits part of it.
//! Each `match` below is exhaustive on purpose: a new checked-IR variant cannot
//! compile until it is given a disposition here.

use vibra_ir::{CallTarget, CheckedFunction, CheckedProgram, Expr, Pattern, Type};

use crate::form::{Form, NotLowered, UnloweredForm};
use crate::pattern::decompose;
use crate::types::{TypeEnv, class_of};

/// The forms of one expression that the emitter does not lower, for the
/// lowering to return when it meets the expression. It names at least one form
/// for an expression the lowering does not handle.
pub(crate) fn forms_of(env: &TypeEnv<'_>, expr: &Expr) -> NotLowered {
    let mut found = Vec::new();
    walk(env, expr, &mut found);
    NotLowered::from_uses(found).unwrap_or_else(|| NotLowered::single(Form::Malformed))
}

/// The form that reports a type whose components the lowering cannot find:
/// a type of an unlowered kind, or a declared type with no definition.
pub(crate) fn forms_of_type(ty: &Type, origin: &vibra_ir::SourceOrigin) -> NotLowered {
    NotLowered::type_of(class_of(ty).err().unwrap_or("shape"), Some(origin.clone()))
}

/// Every use of a form the emitter does not lower, in program order.
pub(crate) fn unlowered(program: &CheckedProgram) -> Vec<UnloweredForm> {
    let env = TypeEnv::new(program.types()).with_functions(program.functions());
    let mut found = Vec::new();
    for global in program.globals() {
        let origin = Some(global.origin().clone());
        type_forms(&global.value_type(), origin, &mut found);
        walk(&env, global.initializer(), &mut found);
    }
    for function in program.functions() {
        function_forms(&env, function, &mut found);
    }
    found
}

/// Reports a type no lowered value kind represents.
fn type_forms(
    ty: &Type,
    origin: Option<vibra_ir::SourceOrigin>,
    found: &mut Vec<UnloweredForm>,
) {
    if let Err(kind) = class_of(ty) {
        found.push(UnloweredForm::new(Form::Malformed, origin).with_detail(kind));
    }
}

fn function_forms(
    env: &TypeEnv<'_>,
    function: &CheckedFunction,
    found: &mut Vec<UnloweredForm>,
) {
    let origin = Some(function.origin().clone());
    let signature = function.signature();
    // A variadic parameter is one more slot, which a call fills with the
    // collection it already packed, so it needs nothing of its own.
    for slot in signature.slot_types() {
        type_forms(&slot, origin.clone(), found);
    }
    type_forms(&signature.result(), origin.clone(), found);
    // A function that implements a contract member is an ordinary function
    // here: what dispatches to it is a contract call, which is reported where
    // it is made (Step 9).
    walk(env, function.module_body().0, found);
}

/// Records a node and every node below it that the emitter does not lower.
fn walk(env: &TypeEnv<'_>, expr: &Expr, found: &mut Vec<UnloweredForm>) {
    type_forms(&expr.result_type(), Some(expr.origin().clone()), found);
    match expr {
        // Every primitive literal is lowered: a scalar is an immediate, and
        // `bool`, `str`, `bytes`, and an atom are arena objects. A read of a
        // local or of a module value is a copy of its cell.
        Expr::Literal { .. } | Expr::Variable { .. } | Expr::Global { .. } => {}
        // A sequence evaluates its expressions in order and has the value of
        // the last, so it adds nothing to what its expressions already need.
        Expr::Sequence { expressions, .. } => walk_all(env, expressions, found),
        // A row of the primitive table is a few instructions the lowering
        // writes inline; every other row is named with its registry symbol,
        // for the step that owns it.
        Expr::External {
            intrinsic,
            arguments,
            ..
        } => {
            if crate::primitive::plan(*intrinsic).is_none()
                && intrinsic.native_import().is_none()
            {
                found.push(
                    UnloweredForm::new(Form::Malformed, Some(expr.origin().clone()))
                        .with_detail(intrinsic.symbol()),
                );
            }
            walk_all(env, arguments, found);
        }
        // A host operation is one import of the closed registry; its operands
        // are the only expressions it adds.
        Expr::Host { arguments, .. } => walk_all(env, arguments, found),
        // An omitted labelled operand is the callee's default, a constant; a
        // module function used as a value, a `lambda`, and a read of what it
        // captured are the function values of a call, and a function value is
        // an arena value like any other.
        Expr::Default { .. } | Expr::Function { .. } | Expr::Captured { .. } => {}
        Expr::Closure { captures, body, .. } => {
            walk_all(env, captures, found);
            walk(env, body, found);
        }
        Expr::Let { value, body, .. } => {
            walk(env, value, found);
            walk(env, body, found);
        }
        // A `match` is its arms in order, each a pattern and a result. Every
        // pattern kind is lowered but the array pattern, and a wrapper pattern
        // over a text type, which belong to Step 8b.
        Expr::Match {
            scrutinee, arms, ..
        } => {
            walk(env, scrutinee, found);
            let scrutinee_type = scrutinee.result_type();
            for arm in arms {
                pattern_forms(env, &arm.pattern, &scrutinee_type, expr.origin(), found);
                walk(env, &arm.body, found);
            }
        }
        // Widening to a union attaches the member's discriminant, and widening
        // an atom singleton to `atom` is erased. Widening to an interface or
        // `any` has an interface type, which is reported as such.
        Expr::Widen { value, .. } => walk(env, value, found),
        // `try` inspects its operand and either continues with the payload or
        // leaves the activation, so it adds nothing to what its operand needs.
        Expr::Try { value, .. } => walk(env, value, found),
        Expr::Return { value, .. } => walk(env, value, found),
        Expr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            walk(env, condition, found);
            walk(env, then_branch, found);
            walk(env, else_branch, found);
        }
        Expr::Call {
            target, arguments, ..
        } => {
            // A call of a module function, of a function value, or of a
            // contract member, in tail position or not, is one mechanism of
            // frames: a contract call selects the function it enters from run-time
            // types (Step 9).
            if let CallTarget::Indirect { callee, .. } = target {
                walk(env, callee, found);
            }
            walk_all(env, arguments, found);
        }
        Expr::Record { fields, .. } => {
            for (_, field) in fields {
                walk(env, field, found);
            }
        }
        Expr::Variant { payload, .. } => {
            if let Some(payload) = payload {
                walk(env, payload, found);
            }
        }
        // A wrapper over a builtin text type is written over an array, and
        // wrapping converts it with a row of the registry.
        Expr::Wrap { value, .. } => walk(env, value, found),
        Expr::Project { record, .. } => walk(env, record, found),
        Expr::Tuple { components, .. } => walk_all(env, components, found),
        Expr::TupleProject { tuple, .. } => walk(env, tuple, found),
        Expr::Array { elements, .. } => walk_all(env, elements, found),
        // A dict orders its keys by a closed shape, by the one `compare` of a
        // declared key, or, for any other key, by dispatch from a run-time type
        // (Step 9), so every dict is lowered.
        Expr::Dict { entries, .. } => {
            for (key, value) in entries {
                walk(env, key, found);
                walk(env, value, found);
            }
        }
        Expr::Lookup {
            collection, key, ..
        } => {
            walk(env, collection, found);
            walk(env, key, found);
        }
    }
}

/// Records what a pattern, met at a value of type `ty`, asks of the lowering
/// that it cannot do, reading the same table the lowering reads: the forms of
/// the pattern itself, and the type of each binder.
fn pattern_forms(
    env: &TypeEnv<'_>,
    pattern: &Pattern,
    ty: &Type,
    origin: &vibra_ir::SourceOrigin,
    found: &mut Vec<UnloweredForm>,
) {
    let node = match decompose(env, pattern, ty, origin) {
        Ok(node) => node,
        Err(not_lowered) => {
            found.extend(not_lowered.forms().iter().cloned());
            return;
        }
    };
    if let Some((_, binder)) = node.binder {
        type_forms(binder, Some(origin.clone()), found);
    }
    for child in &node.children {
        pattern_forms(env, child.pattern, &child.ty, origin, found);
    }
}

fn walk_all(env: &TypeEnv<'_>, expressions: &[Expr], found: &mut Vec<UnloweredForm>) {
    for expression in expressions {
        walk(env, expression, found);
    }
}
