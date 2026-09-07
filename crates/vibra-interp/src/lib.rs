//! Deterministic execution for the checked M2 function and binding IR.
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

use vibra_ir::{CheckedProgram, Expr, FunctionSignature, PrimitiveType, Value};

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
        let mut arguments = vec![None; function.slot_count()];
        for (offset, parameter) in function.signature().labelled().iter().enumerate() {
            if let Some(default) = parameter.default()
                && let Some(slot) =
                    arguments.get_mut(function.signature().parameters().len() + offset)
            {
                *slot = Some(RuntimeValue::Primitive(default.clone()));
            }
        }
        let value = machine
            .evaluate_function(
                program
                    .functions()
                    .iter()
                    .position(|candidate| std::ptr::eq(candidate, function))
                    .unwrap_or(0),
                arguments,
                Vec::new(),
            )
            .ok_or_else(|| RuntimeError::InvalidBody {
                function: function.name().to_owned(),
            })?;
        let RuntimeValue::Primitive(value) = value else {
            return Err(RuntimeError::InvalidBody {
                function: function.name().to_owned(),
            });
        };
        if !value.ty().same_shape(&function.signature().result()) {
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
    Ready(RuntimeValue),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum RuntimeValue {
    Primitive(Value),
    Function(Callable),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Callable {
    Named {
        index: usize,
        signature: FunctionSignature,
        captures: Vec<RuntimeValue>,
    },
    Lambda {
        signature: FunctionSignature,
        parameters: Vec<PrimitiveType>,
        body: Box<Expr>,
        captures: Vec<RuntimeValue>,
    },
}

struct Machine<'a> {
    program: &'a CheckedProgram,
    globals: Vec<GlobalState>,
}

impl Machine<'_> {
    fn evaluate_function(
        &mut self,
        index: usize,
        slots: Vec<Option<RuntimeValue>>,
        captures: Vec<RuntimeValue>,
    ) -> Option<RuntimeValue> {
        let function = self.program.functions().get(index)?;
        if slots.len() != function.slot_count()
            || !slots_match_signature(&slots, function.signature())
        {
            return None;
        }
        self.evaluate(function.body(), slots, &captures)
    }

    fn evaluate(
        &mut self,
        expression: &Expr,
        mut slots: Vec<Option<RuntimeValue>>,
        captures: &[RuntimeValue],
    ) -> Option<RuntimeValue> {
        match expression {
            Expr::Literal { value, .. } => Some(RuntimeValue::Primitive(value.clone())),
            Expr::External {
                intrinsic,
                arguments,
                ..
            } => {
                let mut values = Vec::with_capacity(arguments.len());
                for argument in arguments {
                    values.push(self.evaluate(argument, slots.clone(), captures)?);
                }
                let primitives = values
                    .into_iter()
                    .map(|value| match value {
                        RuntimeValue::Primitive(value) => Some(value),
                        RuntimeValue::Function(_) => None,
                    })
                    .collect::<Option<Vec<_>>>()?;
                let value = match intrinsic {
                    vibra_ir::external::CompilerIntrinsic::TextConcat => {
                        let [Value::Str(left), Value::Str(right)] =
                            primitives.as_slice()
                        else {
                            return None;
                        };
                        Value::Str(format!("{left}{right}"))
                    }
                    vibra_ir::external::CompilerIntrinsic::TextLength => {
                        let [Value::Str(value)] = primitives.as_slice() else {
                            return None;
                        };
                        Value::U64(value.chars().count() as u64)
                    }
                };
                Some(RuntimeValue::Primitive(value))
            }
            Expr::Default { .. } => None,
            Expr::Sequence { expressions, .. } => {
                let mut result = RuntimeValue::Primitive(Value::Void);
                for expression in expressions {
                    result = self.evaluate(expression, slots.clone(), captures)?;
                }
                Some(result)
            }
            Expr::Variable {
                slot, value_type, ..
            } => slots
                .get(*slot)
                .and_then(Option::as_ref)
                .filter(|value| runtime_type(value).same_shape(value_type))
                .cloned(),
            Expr::Global {
                index, value_type, ..
            } => self
                .evaluate_global(*index)
                .filter(|value| runtime_type(value).same_shape(value_type)),
            Expr::Function { function, .. } => {
                Some(RuntimeValue::Function(Callable::Named {
                    index: *function,
                    signature: match self.program.functions().get(*function) {
                        Some(function) => function.signature().clone(),
                        None => return None,
                    },
                    captures: Vec::new(),
                }))
            }
            Expr::Captured {
                slot, value_type, ..
            } => captures
                .get(*slot)
                .filter(|value| runtime_type(value).same_shape(value_type))
                .cloned(),
            Expr::Closure {
                signature,
                parameters,
                captures: capture_expressions,
                body,
                ..
            } => {
                let mut environment = Vec::with_capacity(capture_expressions.len());
                for capture in capture_expressions {
                    environment.push(self.evaluate(
                        capture,
                        slots.clone(),
                        captures,
                    )?);
                }
                Some(RuntimeValue::Function(Callable::Lambda {
                    signature: signature.clone(),
                    parameters: parameters.clone(),
                    body: body.clone(),
                    captures: environment,
                }))
            }
            Expr::Let {
                slot, value, body, ..
            } => {
                let value = self.evaluate(value, slots.clone(), captures)?;
                if let Some(slot) = slot {
                    *slots.get_mut(*slot)? = Some(value);
                }
                self.evaluate(body, slots, captures)
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let condition = self.evaluate(condition, slots.clone(), captures)?;
                match condition {
                    RuntimeValue::Primitive(Value::Bool(true)) => {
                        self.evaluate(then_branch, slots, captures)
                    }
                    RuntimeValue::Primitive(Value::Bool(false)) => {
                        self.evaluate(else_branch, slots, captures)
                    }
                    _ => None,
                }
            }
            Expr::Call {
                function,
                arguments,
                result,
                callee,
                ..
            } => {
                let callable = if let Some(callee) = callee {
                    self.evaluate(callee, slots.clone(), captures)?
                } else {
                    let signature =
                        self.program.functions().get(*function)?.signature().clone();
                    RuntimeValue::Function(Callable::Named {
                        index: *function,
                        signature,
                        captures: Vec::new(),
                    })
                };
                let callable_signature = match &callable {
                    RuntimeValue::Function(Callable::Named { signature, .. })
                    | RuntimeValue::Function(Callable::Lambda { signature, .. }) => {
                        signature
                    }
                    RuntimeValue::Primitive(_) => return None,
                };
                let mut values = Vec::with_capacity(arguments.len());
                for (argument_index, argument) in arguments.iter().enumerate() {
                    if matches!(argument, Expr::Default { .. }) {
                        let labelled_index = argument_index
                            .checked_sub(callable_signature.parameters().len())?;
                        let parameter =
                            callable_signature.labelled().get(labelled_index)?;
                        let default = parameter.default()?.clone();
                        if !default.ty().same_shape(&argument.result_type()) {
                            return None;
                        }
                        values.push(RuntimeValue::Primitive(default));
                    } else {
                        values.push(self.evaluate(
                            argument,
                            slots.clone(),
                            captures,
                        )?);
                    }
                }
                match callable {
                    RuntimeValue::Function(Callable::Named {
                        index, captures, ..
                    }) => {
                        let callee = self.program.functions().get(index)?;
                        if callee.signature().fixed_parameter_count() != values.len()
                            || !values_match_signature(&values, callee.signature())
                            || !callee.signature().result().same_shape(result)
                        {
                            return None;
                        }
                        let call_slots = values
                            .into_iter()
                            .map(Some)
                            .chain(std::iter::repeat(None))
                            .take(callee.slot_count())
                            .collect();
                        self.evaluate_function(index, call_slots, captures)
                    }
                    RuntimeValue::Function(Callable::Lambda {
                        signature,
                        parameters,
                        body,
                        captures,
                    }) => {
                        if signature.fixed_parameter_count() != values.len()
                            || !values_match_signature(&values, &signature)
                            || !signature.result().same_shape(result)
                        {
                            return None;
                        }
                        let slot_count =
                            parameters.len().max(values.len()).max(body.slot_count());
                        let call_slots = values
                            .into_iter()
                            .map(Some)
                            .chain(std::iter::repeat(None))
                            .take(slot_count)
                            .collect();
                        self.evaluate(&body, call_slots, &captures)
                    }
                    RuntimeValue::Primitive(_) => None,
                }
            }
        }
    }

    fn evaluate_global(&mut self, index: usize) -> Option<RuntimeValue> {
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
            self.evaluate(global.initializer(), vec![None; global.slot_count()], &[]);
        if let Some(value) = &value {
            *self.globals.get_mut(index)? = GlobalState::Ready(value.clone());
        }
        value
    }
}

fn runtime_type(value: &RuntimeValue) -> PrimitiveType {
    match value {
        RuntimeValue::Primitive(value) => value.ty(),
        RuntimeValue::Function(Callable::Named { signature, .. }) => {
            PrimitiveType::Function(Box::new(signature.clone()))
        }
        RuntimeValue::Function(Callable::Lambda { signature, .. }) => {
            PrimitiveType::Function(Box::new(signature.clone()))
        }
    }
}

fn slots_match_signature(
    slots: &[Option<RuntimeValue>],
    signature: &FunctionSignature,
) -> bool {
    let expected = signature
        .parameters()
        .iter()
        .cloned()
        .chain(
            signature
                .labelled()
                .iter()
                .map(|parameter| parameter.value_type()),
        )
        .collect::<Vec<_>>();
    slots
        .iter()
        .take(expected.len())
        .zip(expected)
        .all(|(value, expected)| {
            value
                .as_ref()
                .is_some_and(|value| runtime_type(value).same_shape(&expected))
        })
}

fn values_match_signature(
    values: &[RuntimeValue],
    signature: &FunctionSignature,
) -> bool {
    let expected = signature.parameters().iter().cloned().chain(
        signature
            .labelled()
            .iter()
            .map(|parameter| parameter.value_type()),
    );
    values
        .iter()
        .zip(expected)
        .all(|(value, expected)| runtime_type(value).same_shape(&expected))
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
            .evaluate_function(0, vec![None; program.entry().slot_count()], Vec::new())
            .expect("untaken branch must not execute");

        assert_eq!(result, super::RuntimeValue::Primitive(Value::I32(7)));
        assert_eq!(machine.globals[0], super::GlobalState::Uninitialized);
    }

    #[test]
    fn compiler_text_intrinsics_execute_without_audit_events() {
        let origin = SourceOrigin::new("external.vib", ByteSpan::new(0, 1));
        let concat = Expr::external(
            vibra_ir::external::CompilerIntrinsic::TextConcat,
            vec![
                Expr::literal(Value::Str("A😀".to_owned()), origin.clone()),
                Expr::literal(Value::Str("Ω".to_owned()), origin.clone()),
            ],
            origin.clone(),
        );
        let length = Expr::external(
            vibra_ir::external::CompilerIntrinsic::TextLength,
            vec![concat],
            origin.clone(),
        );
        let function = CheckedFunction::new(
            "answer",
            FunctionSignature::new(Vec::new(), PrimitiveType::U64),
            length,
            origin,
        )
        .expect("valid intrinsic function");
        let program = vibra_ir::CheckedProgram::try_new(vec![function], 0)
            .expect("valid intrinsic program");
        let result = run(&program).expect("execution");
        assert_eq!(result.value(), &Value::U64(3));
        assert!(result.audit_trace().is_empty());
    }
}
