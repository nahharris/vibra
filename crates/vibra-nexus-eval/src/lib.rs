//! The reference stepper: verified execution IR, executed on Nexus by a loop
//! instead of by generated code.
//!
//! `docs/roadmap/nexus/decisions.md`, N6, records why this exists. Native
//! execution has two places to be wrong: the lowering from checked IR to
//! execution IR, and the generation of machine code from execution IR. The
//! stepper separates them. It runs the same execution IR, on the same
//! runtime, through the same transfers of [`Context`], on any platform and
//! with no linker, so:
//!
//! - a lowering is compared with the reference interpreter before any code
//!   generator exists; and
//! - a code generator is compared with the stepper one instruction at a time,
//!   and the two must agree on every observation.
//!
//! It is an implementation of [`Code`], exactly as generated code is: one
//! [`Code::step`] runs the top frame from its resume block to its next
//! checkpoint. It is a test instrument of the toolchain and no command of the
//! CLI reaches it.

use vibra_exec::{
    BlockId, Callee, Constant, FuncId, Function, Inst, IntOp as ExecInt, ObjectKind,
    Program, ScalarClass, ScalarOp as ExecOp, Slot, Terminator, VerifyError, verify,
};
use vibra_ir::arena::{class, kind};
use vibra_ir::boundary::TrapCode;
use vibra_nexus::{
    ABI_VERSION, Code, Context, Halt, IntOp, Requirements, ScalarOp, Step,
};

/// The runtime operation that executes an execution IR scalar operation: the
/// stepper's one mapping, with no wildcard, so a new `vibra_exec::ScalarOp` does
/// not compile until it is mapped. `vibra-conformance` holds it to the name
/// the execution IR gives the operation, and the generator's tests hold the
/// bridge's decoding of the generator's encoding to the same name.
#[must_use]
pub const fn runtime_op(op: ExecOp) -> ScalarOp {
    match op {
        ExecOp::I64Add => ScalarOp::I64Add,
        ExecOp::I64Sub => ScalarOp::I64Sub,
        ExecOp::I64Eq => ScalarOp::I64Eq,
        ExecOp::I64LtS => ScalarOp::I64LtS,
        ExecOp::Int(op, numeric) => ScalarOp::Int(
            match op {
                ExecInt::Add => IntOp::Add,
                ExecInt::Sub => IntOp::Sub,
                ExecInt::Mul => IntOp::Mul,
                ExecInt::Div => IntOp::Div,
                ExecInt::Rem => IntOp::Rem,
                ExecInt::ShiftLeft => IntOp::ShiftLeft,
                ExecInt::ShiftRight => IntOp::ShiftRight,
                ExecInt::Equal => IntOp::Equal,
                ExecInt::Less => IntOp::Less,
            },
            numeric,
        ),
    }
}

/// Why a program cannot be executed by the stepper.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvalError {
    /// The program is not valid execution IR.
    Invalid(VerifyError),
    /// The program uses an instruction the stepper does not execute yet. It
    /// is rejected whole: the stepper never runs part of a program.
    Unsupported(&'static str),
}

impl From<VerifyError> for EvalError {
    fn from(error: VerifyError) -> Self {
        Self::Invalid(error)
    }
}

/// A verified program, ready to step.
#[derive(Clone, Copy, Debug)]
pub struct Evaluator<'p> {
    program: &'p Program,
}

type Done<T> = Result<T, Halt>;

/// What a block's terminator decided.
enum Next {
    /// Continue at another block of the same frame, within this step.
    Block(BlockId),
    /// A checkpoint was reached.
    Checkpoint(Step),
}

impl<'p> Evaluator<'p> {
    /// Verifies `program` and prepares to execute it.
    ///
    /// # Errors
    ///
    /// [`EvalError`] for a program that is invalid or that the stepper cannot
    /// execute in full.
    pub fn new(program: &'p Program) -> Result<Self, EvalError> {
        verify(program)?;
        let uses_intrinsic = program
            .functions
            .iter()
            .flat_map(|function| &function.blocks)
            .flat_map(|block| &block.insts)
            .any(|inst| matches!(inst, Inst::Intrinsic { .. }));
        if uses_intrinsic {
            return Err(EvalError::Unsupported("intrinsic"));
        }
        Ok(Self { program })
    }

    /// What the program requires of a runtime, for [`vibra_nexus::admit`].
    #[must_use]
    pub fn requirements(&self) -> Requirements<'static> {
        Requirements {
            abi: ABI_VERSION,
            registry_minor: self.program.registry_minor(),
            host_symbols: self.program.host_symbols().into_iter().collect(),
        }
    }

    /// The number of module values a context for this program holds.
    #[must_use]
    pub const fn globals(&self) -> usize {
        self.program.globals.len()
    }

    /// Pushes the entry frame.
    ///
    /// # Errors
    ///
    /// The host event when the context's limit does not admit the frame.
    pub fn start(&self, ctx: &mut Context<'_>) -> Result<(), Halt> {
        let entry = self.program.entry;
        let slots = self.program.function(entry).map_or(0, |entry| entry.slots);
        ctx.enter(entry.0, slots)
    }

    fn function(&self, ctx: &mut Context<'_>, id: FuncId) -> Done<&'p Function> {
        self.program
            .function(id)
            .ok_or_else(|| ctx.trap(TrapCode::InvalidCheckedProgram, 0))
    }

    fn run(&self, ctx: &mut Context<'_>) -> Done<Step> {
        let function = FuncId(ctx.function()?);
        let function = self.function(ctx, function)?;
        let mut block = BlockId(ctx.resume()?);
        loop {
            let Some(body) = function.blocks.get(block.index()) else {
                return Err(ctx.trap(TrapCode::InvalidCheckedProgram, 0));
            };
            for inst in &body.insts {
                self.inst(ctx, inst)?;
            }
            match self.terminator(ctx, &body.term)? {
                Next::Block(next) => block = next,
                Next::Checkpoint(step) => return Ok(step),
            }
        }
    }

    fn inst(&self, ctx: &mut Context<'_>, inst: &Inst) -> Done<()> {
        match inst {
            Inst::Scalar { dst, class, bits } => {
                let class = match class {
                    ScalarClass::I32 => class::I32,
                    ScalarClass::I64 => class::I64,
                    ScalarClass::F32 => class::F32,
                    ScalarClass::F64 => class::F64,
                };
                ctx.scalar(dst.0, class, *bits)
            }
            Inst::Constant { dst, constant } => {
                match self.program.constants.get(constant.index()) {
                    Some(Constant::Str(text)) => {
                        ctx.constant_text(dst.0, kind::STR, text)
                    }
                    Some(Constant::Atom(text)) => {
                        ctx.constant_text(dst.0, kind::ATOM, text)
                    }
                    Some(Constant::Bytes(bytes)) => ctx.constant_bytes(dst.0, bytes),
                    None => Err(ctx.trap(TrapCode::InvalidCheckedProgram, 0)),
                }
            }
            Inst::Copy { dst, src } => ctx.copy(dst.0, src.0),
            Inst::Move { dst, src } => ctx.move_slot(dst.0, src.0),
            Inst::Drop { slot } => ctx.drop_slot(slot.0),
            Inst::Op { dst, op, args } => {
                let op = runtime_op(*op);
                let (Some(left), Some(right)) = (args.first(), args.get(1)) else {
                    return Err(ctx.trap(TrapCode::InvalidCheckedProgram, 0));
                };
                ctx.scalar_op(dst.0, op, left.0, right.0)
            }
            // `Evaluator::new` rejects a program that has one.
            Inst::Intrinsic { .. } => Err(ctx.trap(TrapCode::InvalidCheckedProgram, 0)),
            Inst::Build {
                dst,
                kind,
                variant,
                cells,
            } => {
                let kind = match kind {
                    ObjectKind::Tuple => kind::TUPLE,
                    ObjectKind::Array => kind::ARRAY,
                    ObjectKind::Record => kind::RECORD,
                    ObjectKind::Enum => kind::ENUM,
                    ObjectKind::Wrapper => kind::WRAPPER,
                    ObjectKind::Union => kind::UNION,
                };
                ctx.build(dst.0, kind, *variant, &slots(cells))
            }
            Inst::Cell { dst, object, index } => ctx.cell(dst.0, object.0, *index),
            Inst::Variant { dst, object } => ctx.variant_of(dst.0, object.0),
            Inst::Function {
                dst,
                function,
                captures,
            } => {
                let body = self.function(ctx, *function)?;
                ctx.make_function(
                    dst.0,
                    function.0,
                    body.slots,
                    body.params,
                    &slots(captures),
                )
            }
        }
    }
    fn terminator(&self, ctx: &mut Context<'_>, term: &Terminator) -> Done<Next> {
        Ok(match term {
            Terminator::Jump(target) => Next::Block(*target),
            Terminator::Branch {
                cond,
                then_block,
                else_block,
            } => {
                Next::Block(BlockId(ctx.branch(cond.0, then_block.0, else_block.0)?))
            }
            Terminator::Switch {
                value,
                targets,
                default,
            } => {
                let targets = targets.iter().map(|block| block.0).collect::<Vec<_>>();
                Next::Block(BlockId(ctx.switch(value.0, &targets, default.0)?))
            }
            Terminator::Call {
                callee,
                args,
                dst,
                resume,
            } => {
                let args = slots(args);
                match callee {
                    Callee::Direct(id) => {
                        let frame = self.function(ctx, *id)?.slots;
                        ctx.call(id.0, frame, &args, dst.0, resume.0)?;
                    }
                    Callee::Indirect(slot) => {
                        ctx.call_value(slot.0, &args, dst.0, resume.0)?;
                    }
                }
                Next::Checkpoint(Step::Continue)
            }
            Terminator::TailCall { callee, args } => {
                let args = slots(args);
                match callee {
                    Callee::Direct(id) => {
                        let frame = self.function(ctx, *id)?.slots;
                        ctx.tail_call(id.0, frame, &args)?;
                    }
                    Callee::Indirect(slot) => ctx.tail_call_value(slot.0, &args)?,
                }
                Next::Checkpoint(Step::Continue)
            }
            Terminator::Host {
                row,
                args,
                dst,
                resume,
                origin,
            } => {
                ctx.host(row, &slots(args), dst.0, resume.0, origin.0)?;
                Next::Checkpoint(Step::Continue)
            }
            Terminator::Global {
                global,
                dst,
                resume,
            } => {
                let Some(initializer) = self.program.globals.get(global.index()) else {
                    return Err(ctx.trap(TrapCode::InvalidCheckedProgram, 0));
                };
                let frame = self.function(ctx, *initializer)?.slots;
                if ctx.global(global.0, initializer.0, frame, dst.0, resume.0)? {
                    Next::Checkpoint(Step::Continue)
                } else {
                    Next::Block(*resume)
                }
            }
            Terminator::Return { value } => Next::Checkpoint(if ctx.ret(value.0)? {
                Step::Done
            } else {
                Step::Continue
            }),
            Terminator::Trap { code, origin } => return Err(ctx.trap(*code, origin.0)),
        })
    }
}

fn slots(slots: &[Slot]) -> Vec<u32> {
    slots.iter().map(|slot| slot.0).collect()
}

impl Code for Evaluator<'_> {
    fn step(&self, ctx: &mut Context<'_>) -> Step {
        match self.run(ctx) {
            Ok(step) => step,
            Err(Halt::Trap) => Step::Trap,
            Err(Halt::Exhausted) => Step::Exhausted,
        }
    }
}
