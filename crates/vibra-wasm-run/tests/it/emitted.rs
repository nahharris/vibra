//! Modules the emitter writes for checked programs, run in the engine and
//! compared with the reference interpreter (`docs/spec/06-runtime.md`, "Wasm
//! transition"; `docs/spec/07-diagnostics-and-conformance.md`, "Differential
//! execution").

use std::collections::BTreeSet;

use vibra_ir::boundary::{Entry, Failure, NATIVE_IMPORT_MODULE, TrapCode, exports};
use vibra_ir::external::{CompilerIntrinsic, IntegerOp, IntegerType};
use vibra_ir::{CheckedProgram, Expr, Node, Pattern, Trap, Type, Value};
use vibra_wasm::support::{Instruction, ValueClass, module_with_entry};
use vibra_wasm_run::{
    MemoryLimit, Observed, Outcome, Runner, Started, TestRun, validate,
};

use crate::programs::{
    budget, call, function, generic_row, if_else, int, integer, lit, main_returning,
    matching, none, option, origin_at, program, runner, set_of, signature, some,
    tail_call, u64_lit, var,
};

/// The result type of a program's entry.
fn result_type(program: &CheckedProgram) -> Type {
    let function = &program.module_set().functions()[program.entry()];
    match function.signature.type_info() {
        Type::Fn(signature) => signature.result.clone(),
        other => panic!("an entry of type {other}"),
    }
}

/// Emits and runs `program`, and checks that the result is the
/// interpreter's, value and canonical bytes; returns the value.
pub(crate) fn agrees(program: &CheckedProgram, runner: &Runner) -> Value {
    let module = vibra_wasm::emit(program).expect("the program emits");
    validate(module.bytes()).expect("the module validates");
    let ty = result_type(program);
    let observed = runner
        .run_observed(module.bytes(), &ty, program.module_set().types())
        .expect("the module runs");
    let Observed::Completed {
        value,
        canonical,
        live,
    } = observed
    else {
        panic!("the run stopped: {observed:?}");
    };
    let reference = vibra_interp::run(program, budget()).expect("the interpreter runs");
    assert_eq!(*value, reference.value, "the value is the interpreter's");
    assert_eq!(
        canonical, reference.canonical,
        "the bytes are the interpreter's"
    );
    assert_eq!(live.end, live.start, "the run releases what it allocated");
    value.value().clone()
}

fn str_type() -> Type {
    Type::Declared(
        vibra_ir::TypeId::new(
            "std.str.Str",
            "sha256:213e3a01ef10d7c10032616f8ab0da9a97fb729b9f98d723f4beb3158a25e2ad",
        )
        .unwrap(),
        Vec::new(),
    )
}

/// `I32.add(1i32 2i32)`.
fn scalar_program() -> CheckedProgram {
    main_returning(
        Type::I32,
        integer(IntegerType::I32, IntegerOp::Add, vec![int(1), int(2)]),
        0,
    )
}

/// `match subject { some(value) -> value, none -> 0i32 }`.
fn variant_program(subject: Node) -> CheckedProgram {
    main_returning(
        Type::I32,
        matching(
            subject,
            vec![
                (
                    Pattern::Case {
                        case: "some".to_owned(),
                        fields: vec![("value".to_owned(), Pattern::Bind(0))],
                    },
                    var(0, Type::I32),
                ),
                (
                    Pattern::Case {
                        case: "none".to_owned(),
                        fields: Vec::new(),
                    },
                    int(0),
                ),
            ],
            Type::I32,
        ),
        1,
    )
}

/// `count(0)`, where `count(n)` returns `n` at 1,000,000 and is otherwise the
/// tail call `count(n + 1)`.
fn tail_program() -> CheckedProgram {
    let n = || var(0, Type::U64);
    let main = function(
        "main",
        signature(Vec::new(), Type::U64),
        call(1, vec![u64_lit(0)]),
        0,
    );
    let count = function(
        "count",
        signature(vec![Type::U64], Type::U64),
        if_else(
            integer(
                IntegerType::U64,
                IntegerOp::Equal,
                vec![n(), u64_lit(1_000_000)],
            ),
            n(),
            tail_call(
                1,
                vec![integer(
                    IntegerType::U64,
                    IntegerOp::Add,
                    vec![n(), u64_lit(1)],
                )],
            ),
        ),
        1,
    );
    program(set_of(vec![main, count]), 0)
}

/// `str.length("abc")`, which a native import answers.
fn native_program() -> CheckedProgram {
    main_returning(
        Type::U64,
        generic_row(
            CompilerIntrinsic::StrLength,
            Vec::new(),
            vec![lit(str_type(), Value::Str("abc".to_owned()))],
        ),
        0,
    )
}

/// The one test `assert.equal(3i32 4i32)`, whose call is written at `10..20`.
fn test_program() -> CheckedProgram {
    let assertion = Node::new(
        Expr::Intrinsic {
            intrinsic: CompilerIntrinsic::AssertEqual,
            type_arguments: vec![Type::I32],
            arguments: vec![int(3), int(4)],
        },
        origin_at(10, 20),
    );
    program(
        set_of(vec![function(
            "adds",
            signature(Vec::new(), Type::Void),
            assertion,
            0,
        )]),
        0,
    )
}

#[test]
fn emit_scalar_program() {
    let program = scalar_program();
    let module = vibra_wasm::emit(&program).expect("the program emits");
    let Started::Ready(mut instance) = runner().start(module.bytes()).expect("starts")
    else {
        panic!("the module fits the limit");
    };
    // `eval` returns, and nothing was recorded: status 0.
    assert!(matches!(
        instance.call_eval().expect("the export exists"),
        Outcome::Completed { .. }
    ));
    assert_eq!(agrees(&program, &runner()), Value::I32(3));
}

#[test]
fn emit_variant_match() {
    assert_eq!(
        agrees(&variant_program(some(Type::I32, int(5))), &runner()),
        Value::I32(5)
    );
    assert_eq!(
        agrees(&variant_program(none(Type::I32)), &runner()),
        Value::I32(0)
    );
    // A variant value is itself observed by its case and fields.
    let some_seven = main_returning(option(Type::I32), some(Type::I32, int(7)), 0);
    assert_eq!(
        agrees(&some_seven, &runner()),
        Value::Variant {
            case: "some".to_owned(),
            fields: vec![("value".to_owned(), Value::I32(7))],
        }
    );
}

#[test]
fn emit_tail_recursion() {
    // A frame per call would need far more than this limit holds, and a host
    // call per call more than this engine stack: the loop runs in one frame.
    let runner = Runner::with_wasm_stack(MemoryLimit::new(4 * 1024 * 1024), 256 * 1024)
        .expect("the engine configures");
    assert_eq!(agrees(&tail_program(), &runner), Value::U64(1_000_000));
}

#[test]
fn emit_native_program() {
    assert_eq!(agrees(&native_program(), &runner()), Value::U64(3));
}

#[test]
fn emit_test_module() {
    let program = test_program();
    let module = vibra_wasm::emit_tests(program.module_set(), &[0]).expect("emits");
    let Started::Ready(mut instance) = runner().start(module.bytes()).expect("starts")
    else {
        panic!("the module fits the limit");
    };
    // `test(0)` stops with status 3, failure 3, and the operands' bits.
    let Outcome::AssertionFailed {
        failure,
        origin,
        operands,
    } = instance.call_test(0).expect("the export exists")
    else {
        panic!("the assertion fails");
    };
    assert_eq!(failure, Failure::AssertEqual);
    assert_eq!(failure.code(), 3);
    assert_eq!(operands, Some((3, 4)));
    let origin = origin.expect("the call has an origin");
    assert_eq!(module.origins().origin(origin), Some(&origin_at(10, 20)));
    // The runner reads the operands by the recorded type, as the
    // interpreter's encodings.
    let run = runner()
        .run_test(
            module.bytes(),
            0,
            program.module_set().types(),
            &|ordinal| module.origins().operand_type(ordinal).cloned(),
        )
        .expect("the module runs");
    let reference = vibra_interp::run_test(&program, budget())
        .expect("the interpreter runs")
        .failure
        .expect("the assertion fails");
    assert_eq!(
        run,
        TestRun::AssertionFailed {
            failure: Failure::AssertEqual,
            origin: Some(origin),
            expected: Some("3i32".to_owned()),
            actual: Some("4i32".to_owned()),
        }
    );
    assert_eq!(reference.expected.as_deref(), Some("3i32"));
    assert_eq!(reference.actual.as_deref(), Some("4i32"));
    assert_eq!(reference.assertion, Failure::AssertEqual.symbol());
}

/// Every module the tests above emit.
fn emitted_modules() -> Vec<(Entry, Vec<u8>)> {
    let mut modules = [
        scalar_program(),
        variant_program(some(Type::I32, int(5))),
        tail_program(),
        native_program(),
    ]
    .iter()
    .map(|program| {
        (
            Entry::Eval,
            vibra_wasm::emit(program).expect("emits").into_bytes(),
        )
    })
    .collect::<Vec<_>>();
    let tests = test_program();
    modules.push((
        Entry::Test,
        vibra_wasm::emit_tests(tests.module_set(), &[0])
            .expect("emits")
            .into_bytes(),
    ));
    modules
}

#[test]
fn module_imports_only_natives() {
    let mut imported = 0;
    for (_, bytes) in emitted_modules() {
        let summary = validate(&bytes).expect("the module validates");
        for (module, name) in &summary.imports {
            assert_eq!(module, NATIVE_IMPORT_MODULE, "{name}");
            imported += 1;
        }
    }
    assert!(imported > 0, "a module that calls a native imports it");
}

#[test]
fn module_exports_are_closed() {
    for (entry, bytes) in emitted_modules() {
        let summary = validate(&bytes).expect("the module validates");
        let names = summary
            .exports
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        assert_eq!(names.len(), summary.exports.len(), "no export is repeated");
        assert_eq!(names, exports(entry).into_iter().collect::<BTreeSet<_>>());
        assert!(!(names.contains("vibra_v1_eval") && names.contains("vibra_v1_test")));
        for name in &names {
            assert!(
                !["vibra_v1_entry", "vibra_v1_new", "vibra_v1_seal"].contains(name)
                    && !name.starts_with("vibra_v1_write_"),
                "{name}"
            );
        }
    }
}

#[test]
fn trap_codes() {
    // A defect the module records itself: `invalidCheckedProgram` is 1.
    let defect = module_with_entry(ValueClass::Void, &[], |fns| {
        vec![Instruction::I32Const(1), Instruction::Call(fns.stop_trap)]
    });
    let outcome = runner().run_eval(&defect).expect("runs");
    assert_eq!(
        outcome,
        Outcome::Trapped {
            code: TrapCode::of(Trap::InvalidCheckedProgram),
            origin: None,
        }
    );
    // An ID the instance never issued: `invalidHostValue` is 3.
    let Started::Ready(mut instance) = runner()
        .start(
            &vibra_wasm::emit(&scalar_program())
                .expect("emits")
                .into_bytes(),
        )
        .expect("starts")
    else {
        panic!("the module fits the limit");
    };
    for raw in [0, 99] {
        let forged = instance.forge_id(raw);
        let Err(stop) = instance.variant(&forged) else {
            panic!("a forged ID is refused");
        };
        assert_eq!(
            stop,
            Outcome::Trapped {
                code: TrapCode::of(Trap::InvalidHostValue),
                origin: None,
            }
        );
    }
    assert_eq!(TrapCode::of(Trap::InvalidCheckedProgram).code(), 1);
    assert_eq!(TrapCode::of(Trap::InvalidHostValue).code(), 3);
    // An assertion outside a test is the defect trap, as in the interpreter.
    let stray = main_returning(
        Type::Void,
        generic_row(
            CompilerIntrinsic::AssertEqual,
            vec![Type::I32],
            vec![int(1), int(2)],
        ),
        0,
    );
    let module = vibra_wasm::emit(&stray).expect("emits");
    let Outcome::Trapped { code, .. } =
        runner().run_eval(module.bytes()).expect("runs")
    else {
        panic!("the assertion traps");
    };
    assert_eq!(code, TrapCode::of(Trap::InvalidCheckedProgram));
    let reference = vibra_interp::run(&stray, budget()).expect_err("it traps");
    assert_eq!(reference.trap(), Some(Trap::InvalidCheckedProgram));
}
