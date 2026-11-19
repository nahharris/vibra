//! Execution-IR programs the lowering slice cannot produce yet, built by hand
//! for the linked-executable tests (package X3-07): a trap, an entry that
//! returns `err`, the foundation's indirect tail-transfer demo, and a deep
//! non-tail recursion.
//!
//! The lowering emits no `Trap` and cannot build a `result.err` (that is the
//! variant form of package X3-11), so these are the only way to put an exit
//! code of 5 or 6 behind a linked process today.

use vibra_diagnostics::ByteSpan;
use vibra_exec::{
    Block, BlockId, Callee, ConstId, Constant, FuncId, Function, Inst, ObjectKind,
    OriginId, Program, ScalarClass, ScalarOp, Slot, Terminator,
};
use vibra_ir::SourceOrigin;
use vibra_ir::boundary::TrapCode;
use vibra_ir::external::host::host_row;

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

fn drop_slot(slot: u32) -> Inst {
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

fn origin(source: &str, start: usize, end: usize) -> SourceOrigin {
    SourceOrigin::new(source, ByteSpan::new(start, end))
}

/// One function of the program called `hop`: tail-calls `other` through a
/// function value with the counter decremented, and returns the counter when
/// it reaches zero.
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

/// The foundation's demo: `rounds` indirect tail transfers between `even` and
/// `odd`, one `io.stdout-text` of `done\n`, then a return of the counter, or a
/// `DivisionByZero` trap at the second origin when `trap` is set.
pub(crate) fn demo(rounds: u64, trap: bool) -> Program {
    let end = if trap {
        block(
            Vec::new(),
            Terminator::Trap {
                code: TrapCode::DivisionByZero,
                origin: OriginId(2),
            },
        )
    } else {
        block(vec![drop_slot(1), drop_slot(3)], ret(2))
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
                    drop_slot(0),
                    Inst::Constant {
                        dst: Slot(3),
                        constant: ConstId(0),
                    },
                ],
                Terminator::Host {
                    row: host_row("io.stdout-text").expect("a registry row"),
                    args: vec![Slot(3)],
                    dst: Slot(1),
                    resume: BlockId(2),
                    origin: OriginId(1),
                },
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

/// An entry that traps at once with `DivisionByZero`, origin 1.
pub(crate) fn trap_at_once() -> Program {
    let main = function(
        "main",
        0,
        false,
        0,
        vec![block(
            Vec::new(),
            Terminator::Trap {
                code: TrapCode::DivisionByZero,
                origin: OriginId(1),
            },
        )],
    );
    Program {
        functions: vec![main],
        entry: FuncId(0),
        globals: Vec::new(),
        constants: Vec::new(),
        origins: vec![origin("tests/trap.vib", 3, 9)],
    }
}

/// An entry that returns the second variant of an enum with no payload, which
/// the test declares to be the `err` variant (`entry_outcome` 2).
pub(crate) fn entry_returns_err() -> Program {
    let main = function(
        "main",
        0,
        false,
        1,
        vec![block(
            vec![Inst::Build {
                dst: Slot(0),
                kind: ObjectKind::Enum,
                variant: 1,
                cells: Vec::new(),
            }],
            ret(0),
        )],
    );
    Program {
        functions: vec![main],
        entry: FuncId(0),
        globals: Vec::new(),
        constants: Vec::new(),
        origins: Vec::new(),
    }
}

/// Non-tail recursion through a direct call, `n` frames deep: `count(n)`
/// returns `n`.
pub(crate) fn deep(n: u64) -> Program {
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
