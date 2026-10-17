//! Every registered trap code, with its origin, in both backends (milestone 4
//! Step 11).
//!
//! `docs/spec/06-runtime.md`, "Traps": both backends MUST report the same code
//! and the same origin for every registered trap. The interpreter is the
//! oracle for `@runtime.invalid-checked-program` and
//! `@runtime.unobservable-function`; `@runtime.invalid-host-value` is raised by
//! the accessors of a module, which the interpreter has none of. The same
//! `@runtime.unobservable-function` in a test, at its assertion call, is held
//! by `tests_traps_m4_step11` and by the corpus case
//! `V1-RUNTIME-workspace-test-hidden-function`.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    missing_docs
)]

use vibra_ir::boundary::TrapCode;
use vibra_ir::{
    CallTarget, CheckedFunction, CheckedProgram, Expr, FunctionSignature, SourceOrigin,
    TestAssertion, Type, Value,
};
use vibra_wasm_run::{MemoryLimit, Observed, Outcome, Runner, Started, TestRun};

fn origin() -> SourceOrigin {
    SourceOrigin::new("trap.vib", vibra_diagnostics::ByteSpan::new(0, 1))
}

/// An entry that calls the verified assertion `assert.true` on `true`.
fn assertion_program(entry_result: Type) -> CheckedProgram {
    let call = Expr::Call {
        target: CallTarget::Direct(1),
        arguments: vec![Expr::literal(Value::Bool(true), origin())],
        result: Type::Void,
        tail: false,
        origin: origin(),
    };
    let body = if entry_result == Type::Void {
        call
    } else {
        Expr::Sequence {
            expressions: vec![call, Expr::literal(Value::I32(1), origin())],
            origin: origin(),
        }
    };
    let entry = CheckedFunction::new(
        "main",
        FunctionSignature::new(Vec::new(), entry_result),
        body,
        origin(),
    )
    .expect("an entry");
    let assertion =
        CheckedFunction::new_test_assertion("assert", TestAssertion::True, origin())
            .expect("an assertion");
    CheckedProgram::try_new(vec![entry, assertion], 0).expect("a program")
}

fn runner() -> Runner {
    Runner::new(MemoryLimit::new(64 * 1024 * 1024)).expect("an engine")
}

#[test]
fn an_assertion_outside_a_test_is_the_invalid_checked_program_in_both_backends() {
    let program = assertion_program(Type::Void);
    let error = vibra_interp::run(&program).expect_err("the interpreter refuses");
    let (code, origin) = error.program_trap().expect("a registered trap");
    assert_eq!(code, TrapCode::InvalidCheckedProgram.diagnostic_code());
    assert!(origin.is_none());
    let module = vibra_wasm::emit(&program).expect("a module");
    let outcome = runner().run_entry(module.bytes()).expect("the module runs");
    assert_eq!(
        outcome,
        Outcome::Trapped {
            code: TrapCode::InvalidCheckedProgram,
            origin: None
        }
    );
}

#[test]
fn a_test_entry_that_is_not_void_is_the_invalid_checked_program_in_both_backends() {
    let program = assertion_program(Type::I32);
    let error = vibra_interp::Interpreter::run_test(&program)
        .expect_err("the interpreter refuses");
    let (code, origin) = error.program_trap().expect("a registered trap");
    assert_eq!(code, TrapCode::InvalidCheckedProgram.diagnostic_code());
    assert!(origin.is_none());
    let module = vibra_wasm::emit_tests(&program, &[0]).expect("a test module");
    let none = |_: u32| None;
    let run = runner()
        .run_test(module.bytes(), 0, program.types(), &none, &none)
        .expect("the module runs");
    assert_eq!(
        run,
        TestRun::Trapped {
            code: TrapCode::InvalidCheckedProgram,
            origin: None
        }
    );
}

#[test]
fn a_function_in_the_entry_result_is_the_unobservable_function_with_no_origin() {
    let result = vibra_types::check_source(
        "input.vib",
        "(defn main () any (as any (lambda (x i32) i32 x)))",
    );
    let program = result.program().expect("an accepted program");
    let error = vibra_interp::run(program).expect_err("the interpreter stops");
    let (code, origin) = error.program_trap().expect("a registered trap");
    assert_eq!(code, TrapCode::UnobservableFunction.diagnostic_code());
    assert!(origin.is_none());
    let module = vibra_wasm::emit(program).expect("a module");
    let heads = |head: u32| module.descriptors().type_of(head).cloned();
    let observed = runner()
        .run_observed_with(
            module.bytes(),
            &program.entry().signature().result(),
            program.types(),
            &heads,
        )
        .expect("the module runs");
    assert_eq!(
        observed,
        Observed::Stopped(Outcome::Trapped {
            code: TrapCode::UnobservableFunction,
            origin: None
        })
    );
}

#[test]
fn an_accessor_given_a_released_id_is_the_invalid_host_value_with_no_origin() {
    let result = vibra_types::check_source("input.vib", "(defn main () str \"held\")");
    let program = result.program().expect("an accepted program");
    let module = vibra_wasm::emit(program).expect("a module");
    let Started::Ready(mut instance) =
        runner().start(module.bytes()).expect("a v1 module")
    else {
        panic!("the module fits the limit");
    };
    let Outcome::Completed { result: slot, .. } = instance.call_entry().expect("runs")
    else {
        panic!("the entry completes");
    };
    let id = instance.result_id(slot);
    assert_eq!(instance.length(&id).expect("a live id"), 4);
    instance.release(id.clone()).expect("the host releases it");
    let stop = instance.length(&id).expect_err("a released id");
    assert_eq!(
        stop,
        Outcome::Trapped {
            code: TrapCode::InvalidHostValue,
            origin: None
        }
    );
    // The accessor that stopped did not end the instance.
    assert!(instance.live_size().is_ok());
}
