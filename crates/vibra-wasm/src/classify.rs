//! Decides which parts of a checked program the emitter lowers.
//!
//! Step 5a lowers a function with no parameter whose body is a primitive
//! literal, alone or in a sequence, and whose result is a type that a lowered
//! value represents: `void`, `bool`, `char`, the integers, the floats, `str`,
//! `bytes`, and the atoms. Every other node is reported, so a program that the
//! emitter cannot lower completely produces an error and never a module that
//! omits part of it.
//! Each `match` below is exhaustive on purpose: a new checked-IR variant
//! cannot compile until it is given a disposition here.

use vibra_ir::{CallTarget, CheckedFunction, CheckedProgram, Expr};

use crate::form::{Form, NotLowered, UnloweredForm};
use crate::lower;

/// The forms of one expression that the emitter does not lower, for the
/// lowering to return when it meets the expression. It names at least one form
/// for an expression the lowering does not handle.
pub(crate) fn forms_of(expr: &Expr) -> NotLowered {
    let mut found = Vec::new();
    walk(expr, &mut found);
    NotLowered::from_uses(found).unwrap_or_else(|| NotLowered::single(Form::Result))
}

/// Every use of a form the skeleton does not lower, in program order.
pub(crate) fn unlowered(program: &CheckedProgram) -> Vec<UnloweredForm> {
    let mut found = Vec::new();
    for global in program.globals() {
        found.push(UnloweredForm::new(
            Form::ModuleValue,
            Some(global.origin().clone()),
        ));
        walk(global.initializer(), &mut found);
    }
    for function in program.functions() {
        function_forms(function, &mut found);
    }
    found
}

fn function_forms(function: &CheckedFunction, found: &mut Vec<UnloweredForm>) {
    let origin = Some(function.origin().clone());
    let signature = function.signature();
    if !signature.parameters().is_empty()
        || !signature.labelled().is_empty()
        || signature.variadic().is_some()
    {
        found.push(UnloweredForm::new(Form::Parameters, origin.clone()));
    }
    if lower::class_of(&signature.result()).is_none() {
        found.push(UnloweredForm::new(Form::Result, origin.clone()));
    }
    if function.test_assertion().is_some() {
        found.push(UnloweredForm::new(Form::TestAssertion, origin.clone()));
    }
    if function.implements().is_some() {
        found.push(UnloweredForm::new(Form::ContractImplementation, origin));
    }
    walk(function.body(), found);
}

/// Records a node and every node below it that the skeleton does not lower.
fn walk(expr: &Expr, found: &mut Vec<UnloweredForm>) {
    let mut report = |form: Form| {
        found.push(UnloweredForm::new(form, Some(expr.origin().clone())));
    };
    match expr {
        // Every primitive literal is lowered: a scalar is an immediate, and
        // `bool`, `str`, `bytes`, and an atom are arena objects.
        Expr::Literal { .. } => {}
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
        Expr::Variable { .. } => report(Form::Variable),
        Expr::Global { .. } => report(Form::Global),
        Expr::Function { .. } => report(Form::Function),
        Expr::Closure { captures, body, .. } => {
            report(Form::Closure);
            walk_all(captures, found);
            walk(body, found);
        }
        Expr::Captured { .. } => report(Form::Captured),
        Expr::Let { value, body, .. } => {
            report(Form::Let);
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
        Expr::Widen { value, .. } => {
            report(Form::Widen);
            walk(value, found);
        }
        Expr::Try { value, .. } => {
            report(Form::Try);
            walk(value, found);
        }
        Expr::Return { value, .. } => {
            report(Form::Return);
            walk(value, found);
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            report(Form::If);
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
            if let CallTarget::Indirect { callee, .. } = target {
                walk(callee, found);
            }
            walk_all(arguments, found);
        }
        Expr::Record { fields, .. } => {
            report(Form::Record);
            for (_, field) in fields {
                walk(field, found);
            }
        }
        Expr::Variant { payload, .. } => {
            report(Form::Variant);
            if let Some(payload) = payload {
                walk(payload, found);
            }
        }
        Expr::Wrap { value, .. } => {
            report(Form::Wrap);
            walk(value, found);
        }
        Expr::Project { record, .. } => {
            report(Form::Project);
            walk(record, found);
        }
        Expr::Tuple { components, .. } => {
            report(Form::Tuple);
            walk_all(components, found);
        }
        Expr::TupleProject { tuple, .. } => {
            report(Form::TupleProject);
            walk(tuple, found);
        }
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
