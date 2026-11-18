//! The translation of one execution-IR function into one step function.
//!
//! The step function is `(ctx, resume) -> status` (the bridge's `VibraStep`):
//! a switch on `resume` over the function's blocks, one `nexus_*` routine call
//! for each instruction, a status check after every call, and the terminators
//! the bridge documents. Nothing is cached in a register across a routine call
//! and the context pointer is only ever passed back to a routine.

use std::collections::BTreeMap;

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{
    self, Block as Label, BlockArg, FuncRef, InstBuilder, JumpTableData, Type, Value,
    types,
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::Module;
use vibra_exec::{
    Block, BlockId, Callee, Function, Inst, IntOp, ObjectKind, ScalarClass, ScalarOp,
    Slot, Terminator,
};
use vibra_ir::boundary::TrapCode;
use vibra_ir::external::NumericType;
use vibra_nexus_bridge::{
    CLASS_F32, CLASS_F64, CLASS_I32, CLASS_I64, INT_TYPE_I8, INT_TYPE_I16,
    INT_TYPE_I32, INT_TYPE_I64, INT_TYPE_U8, INT_TYPE_U16, INT_TYPE_U32, INT_TYPE_U64,
    KIND_ARRAY, KIND_ENUM, KIND_RECORD, KIND_TUPLE, KIND_UNION, KIND_WRAPPER,
    MAX_SWITCH_TARGETS, OP_I64_ADD, OP_I64_EQ, OP_I64_LT_S, OP_I64_SUB, OP_INT_ADD,
    OP_INT_DIV, OP_INT_EQUAL, OP_INT_LESS, OP_INT_MUL, OP_INT_REM, OP_INT_SHIFT_LEFT,
    OP_INT_SHIFT_RIGHT, OP_INT_SUB, STATUS_PROCEED, int_op_code,
};

use crate::error::EmitError;
use crate::services::Services;

/// The `nexus_op` code of `op`: the generator's one mapping, with no wildcard.
/// The generator's tests hold it to the bridge's decoding, and `vibra-conformance`
/// holds the stepper's mapping to the same names. A floating-point type has no integer operation, so
/// its code names no operation (the verifier refuses the program first).
#[must_use]
pub const fn op_code(op: ScalarOp) -> u32 {
    match op {
        ScalarOp::I64Add => OP_I64_ADD,
        ScalarOp::I64Sub => OP_I64_SUB,
        ScalarOp::I64Eq => OP_I64_EQ,
        ScalarOp::I64LtS => OP_I64_LT_S,
        ScalarOp::Int(operation, numeric) => int_op_code(
            match operation {
                IntOp::Add => OP_INT_ADD,
                IntOp::Sub => OP_INT_SUB,
                IntOp::Mul => OP_INT_MUL,
                IntOp::Div => OP_INT_DIV,
                IntOp::Rem => OP_INT_REM,
                IntOp::ShiftLeft => OP_INT_SHIFT_LEFT,
                IntOp::ShiftRight => OP_INT_SHIFT_RIGHT,
                IntOp::Equal => OP_INT_EQUAL,
                IntOp::Less => OP_INT_LESS,
            },
            match numeric {
                NumericType::I8 => INT_TYPE_I8,
                NumericType::I16 => INT_TYPE_I16,
                NumericType::I32 => INT_TYPE_I32,
                NumericType::I64 => INT_TYPE_I64,
                NumericType::U8 => INT_TYPE_U8,
                NumericType::U16 => INT_TYPE_U16,
                NumericType::U32 => INT_TYPE_U32,
                NumericType::U64 => INT_TYPE_U64,
                // Unreachable for a verified program.
                NumericType::F32 | NumericType::F64 => u32::MAX,
            },
        ),
    }
}

const NO_ARGS: [BlockArg; 0] = [];

/// The routines a function called, by the id of their declaration, for the
/// tests that read the Cranelift IR.
pub(crate) type Callees = BTreeMap<u32, &'static str>;

/// Translates `function` into `func`, whose signature is the step signature.
/// `symbols` is the program's required host symbols, in first-use order.
pub(crate) fn translate(
    services: &mut Services<'_>,
    symbols: &[&'static str],
    function: &Function,
    func: &mut ir::Function,
    builder_context: &mut FunctionBuilderContext,
    callees: &mut Callees,
) -> Result<(), EmitError> {
    let frontend = services.module.isa().frontend_config();
    let pointer = services.module.isa().pointer_type();
    let mut builder = FunctionBuilder::new(func, builder_context);

    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    let (ctx, resume) = match builder.block_params(entry) {
        [ctx, resume] => (*ctx, *resume),
        _ => {
            return Err(EmitError::Contract(
                "the step signature does not take a context and a block".to_owned(),
            ));
        }
    };
    let dispatch = builder.create_block();
    let target = builder.append_block_param(dispatch, types::I32);
    let exit = builder.create_block();
    let status = builder.append_block_param(exit, types::I32);
    let invalid = builder.create_block();
    let blocks = function
        .blocks
        .iter()
        .map(|_| builder.create_block())
        .collect::<Vec<_>>();

    let mut translator = Translator {
        builder,
        services,
        symbols,
        function,
        pointer,
        ctx,
        dispatch,
        exit,
        blocks,
        refs: BTreeMap::new(),
        callees,
    };

    // The entry hands `resume` to the one switch of the function.
    translator.builder.switch_to_block(entry);
    translator
        .builder
        .ins()
        .jump(dispatch, &[BlockArg::Value(resume)]);

    // The switch on the block. A block past the table is a defect of the
    // caller (the dispatcher refuses it first): record the invalid-program
    // trap and return its status.
    translator.builder.switch_to_block(dispatch);
    let default = translator.builder.func.dfg.block_call(invalid, &NO_ARGS);
    let table = translator
        .blocks
        .iter()
        .map(|label| translator.builder.func.dfg.block_call(*label, &NO_ARGS))
        .collect::<Vec<_>>();
    let jump_table = translator
        .builder
        .create_jump_table(JumpTableData::new(default, &table));
    translator.builder.ins().br_table(target, jump_table);

    translator.builder.switch_to_block(invalid);
    let code = u32::try_from(TrapCode::InvalidCheckedProgram.code())
        .map_err(|_| EmitError::TooLarge("a trap code"))?;
    let code = translator.imm32(code);
    let origin = translator.imm32(0);
    let trapped = translator.call("nexus_trap", &[ctx, code, origin])?;
    translator.builder.ins().return_(&[trapped]);

    translator.builder.switch_to_block(exit);
    translator.builder.ins().return_(&[status]);

    for (index, block) in function.blocks.iter().enumerate() {
        translator.block(index, block)?;
    }
    translator.builder.seal_all_blocks();
    translator.builder.finalize(frontend);
    Ok(())
}

struct Translator<'a, 'm, 'f> {
    builder: FunctionBuilder<'f>,
    services: &'a mut Services<'m>,
    symbols: &'a [&'static str],
    function: &'a Function,
    pointer: Type,
    ctx: Value,
    dispatch: Label,
    exit: Label,
    blocks: Vec<Label>,
    refs: BTreeMap<&'static str, FuncRef>,
    callees: &'a mut Callees,
}

/// A `u32` as the sign-extended immediate Cranelift expects for an `i32`.
fn as_i32(value: u32) -> i64 {
    i64::from(i32::from_ne_bytes(value.to_ne_bytes()))
}

fn count(len: usize) -> Result<u32, EmitError> {
    u32::try_from(len).map_err(|_| EmitError::TooLarge("an operand list"))
}

fn slots(list: &[Slot]) -> Vec<u32> {
    list.iter().map(|slot| slot.0).collect()
}

impl Translator<'_, '_, '_> {
    fn imm32(&mut self, value: u32) -> Value {
        self.builder.ins().iconst(types::I32, as_i32(value))
    }

    fn imm64(&mut self, value: u64) -> Value {
        let bits = i64::from_ne_bytes(value.to_ne_bytes());
        self.builder.ins().iconst(types::I64, bits)
    }

    fn label(&self, block: BlockId) -> Result<Label, EmitError> {
        self.blocks.get(block.index()).copied().ok_or_else(|| {
            EmitError::Contract(format!("block {} does not exist", block.0))
        })
    }

    /// The pointer and the count of an operand list in read-only data, or a
    /// null pointer and zero for the empty list.
    fn list(&mut self, values: &[u32]) -> Result<(Value, Value), EmitError> {
        let length = count(values.len())?;
        let length = self.imm32(length);
        if values.is_empty() {
            let null = self.builder.ins().iconst(self.pointer, 0);
            return Ok((null, length));
        }
        let id = self.services.lists.get(self.services.module, values)?;
        let global = self
            .services
            .module
            .declare_data_in_func(id, self.builder.func);
        let address = self.builder.ins().symbol_value(self.pointer, global);
        Ok((address, length))
    }

    fn routine(&mut self, name: &'static str) -> Result<(FuncRef, usize), EmitError> {
        let (id, arity) = self.services.imports.get(self.services.module, name)?;
        if let Some(known) = self.refs.get(name) {
            return Ok((*known, arity));
        }
        let reference = self
            .services
            .module
            .declare_func_in_func(id, self.builder.func);
        self.callees.insert(id.as_u32(), name);
        self.refs.insert(name, reference);
        Ok((reference, arity))
    }

    /// One call of a routine; its first result.
    fn call(
        &mut self,
        name: &'static str,
        arguments: &[Value],
    ) -> Result<Value, EmitError> {
        let (reference, arity) = self.routine(name)?;
        if arity != arguments.len() {
            return Err(EmitError::Contract(format!(
                "`{name}` takes {arity} arguments, the generator passes {}",
                arguments.len()
            )));
        }
        let call = self.builder.ins().call(reference, arguments);
        self.builder
            .inst_results(call)
            .first()
            .copied()
            .ok_or_else(|| EmitError::Contract(format!("`{name}` answers nothing")))
    }

    /// A call whose status ends the step unless it is `STATUS_CONTINUE` (zero).
    fn checked(
        &mut self,
        name: &'static str,
        arguments: &[Value],
    ) -> Result<(), EmitError> {
        let status = self.call(name, arguments)?;
        let next = self.builder.create_block();
        self.builder.ins().brif(
            status,
            self.exit,
            &[BlockArg::Value(status)],
            next,
            &NO_ARGS,
        );
        self.builder.switch_to_block(next);
        Ok(())
    }

    /// A call whose status is the step's answer.
    fn finishing(
        &mut self,
        name: &'static str,
        arguments: &[Value],
    ) -> Result<(), EmitError> {
        let status = self.call(name, arguments)?;
        self.builder.ins().return_(&[status]);
        Ok(())
    }

    fn block(&mut self, index: usize, block: &Block) -> Result<(), EmitError> {
        let label = self.label(BlockId(count(index)?))?;
        self.builder.switch_to_block(label);
        for inst in &block.insts {
            self.inst(inst)?;
        }
        self.terminator(&block.term)
    }

    fn inst(&mut self, inst: &Inst) -> Result<(), EmitError> {
        let ctx = self.ctx;
        match inst {
            Inst::Scalar { dst, class, bits } => {
                let class = match class {
                    ScalarClass::I32 => CLASS_I32,
                    ScalarClass::I64 => CLASS_I64,
                    ScalarClass::F32 => CLASS_F32,
                    ScalarClass::F64 => CLASS_F64,
                };
                let (dst, class) = (self.imm32(dst.0), self.imm32(class));
                let bits = self.imm64(*bits);
                self.checked("nexus_scalar", &[ctx, dst, class, bits])
            }
            Inst::Constant { dst, constant } => {
                let (dst, constant) = (self.imm32(dst.0), self.imm32(constant.0));
                self.checked("nexus_constant", &[ctx, dst, constant])
            }
            Inst::Copy { dst, src } => {
                let (dst, src) = (self.imm32(dst.0), self.imm32(src.0));
                self.checked("nexus_copy", &[ctx, dst, src])
            }
            Inst::Move { dst, src } => {
                let (dst, src) = (self.imm32(dst.0), self.imm32(src.0));
                self.checked("nexus_move", &[ctx, dst, src])
            }
            Inst::Drop { slot } => {
                let slot = self.imm32(slot.0);
                self.checked("nexus_drop", &[ctx, slot])
            }
            Inst::Op { dst, op, args } => {
                let op = op_code(*op);
                let [left, right] = args.as_slice() else {
                    return Err(EmitError::Contract(
                        "a scalar operation without two operands passed verification"
                            .to_owned(),
                    ));
                };
                let dst = self.imm32(dst.0);
                let op = self.imm32(op);
                let (left, right) = (self.imm32(left.0), self.imm32(right.0));
                self.checked("nexus_op", &[ctx, dst, op, left, right])
            }
            Inst::Intrinsic { .. } => Err(EmitError::Unsupported("an intrinsic")),
            Inst::Build {
                dst,
                kind,
                variant,
                cells,
            } => {
                let kind = match kind {
                    ObjectKind::Tuple => KIND_TUPLE,
                    ObjectKind::Array => KIND_ARRAY,
                    ObjectKind::Record => KIND_RECORD,
                    ObjectKind::Enum => KIND_ENUM,
                    ObjectKind::Wrapper => KIND_WRAPPER,
                    ObjectKind::Union => KIND_UNION,
                };
                let (cells, length) = self.list(&slots(cells))?;
                let (dst, kind, variant) =
                    (self.imm32(dst.0), self.imm32(kind), self.imm32(*variant));
                self.checked("nexus_build", &[ctx, dst, kind, variant, cells, length])
            }
            Inst::Cell { dst, object, index } => {
                let (dst, object) = (self.imm32(dst.0), self.imm32(object.0));
                let index = self.imm32(*index);
                self.checked("nexus_cell", &[ctx, dst, object, index])
            }
            Inst::Variant { dst, object } => {
                let (dst, object) = (self.imm32(dst.0), self.imm32(object.0));
                self.checked("nexus_variant", &[ctx, dst, object])
            }
            Inst::Function {
                dst,
                function,
                captures,
            } => {
                let (captures, length) = self.list(&slots(captures))?;
                let (dst, function) = (self.imm32(dst.0), self.imm32(function.0));
                self.checked("nexus_function", &[ctx, dst, function, captures, length])
            }
        }
    }

    /// The step answers with the status of `nexus_branch` or `nexus_switch`
    /// unless it is `STATUS_CONTINUE`; otherwise it continues at the block the
    /// routine answered, through the function's one switch.
    fn selected(&mut self, selector: Value) -> Result<(), EmitError> {
        let high = self.builder.ins().ushr_imm_u(selector, 32);
        let status = self.builder.ins().ireduce(types::I32, high);
        let block = self.builder.ins().ireduce(types::I32, selector);
        let next = self.builder.create_block();
        self.builder.ins().brif(
            status,
            self.exit,
            &[BlockArg::Value(status)],
            next,
            &NO_ARGS,
        );
        self.builder.switch_to_block(next);
        self.builder
            .ins()
            .jump(self.dispatch, &[BlockArg::Value(block)]);
        Ok(())
    }

    fn terminator(&mut self, term: &Terminator) -> Result<(), EmitError> {
        let ctx = self.ctx;
        match term {
            Terminator::Jump(target) => {
                let target = self.label(*target)?;
                self.builder.ins().jump(target, &NO_ARGS);
                Ok(())
            }
            Terminator::Branch {
                cond,
                then_block,
                else_block,
            } => {
                let cond = self.imm32(cond.0);
                let (then_block, else_block) =
                    (self.imm32(then_block.0), self.imm32(else_block.0));
                let selector =
                    self.call("nexus_branch", &[ctx, cond, then_block, else_block])?;
                self.selected(selector)
            }
            Terminator::Switch {
                value,
                targets,
                default,
            } => {
                if u32::try_from(targets.len())
                    .map_or(true, |length| length > MAX_SWITCH_TARGETS)
                {
                    return Err(EmitError::TooLarge("a switch"));
                }
                let blocks = targets.iter().map(|block| block.0).collect::<Vec<_>>();
                let (targets, length) = self.list(&blocks)?;
                let value = self.imm32(value.0);
                let default = self.imm32(default.0);
                let selector =
                    self.call("nexus_switch", &[ctx, value, targets, length, default])?;
                self.selected(selector)
            }
            Terminator::Call {
                callee,
                args,
                dst,
                resume,
            } => {
                let (list, length) = self.list(&slots(args))?;
                let (dst, resume) = (self.imm32(dst.0), self.imm32(resume.0));
                match callee {
                    Callee::Direct(function) => {
                        let function = self.imm32(function.0);
                        self.finishing(
                            "nexus_call",
                            &[ctx, function, list, length, dst, resume],
                        )
                    }
                    Callee::Indirect(slot) => {
                        let slot = self.imm32(slot.0);
                        self.finishing(
                            "nexus_call_value",
                            &[ctx, slot, list, length, dst, resume],
                        )
                    }
                }
            }
            Terminator::TailCall { callee, args } => {
                let (list, length) = self.list(&slots(args))?;
                match callee {
                    Callee::Direct(function) => {
                        let function = self.imm32(function.0);
                        self.finishing(
                            "nexus_tail_call",
                            &[ctx, function, list, length],
                        )
                    }
                    Callee::Indirect(slot) => {
                        let slot = self.imm32(slot.0);
                        self.finishing(
                            "nexus_tail_call_value",
                            &[ctx, slot, list, length],
                        )
                    }
                }
            }
            Terminator::Host {
                row,
                args,
                dst,
                resume,
                origin,
            } => {
                let operands = slots(args);
                if usize::try_from(self.function.slots)
                    .map_or(true, |frame| operands.len() > frame)
                {
                    // The runtime bounds an operand list by the frame. The
                    // stepper does not, and verification allows a borrowed
                    // slot to repeat, so such a list has no native form.
                    return Err(EmitError::Unsupported(
                        "a host call with more operands than the frame has slots",
                    ));
                }
                let symbol = self
                    .symbols
                    .iter()
                    .position(|symbol| *symbol == row.symbol)
                    .and_then(|position| u32::try_from(position).ok())
                    .ok_or_else(|| {
                        EmitError::Contract(format!(
                            "the host symbol `{}` has no table entry",
                            row.symbol
                        ))
                    })?;
                let (list, length) = self.list(&operands)?;
                let symbol = self.imm32(symbol);
                let (dst, resume, origin) = (
                    self.imm32(dst.0),
                    self.imm32(resume.0),
                    self.imm32(origin.0),
                );
                self.finishing(
                    "nexus_host",
                    &[ctx, symbol, list, length, dst, resume, origin],
                )
            }
            Terminator::Global {
                global,
                dst,
                resume,
            } => {
                let continuation = self.label(*resume)?;
                let (global, dst, resume) = (
                    self.imm32(global.0),
                    self.imm32(dst.0),
                    self.imm32(resume.0),
                );
                let status = self.call("nexus_global", &[ctx, global, dst, resume])?;
                // `STATUS_PROCEED`: the value is ready, carry on at `resume`.
                // Anything else, `STATUS_CONTINUE` included, is the step's
                // answer: the initializer's frame was pushed, or a trap.
                let ready = self.builder.ins().icmp_imm_u(
                    IntCC::Equal,
                    status,
                    i64::from(STATUS_PROCEED),
                );
                self.builder.ins().brif(
                    ready,
                    continuation,
                    &NO_ARGS,
                    self.exit,
                    &[BlockArg::Value(status)],
                );
                Ok(())
            }
            Terminator::Return { value } => {
                let value = self.imm32(value.0);
                self.finishing("nexus_ret", &[ctx, value])
            }
            Terminator::Trap { code, origin } => {
                let code = u32::try_from(code.code())
                    .map_err(|_| EmitError::TooLarge("a trap code"))?;
                let (code, origin) = (self.imm32(code), self.imm32(origin.0));
                self.finishing("nexus_trap", &[ctx, code, origin])
            }
        }
    }
}
