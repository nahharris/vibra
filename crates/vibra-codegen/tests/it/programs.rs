//! Programs the tests emit: the foundation's proof cases, and one program that
//! uses every instruction and terminator that has a native form.

use vibra_diagnostics::ByteSpan;
use vibra_exec::{
    Block, BlockId, Callee, ConstId, Constant, FuncId, Function, GlobalId, Inst,
    ObjectKind, OriginId, Program, ScalarClass, ScalarOp, Slot, Terminator,
};
use vibra_ir::SourceOrigin;
use vibra_ir::boundary::TrapCode;
use vibra_ir::external::CompilerIntrinsic;
use vibra_ir::external::host::host_row;

pub fn scalar(dst: u32, bits: u64) -> Inst {
    Inst::Scalar {
        dst: Slot(dst),
        class: ScalarClass::I64,
        bits,
    }
}

pub fn op(dst: u32, op: ScalarOp, left: u32, right: u32) -> Inst {
    Inst::Op {
        dst: Slot(dst),
        op,
        args: vec![Slot(left), Slot(right)],
    }
}

pub fn drop(slot: u32) -> Inst {
    Inst::Drop { slot: Slot(slot) }
}

pub fn constant(dst: u32, constant: u32) -> Inst {
    Inst::Constant {
        dst: Slot(dst),
        constant: ConstId(constant),
    }
}

pub fn build(dst: u32, kind: ObjectKind, variant: u32, cells: &[u32]) -> Inst {
    Inst::Build {
        dst: Slot(dst),
        kind,
        variant,
        cells: cells.iter().copied().map(Slot).collect(),
    }
}

pub fn block(insts: Vec<Inst>, term: Terminator) -> Block {
    Block { insts, term }
}

pub fn ret(slot: u32) -> Terminator {
    Terminator::Return { value: Slot(slot) }
}

pub fn function(
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

pub fn host(
    symbol: &str,
    args: &[u32],
    dst: u32,
    resume: u32,
    origin: u32,
) -> Terminator {
    Terminator::Host {
        row: host_row(symbol).expect("a registry row"),
        args: args.iter().copied().map(Slot).collect(),
        dst: Slot(dst),
        resume: BlockId(resume),
        origin: OriginId(origin),
    }
}

pub fn origin(source: &str, start: usize, end: usize) -> SourceOrigin {
    SourceOrigin::new(source, ByteSpan::new(start, end))
}

pub fn program(functions: Vec<Function>) -> Program {
    Program {
        functions,
        entry: FuncId(0),
        globals: Vec::new(),
        constants: Vec::new(),
        origins: Vec::new(),
    }
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

/// The M1 demo of the foundation: an indirect tail transfer between two
/// functions `rounds` times, one stdout operation, then a trap or a return.
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
                vec![drop(0), constant(3, 0)],
                host("io.stdout-text", &[3], 1, 2, 1),
            ),
            end,
        ],
    );
    Program {
        functions: vec![main, hop("even", 2), hop("odd", 1)],
        entry: FuncId(0),
        globals: Vec::new(),
        constants: vec![Constant::Str("done\n".to_owned())],
        origins: vec![
            origin("app/main.vib", 10, 11),
            origin("app/main.vib", 20, 21),
        ],
    }
}

/// Non-tail recursion through a direct call.
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
    program(vec![main, count])
}

/// A module value read twice, whose initializer performs a host operation.
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
                vec![build(2, ObjectKind::Tuple, 0, &[0, 1]), scalar(3, 7)],
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
            block(vec![constant(0, 0)], host("io.stdout-text", &[0], 1, 1, 0)),
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

/// A program with every instruction and terminator that has a native form,
/// three kinds of constant, two host rows, two sources, and a `Switch`.
pub fn everything() -> Program {
    let class = |dst: u32, class: ScalarClass, bits: u64| Inst::Scalar {
        dst: Slot(dst),
        class,
        bits,
    };
    let switch = |value: u32, targets: &[u32], default: u32| Terminator::Switch {
        value: Slot(value),
        targets: targets.iter().copied().map(BlockId).collect(),
        default: BlockId(default),
    };
    let tail_direct = |function: u32| Terminator::TailCall {
        callee: Callee::Direct(FuncId(function)),
        args: Vec::new(),
    };
    let main = function(
        "main",
        0,
        false,
        24,
        vec![
            // 0
            block(
                vec![
                    class(0, ScalarClass::I32, 1),
                    scalar(1, 2),
                    class(2, ScalarClass::F32, 3),
                    class(3, ScalarClass::F64, 4),
                    constant(4, 0),
                    constant(5, 1),
                    constant(6, 2),
                    Inst::Copy {
                        dst: Slot(7),
                        src: Slot(4),
                    },
                    Inst::Move {
                        dst: Slot(8),
                        src: Slot(5),
                    },
                    drop(8),
                    op(9, ScalarOp::I64Add, 1, 1),
                    op(10, ScalarOp::I64Sub, 1, 9),
                    op(11, ScalarOp::I64Eq, 1, 9),
                    op(12, ScalarOp::I64LtS, 1, 9),
                    build(13, ObjectKind::Tuple, 0, &[2, 3]),
                    Inst::Cell {
                        dst: Slot(14),
                        object: Slot(13),
                        index: 0,
                    },
                    build(15, ObjectKind::Enum, 1, &[0]),
                    Inst::Variant {
                        dst: Slot(16),
                        object: Slot(15),
                    },
                    build(17, ObjectKind::Array, 0, &[9]),
                    build(18, ObjectKind::Record, 0, &[10]),
                    build(19, ObjectKind::Wrapper, 0, &[11]),
                    build(20, ObjectKind::Union, 2, &[12]),
                    Inst::Function {
                        dst: Slot(21),
                        function: FuncId(2),
                        captures: Vec::new(),
                    },
                ],
                Terminator::Branch {
                    cond: Slot(16),
                    then_block: BlockId(1),
                    else_block: BlockId(2),
                },
            ),
            // 1
            block(Vec::new(), switch(16, &[3, 4], 5)),
            // 2
            block(Vec::new(), Terminator::Jump(BlockId(5))),
            // 3
            block(
                Vec::new(),
                Terminator::Call {
                    callee: Callee::Direct(FuncId(1)),
                    args: vec![Slot(1)],
                    dst: Slot(22),
                    resume: BlockId(6),
                },
            ),
            // 4
            block(
                Vec::new(),
                Terminator::Call {
                    callee: Callee::Indirect(Slot(21)),
                    args: Vec::new(),
                    dst: Slot(22),
                    resume: BlockId(6),
                },
            ),
            // 5
            block(
                Vec::new(),
                Terminator::Global {
                    global: GlobalId(0),
                    dst: Slot(22),
                    resume: BlockId(6),
                },
            ),
            // 6
            block(Vec::new(), host("io.stdout-text", &[4], 23, 7, 1)),
            // 7
            block(Vec::new(), host("io.stdin-text", &[], 2, 8, 2)),
            // 8
            block(Vec::new(), switch(16, &[9, 10, 12], 11)),
            // 9
            block(Vec::new(), tail_direct(4)),
            // 10
            block(Vec::new(), ret(22)),
            // 11
            block(
                Vec::new(),
                Terminator::Trap {
                    code: TrapCode::DivisionByZero,
                    origin: OriginId(2),
                },
            ),
            // 12
            block(
                Vec::new(),
                Terminator::TailCall {
                    callee: Callee::Indirect(Slot(21)),
                    args: Vec::new(),
                },
            ),
        ],
    );
    let identity = function("identity", 1, false, 2, vec![block(Vec::new(), ret(0))]);
    let closure = function("closure", 0, true, 1, vec![block(Vec::new(), ret(0))]);
    let initializer = function(
        "initializer",
        0,
        false,
        1,
        vec![block(vec![class(0, ScalarClass::I32, 7)], ret(0))],
    );
    let leaf = function(
        "leaf",
        0,
        false,
        1,
        vec![block(vec![class(0, ScalarClass::I32, 0)], ret(0))],
    );
    Program {
        functions: vec![main, identity, closure, initializer, leaf],
        entry: FuncId(0),
        globals: vec![FuncId(3)],
        constants: vec![
            Constant::Str("hello \u{e9}".to_owned()),
            Constant::Bytes(vec![0, 255, 1]),
            Constant::Atom("ok".to_owned()),
        ],
        origins: vec![origin("app/main.vib", 0, 5), origin("lib/util.vib", 7, 9)],
    }
}

/// The programs the determinism and object-shape tests use.
pub fn all() -> Vec<(&'static str, Program)> {
    vec![
        ("demo", demo(1000, false)),
        ("demo-trap", demo(3, true)),
        ("deep", deep(100)),
        ("module-value", module_value()),
        ("everything", everything()),
    ]
}

/// A program with an intrinsic, which the generator refuses.
pub fn with_intrinsic() -> Program {
    let mut program = program(vec![function(
        "main",
        0,
        false,
        2,
        vec![block(
            vec![
                constant(0, 0),
                Inst::Intrinsic {
                    dst: Slot(1),
                    intrinsic: CompilerIntrinsic::StrLength,
                    args: vec![Slot(0)],
                    origin: OriginId::NONE,
                },
            ],
            ret(1),
        )],
    )]);
    program.constants = vec![Constant::Str("x".to_owned())];
    program
}
