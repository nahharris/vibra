//! Decides which parts of a checked program the emitter lowers.
//!
//! Step 5b lowers the literals and sequences of Step 5a and, with them: module
//! values; functions with fixed and labelled parameters; reads of a local;
//! `let`, `if`, and `return`; direct calls in non-tail position; and the data
//! forms (a record, a variant, a wrapper, a tuple, their projections, and the
//! widening of a member into a union), over every type that has a lowered value
//! kind. Every other node is reported, so a program that the emitter cannot
//! lower completely produces an error and never a module that omits part of it.
//! Each `match` below is exhaustive on purpose: a new checked-IR variant cannot
//! compile until it is given a disposition here.

use vibra_ir::{CallTarget, CheckedFunction, CheckedProgram, Expr, Type};

use crate::form::{Form, NotLowered, UnloweredForm};
use crate::types::class_of;

/// The forms of one expression that the emitter does not lower, for the
/// lowering to return when it meets the expression. It names at least one form
/// for an expression the lowering does not handle.
pub(crate) fn forms_of(expr: &Expr) -> NotLowered {
    let mut found = Vec::new();
    walk(expr, &mut found);
    NotLowered::from_uses(found).unwrap_or_else(|| NotLowered::single(Form::Type))
}

/// The form that reports a type whose components the lowering cannot find:
/// a type of an unlowered kind, or a declared type with no definition.
pub(crate) fn forms_of_type(ty: &Type, origin: &vibra_ir::SourceOrigin) -> NotLowered {
    NotLowered::type_of(class_of(ty).err().unwrap_or("shape"), Some(origin.clone()))
}

/// Every use of a form the emitter does not lower, in program order.
pub(crate) fn unlowered(program: &CheckedProgram) -> Vec<UnloweredForm> {
    let mut found = Vec::new();
    for global in program.globals() {
        let origin = Some(global.origin().clone());
        type_forms(&global.value_type(), origin, &mut found);
        walk(global.initializer(), &mut found);
    }
    for function in program.functions() {
        function_forms(function, &mut found);
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
        found.push(UnloweredForm::new(Form::Type, origin).with_detail(kind));
    }
}

fn function_forms(function: &CheckedFunction, found: &mut Vec<UnloweredForm>) {
    let origin = Some(function.origin().clone());
    let signature = function.signature();
    if signature.variadic().is_some() {
        found.push(
            UnloweredForm::new(Form::Parameters, origin.clone()).with_detail("variadic"),
        );
    }
    for slot in signature.slot_types() {
        type_forms(&slot, origin.clone(), found);
    }
    type_forms(&signature.result(), origin.clone(), found);
    if function.test_assertion().is_some() {
        found.push(UnloweredForm::new(Form::TestAssertion, origin.clone()));
    }
    if function.implements().is_some() {
        found.push(UnloweredForm::new(Form::ContractImplementation, origin));
    }
    walk(function.body(), found);
}

/// Records a node and every node below it that the emitter does not lower.
fn walk(expr: &Expr, found: &mut Vec<UnloweredForm>) {
    type_forms(&expr.result_type(), Some(expr.origin().clone()), found);
    let mut report = |form: Form| {
        found.push(UnloweredForm::new(form, Some(expr.origin().clone())));
    };
    match expr {
        // Every primitive literal is lowered: a scalar is an immediate, and
        // `bool`, `str`, `bytes`, and an atom are arena objects. A read of a
        // local or of a module value is a copy of its cell.
        Expr::Literal { .. } | Expr::Variable { .. } | Expr::Global { .. } => {}
        // A sequence evaluates its expressions in order and has the value of
        // the last, so it adds nothing to what its expressions already need.
        Expr::Sequence { expressions, .. } => walk_all(expressions, found),
        Expr::External {
            intrinsic,
            arguments,
            ..
        } => {
            found.push(
                UnloweredForm::new(Form::External, Some(expr.origin().clone()))
                    .with_detail(intrinsic.symbol()),
            );
            walk_all(arguments, found);
        }
        Expr::Default { .. } => report(Form::Default),
        Expr::Function { .. } => report(Form::Function),
        Expr::Closure { captures, body, .. } => {
            report(Form::Closure);
            walk_all(captures, found);
            walk(body, found);
        }
        Expr::Captured { .. } => report(Form::Captured),
        Expr::Let { value, body, .. } => {
            walk(value, found);
            walk(body, found);
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            report(Form::Match);
            walk(scrutinee, found);
            for arm in arms {
                walk(&arm.body, found);
            }
        }
        // Widening to a union attaches the member's discriminant, and widening
        // an atom singleton to `atom` is erased. Widening to an interface or
        // `any` has an interface type, which is reported as such.
        Expr::Widen { value, .. } => walk(value, found),
        Expr::Try { value, .. } => {
            report(Form::Try);
            walk(value, found);
        }
        Expr::Return { value, .. } => walk(value, found),
        Expr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            walk(condition, found);
            walk(then_branch, found);
            walk(else_branch, found);
        }
        Expr::Call {
            target,
            arguments,
            tail,
            ..
        } => {
            // A direct call in non-tail position pushes a frame. Every other
            // call is the work of Step 6 (a tail call, an indirect call) or
            // Step 9 (a contract call).
            if !matches!((target, tail), (CallTarget::Direct(_), false)) {
                let kind = match (target, tail) {
                    (CallTarget::Direct(_), false) => "direct",
                    (CallTarget::Direct(_), true) => "tail-direct",
                    (CallTarget::Indirect { .. }, false) => "indirect",
                    (CallTarget::Indirect { .. }, true) => "tail-indirect",
                    (CallTarget::Contract { .. }, false) => "contract",
                    (CallTarget::Contract { .. }, true) => "tail-contract",
                };
                found.push(
                    UnloweredForm::new(Form::Call, Some(expr.origin().clone()))
                        .with_detail(kind),
                );
            }
            if let CallTarget::Indirect { callee, .. } = target {
                walk(callee, found);
            }
            walk_all(arguments, found);
        }
        Expr::Record { fields, .. } => {
            for (_, field) in fields {
                walk(field, found);
            }
        }
        Expr::Variant { payload, .. } => {
            if let Some(payload) = payload {
                walk(payload, found);
            }
        }
        // A wrapper over a builtin text type is written over an array.
        Expr::Wrap {
            value, value_type, ..
        } => {
            if matches!(value_type, Type::Str | Type::Bytes) {
                report(Form::Wrap);
            }
            walk(value, found);
        }
        Expr::Project { record, .. } => walk(record, found),
        Expr::Tuple { components, .. } => walk_all(components, found),
        Expr::TupleProject { tuple, .. } => walk(tuple, found),
        Expr::Array { elements, .. } => {
            report(Form::Array);
            walk_all(elements, found);
        }
        Expr::Dict { entries, .. } => {
            report(Form::Dict);
            for (key, value) in entries {
                walk(key, found);
                walk(value, found);
            }
        }
        Expr::Lookup {
            collection, key, ..
        } => {
            report(Form::Lookup);
            walk(collection, found);
            walk(key, found);
        }
    }
}

fn walk_all(expressions: &[Expr], found: &mut Vec<UnloweredForm>) {
    for expression in expressions {
        walk(expression, found);
    }
}
