//! The `vibra-exec` integration tests: one accepted program and one rejected
//! program for each verification rule of `vibra_exec::verify`.
//!
//! Run them with `cargo test --locked -p vibra-exec --test it`.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    missing_docs
)]

use vibra_exec::{
    Block, BlockId, Callee, ConstId, FuncId, Function, GlobalId, Inst, MAX_SLOTS,
    OriginId, Program, ScalarClass, ScalarOp, Slot, Terminator, VerifyErrorKind,
    verify,
};
use vibra_ir::boundary::TrapCode;
use vibra_ir::external::host::host_row;

mod fuzz;
mod kinds;
mod text;

fn scalar(dst: u32, bits: u64) -> Inst {
    Inst::Scalar {
        dst: Slot(dst),
        class: ScalarClass::I64,
        bits,
    }
}

fn function(params: u32, env: bool, slots: u32, blocks: Vec<Block>) -> Function {
    Function {
        name: "f".to_owned(),
        params,
        env,
        slots,
        blocks,
    }
}

fn block(insts: Vec<Inst>, term: Terminator) -> Block {
    Block { insts, term }
}

fn ret(slot: u32) -> Terminator {
    Terminator::Return { value: Slot(slot) }
}

fn program(functions: Vec<Function>) -> Program {
    Program {
        functions,
        entry: FuncId(0),
        globals: Vec::new(),
        constants: Vec::new(),
        origins: Vec::new(),
    }
}

/// A program of one entry function with `slots` slots and these blocks.
fn entry(slots: u32, blocks: Vec<Block>) -> Program {
    program(vec![function(0, false, slots, blocks)])
}

fn rejected(program: &Program) -> VerifyErrorKind {
    verify(program).expect_err("the program is invalid").kind
}

#[test]
fn a_well_formed_program_verifies() {
    let callee = function(
        1,
        false,
        2,
        vec![block(
            vec![Inst::Copy {
                dst: Slot(1),
                src: Slot(0),
            }],
            ret(1),
        )],
    );
    let main = function(
        0,
        false,
        2,
        vec![
            block(
                vec![scalar(0, 1)],
                Terminator::Call {
                    callee: Callee::Direct(FuncId(1)),
                    args: vec![Slot(0)],
                    // The operand is consumed before the result is written.
                    dst: Slot(0),
                    resume: BlockId(1),
                },
            ),
            block(Vec::new(), ret(0)),
        ],
    );
    assert_eq!(verify(&program(vec![main, callee])), Ok(()));
}

#[test]
fn verified_wraps_exactly_the_programs_that_verify() {
    let good = entry(1, vec![block(vec![scalar(0, 0)], ret(0))]);
    let proof = vibra_exec::verified(&good).expect("a valid program");
    assert!(std::ptr::eq(proof.program(), &good));

    let bad = entry(1, vec![block(Vec::new(), ret(0))]);
    assert_eq!(
        vibra_exec::verified(&bad).expect_err("invalid").kind,
        VerifyErrorKind::ReadOfEmpty(Slot(0))
    );
}

#[test]
fn v1_roots_exist_and_take_no_operands() {
    assert_eq!(rejected(&program(Vec::new())), VerifyErrorKind::NoFunctions);

    let mut missing = entry(1, vec![block(vec![scalar(0, 0)], ret(0))]);
    missing.entry = FuncId(9);
    assert_eq!(rejected(&missing), VerifyErrorKind::MissingRoot(FuncId(9)));

    let takes = program(vec![function(1, false, 1, vec![block(Vec::new(), ret(0))])]);
    assert_eq!(
        rejected(&takes),
        VerifyErrorKind::RootTakesOperands(FuncId(0))
    );

    let mut initializer = entry(1, vec![block(vec![scalar(0, 0)], ret(0))]);
    initializer.globals = vec![FuncId(4)];
    assert_eq!(
        rejected(&initializer),
        VerifyErrorKind::MissingRoot(FuncId(4))
    );
}

#[test]
fn v2_a_function_has_reachable_blocks_and_a_frame_for_its_operands() {
    assert_eq!(rejected(&entry(0, Vec::new())), VerifyErrorKind::NoBlocks);

    let small = program(vec![
        function(0, false, 1, vec![block(vec![scalar(0, 0)], ret(0))]),
        function(2, true, 2, vec![block(Vec::new(), ret(0))]),
    ]);
    assert_eq!(rejected(&small), VerifyErrorKind::FrameTooSmall);

    let unreachable = entry(
        1,
        vec![
            block(vec![scalar(0, 0)], ret(0)),
            block(vec![scalar(0, 0)], ret(0)),
        ],
    );
    assert_eq!(rejected(&unreachable), VerifyErrorKind::UnreachableBlock);
}

#[test]
fn a_frame_larger_than_the_bound_is_rejected_without_allocating_it() {
    // Were any per-slot state allocated first, this would try to take
    // gigabytes; it must answer at once.
    let huge = entry(u32::MAX, vec![block(vec![scalar(0, 0)], ret(0))]);
    assert_eq!(rejected(&huge), VerifyErrorKind::FrameTooLarge);

    let just_over = entry(MAX_SLOTS + 1, vec![block(vec![scalar(0, 0)], ret(0))]);
    assert_eq!(rejected(&just_over), VerifyErrorKind::FrameTooLarge);

    let at_the_bound = entry(MAX_SLOTS, vec![block(vec![scalar(0, 0)], ret(0))]);
    assert_eq!(verify(&at_the_bound), Ok(()));
}

#[test]
fn v3_every_name_exists() {
    let jump = entry(1, vec![block(Vec::new(), Terminator::Jump(BlockId(3)))]);
    assert_eq!(rejected(&jump), VerifyErrorKind::UnknownBlock(BlockId(3)));

    let slot = entry(1, vec![block(vec![scalar(5, 0)], ret(0))]);
    assert_eq!(rejected(&slot), VerifyErrorKind::UnknownSlot(Slot(5)));

    let call = entry(
        1,
        vec![block(
            Vec::new(),
            Terminator::TailCall {
                callee: Callee::Direct(FuncId(7)),
                args: Vec::new(),
            },
        )],
    );
    assert_eq!(rejected(&call), VerifyErrorKind::UnknownFunction(FuncId(7)));

    let constant = entry(
        1,
        vec![block(
            vec![Inst::Constant {
                dst: Slot(0),
                constant: ConstId(0),
            }],
            ret(0),
        )],
    );
    assert_eq!(rejected(&constant), VerifyErrorKind::UnknownConstant(0));

    let global = entry(
        1,
        vec![
            block(
                Vec::new(),
                Terminator::Global {
                    global: GlobalId(0),
                    dst: Slot(0),
                    resume: BlockId(1),
                },
            ),
            block(Vec::new(), ret(0)),
        ],
    );
    assert_eq!(rejected(&global), VerifyErrorKind::UnknownGlobal(0));

    let origin = entry(
        1,
        vec![block(
            Vec::new(),
            Terminator::Trap {
                code: TrapCode::DivisionByZero,
                origin: OriginId(1),
            },
        )],
    );
    assert_eq!(
        rejected(&origin),
        VerifyErrorKind::UnknownOrigin(OriginId(1))
    );
}

#[test]
fn v4_operand_counts_and_environments_agree() {
    let callee = || function(1, false, 1, vec![block(Vec::new(), ret(0))]);
    let closure = || function(0, true, 1, vec![block(Vec::new(), ret(0))]);
    let calls = |target: Function, args: Vec<Slot>| {
        program(vec![
            function(
                0,
                false,
                2,
                vec![block(
                    vec![scalar(0, 0), scalar(1, 0)],
                    Terminator::TailCall {
                        callee: Callee::Direct(FuncId(1)),
                        args,
                    },
                )],
            ),
            target,
        ])
    };
    assert_eq!(
        rejected(&calls(callee(), vec![Slot(0), Slot(1)])),
        VerifyErrorKind::Arity {
            expected: 1,
            found: 2
        }
    );
    assert_eq!(
        rejected(&calls(closure(), Vec::new())),
        VerifyErrorKind::DirectCallNeedsEnvironment(FuncId(1))
    );

    let value = program(vec![
        function(
            0,
            false,
            1,
            vec![block(
                vec![Inst::Function {
                    dst: Slot(0),
                    function: FuncId(1),
                    captures: Vec::new(),
                }],
                ret(0),
            )],
        ),
        callee(),
    ]);
    assert_eq!(
        rejected(&value),
        VerifyErrorKind::FunctionValueWithoutEnvironment(FuncId(1))
    );

    let op = entry(
        2,
        vec![block(
            vec![
                scalar(0, 0),
                Inst::Op {
                    dst: Slot(1),
                    op: ScalarOp::I64Add,
                    args: vec![Slot(0)],
                },
            ],
            ret(1),
        )],
    );
    assert_eq!(
        rejected(&op),
        VerifyErrorKind::Arity {
            expected: 2,
            found: 1
        }
    );

    // An integer operation passes exactly two operands at every integer type,
    // and names an integer type.
    for numeric in vibra_exec::INTEGER_TYPES {
        for int in vibra_exec::IntOp::ALL {
            let build = |args: Vec<Slot>| {
                entry(
                    3,
                    vec![block(
                        vec![
                            scalar(0, 0),
                            scalar(1, 0),
                            Inst::Op {
                                dst: Slot(2),
                                op: ScalarOp::Int(int, numeric),
                                args,
                            },
                        ],
                        ret(2),
                    )],
                )
            };
            assert_eq!(verify(&build(vec![Slot(0), Slot(1)])), Ok(()));
            assert_eq!(
                rejected(&build(vec![Slot(0)])),
                VerifyErrorKind::Arity {
                    expected: 2,
                    found: 1
                }
            );
            assert_eq!(
                rejected(&build(vec![Slot(0), Slot(1), Slot(0)])),
                VerifyErrorKind::Arity {
                    expected: 2,
                    found: 3
                }
            );
        }
    }

    let host = entry(
        1,
        vec![
            block(
                Vec::new(),
                Terminator::Host {
                    row: host_row("io.stdout-text").unwrap(),
                    args: Vec::new(),
                    dst: Slot(0),
                    resume: BlockId(1),
                    origin: OriginId::NONE,
                },
            ),
            block(Vec::new(), ret(0)),
        ],
    );
    assert_eq!(
        rejected(&host),
        VerifyErrorKind::Arity {
            expected: 1,
            found: 0
        }
    );
}

#[test]
fn v5_a_consumed_operand_is_named_once() {
    let twice = program(vec![
        function(
            0,
            false,
            1,
            vec![block(
                vec![scalar(0, 0)],
                Terminator::TailCall {
                    callee: Callee::Direct(FuncId(1)),
                    args: vec![Slot(0), Slot(0)],
                },
            )],
        ),
        function(2, false, 2, vec![block(Vec::new(), ret(0))]),
    ]);
    assert_eq!(rejected(&twice), VerifyErrorKind::ConsumedTwice(Slot(0)));
}

#[test]
fn v6_a_slot_is_full_where_read_and_empty_where_written() {
    let read = entry(1, vec![block(Vec::new(), ret(0))]);
    assert_eq!(rejected(&read), VerifyErrorKind::ReadOfEmpty(Slot(0)));

    let write = entry(1, vec![block(vec![scalar(0, 0), scalar(0, 1)], ret(0))]);
    assert_eq!(rejected(&write), VerifyErrorKind::WriteOfFull(Slot(0)));

    let moved = entry(
        2,
        vec![block(
            vec![
                scalar(0, 0),
                Inst::Move {
                    dst: Slot(1),
                    src: Slot(0),
                },
            ],
            ret(0),
        )],
    );
    assert_eq!(rejected(&moved), VerifyErrorKind::ReadOfEmpty(Slot(0)));
}

/// Two arms that leave slot 1 in different states, then a join that uses it.
fn join(at_join: Vec<Inst>) -> Program {
    entry(
        2,
        vec![
            block(
                vec![scalar(0, 1)],
                Terminator::Branch {
                    cond: Slot(0),
                    then_block: BlockId(1),
                    else_block: BlockId(2),
                },
            ),
            block(vec![scalar(1, 7)], Terminator::Jump(BlockId(3))),
            block(Vec::new(), Terminator::Jump(BlockId(3))),
            block(at_join, ret(0)),
        ],
    )
}

#[test]
fn v6_paths_that_disagree_leave_a_slot_unusable_until_it_is_dropped() {
    assert_eq!(
        rejected(&join(vec![Inst::Copy {
            dst: Slot(1),
            src: Slot(0),
        }])),
        VerifyErrorKind::WriteOfFull(Slot(1))
    );
    let dropped = join(vec![
        Inst::Drop { slot: Slot(1) },
        Inst::Copy {
            dst: Slot(1),
            src: Slot(0),
        },
    ]);
    assert_eq!(verify(&dropped), Ok(()));
}

#[test]
fn v6_a_loop_reaches_a_fixed_point() {
    // Block 1 loops on itself: slot 1 is written and dropped on each round.
    let looping = entry(
        2,
        vec![
            block(vec![scalar(0, 1)], Terminator::Jump(BlockId(1))),
            block(
                vec![scalar(1, 0), Inst::Drop { slot: Slot(1) }],
                Terminator::Branch {
                    cond: Slot(0),
                    then_block: BlockId(1),
                    else_block: BlockId(2),
                },
            ),
            block(Vec::new(), ret(0)),
        ],
    );
    assert_eq!(verify(&looping), Ok(()));
}
