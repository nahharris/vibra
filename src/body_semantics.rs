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
    // `Some(live)` means execution can fall through with `live` handles;
    // `None` means every path through the block exits via `return`. Keeping
    // those states distinct is necessary for a branch such as
    // `if ... return ... else spawn ...; join ...`: the returning arm does
    // not participate in the continuing-path handle merge.
    fn walk(
        statements: &[Statement],
        mut live: BTreeSet<String>,
    ) -> Result<Option<BTreeSet<String>>> {
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
                Statement::Return(_) => {
                    if !live.is_empty() {
                        bail!("task handles must be joined before leaving their scope");
                    }
                    // A return exits the current scope. Statements after it
                    // are unreachable and must not affect branch/loop state.
                    return Ok(None);
                }
                Statement::If {
                    then_body,
                    else_body,
                    ..
                } => {
                    let then_live = walk(then_body, live.clone())?;
                    let else_live = walk(else_body, live.clone())?;
                    live = match (then_live, else_live) {
                        (None, None) => return Ok(None),
                        (None, Some(else_live)) | (Some(else_live), None) => else_live,
                        (Some(then_live), Some(else_live)) => {
                            if then_live != else_live {
                                bail!("both branches must consume the same task handles");
                            }
                            then_live
                        }
                    };
                }
                Statement::Match { arms, .. } => {
                    let mut merged = None;
                    for arm in arms {
                        let arm_live = walk(&arm.body, live.clone())?;
                        if let Some(arm_live) = arm_live {
                            if merged.as_ref().is_some_and(|prior| prior != &arm_live) {
                                bail!("every match arm must consume the same task handles");
                            }
                            merged = Some(arm_live);
                        }
                    }
                    if let Some(arm_live) = merged {
                        live = arm_live;
                    } else if !arms.is_empty() {
                        return Ok(None);
                    }
                }
                Statement::While { body, .. } | Statement::For { body, .. } => {
                    if let Some(body_live) = walk(body, live.clone())? {
                        if body_live != live {
                            bail!("loops cannot create or consume task handles across iterations");
                        }
                    }
                }
                Statement::Task { body, .. } => {
                    if walk(body, BTreeSet::new())?.is_some_and(|nested| !nested.is_empty()) {
                        bail!("nested task left unjoined handles");
                    }
                }
                _ => {}
            }
        }
        Ok(Some(live))
    }
    if walk(statements, BTreeSet::new())?.is_some_and(|live| !live.is_empty()) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lower::{Expr, RuntimeValue};

    fn spawn(handle: &str) -> Statement {
        Statement::Spawn {
            handle: handle.to_string(),
            captures: Vec::new(),
            value: Expr::Value(RuntimeValue::Int(0)),
            result_type: TypeRef::Int64,
        }
    }

    fn join(handle: &str, var: &str) -> Statement {
        Statement::Join {
            handle: handle.to_string(),
            var: var.to_string(),
        }
    }

    fn return_statement() -> Statement {
        Statement::Return(Expr::Value(RuntimeValue::Void))
    }

    #[test]
    fn rejects_an_early_return_with_a_live_task_handle() {
        let error = validate_task_handles(&[spawn("worker"), return_statement()]).unwrap_err();
        assert!(error
            .to_string()
            .contains("task handles must be joined before leaving their scope"));
    }

    #[test]
    fn accepts_an_early_return_after_joining_all_task_handles() {
        validate_task_handles(&[spawn("worker"), join("worker", "value"), return_statement()])
            .expect("joined task handles may leave through return");
    }

    #[test]
    fn return_flow_merges_as_an_empty_task_handle_set() {
        let statements = [
            Statement::If {
                cond: Expr::Value(RuntimeValue::Bool(true)),
                then_body: vec![return_statement()],
                else_body: vec![spawn("worker")],
            },
            join("worker", "value"),
        ];
        validate_task_handles(&statements)
            .expect("a returning branch does not participate in the continuing merge");
    }

    #[test]
    fn loop_return_does_not_change_the_loop_entry_handle_set() {
        let statements = [Statement::While {
            cond: Expr::Value(RuntimeValue::Bool(true)),
            body: vec![return_statement()],
        }];
        validate_task_handles(&statements)
            .expect("a loop body that returns cannot leak a task handle");
    }
}
