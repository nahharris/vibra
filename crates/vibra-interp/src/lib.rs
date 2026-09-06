//! Deterministic execution for the checked M2 primitive and binding IR.
//!
//! No parser, resolver, type checker, filesystem, clock, random source, or
//! host provider is reachable from this crate.  The only executable input is
//! [`vibra_ir::CheckedProgram`], which is the controlled boundary produced by
//! `vibra-types`.

#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used
    )
)]

use std::fmt;

use vibra_ir::{CheckedProgram, Expr, Value};

/// One successful reference-interpreter run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Execution {
    value: Value,
    audit_trace: Vec<String>,
}

impl Execution {
    /// The value returned by the selected entry function.
    #[must_use]
    pub const fn value(&self) -> &Value {
        &self.value
    }

    /// Ordered audit events.  Pure M2 literals always return an empty trace.
    #[must_use]
    pub fn audit_trace(&self) -> &[String] {
        &self.audit_trace
    }

    /// Canonical typed value observation.
    #[must_use]
    pub fn canonical_result(&self) -> String {
        self.value.canonical_observation()
    }
}

/// A failure at the checked-program execution boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeError {
    /// The checked program contained no executable entry.
    NoEntry,
    /// A checked IR invariant was violated before an expression ran.
    InvalidBody {
        /// The function that contained the invalid body.
        function: String,
    },
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoEntry => {
                formatter.write_str("checked program has no entry function")
            }
            Self::InvalidBody { function } => {
                write!(formatter, "checked body for `{function}` is invalid")
            }
        }
    }
}

impl std::error::Error for RuntimeError {}

/// The pure M2 reference interpreter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Interpreter;

impl Interpreter {
    /// Executes the checked program's validated entry function.
    pub fn run(program: &CheckedProgram) -> Result<Execution, RuntimeError> {
        let mut machine = Machine {
            program,
            globals: vec![GlobalState::Uninitialized; program.globals().len()],
        };
        let function = program.entry();
        let arguments = vec![None; function.slot_count()];
        let value = machine
            .evaluate_function(
                program
                    .functions()
                    .iter()
                    .position(|candidate| std::ptr::eq(candidate, function))
                    .unwrap_or(0),
                arguments,
            )
            .ok_or_else(|| RuntimeError::InvalidBody {
                function: function.name().to_owned(),
            })?;
        if value.ty() != function.signature().result() {
            return Err(RuntimeError::InvalidBody {
                function: function.name().to_owned(),
            });
        }
        Ok(Execution {
            value,
            audit_trace: Vec::new(),
        })
    }
}

/// Convenience entry point for the reference interpreter.
pub fn run(program: &CheckedProgram) -> Result<Execution, RuntimeError> {
    Interpreter::run(program)
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum GlobalState {
    Uninitialized,
    Evaluating,
    Ready(Value),
}

struct Machine<'a> {
    program: &'a CheckedProgram,
    globals: Vec<GlobalState>,
}

impl Machine<'_> {
    fn evaluate_function(
        &mut self,
        index: usize,
        slots: Vec<Option<Value>>,
    ) -> Option<Value> {
        let function = self.program.functions().get(index)?;
        if slots.len() != function.slot_count()
            || slots
                .iter()
                .take(function.signature().parameters().len())
                .zip(function.signature().parameters())
                .any(|(value, expected)| {
                    value.as_ref().is_some_and(|value| value.ty() != *expected)
                })
        {
            return None;
        }
        self.evaluate(function.body(), slots)
    }

    fn evaluate(
        &mut self,
        expression: &Expr,
        mut slots: Vec<Option<Value>>,
    ) -> Option<Value> {
        match expression {
            Expr::Literal { value, .. } => Some(value.clone()),
            Expr::Sequence { expressions, .. } => {
                let mut result = Value::Void;
                for expression in expressions {
                    result = self.evaluate(expression, slots.clone())?;
                }
                Some(result)
            }
            Expr::Variable {
                slot, value_type, ..
            } => slots
                .get(*slot)
                .and_then(Option::as_ref)
                .filter(|value| value.ty() == *value_type)
                .cloned(),
            Expr::Global {
                index, value_type, ..
            } => self
                .evaluate_global(*index)
                .filter(|value| value.ty() == *value_type),
            Expr::Let {
                slot, value, body, ..
            } => {
                let value = self.evaluate(value, slots.clone())?;
                if let Some(slot) = slot {
                    *slots.get_mut(*slot)? = Some(value);
                }
                self.evaluate(body, slots)
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let condition = self.evaluate(condition, slots.clone())?;
                match condition {
                    Value::Bool(true) => self.evaluate(then_branch, slots),
                    Value::Bool(false) => self.evaluate(else_branch, slots),
                    _ => None,
                }
            }
            Expr::Call {
                function,
                arguments,
                result,
                ..
            } => {
                let mut values = Vec::with_capacity(arguments.len());
                for argument in arguments {
                    values.push(self.evaluate(argument, slots.clone())?);
                }
                let callee = self.program.functions().get(*function)?;
                if callee.signature().parameters().len() != values.len()
                    || callee
                        .signature()
                        .parameters()
                        .iter()
                        .zip(&values)
                        .any(|(expected, value)| value.ty() != *expected)
                    || callee.signature().result() != *result
                {
                    return None;
                }
                let call_slots = values
                    .into_iter()
                    .map(Some)
                    .chain(std::iter::repeat(None))
                    .take(callee.slot_count())
                    .collect();
                self.evaluate_function(*function, call_slots)
            }
        }
    }

    fn evaluate_global(&mut self, index: usize) -> Option<Value> {
        let state = self.globals.get(index)?.clone();
        if state == GlobalState::Evaluating {
            return None;
        }
        if let GlobalState::Ready(value) = state {
            return Some(value);
        }
        *self.globals.get_mut(index)? = GlobalState::Evaluating;
        let global = self.program.globals().get(index)?;
        let value =
            self.evaluate(global.initializer(), vec![None; global.slot_count()]);
        if let Some(value) = &value {
            *self.globals.get_mut(index)? = GlobalState::Ready(value.clone());
        }
        value
    }
}

#[cfg(test)]
mod tests {
    use vibra_diagnostics::ByteSpan;
    use vibra_ir::{
        CheckedFunction, CheckedGlobal, Expr, FunctionSignature, PrimitiveType,
        SourceOrigin, Value,
    };

    use super::run;

    #[test]
    fn a_checked_literal_has_no_audit_events() {
        let origin = SourceOrigin::new("test.vib", ByteSpan::new(0, 1));
        let function = CheckedFunction::new(
            "answer",
            FunctionSignature::new(Vec::new(), PrimitiveType::I32),
            vibra_ir::Expr::literal(vibra_ir::Value::I32(42), origin.clone()),
            origin,
        )
        .expect("valid checked function");
        let program = vibra_ir::CheckedProgram::try_new(vec![function], 0)
            .expect("valid checked program");
        let result = run(&program).expect("execution");
        assert_eq!(result.value(), &vibra_ir::Value::I32(42));
        assert!(result.audit_trace().is_empty());
    }

    #[test]
    fn execution_uses_the_checked_entry_index() {
        let first_origin = SourceOrigin::new("test.vib", ByteSpan::new(0, 1));
        let first = CheckedFunction::new(
            "first",
            FunctionSignature::new(Vec::new(), PrimitiveType::I32),
            vibra_ir::Expr::literal(vibra_ir::Value::I32(1), first_origin.clone()),
            first_origin,
        )
        .expect("valid first function");
        let entry_origin = SourceOrigin::new("test.vib", ByteSpan::new(2, 3));
        let entry = CheckedFunction::new(
            "entry",
            FunctionSignature::new(Vec::new(), PrimitiveType::I32),
            vibra_ir::Expr::literal(vibra_ir::Value::I32(2), entry_origin.clone()),
            entry_origin,
        )
        .expect("valid entry function");
        let program = vibra_ir::CheckedProgram::try_new(vec![first, entry], 1)
            .expect("valid checked program");

        let result = run(&program).expect("execution");

        assert_eq!(result.value(), &vibra_ir::Value::I32(2));
    }

    #[test]
    fn an_untaken_branch_does_not_force_a_global_initializer() {
        let origin = SourceOrigin::new("test.vib", ByteSpan::new(0, 1));
        let global = CheckedGlobal::new(
            "value",
            PrimitiveType::I32,
            Expr::literal(Value::I32(9), origin.clone()),
            origin.clone(),
        )
        .expect("valid checked global shape");
        let body = Expr::if_expression(
            Expr::literal(Value::Bool(true), origin.clone()),
            Expr::literal(Value::I32(7), origin.clone()),
            Expr::global(0, PrimitiveType::I32, origin.clone()),
            origin.clone(),
        );
        let function = CheckedFunction::new(
            "answer",
            FunctionSignature::new(Vec::new(), PrimitiveType::I32),
            body,
            origin,
        )
        .expect("valid checked function");
        let program = vibra_ir::CheckedProgram::try_new_with_globals(
            vec![global],
            vec![function],
            0,
        )
        .expect("valid checked program");

        let mut machine = super::Machine {
            program: &program,
            globals: vec![super::GlobalState::Uninitialized; program.globals().len()],
        };
        let result = machine
            .evaluate_function(0, vec![None; program.entry().slot_count()])
            .expect("untaken branch must not execute");

        assert_eq!(result, Value::I32(7));
        assert_eq!(machine.globals[0], super::GlobalState::Uninitialized);
    }
}
