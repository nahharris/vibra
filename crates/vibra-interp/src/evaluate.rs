//! The interpreter's entry points: a program's run, a test body's run, and
//! the evaluation of one expression.

use std::rc::Rc;

use vibra_diagnostics::ByteSpan;
use vibra_ir::authority::{GrantToken, HostContext};
use vibra_ir::{
    CheckedFunction, CheckedModuleSet, CheckedProgram, Expr, ObservedValue,
    SourceOrigin, Trap, Type, Value,
};

use crate::encode::{ObserveError, observe, to_neutral};
use crate::host::EntryArgument;
use crate::machine::{Halt, Machine, Mode, Resolved, Target};
use crate::types::type_of_expr;
use crate::value::Val;
use crate::{
    Evaluation, Execution, Invocation, Limit, Limits, MemoryBudget, Reads,
    RuntimeError, TestExecution,
};

/// The [`RuntimeError`] of `halt`, which stopped a run at `origin`.
pub(crate) fn runtime_error(
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
        Halt::HostValue(message) => RuntimeError::Trap {
            trap: Trap::InvalidHostValue,
            origin: origin.cloned(),
            message,
        },
        Halt::Memory => RuntimeError::MemoryExhausted { budget },
        Halt::Unavailable(message) => RuntimeError::Unavailable { message },
        Halt::Reductions => trap("a reduction limit outside an evaluation".to_owned()),
        Halt::Failed(failure) => {
            trap(format!("`{}` outside a test", failure.assertion))
        }
        Halt::NoEncoding => {
            trap("a plan or a grant where a value encoding is required".to_owned())
        }
    }
}

/// The entry function `entry` of `set`.
fn entry_function(
    set: &CheckedModuleSet,
    entry: usize,
) -> Result<&CheckedFunction, RuntimeError> {
    set.functions()
        .get(entry)
        .ok_or_else(|| RuntimeError::Trap {
            trap: Trap::InvalidCheckedProgram,
            origin: None,
            message: format!("no entry function {entry}"),
        })
}

/// The declared result type of a non-generic entry that is a `fn`.
fn entry_result(set: &CheckedModuleSet, entry: usize) -> Result<Type, RuntimeError> {
    let function = entry_function(set, entry)?;
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

/// Calls the entry on the arguments `arguments` hands in, at the entry's
/// parameter types, and runs the machine to its end.
fn call_entry<'a>(
    machine: &mut Machine<'a>,
    entry: usize,
    arguments: Vec<EntryArgument>,
    budget: MemoryBudget,
) -> Result<Result<Val<'a>, Halt>, RuntimeError> {
    let resolved = Resolved {
        target: Target::Function(entry),
        env: Rc::clone(&machine.empty_env),
    };
    let set = machine.set;
    let parameters = entry_function(set, entry)?.signature.type_info();
    let parameters: &[Type] = match parameters {
        Type::Fn(signature) => &signature.parameters,
        Type::Proc(signature) => &signature.parameters,
        _ => &[],
    };
    let started = machine
        .entry_arguments(parameters, arguments)
        .and_then(|args| machine.invoke(resolved, args, false, None))
        .and_then(|step| machine.prelude(step));
    let step = match started {
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
/// behind an interface value or a plan, or a form this interpreter does
/// not run.
pub fn run(
    program: &CheckedProgram,
    budget: MemoryBudget,
) -> Result<Execution, RuntimeError> {
    let set: &CheckedModuleSet = program.module_set();
    let result_type = entry_result(set, program.entry())?;
    let function = entry_function(set, program.entry())?;
    if let Type::Fn(signature) = function.signature.type_info()
        && !signature.parameters.is_empty()
    {
        return Err(RuntimeError::Unavailable {
            message: format!(
                "the entry `{}` takes grants, which only a host context issues",
                function.name
            ),
        });
    }
    let mut machine = Machine::new(set, Mode::Run, budget.bytes, u64::MAX);
    let value = call_entry(&mut machine, program.entry(), Vec::new(), budget)?
        .map_err(|halt| runtime_error(halt, machine.origin(), budget))?;
    finish(machine, value, result_type, budget)
}

/// Observes `value`, the result of a run's entry, at `result_type`.
fn finish<'a>(
    mut machine: Machine<'a>,
    value: Val<'a>,
    result_type: Type,
    budget: MemoryBudget,
) -> Result<Execution, RuntimeError> {
    let set = machine.set;
    machine.stats.live_memory_bytes = machine.measure(std::slice::from_ref(&value));
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

/// Runs the entry of `program`, a `proc`, on `grants` through `host`, under
/// `budget`, and observes the result of its plan at the plan's result type.
///
/// The entry's plan is awaited as invocation `first_invocation`, with no
/// parent (`docs/spec/06-runtime.md`, "Cold plans and invocation
/// identity"): the caller has spent the ordinals before it on the reads of
/// startup. Each grant argument is a sealed grant at the entry parameter
/// it stands for, an absent optional grant is `Option.none()`, and a bundle
/// is the record of its fields.
///
/// # Errors
///
/// A trap, among them `@runtime.invalidHostValue` for an argument that is
/// no value of its parameter and for a fault of `host`, the exhaustion of
/// `budget`, an entry that is not a `proc`, or a result that has no value
/// encoding.
pub fn run_entry(
    program: &CheckedProgram,
    grants: Vec<EntryArgument>,
    first_invocation: u64,
    host: &mut dyn HostContext,
    budget: MemoryBudget,
) -> Result<Execution, RuntimeError> {
    let set: &CheckedModuleSet = program.module_set();
    let function = entry_function(set, program.entry())?;
    let Type::Proc(signature) = function.signature.type_info() else {
        return Err(RuntimeError::Trap {
            trap: Trap::InvalidCheckedProgram,
            origin: Some(function.origin.clone()),
            message: format!("the entry `{}` is not a `proc`", function.name),
        });
    };
    if first_invocation == 0 {
        return Err(RuntimeError::Trap {
            trap: Trap::InvalidHostValue,
            origin: None,
            message: "an entry plan awaited as invocation 0".to_owned(),
        });
    }
    let mut machine = Machine::new(set, Mode::Run, budget.bytes, u64::MAX);
    machine.host.context = Some(host);
    machine.invocation = Invocation {
        ordinal: first_invocation,
        parent: None,
    };
    machine.stats.invocation_count = first_invocation;
    let value = call_entry(&mut machine, program.entry(), grants, budget)?
        .map_err(|halt| runtime_error(halt, machine.origin(), budget))?;
    finish(machine, value, signature.result.clone(), budget)
}

/// Runs a test body: an entry of result type `Void`, in which the
/// assertion rows are defined. A false assertion ends the body and is
/// reported as its failure.
///
/// The body is invocation 1 (`docs/spec/06-runtime.md`, "Audit trace"),
/// and it runs in the procedure context its function's context states: a
/// test body whose context is `Procedure` may `await`, and each `await` in
/// it is an invocation whose parent is 1. A body that takes a `TestWorld`
/// receives `world` as its argument; host-bound members and narrowings
/// run through `host`.
///
/// # Errors
///
/// A trap, the exhaustion of `budget`, an entry that does not return
/// `Void`, a `world` the body does not take, or a form this interpreter
/// does not run, such as a host-bound member when `host` is `None`.
pub fn run_test(
    program: &CheckedProgram,
    world: Option<GrantToken>,
    host: Option<&mut dyn HostContext>,
    budget: MemoryBudget,
) -> Result<TestExecution, RuntimeError> {
    test_run(program, world, host, budget, None).map(|(execution, _)| execution)
}

/// [`run_test`], with the [`Invocation`] current at each entry of the
/// function `probe` of the program's module set, in the order of the
/// entries. A function called from a body runs in that body's invocation,
/// so a probe called in a body reports the invocation of that body.
///
/// # Errors
///
/// As [`run_test`].
pub fn run_test_probing(
    program: &CheckedProgram,
    budget: MemoryBudget,
    probe: usize,
) -> Result<(TestExecution, Vec<Invocation>), RuntimeError> {
    test_run(program, None, None, budget, Some(probe))
}

fn test_run(
    program: &CheckedProgram,
    world: Option<GrantToken>,
    host: Option<&mut dyn HostContext>,
    budget: MemoryBudget,
    probe: Option<usize>,
) -> Result<(TestExecution, Vec<Invocation>), RuntimeError> {
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
    machine.probe = probe;
    if let Some(host) = host {
        machine.host.context = Some(host);
    }
    let arguments = world.map(EntryArgument::Grant).into_iter().collect();
    let failure = match call_entry(&mut machine, program.entry(), arguments, budget)? {
        Ok(value) => {
            machine.stats.live_memory_bytes =
                machine.measure(std::slice::from_ref(&value));
            None
        }
        Err(Halt::Failed(failure)) => Some(*failure),
        Err(halt) => return Err(runtime_error(halt, machine.origin(), budget)),
    };
    let execution = TestExecution {
        failure,
        instrumentation: machine.stats,
    };
    Ok((execution, machine.probed))
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
    evaluate_reading(set, expr, inputs, limits).0
}

/// [`evaluate`], with what the evaluation read of `set`: every function it
/// entered and every module value it read, whatever its outcome.
#[must_use]
pub fn evaluate_reading(
    set: &CheckedModuleSet,
    expr: &Expr,
    inputs: &[Value],
    limits: Limits,
) -> (Evaluation, Reads) {
    let defect = Evaluation::Defect(Trap::InvalidCheckedProgram);
    let origin = SourceOrigin::new("", ByteSpan::empty_at(0));
    if expr.check_type_depth().is_err() {
        return (defect, Reads::default());
    }
    let Ok(value_type) = type_of_expr(set, expr) else {
        return (defect, Reads::default());
    };
    let mut machine =
        Machine::new(set, Mode::Evaluate, limits.memory.bytes, limits.reductions);
    let env = Rc::clone(&machine.empty_env);
    let outcome = machine
        .new_frame(inputs.len(), env, &[])
        .and_then(|mut frame| {
            frame.inputs = inputs;
            machine.enter_root(frame)
        })
        .and_then(|()| machine.prelude(crate::machine::Step::Root(expr, &origin)))
        .and_then(|step| machine.execute(step))
        .and_then(|value| to_neutral(set, &value, &value_type));
    let evaluation = match outcome {
        Ok(value) => Evaluation::Value(value),
        Err(Halt::Reductions) => Evaluation::LimitReached(Limit::Reductions),
        Err(Halt::Memory) => Evaluation::LimitReached(Limit::Memory),
        Err(Halt::HostValue(_)) => Evaluation::Defect(Trap::InvalidHostValue),
        Err(_) => defect,
    };
    (evaluation, machine.reads)
}
