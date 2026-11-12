//! The foundation's proof cases: hand-built execution IR, run by the reference
//! stepper on Nexus. They are the execution-IR form of the v0.0.3 M1 demo and
//! exit clauses, and `docs/roadmap/nexus/validation.md` maps each test to the
//! clause it evidences.
//!
//! Run them with `cargo test --locked -p vibra-nexus-eval --test it`.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    missing_docs
)]

use vibra_diagnostics::ByteSpan;
use vibra_exec::{
    Block, BlockId, Callee, ConstId, Constant, FuncId, Function, GlobalId, Inst,
    ObjectKind, OriginId, Program, ScalarClass, ScalarOp, Slot, Terminator,
};
use vibra_host::{AuditEvent, HostWorld, WorldProvider};
use vibra_ir::SourceOrigin;
use vibra_ir::boundary::TrapCode;
use vibra_ir::external::host::host_row;
use vibra_nexus::{
    ABI_VERSION, AdmitError, Cell, Context, Outcome, Requirements, Stats, Trap,
    UNBOUNDED, admit, run,
};
use vibra_nexus_eval::{EvalError, Evaluator};

const MIB: u64 = 1024 * 1024;
/// The limit the conformance runner gives every instance.
const LIMIT: u64 = 64 * MIB;

fn scalar(dst: u32, bits: u64) -> Inst {
    Inst::Scalar {
        dst: Slot(dst),
        class: ScalarClass::I64,
        bits,
    }
}

fn op(dst: u32, op: ScalarOp, left: u32, right: u32) -> Inst {
    Inst::Op {
        dst: Slot(dst),
        op,
        args: vec![Slot(left), Slot(right)],
    }
}

fn drop(slot: u32) -> Inst {
    Inst::Drop { slot: Slot(slot) }
}

fn block(insts: Vec<Inst>, term: Terminator) -> Block {
    Block { insts, term }
}

fn ret(slot: u32) -> Terminator {
    Terminator::Return { value: Slot(slot) }
}

fn function(
    name: &str,
    params: u32,
    env: bool,
    slots: u32,
    blocks: Vec<Block>,
) -> Function {
    Function {
        name: name.to_owned(),
        params,
        env,
        slots,
        blocks,
    }
}

fn stdout(arg: u32, dst: u32, resume: u32, origin: u32) -> Terminator {
    Terminator::Host {
        row: host_row("io.stdout-text").unwrap(),
        args: vec![Slot(arg)],
        dst: Slot(dst),
        resume: BlockId(resume),
        origin: OriginId(origin),
    }
}

fn origin(start: usize) -> SourceOrigin {
    SourceOrigin::new("app/main.vib", ByteSpan::new(start, start + 1))
}

/// One half of a mutual recursion through function values: with `n` zero it
/// returns `n`, and otherwise it builds a value of `other` and tail-calls it
/// with `n - 1`. Slot 0 is `n` and slot 1 is the function's own value.
fn hop(name: &str, other: u32) -> Function {
    function(
        name,
        1,
        true,
        7,
        vec![
            block(
                vec![scalar(2, 0), op(3, ScalarOp::I64Eq, 0, 2)],
                Terminator::Branch {
                    cond: Slot(3),
                    then_block: BlockId(1),
                    else_block: BlockId(2),
                },
            ),
            block(Vec::new(), ret(0)),
            block(
                vec![
                    scalar(4, 1),
                    op(5, ScalarOp::I64Sub, 0, 4),
                    Inst::Function {
                        dst: Slot(6),
                        function: FuncId(other),
                        captures: Vec::new(),
                    },
                ],
                Terminator::TailCall {
                    callee: Callee::Indirect(Slot(6)),
                    args: vec![Slot(5)],
                },
            ),
        ],
    )
}

/// The M1 demo in execution IR: `rounds` indirect tail transfers between two
/// functions, then one audited host operation, then either a mapped trap or a
/// return of the loop's result.
fn demo(rounds: u64, trap: bool) -> Program {
    let end = if trap {
        block(
            Vec::new(),
            Terminator::Trap {
                code: TrapCode::DivisionByZero,
                origin: OriginId(2),
            },
        )
    } else {
        block(vec![drop(1), drop(3)], ret(2))
    };
    let main = function(
        "main",
        0,
        false,
        4,
        vec![
            block(
                vec![
                    Inst::Function {
                        dst: Slot(0),
                        function: FuncId(1),
                        captures: Vec::new(),
                    },
                    scalar(1, rounds),
                ],
                Terminator::Call {
                    callee: Callee::Indirect(Slot(0)),
                    args: vec![Slot(1)],
                    dst: Slot(2),
                    resume: BlockId(1),
                },
            ),
            block(
                vec![
                    drop(0),
                    Inst::Constant {
                        dst: Slot(3),
                        constant: ConstId(0),
                    },
                ],
                stdout(3, 1, 2, 1),
            ),
            end,
        ],
    );
    Program {
        functions: vec![main, hop("even", 2), hop("odd", 1)],
        entry: FuncId(0),
        globals: Vec::new(),
        constants: vec![Constant::Str("done\n".to_owned())],
        origins: vec![origin(10), origin(20)],
    }
}

/// `count(n)` is `0` for zero and `count(n - 1) + 1` otherwise: one frame for
/// each unit of `n`, none of them in tail position.
fn deep(n: u64) -> Program {
    let main = function(
        "main",
        0,
        false,
        2,
        vec![
            block(
                vec![scalar(0, n)],
                Terminator::Call {
                    callee: Callee::Direct(FuncId(1)),
                    args: vec![Slot(0)],
                    dst: Slot(1),
                    resume: BlockId(1),
                },
            ),
            block(Vec::new(), ret(1)),
        ],
    );
    let count = function(
        "count",
        1,
        false,
        6,
        vec![
            block(
                vec![scalar(1, 0), op(2, ScalarOp::I64Eq, 0, 1)],
                Terminator::Branch {
                    cond: Slot(2),
                    then_block: BlockId(1),
                    else_block: BlockId(2),
                },
            ),
            block(Vec::new(), ret(1)),
            block(
                vec![scalar(3, 1), op(4, ScalarOp::I64Sub, 0, 3)],
                Terminator::Call {
                    callee: Callee::Direct(FuncId(1)),
                    args: vec![Slot(4)],
                    dst: Slot(4),
                    resume: BlockId(3),
                },
            ),
            block(vec![op(5, ScalarOp::I64Add, 4, 3)], ret(5)),
        ],
    );
    Program {
        functions: vec![main, count],
        entry: FuncId(0),
        globals: Vec::new(),
        constants: Vec::new(),
        origins: Vec::new(),
    }
}

/// A module value whose initializer performs a host operation, read twice.
fn module_value() -> Program {
    let read = |dst: u32, resume: u32| Terminator::Global {
        global: GlobalId(0),
        dst: Slot(dst),
        resume: BlockId(resume),
    };
    let main = function(
        "main",
        0,
        false,
        4,
        vec![
            block(Vec::new(), read(0, 1)),
            block(Vec::new(), read(1, 2)),
            block(
                vec![
                    // The two reads share one value: a pair of them is built
                    // and released with the frame.
                    Inst::Build {
                        dst: Slot(2),
                        kind: ObjectKind::Tuple,
                        variant: 0,
                        cells: vec![Slot(0), Slot(1)],
                    },
                    scalar(3, 7),
                ],
                ret(3),
            ),
        ],
    );
    let initializer = function(
        "greeting",
        0,
        false,
        2,
        vec![
            block(
                vec![Inst::Constant {
                    dst: Slot(0),
                    constant: ConstId(0),
                }],
                stdout(0, 1, 1, 0),
            ),
            block(vec![drop(1)], ret(0)),
        ],
    );
    Program {
        functions: vec![main, initializer],
        entry: FuncId(0),
        globals: vec![FuncId(1)],
        constants: vec![Constant::Str("init\n".to_owned())],
        origins: Vec::new(),
    }
}

struct Ran {
    outcome: Outcome,
    audit: Vec<AuditEvent>,
    result: Cell,
    stats: Stats,
    live: u64,
}

/// Runs `program` to its end, continuing after every yield.
fn execute(program: &Program, limit: u64, quota: u64) -> Ran {
    let code = Evaluator::new(program).expect("valid execution IR");
    admit(&code.requirements()).expect("an admissible program");
    let mut host = WorldProvider::new(&HostWorld::default());
    let mut ctx = Context::new(limit, code.globals(), &mut host);
    code.start(&mut ctx).expect("room for the entry frame");
    let outcome = loop {
        match run(&mut ctx, &code, quota) {
            Outcome::Yielded => {}
            other => break other,
        }
    };
    Ran {
        outcome,
        audit: ctx.audit().to_vec(),
        result: ctx.result(),
        stats: ctx.stats(),
        live: ctx.live_size(),
    }
}

fn stdout_event(text: &str) -> String {
    format!(
        "(record kind: @host op: \"io.stdout-text\" args: (array \"{text}\") result: \
         (record kind: @enum type: @std.result.result variant: @ok))"
    )
}

#[test]
fn the_demo_tail_calls_indirectly_performs_one_host_operation_and_traps_with_its_origin()
 {
    let ran = execute(&demo(1000, true), LIMIT, UNBOUNDED);
    assert_eq!(
        ran.outcome,
        Outcome::Trapped(Trap {
            code: TrapCode::DivisionByZero,
            origin: 2
        })
    );
    assert_eq!(
        ran.audit
            .iter()
            .map(AuditEvent::to_vibon)
            .collect::<Vec<_>>(),
        [stdout_event("done\\n")]
    );
    assert_eq!(ran.stats.tail_transfers, 1000);
    assert_eq!(ran.stats.yields, 0);
}

#[test]
fn yielding_at_every_checkpoint_changes_no_observation() {
    for trap in [false, true] {
        let whole = execute(&demo(1000, trap), LIMIT, UNBOUNDED);
        for quota in [1, 2, 7] {
            let sliced = execute(&demo(1000, trap), LIMIT, quota);
            assert_eq!(sliced.outcome, whole.outcome, "quota {quota}");
            assert_eq!(sliced.audit, whole.audit, "quota {quota}");
            assert_eq!(sliced.result, whole.result, "quota {quota}");
            assert_eq!(
                sliced.audit.len(),
                1,
                "the host operation runs exactly once"
            );
            assert!(sliced.stats.yields >= 1000 / quota, "quota {quota}");
        }
    }
}

#[test]
fn tail_transfers_keep_the_frame_count_and_the_live_size_bounded() {
    let short = execute(&demo(10_000, false), LIMIT, UNBOUNDED);
    let long = execute(&demo(100_000, false), LIMIT, UNBOUNDED);
    for ran in [&short, &long] {
        assert_eq!(ran.outcome, Outcome::Completed);
        assert_eq!(ran.result, Cell::i64(0));
        assert_eq!(ran.stats.max_depth, 2, "the entry and one activation");
        assert_eq!(ran.live, 0, "a finished run holds nothing");
    }
    assert_eq!(long.stats.tail_transfers, 100_000);
    // Ten times the rounds in one megabyte: nothing grows with the count.
    let small = execute(&demo(100_000, false), MIB, UNBOUNDED);
    assert_eq!(small.outcome, Outcome::Completed);
}

#[test]
fn non_tail_recursion_is_bounded_by_memory_and_uses_no_native_stack() {
    // A thread stack that a native frame per activation would overflow.
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let ran = execute(&deep(100_000), LIMIT, UNBOUNDED);
            assert_eq!(ran.outcome, Outcome::Completed);
            assert_eq!(ran.result, Cell::i64(100_000));
            assert_eq!(ran.stats.max_depth, 100_002);
            assert_eq!(ran.live, 0);
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn exhausting_the_limit_is_the_host_event_and_a_larger_limit_recovers() {
    let small = execute(&deep(100_000), 64 * 1024, UNBOUNDED);
    assert_eq!(small.outcome, Outcome::MemoryExhausted);
    assert!(small.audit.is_empty());
    let sliced = execute(&deep(100_000), 64 * 1024, 3);
    assert_eq!(sliced.outcome, Outcome::MemoryExhausted);
    assert_eq!(
        execute(&deep(100_000), LIMIT, UNBOUNDED).outcome,
        Outcome::Completed
    );
}

#[test]
fn a_module_value_is_initialized_once_however_often_it_is_read() {
    for quota in [UNBOUNDED, 1] {
        let ran = execute(&module_value(), LIMIT, quota);
        assert_eq!(ran.outcome, Outcome::Completed);
        assert_eq!(ran.result, Cell::i64(7));
        assert_eq!(
            ran.audit
                .iter()
                .map(AuditEvent::to_vibon)
                .collect::<Vec<_>>(),
            [stdout_event("init\\n")],
            "quota {quota}"
        );
        // A 24-byte header and five 4-byte scalars, in a 64-byte block.
        assert_eq!(ran.live, 64, "only the module state's `str` is held");
    }
}

#[test]
fn a_function_value_called_with_another_arity_is_a_toolchain_defect() {
    let mut program = demo(1, false);
    // `even` now reads two operands, and `main` still passes one.
    program.functions[1].params = 2;
    program.functions[1].slots = 8;
    program.functions[1].blocks = vec![block(Vec::new(), ret(0))];
    program.functions[2] =
        function("odd", 0, false, 1, vec![block(Vec::new(), ret(0))]);
    program.functions.truncate(2);
    let ran = execute(&program, LIMIT, UNBOUNDED);
    assert_eq!(
        ran.outcome,
        Outcome::Trapped(Trap {
            code: TrapCode::InvalidCheckedProgram,
            origin: 0
        })
    );
    assert!(ran.audit.is_empty());
}

#[test]
fn invalid_execution_ir_is_rejected_before_it_runs() {
    let mut program = demo(1, false);
    program.functions[0].blocks[0].insts.clear();
    assert!(matches!(
        Evaluator::new(&program),
        Err(EvalError::Invalid(_))
    ));
}

#[test]
fn an_incompatible_runtime_identity_and_an_unknown_symbol_are_refused() {
    let program = demo(1, false);
    let code = Evaluator::new(&program).unwrap();
    let required = code.requirements();
    assert_eq!(required.host_symbols, ["io.stdout-text"]);
    assert_eq!(admit(&required), Ok(()));

    let other_abi = Requirements {
        abi: ABI_VERSION + 1,
        ..required.clone()
    };
    assert_eq!(
        admit(&other_abi),
        Err(AdmitError::Abi {
            runtime: ABI_VERSION,
            program: ABI_VERSION + 1
        })
    );
    let later_registry = Requirements {
        registry_minor: 99,
        ..required.clone()
    };
    assert!(matches!(
        admit(&later_registry),
        Err(AdmitError::RegistryMinor { program: 99, .. })
    ));
    let unknown = Requirements {
        host_symbols: vec!["net.connect"],
        ..required
    };
    assert_eq!(
        admit(&unknown),
        Err(AdmitError::UnknownSymbol("net.connect".to_owned()))
    );
}
