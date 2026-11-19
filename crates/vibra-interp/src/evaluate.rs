//! The interpreter's entry points: a program's run, a test body's run, and
//! the evaluation of one expression.

use std::rc::Rc;

use vibra_diagnostics::ByteSpan;
use vibra_ir::{
    CheckedModuleSet, CheckedProgram, Expr, Node, ObservedValue, SourceOrigin, Trap,
    Type, Value,
};

use crate::encode::{ObserveError, observe, to_neutral};
use crate::frame::Frame;
use crate::machine::{Halt, Machine, Mode, Resolved, Target};
use crate::types::type_of;
use crate::value::Val;
use crate::{
    Evaluation, Execution, Limit, Limits, MemoryBudget, RuntimeError, TestExecution,
};

fn runtime_error(
    halt: Halt,
    origin: Option<&SourceOrigin>,
    budget: MemoryBudget,
) -> RuntimeError {
    let trap = |message: String| RuntimeError::Trap {
        trap: Trap::InvalidCheckedProgram,
        origin: origin.cloned(),
        message,
    };
    match halt {
        Halt::Invalid(message) => trap(message),
        Halt::Memory => RuntimeError::MemoryExhausted { budget },
        Halt::Unavailable(message) => RuntimeError::Unavailable { message },
        Halt::Reductions => trap("a reduction limit outside an evaluation".to_owned()),
        Halt::Failed(failure) => {
            trap(format!("`{}` outside a test", failure.assertion))
        }
    }
}

/// The declared result type of a non-generic entry with no parameters.
fn entry_result(set: &CheckedModuleSet, entry: usize) -> Result<Type, RuntimeError> {
    let function = set
        .functions()
        .get(entry)
        .ok_or_else(|| RuntimeError::Trap {
            trap: Trap::InvalidCheckedProgram,
            origin: None,
            message: format!("no entry function {entry}"),
        })?;
    match function.signature.type_info() {
        Type::Fn(signature) => Ok(signature.result.clone()),
        Type::Proc(_) => Err(RuntimeError::Unavailable {
            message: format!(
                "the entry `{}` is a `proc`, whose plan this interpreter does not run",
                function.name
            ),
        }),
        other => Err(RuntimeError::Trap {
            trap: Trap::InvalidCheckedProgram,
            origin: Some(function.origin.clone()),
            message: format!("an entry of {other}"),
        }),
    }
}

/// Calls the entry and runs the machine to its end.
fn call_entry<'a>(
    machine: &mut Machine<'a>,
    entry: usize,
    budget: MemoryBudget,
) -> Result<Result<Val<'a>, Halt>, RuntimeError> {
    let resolved = Resolved {
        target: Target::Function(entry),
        env: Rc::clone(&machine.empty_env),
    };
    let step = match machine.invoke(resolved, Vec::new(), false, None) {
        Ok(step) => step,
        Err(halt) => return Err(runtime_error(halt, machine.origin(), budget)),
    };
    Ok(machine.execute(step))
}

/// Runs `program`'s entry to its end under `budget`, and observes its
/// result at the entry's declared result type.
///
/// # Errors
///
/// A trap, the exhaustion of `budget`, a result that holds a function
/// behind an interface value, or a form this interpreter does not run.
pub fn run(
    program: &CheckedProgram,
    budget: MemoryBudget,
) -> Result<Execution, RuntimeError> {
    let set: &CheckedModuleSet = program.module_set();
    let result_type = entry_result(set, program.entry())?;
    let mut machine = Machine::new(set, Mode::Run, budget.bytes, u64::MAX);
    let value = call_entry(&mut machine, program.entry(), budget)?
        .map_err(|halt| runtime_error(halt, machine.origin(), budget))?;
    machine.stats.live_memory_bytes = machine.measure(Some(&value));
    let (neutral, canonical) = match observe(set, &value, &result_type) {
        Ok(observed) => observed,
        Err(ObserveError::Unsupported) => {
            return Err(RuntimeError::UnsupportedValue {
                value_type: result_type,
            });
        }
        Err(ObserveError::Halt(halt)) => return Err(runtime_error(halt, None, budget)),
    };
    Ok(Execution {
        value: ObservedValue::new(result_type, neutral),
        canonical,
        instrumentation: machine.stats,
    })
}

/// Runs a test body: an entry of result type `Void`, in which the
/// assertion rows are defined. A false assertion ends the body and is
/// reported as its failure.
///
/// # Errors
///
/// A trap, the exhaustion of `budget`, an entry that does not return
/// `Void`, or a form this interpreter does not run.
pub fn run_test(
    program: &CheckedProgram,
    budget: MemoryBudget,
) -> Result<TestExecution, RuntimeError> {
    let set: &CheckedModuleSet = program.module_set();
    let result_type = entry_result(set, program.entry())?;
    if result_type != Type::Void {
        return Err(RuntimeError::Trap {
            trap: Trap::InvalidCheckedProgram,
            origin: None,
            message: format!("a test body returning {result_type}"),
        });
    }
    let mut machine = Machine::new(set, Mode::Test, budget.bytes, u64::MAX);
    let failure = match call_entry(&mut machine, program.entry(), budget)? {
        Ok(value) => {
            machine.stats.live_memory_bytes = machine.measure(Some(&value));
            None
        }
        Err(Halt::Failed(failure)) => Some(*failure),
        Err(halt) => return Err(runtime_error(halt, machine.origin(), budget)),
    };
    Ok(TestExecution {
        failure,
        instrumentation: machine.stats,
    })
}

/// Evaluates `expr` over `set`, whose variable slots `0..inputs.len()` hold
/// `inputs`, within `limits`. The reduction count is the limit on work.
///
/// The value is the neutral value of the expression's static type. A trap,
/// and any form this interpreter does not run, is a
/// [`Evaluation::Defect`].
#[must_use]
pub fn evaluate(
    set: &CheckedModuleSet,
    expr: &Expr,
    inputs: &[Value],
    limits: Limits,
) -> Evaluation {
    let defect = Evaluation::Defect(Trap::InvalidCheckedProgram);
    let root = Node::new(expr.clone(), SourceOrigin::new("", ByteSpan::empty_at(0)));
    let Ok(value_type) = type_of(set, &root) else {
        return defect;
    };
    let mut machine =
        Machine::new(set, Mode::Evaluate, limits.memory.bytes, limits.reductions);
    let mut frame = Frame::new(inputs.len(), Rc::clone(&machine.empty_env));
    frame.inputs = inputs;
    let outcome = machine
        .enter_root(frame)
        .and_then(|()| machine.execute(crate::machine::Step::Eval(&root)))
        .and_then(|value| to_neutral(set, &value, &value_type));
    match outcome {
        Ok(value) => Evaluation::Value(value),
        Err(Halt::Reductions) => Evaluation::LimitReached(Limit::Reductions),
        Err(Halt::Memory) => Evaluation::LimitReached(Limit::Memory),
        Err(_) => defect,
    }
}
