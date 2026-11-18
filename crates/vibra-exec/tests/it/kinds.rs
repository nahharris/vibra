//! Every kind of rejection has a program that is rejected for exactly it.

use std::collections::BTreeSet;

use vibra_exec::{
    BlockId, Callee, ConstId, FuncId, GlobalId, Inst, IntOp, OriginId, Program,
    ScalarOp, Slot, Terminator, VerifyErrorKind, verify,
};
use vibra_ir::boundary::TrapCode;
use vibra_ir::external::NumericType;

use super::{block, entry, function, program, ret, scalar};

/// The name of a kind. The `match` names every variant and has no wildcard,
/// so a new `VerifyErrorKind` does not compile until it is added here, and
/// [`NAMES`] and [`rejected_programs`] then have to follow.
fn name(kind: &VerifyErrorKind) -> &'static str {
    match kind {
        VerifyErrorKind::NoFunctions => "NoFunctions",
        VerifyErrorKind::MissingRoot(_) => "MissingRoot",
        VerifyErrorKind::RootTakesOperands(_) => "RootTakesOperands",
        VerifyErrorKind::NoBlocks => "NoBlocks",
        VerifyErrorKind::FrameTooSmall => "FrameTooSmall",
        VerifyErrorKind::FrameTooLarge => "FrameTooLarge",
        VerifyErrorKind::UnreachableBlock => "UnreachableBlock",
        VerifyErrorKind::UnknownBlock(_) => "UnknownBlock",
        VerifyErrorKind::UnknownSlot(_) => "UnknownSlot",
        VerifyErrorKind::UnknownFunction(_) => "UnknownFunction",
        VerifyErrorKind::UnknownConstant(_) => "UnknownConstant",
        VerifyErrorKind::UnknownGlobal(_) => "UnknownGlobal",
        VerifyErrorKind::UnknownOrigin(_) => "UnknownOrigin",
        VerifyErrorKind::Arity { .. } => "Arity",
        VerifyErrorKind::NotAnIntegerType(_) => "NotAnIntegerType",
        VerifyErrorKind::DirectCallNeedsEnvironment(_) => "DirectCallNeedsEnvironment",
        VerifyErrorKind::FunctionValueWithoutEnvironment(_) => {
            "FunctionValueWithoutEnvironment"
        }
        VerifyErrorKind::ConsumedTwice(_) => "ConsumedTwice",
        VerifyErrorKind::ReadOfEmpty(_) => "ReadOfEmpty",
        VerifyErrorKind::WriteOfFull(_) => "WriteOfFull",
    }
}

/// Every name [`name`] can answer: one per variant.
const NAMES: [&str; 20] = [
    "NoFunctions",
    "MissingRoot",
    "RootTakesOperands",
    "NoBlocks",
    "FrameTooSmall",
    "FrameTooLarge",
    "UnreachableBlock",
    "UnknownBlock",
    "UnknownSlot",
    "UnknownFunction",
    "UnknownConstant",
    "UnknownGlobal",
    "UnknownOrigin",
    "Arity",
    "NotAnIntegerType",
    "DirectCallNeedsEnvironment",
    "FunctionValueWithoutEnvironment",
    "ConsumedTwice",
    "ReadOfEmpty",
    "WriteOfFull",
];

fn tail_call(callee: FuncId, args: Vec<Slot>) -> Terminator {
    Terminator::TailCall {
        callee: Callee::Direct(callee),
        args,
    }
}

/// A program and the one kind it is rejected with.
fn rejected_programs() -> Vec<(Program, VerifyErrorKind)> {
    let one_slot = || entry(1, vec![block(vec![scalar(0, 0)], ret(0))]);

    let mut missing_root = one_slot();
    missing_root.entry = FuncId(9);

    let root_takes_operands =
        program(vec![function(1, false, 1, vec![block(Vec::new(), ret(0))])]);

    let frame_too_small = program(vec![
        function(0, false, 1, vec![block(vec![scalar(0, 0)], ret(0))]),
        function(2, true, 2, vec![block(Vec::new(), ret(0))]),
    ]);

    let unreachable_block = entry(
        1,
        vec![
            block(vec![scalar(0, 0)], ret(0)),
            block(vec![scalar(0, 0)], ret(0)),
        ],
    );

    let unknown_constant = entry(
        1,
        vec![block(
            vec![Inst::Constant {
                dst: Slot(0),
                constant: ConstId(0),
            }],
            ret(0),
        )],
    );

    let unknown_global = entry(
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

    let unknown_origin = entry(
        1,
        vec![block(
            Vec::new(),
            Terminator::Trap {
                code: TrapCode::DivisionByZero,
                origin: OriginId(1),
            },
        )],
    );

    let takes_one = || function(1, false, 1, vec![block(Vec::new(), ret(0))]);
    let arity = program(vec![
        function(
            0,
            false,
            2,
            vec![block(
                vec![scalar(0, 0), scalar(1, 0)],
                tail_call(FuncId(1), vec![Slot(0), Slot(1)]),
            )],
        ),
        takes_one(),
    ]);
    let not_an_integer_type = entry(
        3,
        vec![block(
            vec![
                scalar(0, 0),
                scalar(1, 0),
                Inst::Op {
                    dst: Slot(2),
                    op: ScalarOp::Int(IntOp::Add, NumericType::F64),
                    args: vec![Slot(0), Slot(1)],
                },
            ],
            ret(2),
        )],
    );
    let direct_call_needs_environment = program(vec![
        function(
            0,
            false,
            1,
            vec![block(Vec::new(), tail_call(FuncId(1), Vec::new()))],
        ),
        function(0, true, 1, vec![block(Vec::new(), ret(0))]),
    ]);

    let function_value_without_environment = program(vec![
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
        takes_one(),
    ]);

    let consumed_twice = program(vec![
        function(
            0,
            false,
            1,
            vec![block(
                vec![scalar(0, 0)],
                tail_call(FuncId(1), vec![Slot(0), Slot(0)]),
            )],
        ),
        function(2, false, 2, vec![block(Vec::new(), ret(0))]),
    ]);

    vec![
        (program(Vec::new()), VerifyErrorKind::NoFunctions),
        (missing_root, VerifyErrorKind::MissingRoot(FuncId(9))),
        (
            root_takes_operands,
            VerifyErrorKind::RootTakesOperands(FuncId(0)),
        ),
        (entry(0, Vec::new()), VerifyErrorKind::NoBlocks),
        (frame_too_small, VerifyErrorKind::FrameTooSmall),
        (
            entry(u32::MAX, vec![block(vec![scalar(0, 0)], ret(0))]),
            VerifyErrorKind::FrameTooLarge,
        ),
        (unreachable_block, VerifyErrorKind::UnreachableBlock),
        (
            entry(1, vec![block(Vec::new(), Terminator::Jump(BlockId(3)))]),
            VerifyErrorKind::UnknownBlock(BlockId(3)),
        ),
        (
            entry(1, vec![block(vec![scalar(5, 0)], ret(0))]),
            VerifyErrorKind::UnknownSlot(Slot(5)),
        ),
        (
            entry(1, vec![block(Vec::new(), tail_call(FuncId(7), Vec::new()))]),
            VerifyErrorKind::UnknownFunction(FuncId(7)),
        ),
        (unknown_constant, VerifyErrorKind::UnknownConstant(0)),
        (unknown_global, VerifyErrorKind::UnknownGlobal(0)),
        (unknown_origin, VerifyErrorKind::UnknownOrigin(OriginId(1))),
        (
            arity,
            VerifyErrorKind::Arity {
                expected: 1,
                found: 2,
            },
        ),
        (
            not_an_integer_type,
            VerifyErrorKind::NotAnIntegerType(NumericType::F64),
        ),
        (
            direct_call_needs_environment,
            VerifyErrorKind::DirectCallNeedsEnvironment(FuncId(1)),
        ),
        (
            function_value_without_environment,
            VerifyErrorKind::FunctionValueWithoutEnvironment(FuncId(1)),
        ),
        (consumed_twice, VerifyErrorKind::ConsumedTwice(Slot(0))),
        (
            entry(1, vec![block(Vec::new(), ret(0))]),
            VerifyErrorKind::ReadOfEmpty(Slot(0)),
        ),
        (
            entry(1, vec![block(vec![scalar(0, 0), scalar(0, 1)], ret(0))]),
            VerifyErrorKind::WriteOfFull(Slot(0)),
        ),
    ]
}

#[test]
fn every_verify_error_kind_has_a_rejected_program() {
    let mut produced = BTreeSet::new();
    for (program, expected) in rejected_programs() {
        let error = verify(&program).expect_err("the program is invalid");
        assert_eq!(error.kind, expected, "for the program\n{program}");
        produced.insert(name(&error.kind));
    }
    let every: BTreeSet<&str> = NAMES.into_iter().collect();
    assert_eq!(every.len(), NAMES.len(), "NAMES repeats a name");
    assert_eq!(
        produced, every,
        "some kind of rejection has no program, or a program is rejected for another kind"
    );
}
