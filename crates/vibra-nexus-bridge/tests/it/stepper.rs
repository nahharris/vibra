//! The same programs as execution IR, run by the reference stepper: the
//! expectation the bridge must reproduce. These are the programs of
//! `crates/vibra-nexus-eval/tests/it/main.rs`, and `docs/roadmap/nexus/validation.md`
//! maps their tests to the exit clauses.

use vibra_exec::{
    Block, BlockId, Callee, ConstId, Constant, FuncId, Function, GlobalId, Inst,
    ObjectKind, OriginId, Program, ScalarClass, ScalarOp, Slot, Terminator,
};
use vibra_host::{HostWorld, WorldProvider};
use vibra_ir::SourceOrigin;
use vibra_ir::boundary::TrapCode;
use vibra_ir::external::host::host_row;
use vibra_nexus::{Context, Outcome, admit, run};
use vibra_nexus_eval::Evaluator;

use crate::fixture::Ran;

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
    SourceOrigin::new(
        "app/main.vib",
        vibra_diagnostics::ByteSpan::new(start, start + 1),
    )
}

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

pub fn demo(rounds: u64, trap: bool) -> Program {
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

pub fn demo_with_arity_defect() -> Program {
    let mut program = demo(1, false);
    program.functions[1].params = 2;
    program.functions[1].slots = 8;
    program.functions[1].blocks = vec![block(Vec::new(), ret(0))];
    program.functions[2] =
        function("odd", 0, false, 1, vec![block(Vec::new(), ret(0))]);
    program.functions.truncate(2);
    program
}

pub fn deep(n: u64) -> Program {
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

pub fn module_value() -> Program {
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

/// Runs `program` on the stepper to its end, continuing after every yield.
pub fn execute(program: &Program, limit: u64, quota: u64) -> Ran {
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
