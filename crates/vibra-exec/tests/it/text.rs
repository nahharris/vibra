//! The text form of a program: exact, complete, and deterministic.

use vibra_exec::{
    Block, BlockId, Callee, ConstId, Constant, FuncId, Function, GlobalId, Inst,
    ObjectKind, OriginId, Program, ScalarClass, ScalarOp, Slot, Terminator,
};
use vibra_ir::boundary::TrapCode;
use vibra_ir::external::CompilerIntrinsic;
use vibra_ir::external::host::{HOST_ROWS, host_row};

/// A program that uses every instruction, every terminator, and every table.
/// It is not meant to verify; the text form must print it all the same.
fn everything() -> Program {
    let stdout = host_row("io.stdout-text").unwrap();
    let main = Function {
        name: "main".to_owned(),
        params: 0,
        env: false,
        slots: 6,
        blocks: vec![
            Block {
                insts: vec![
                    Inst::Scalar {
                        dst: Slot(0),
                        class: ScalarClass::I64,
                        bits: 7,
                    },
                    Inst::Constant {
                        dst: Slot(1),
                        constant: ConstId(0),
                    },
                    Inst::Copy {
                        dst: Slot(2),
                        src: Slot(0),
                    },
                    Inst::Move {
                        dst: Slot(3),
                        src: Slot(2),
                    },
                    Inst::Drop { slot: Slot(3) },
                    Inst::Op {
                        dst: Slot(2),
                        op: ScalarOp::I64Add,
                        args: vec![Slot(0), Slot(0)],
                    },
                    Inst::Intrinsic {
                        dst: Slot(3),
                        intrinsic: CompilerIntrinsic::StrLength,
                        args: vec![Slot(1)],
                        origin: OriginId(1),
                    },
                    Inst::Build {
                        dst: Slot(4),
                        kind: ObjectKind::Enum,
                        variant: 1,
                        cells: vec![Slot(3)],
                    },
                    Inst::Cell {
                        dst: Slot(5),
                        object: Slot(4),
                        index: 0,
                    },
                    Inst::Variant {
                        dst: Slot(3),
                        object: Slot(4),
                    },
                    Inst::Function {
                        dst: Slot(1),
                        function: FuncId(1),
                        captures: vec![Slot(0)],
                    },
                ],
                term: Terminator::Branch {
                    cond: Slot(0),
                    then_block: BlockId(1),
                    else_block: BlockId(2),
                },
            },
            Block {
                insts: Vec::new(),
                term: Terminator::Switch {
                    value: Slot(0),
                    targets: vec![BlockId(2), BlockId(3)],
                    default: BlockId(3),
                },
            },
            Block {
                insts: Vec::new(),
                term: Terminator::Call {
                    callee: Callee::Direct(FuncId(1)),
                    args: vec![Slot(0), Slot(1)],
                    dst: Slot(2),
                    resume: BlockId(3),
                },
            },
            Block {
                insts: Vec::new(),
                term: Terminator::Host {
                    row: stdout,
                    args: vec![Slot(1)],
                    dst: Slot(2),
                    resume: BlockId(4),
                    origin: OriginId::NONE,
                },
            },
            Block {
                insts: Vec::new(),
                term: Terminator::Global {
                    global: GlobalId(0),
                    dst: Slot(2),
                    resume: BlockId(5),
                },
            },
            Block {
                insts: Vec::new(),
                term: Terminator::TailCall {
                    callee: Callee::Indirect(Slot(2)),
                    args: vec![Slot(3)],
                },
            },
            Block {
                insts: Vec::new(),
                term: Terminator::Trap {
                    code: TrapCode::DivisionByZero,
                    origin: OriginId(1),
                },
            },
            Block {
                insts: Vec::new(),
                term: Terminator::Return { value: Slot(0) },
            },
            Block {
                insts: Vec::new(),
                term: Terminator::Jump(BlockId(7)),
            },
        ],
    };
    let closure = Function {
        name: "closure".to_owned(),
        params: 1,
        env: true,
        slots: 3,
        blocks: vec![Block {
            insts: Vec::new(),
            term: Terminator::Return { value: Slot(1) },
        }],
    };
    Program {
        functions: vec![main, closure],
        entry: FuncId(0),
        globals: vec![FuncId(0)],
        constants: vec![
            Constant::Str("hi\n".to_owned()),
            Constant::Bytes(vec![1, 255]),
            Constant::Atom("ok".to_owned()),
        ],
        origins: Vec::new(),
    }
}

const EXPECTED: &str = r#"program entry=f0 globals=[f0]
  constants
    c0 str "hi\n"
    c1 bytes [1, 255]
    c2 atom "ok"
  origins
fn f0 "main" params=0 env=false slots=6
  b0:
    s0 = scalar I64 7
    s1 = const c0
    s2 = copy s0
    s3 = move s2
    drop s3
    s2 = op I64Add [s0, s0]
    s3 = intrinsic "str.length" [s1] o1
    s4 = build Enum variant=1 [s3]
    s5 = cell s4 .0
    s3 = variant s4
    s1 = function f1 [s0]
    branch s0 then b1 else b2
  b1:
    switch s0 [b2, b3] default b3
  b2:
    s2 = call f1 [s0, s1] -> b3
  b3:
    s2 = host "io.stdout-text" [s1] -> b4 o0
  b4:
    s2 = global g0 -> b5
  b5:
    tailcall *s2 [s3]
  b6:
    trap DivisionByZero o1
  b7:
    return s0
  b8:
    jump b7
fn f1 "closure" params=1 env=true slots=3
  b0:
    return s1
"#;

type Change = fn(&mut Program);

/// One change to one field each; every one must change the text.
fn changes() -> Vec<(&'static str, Change)> {
    fn main(program: &mut Program) -> &mut Function {
        program.functions.first_mut().unwrap()
    }
    fn first_inst(program: &mut Program, index: usize) -> &mut Inst {
        main(program)
            .blocks
            .first_mut()
            .unwrap()
            .insts
            .get_mut(index)
            .unwrap()
    }
    fn term(program: &mut Program, block: usize) -> &mut Terminator {
        &mut main(program).blocks.get_mut(block).unwrap().term
    }
    vec![
        ("entry", |p| p.entry = FuncId(1)),
        ("globals", |p| p.globals.push(FuncId(1))),
        ("global target", |p| p.globals = vec![FuncId(1)]),
        ("constant text", |p| {
            p.constants[0] = Constant::Str("hi".to_owned());
        }),
        ("constant kind", |p| {
            p.constants[0] = Constant::Atom("hi\n".to_owned());
        }),
        ("constant bytes", |p| {
            p.constants[1] = Constant::Bytes(vec![1, 254]);
        }),
        ("constant atom", |p| {
            p.constants[2] = Constant::Atom("no".to_owned());
        }),
        ("constant count", |p| {
            p.constants.pop();
        }),
        ("function name", |p| main(p).name = "main2".to_owned()),
        ("function params", |p| main(p).params = 1),
        ("function env", |p| main(p).env = true),
        ("function slots", |p| main(p).slots = 7),
        ("function count", |p| {
            p.functions.pop();
        }),
        ("block count", |p| {
            main(p).blocks.pop();
        }),
        ("scalar dst", |p| {
            if let Inst::Scalar { dst, .. } = first_inst(p, 0) {
                *dst = Slot(1);
            }
        }),
        ("scalar class", |p| {
            if let Inst::Scalar { class, .. } = first_inst(p, 0) {
                *class = ScalarClass::F64;
            }
        }),
        ("scalar bits", |p| {
            if let Inst::Scalar { bits, .. } = first_inst(p, 0) {
                *bits = 8;
            }
        }),
        ("constant ref", |p| {
            if let Inst::Constant { constant, .. } = first_inst(p, 1) {
                *constant = ConstId(1);
            }
        }),
        ("copy src", |p| {
            if let Inst::Copy { src, .. } = first_inst(p, 2) {
                *src = Slot(1);
            }
        }),
        ("move src", |p| {
            if let Inst::Move { src, .. } = first_inst(p, 3) {
                *src = Slot(1);
            }
        }),
        ("drop slot", |p| {
            if let Inst::Drop { slot } = first_inst(p, 4) {
                *slot = Slot(2);
            }
        }),
        ("op kind", |p| {
            if let Inst::Op { op, .. } = first_inst(p, 5) {
                *op = ScalarOp::I64Sub;
            }
        }),
        ("op args", |p| {
            if let Inst::Op { args, .. } = first_inst(p, 5) {
                args.reverse();
                args.push(Slot(1));
            }
        }),
        ("intrinsic", |p| {
            if let Inst::Intrinsic { intrinsic, .. } = first_inst(p, 6) {
                *intrinsic = CompilerIntrinsic::StrConcat;
            }
        }),
        ("intrinsic origin", |p| {
            if let Inst::Intrinsic { origin, .. } = first_inst(p, 6) {
                *origin = OriginId::NONE;
            }
        }),
        ("build kind", |p| {
            if let Inst::Build { kind, .. } = first_inst(p, 7) {
                *kind = ObjectKind::Union;
            }
        }),
        ("build variant", |p| {
            if let Inst::Build { variant, .. } = first_inst(p, 7) {
                *variant = 2;
            }
        }),
        ("build cells", |p| {
            if let Inst::Build { cells, .. } = first_inst(p, 7) {
                cells.clear();
            }
        }),
        ("cell index", |p| {
            if let Inst::Cell { index, .. } = first_inst(p, 8) {
                *index = 1;
            }
        }),
        ("variant object", |p| {
            if let Inst::Variant { object, .. } = first_inst(p, 9) {
                *object = Slot(5);
            }
        }),
        ("function value target", |p| {
            if let Inst::Function { function, .. } = first_inst(p, 10) {
                *function = FuncId(0);
            }
        }),
        ("function value captures", |p| {
            if let Inst::Function { captures, .. } = first_inst(p, 10) {
                captures.push(Slot(2));
            }
        }),
        ("branch else", |p| {
            if let Terminator::Branch { else_block, .. } = term(p, 0) {
                *else_block = BlockId(3);
            }
        }),
        ("switch targets", |p| {
            if let Terminator::Switch { targets, .. } = term(p, 1) {
                targets.swap(0, 1);
            }
        }),
        ("switch default", |p| {
            if let Terminator::Switch { default, .. } = term(p, 1) {
                *default = BlockId(2);
            }
        }),
        ("call callee", |p| {
            if let Terminator::Call { callee, .. } = term(p, 2) {
                *callee = Callee::Indirect(Slot(1));
            }
        }),
        ("call args", |p| {
            if let Terminator::Call { args, .. } = term(p, 2) {
                args.reverse();
            }
        }),
        ("call dst", |p| {
            if let Terminator::Call { dst, .. } = term(p, 2) {
                *dst = Slot(3);
            }
        }),
        ("call resume", |p| {
            if let Terminator::Call { resume, .. } = term(p, 2) {
                *resume = BlockId(4);
            }
        }),
        ("host row", |p| {
            if let Terminator::Host { row, .. } = term(p, 3) {
                *row = HOST_ROWS.iter().find(|r| r.symbol != row.symbol).unwrap();
            }
        }),
        ("host origin", |p| {
            if let Terminator::Host { origin, .. } = term(p, 3) {
                *origin = OriginId(1);
            }
        }),
        ("global id", |p| {
            if let Terminator::Global { global, .. } = term(p, 4) {
                *global = GlobalId(1);
            }
        }),
        ("tail call callee", |p| {
            if let Terminator::TailCall { callee, .. } = term(p, 5) {
                *callee = Callee::Direct(FuncId(2));
            }
        }),
        ("trap code", |p| {
            if let Terminator::Trap { code, .. } = term(p, 6) {
                *code = TrapCode::InvalidHostValue;
            }
        }),
        ("trap origin", |p| {
            if let Terminator::Trap { origin, .. } = term(p, 6) {
                *origin = OriginId::NONE;
            }
        }),
        ("return value", |p| {
            if let Terminator::Return { value } = term(p, 7) {
                *value = Slot(1);
            }
        }),
        ("jump target", |p| {
            if let Terminator::Jump(target) = term(p, 8) {
                *target = BlockId(0);
            }
        }),
    ]
}

#[test]
fn text_form_is_deterministic() {
    let program = everything();
    let text = program.to_string();
    assert_eq!(text, EXPECTED);

    // The same program built again, and a clone, print the same text.
    assert_eq!(everything().to_string(), text);
    assert_eq!(program.clone().to_string(), text);
    assert_eq!(format!("{program}"), text);

    // Changing any one field changes the text.
    for (name, change) in changes() {
        let mut changed = everything();
        change(&mut changed);
        assert_ne!(changed, program, "the change `{name}` changed nothing");
        assert_ne!(
            changed.to_string(),
            text,
            "the change `{name}` left the text as it was"
        );
    }
}
