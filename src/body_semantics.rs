//! Shared control-flow validation over lowered body IR.
//!
//! Both source frontends use these return-termination and affine-task checks.
//! Expression inference remains owned by the legacy lowerer until its
//! enum/alias dependencies can be extracted as one semantic unit.

use crate::lower::{Statement, TypeRef};
use anyhow::{bail, Result};
use std::collections::BTreeSet;

pub(crate) fn validate_function_body(
    statements: &[Statement],
    return_type: &TypeRef,
) -> Result<()> {
    if *return_type != TypeRef::Void && !body_terminates(statements) {
        bail!(
            "non-void function must end with `$return`, or with `$match` whose every arm ends with `$return`"
        );
    }
    Ok(())
}

pub(crate) fn validate_task_handles(statements: &[Statement]) -> Result<()> {
    fn walk(statements: &[Statement], mut live: BTreeSet<String>) -> Result<BTreeSet<String>> {
        for statement in statements {
            match statement {
                Statement::Spawn { handle, .. } => {
                    if !live.insert(handle.clone()) {
                        bail!("task handle `{handle}` is spawned more than once");
                    }
                }
                Statement::Join { handle, .. } => {
                    if !live.remove(handle) {
                        bail!("task handle `{handle}` is joined more than once or out of scope");
                    }
                }
                Statement::If {
                    then_body,
                    else_body,
                    ..
                } => {
                    let then_live = walk(then_body, live.clone())?;
                    let else_live = walk(else_body, live.clone())?;
                    if then_live != else_live {
                        bail!("both branches must consume the same task handles");
                    }
                    live = then_live;
                }
                Statement::Match { arms, .. } => {
                    let mut merged = None;
                    for arm in arms {
                        let arm_live = walk(&arm.body, live.clone())?;
                        if merged.as_ref().is_some_and(|prior| prior != &arm_live) {
                            bail!("every match arm must consume the same task handles");
                        }
                        merged = Some(arm_live);
                    }
                    if let Some(arm_live) = merged {
                        live = arm_live;
                    }
                }
                Statement::While { body, .. } | Statement::For { body, .. } => {
                    if walk(body, live.clone())? != live {
                        bail!("loops cannot create or consume task handles across iterations");
                    }
                }
                Statement::Task { body, .. } => {
                    let nested = walk(body, BTreeSet::new())?;
                    if !nested.is_empty() {
                        bail!("nested task left unjoined handles");
                    }
                }
                _ => {}
            }
        }
        Ok(live)
    }
    let live = walk(statements, BTreeSet::new())?;
    if !live.is_empty() {
        bail!("task handles must be joined before leaving their scope");
    }
    Ok(())
}

fn body_terminates(statements: &[Statement]) -> bool {
    match statements.last() {
        Some(Statement::Return(_)) => true,
        Some(Statement::Match { arms, .. }) => arms.iter().all(|arm| body_terminates(&arm.body)),
        Some(Statement::If {
            then_body,
            else_body,
            ..
        }) => body_terminates(then_body) && body_terminates(else_body),
        _ => false,
    }
}
