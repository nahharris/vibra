//! Every instruction and terminator the stepper runs has a native form, and
//! the generator says so by name when it has not: a variant added to the
//! execution IR without generator support fails to compile here, because each
//! `match` below names every variant and has no wildcard.

use std::mem::discriminant;

use vibra_codegen::{EmitError, emit};
use vibra_exec::{
    BlockId, Callee, ConstId, Constant, FuncId, GlobalId, Inst, ObjectKind, OriginId,
    Program, ScalarClass, ScalarOp, Slot, Terminator, verified,
};
use vibra_ir::boundary::TrapCode;
use vibra_ir::external::CompilerIntrinsic;

use crate::programs::{block, build, constant, function, host, program, ret, scalar};
use crate::{TARGETS, inputs};

/// One instruction of every variant. The count is checked against the number
/// of arms of [`around_inst`].
fn instructions() -> Vec<Inst> {
    let slot = Slot(0);
    vec![
        Inst::Scalar {
            dst: slot,
            class: ScalarClass::I64,
            bits: 0,
        },
        Inst::Constant {
            dst: slot,
            constant: ConstId(0),
        },
        Inst::Copy {
            dst: slot,
            src: slot,
        },
        Inst::Move {
            dst: slot,
            src: slot,
        },
        Inst::Drop { slot },
        Inst::Op {
            dst: slot,
            op: ScalarOp::I64Add,
            args: Vec::new(),
        },
        Inst::Intrinsic {
            dst: slot,
            intrinsic: CompilerIntrinsic::StrLength,
            args: Vec::new(),
            origin: OriginId::NONE,
        },
        Inst::Build {
            dst: slot,
            kind: ObjectKind::Tuple,
            variant: 0,
            cells: Vec::new(),
        },
        Inst::Cell {
            dst: slot,
            object: slot,
            index: 0,
        },
        Inst::Variant {
            dst: slot,
            object: slot,
        },
        Inst::Function {
            dst: slot,
            function: FuncId(0),
            captures: Vec::new(),
        },
    ]
}

/// A small verified program that contains an instruction of the variant of
/// `inst`, and the number of the arm. No wildcard.
fn around_inst(inst: &Inst) -> (usize, Program) {
    let one = |insts: Vec<Inst>, result: u32| {
        program(vec![function(
            "main",
            0,
            false,
            4,
            vec![block(insts, ret(result))],
        )])
    };
    match inst {
        Inst::Scalar { .. } => (0, one(vec![scalar(0, 1)], 0)),
        Inst::Constant { .. } => {
            let mut p = one(vec![constant(0, 0)], 0);
            p.constants = vec![Constant::Atom("a".to_owned())];
            (1, p)
        }
        Inst::Copy { .. } => (
            2,
            one(
                vec![
                    scalar(0, 1),
                    Inst::Copy {
                        dst: Slot(1),
                        src: Slot(0),
                    },
                ],
                1,
            ),
        ),
        Inst::Move { .. } => (
            3,
            one(
                vec![
                    scalar(0, 1),
                    Inst::Move {
                        dst: Slot(1),
                        src: Slot(0),
                    },
                ],
                1,
            ),
        ),
        Inst::Drop { .. } => (
            4,
            one(
                vec![scalar(0, 1), Inst::Drop { slot: Slot(0) }, scalar(1, 2)],
                1,
            ),
        ),
        Inst::Op { .. } => (
            5,
            one(
                vec![
                    scalar(0, 1),
                    Inst::Op {
                        dst: Slot(1),
                        op: ScalarOp::I64LtS,
                        args: vec![Slot(0), Slot(0)],
                    },
                ],
                1,
            ),
        ),
        Inst::Intrinsic { .. } => {
            let mut p = one(
                vec![
                    constant(0, 0),
                    Inst::Intrinsic {
                        dst: Slot(1),
                        intrinsic: CompilerIntrinsic::StrLength,
                        args: vec![Slot(0)],
                        origin: OriginId::NONE,
                    },
                ],
                1,
            );
            p.constants = vec![Constant::Str("s".to_owned())];
            (6, p)
        }
        Inst::Build { .. } => (
            7,
            one(vec![scalar(0, 1), build(1, ObjectKind::Tuple, 0, &[0])], 1),
        ),
        Inst::Cell { .. } => (
            8,
            one(
                vec![
                    scalar(0, 1),
                    build(1, ObjectKind::Tuple, 0, &[0]),
                    Inst::Cell {
                        dst: Slot(2),
                        object: Slot(1),
                        index: 0,
                    },
                ],
                2,
            ),
        ),
        Inst::Variant { .. } => (
            9,
            one(
                vec![
                    scalar(0, 1),
                    build(1, ObjectKind::Enum, 1, &[0]),
                    Inst::Variant {
                        dst: Slot(2),
                        object: Slot(1),
                    },
                ],
                2,
            ),
        ),
        Inst::Function { .. } => (
            10,
            program(vec![
                function(
                    "main",
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
                function("closure", 0, true, 1, vec![block(Vec::new(), ret(0))]),
            ]),
        ),
    }
}

/// The number of arms of [`around_inst`].
const INSTRUCTION_VARIANTS: usize = 11;

/// One terminator of every variant.
fn terminators() -> Vec<Terminator> {
    let slot = Slot(0);
    let target = BlockId(0);
    let callee = Callee::Direct(FuncId(0));
    vec![
        Terminator::Jump(target),
        Terminator::Branch {
            cond: slot,
            then_block: target,
            else_block: target,
        },
        Terminator::Switch {
            value: slot,
            targets: Vec::new(),
            default: target,
        },
        Terminator::Call {
            callee,
            args: Vec::new(),
            dst: slot,
            resume: target,
        },
        Terminator::TailCall {
            callee,
            args: Vec::new(),
        },
        Terminator::Host {
            row: vibra_ir::external::host::host_row("io.stdin-text").expect("row"),
            args: Vec::new(),
            dst: slot,
            resume: target,
            origin: OriginId::NONE,
        },
        Terminator::Global {
            global: GlobalId(0),
            dst: slot,
            resume: target,
        },
        Terminator::Return { value: slot },
        Terminator::Trap {
            code: TrapCode::DivisionByZero,
            origin: OriginId::NONE,
        },
    ]
}

/// A small verified program that ends a block with a terminator of the variant
/// of `term`, and the number of the arm. No wildcard.
fn around_terminator(term: &Terminator) -> (usize, Program) {
    let leaf =
        || function("leaf", 0, false, 1, vec![block(vec![scalar(0, 1)], ret(0))]);
    let main = |blocks| function("main", 0, false, 2, blocks);
    match term {
        Terminator::Jump(_) => (
            0,
            program(vec![main(vec![
                block(vec![scalar(0, 1)], Terminator::Jump(BlockId(1))),
                block(Vec::new(), ret(0)),
            ])]),
        ),
        Terminator::Branch { .. } => (
            1,
            program(vec![main(vec![
                block(
                    vec![scalar(0, 1)],
                    Terminator::Branch {
                        cond: Slot(0),
                        then_block: BlockId(1),
                        else_block: BlockId(2),
                    },
                ),
                block(Vec::new(), ret(0)),
                block(Vec::new(), ret(0)),
            ])]),
        ),
        Terminator::Switch { .. } => (
            2,
            program(vec![main(vec![
                block(
                    vec![scalar(0, 1)],
                    Terminator::Switch {
                        value: Slot(0),
                        targets: vec![BlockId(1), BlockId(2)],
                        default: BlockId(2),
                    },
                ),
                block(Vec::new(), ret(0)),
                block(Vec::new(), ret(0)),
            ])]),
        ),
        Terminator::Call { .. } => (
            3,
            program(vec![
                main(vec![
                    block(
                        Vec::new(),
                        Terminator::Call {
                            callee: Callee::Direct(FuncId(1)),
                            args: Vec::new(),
                            dst: Slot(0),
                            resume: BlockId(1),
                        },
                    ),
                    block(Vec::new(), ret(0)),
                ]),
                leaf(),
            ]),
        ),
        Terminator::TailCall { .. } => (
            4,
            program(vec![
                main(vec![block(
                    Vec::new(),
                    Terminator::TailCall {
                        callee: Callee::Direct(FuncId(1)),
                        args: Vec::new(),
                    },
                )]),
                leaf(),
            ]),
        ),
        Terminator::Host { .. } => (
            5,
            program(vec![main(vec![
                block(Vec::new(), host("io.stdin-text", &[], 0, 1, 0)),
                block(Vec::new(), ret(0)),
            ])]),
        ),
        Terminator::Global { .. } => {
            let mut p = program(vec![
                main(vec![
                    block(
                        Vec::new(),
                        Terminator::Global {
                            global: GlobalId(0),
                            dst: Slot(0),
                            resume: BlockId(1),
                        },
                    ),
                    block(Vec::new(), ret(0)),
                ]),
                leaf(),
            ]);
            p.globals = vec![FuncId(1)];
            (6, p)
        }
        Terminator::Return { .. } => (
            7,
            program(vec![main(vec![block(vec![scalar(0, 1)], ret(0))])]),
        ),
        Terminator::Trap { .. } => (
            8,
            program(vec![main(vec![block(
                Vec::new(),
                Terminator::Trap {
                    code: TrapCode::DivisionByZero,
                    origin: OriginId::NONE,
                },
            )])]),
        ),
    }
}

/// The number of arms of [`around_terminator`].
const TERMINATOR_VARIANTS: usize = 9;

fn contains_inst(program: &Program, wanted: &Inst) -> bool {
    program
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.insts)
        .any(|inst| discriminant(inst) == discriminant(wanted))
}

fn contains_terminator(program: &Program, wanted: &Terminator) -> bool {
    program
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .any(|block| discriminant(&block.term) == discriminant(wanted))
}

#[test]
fn every_instruction_the_stepper_runs_is_generated() {
    let all = instructions();
    assert_eq!(all.len(), INSTRUCTION_VARIANTS);
    let mut seen = [false; INSTRUCTION_VARIANTS];
    for inst in &all {
        let (arm, small) = around_inst(inst);
        assert!(!seen[arm], "arm {arm} is named twice");
        seen[arm] = true;
        assert!(
            contains_inst(&small, inst),
            "the program for arm {arm} has no instruction of the variant"
        );
        let proof = verified(&small).expect("a verified program");
        for target in TARGETS {
            let result = emit(proof, &target, &inputs(&small));
            if matches!(inst, Inst::Intrinsic { .. }) {
                // The stepper rejects an intrinsic too, whole and by name.
                assert_eq!(result.err(), Some(EmitError::Unsupported("an intrinsic")));
            } else {
                assert!(result.is_ok(), "{inst:?}: {:?}", result.err());
            }
        }
    }
    assert!(
        seen.iter().all(|seen| *seen),
        "every arm has a representative"
    );
}

#[test]
fn every_terminator_the_stepper_runs_is_generated() {
    let all = terminators();
    assert_eq!(all.len(), TERMINATOR_VARIANTS);
    let mut seen = [false; TERMINATOR_VARIANTS];
    for term in &all {
        let (arm, small) = around_terminator(term);
        assert!(!seen[arm], "arm {arm} is named twice");
        seen[arm] = true;
        assert!(
            contains_terminator(&small, term),
            "the program for arm {arm} has no terminator of the variant"
        );
        let proof = verified(&small).expect("a verified program");
        for target in TARGETS {
            let result = emit(proof, &target, &inputs(&small));
            assert!(result.is_ok(), "{term:?}: {:?}", result.err());
        }
    }
    assert!(
        seen.iter().all(|seen| *seen),
        "every arm has a representative"
    );
}

#[test]
fn an_intrinsic_anywhere_refuses_the_whole_program() {
    // The intrinsic is in the last function; the first ones are fine. No
    // object is produced, so nothing is emitted in part.
    let mut mixed = crate::programs::everything();
    mixed
        .functions
        .push(crate::programs::with_intrinsic().functions.remove(0));
    mixed.constants.push(Constant::Str("x".to_owned()));
    // The constant the intrinsic reads is constant 3 now; point it there.
    if let Some(Inst::Constant { constant, .. }) = mixed
        .functions
        .last_mut()
        .and_then(|function| function.blocks.first_mut())
        .and_then(|block| block.insts.first_mut())
    {
        *constant = ConstId(3);
    }
    let proof = verified(&mixed).expect("verified");
    for target in TARGETS {
        assert_eq!(
            emit(proof, &target, &inputs(&mixed)).err(),
            Some(EmitError::Unsupported("an intrinsic"))
        );
    }
}

/// Every scalar operation of the execution IR has a native form: the program
/// with one of each emits for every target, and the code the generator names is
/// the code the runtime decodes back to the same operation, in the bridge's own
/// table.
#[test]
fn every_scalar_operation_the_stepper_runs_is_generated() {
    let all = ScalarOp::all();
    assert_eq!(all.len(), 4 + 9 * 8);
    let mut codes = std::collections::BTreeSet::new();
    let mut insts = vec![scalar(0, 1)];
    for (index, op) in all.iter().enumerate() {
        let code = vibra_codegen::op_code(*op);
        assert!(codes.insert(code), "{op:?} shares a code");
        // The bridge decodes the code to the runtime's own name for the same
        // operation (the two enums spell their variants alike).
        let decoded = vibra_nexus_bridge::decode_scalar_op(code);
        assert_eq!(
            decoded.map(|runtime| format!("{runtime:?}")),
            Some(format!("{op:?}")),
            "{code:#x}"
        );
        insts.push(Inst::Op {
            dst: Slot(u32::try_from(index).unwrap() + 1),
            op: *op,
            args: vec![Slot(0), Slot(0)],
        });
    }
    let slots = u32::try_from(all.len()).unwrap() + 1;
    let small = program(vec![function(
        "main",
        0,
        false,
        slots,
        vec![block(insts, ret(0))],
    )]);
    let proof = verified(&small).expect("a verified program");
    for target in TARGETS {
        let result = emit(proof, &target, &inputs(&small));
        assert!(result.is_ok(), "{:?}", result.err());
    }
}
