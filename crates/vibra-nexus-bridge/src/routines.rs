//! The `nexus_*` routines: the whole surface generated code calls.
//!
//! Each routine converts its arguments and calls the [`Context`] method the
//! reference stepper calls for the same transfer or instruction, so the
//! stepper's evidence transfers to generated code. Generated code names slots
//! by index; no pointer into the value store or the cell stack crosses.
//!
//! # Contract of every routine
//!
//! - `ctx` is the handle the dispatcher passed to the running step function,
//!   used on that thread, during that call. A null `ctx` is answered with
//!   [`STATUS_TRAP`].
//! - An operand list is `(args, count)`: `args` addresses `count` readable,
//!   aligned `u32` slot indices, or may be null when `count` is zero. `count`
//!   may not exceed the frame of the running function.
//! - A block (`resume`, or the block a branch or switch answers) at or past
//!   the running function's `blocks` is a defect, like the following.
//! - A slot, function, constant, symbol, or origin outside its range is a
//!   defect of the toolchain: the routine records
//!   `@runtime.invalid-checked-program` and answers [`STATUS_TRAP`]. The
//!   verifier rules such calls out; the routines are nevertheless memory-safe
//!   when the rule is broken.
//! - No routine panics or unwinds. A routine answers `STATUS_CONTINUE` when it
//!   completed, [`STATUS_TRAP`] when a trap was recorded, and
//!   [`STATUS_EXHAUSTED`] at the host event. Exceptions are documented on the
//!   routine.

use vibra_ir::arena::kind;
use vibra_ir::boundary::TrapCode;
use vibra_nexus::{Context, Halt, ScalarOp};

use crate::abi::{
    CLASS_F32, CLASS_F64, CLASS_I32, CLASS_I64, KIND_ARRAY, KIND_ENUM, KIND_RECORD,
    KIND_TUPLE, KIND_UNION, KIND_WRAPPER, NexusContext, OP_I64_ADD, OP_I64_EQ,
    OP_I64_LT_S, OP_I64_SUB, STATUS_CONTINUE, STATUS_DONE, STATUS_EXHAUSTED,
    STATUS_PROCEED, STATUS_TRAP,
};
use crate::admission::{Admitted, ConstantData, FunctionInfo};

type Done<T> = Result<T, Halt>;

/// The most targets a `nexus_switch` may name.
pub const MAX_SWITCH_TARGETS: u32 = 1 << 20;

/// What a routine can reach: the context and the admitted program. It lives on
/// the dispatcher's stack for one step, and `*mut NexusContext` is its address.
pub(crate) struct Session<'a, 'h> {
    pub(crate) ctx: &'a mut Context<'h>,
    pub(crate) code: &'a Admitted,
}

const fn halt_status(halt: Halt) -> u32 {
    match halt {
        Halt::Trap => STATUS_TRAP,
        Halt::Exhausted => STATUS_EXHAUSTED,
    }
}

impl Session<'_, '_> {
    fn defect(&mut self) -> Halt {
        self.ctx.trap(TrapCode::InvalidCheckedProgram, 0)
    }

    fn function(&mut self, function: u32) -> Done<FunctionInfo> {
        let index = usize::try_from(function).unwrap_or(usize::MAX);
        match self.code.functions.get(index) {
            Some(info) => Ok(*info),
            None => Err(self.defect()),
        }
    }

    /// A function a direct transfer may name: no environment, and exactly
    /// `count` parameters.
    fn direct(&mut self, function: u32, count: usize) -> Done<FunctionInfo> {
        let info = self.function(function)?;
        if info.env || usize::try_from(info.params).ok() != Some(count) {
            return Err(self.defect());
        }
        Ok(info)
    }

    /// The running function, whose blocks bound every block a routine names.
    fn running(&mut self) -> Done<FunctionInfo> {
        let running = self.ctx.function()?;
        self.function(running)
    }

    /// A block of the running function.
    fn block(&mut self, block: u32) -> Done<u32> {
        if block < self.running()?.blocks {
            Ok(block)
        } else {
            Err(self.defect())
        }
    }

    fn origin(&mut self, origin: u32) -> Done<u32> {
        let known = usize::try_from(origin)
            .is_ok_and(|ordinal| ordinal <= self.code.origins.len());
        if known {
            Ok(origin)
        } else {
            Err(self.defect())
        }
    }

    /// The operand list `(args, count)`, bounded by the running frame.
    fn operands<'l>(&mut self, args: *const u32, count: u32) -> Done<&'l [u32]> {
        if count == 0 {
            return Ok(&[]);
        }
        let frame = self.running()?.slots;
        self.list(args, count, frame)
    }

    /// The list `(args, count)` of at most `bound` elements.
    fn list<'l>(
        &mut self,
        args: *const u32,
        count: u32,
        bound: u32,
    ) -> Done<&'l [u32]> {
        if count == 0 {
            return Ok(&[]);
        }
        if count > bound || args.is_null() || !args.is_aligned() {
            return Err(self.defect());
        }
        let Ok(len) = usize::try_from(count) else {
            return Err(self.defect());
        };
        // SAFETY: the pointer is non-null and aligned, and `count` is at most
        // `bound`, which is the frame size (admission bounded it by
        // `MAX_FRAME_SLOTS`) or `MAX_SWITCH_TARGETS`, so the byte size is far
        // below `isize::MAX`. That `args` addresses `count`
        // readable `u32` is the routine's documented precondition, which the
        // code generator meets by emitting the list in the step's own frame or
        // in read-only data.
        Ok(unsafe { std::slice::from_raw_parts(args, len) })
    }
}

/// Turns `ctx` into the session and runs `body` on it; answers the status.
fn routine(
    ctx: *mut NexusContext,
    body: impl FnOnce(&mut Session<'_, '_>) -> Done<u32>,
) -> u32 {
    if ctx.is_null() {
        return STATUS_TRAP;
    }
    // SAFETY: every routine is `unsafe` with the precondition that `ctx` is
    // the handle the dispatcher passed to the running step function. The
    // dispatcher made it from `&mut Session` and passes it to no one else, so
    // the cast recovers the same type, and the session is exclusively ours
    // until the step function returns.
    let session = unsafe { &mut *ctx.cast::<Session<'_, '_>>() };
    match body(session) {
        Ok(status) => status,
        Err(halt) => halt_status(halt),
    }
}

/// As [`routine`], for the routines that answer a block and a status together:
/// the low 32 bits are the block, the high 32 the status.
fn selector(
    ctx: *mut NexusContext,
    body: impl FnOnce(&mut Session<'_, '_>) -> Done<u32>,
) -> u64 {
    let mut block = 0;
    let status = routine(ctx, |session| {
        block = body(session)?;
        Ok(STATUS_CONTINUE)
    });
    (u64::from(status) << 32) | u64::from(block)
}

fn scalar_class(class: u32) -> Option<u8> {
    use vibra_ir::arena::class;
    match class {
        CLASS_I32 => Some(class::I32),
        CLASS_I64 => Some(class::I64),
        CLASS_F32 => Some(class::F32),
        CLASS_F64 => Some(class::F64),
        _ => None,
    }
}

const fn scalar_op(op: u32) -> Option<ScalarOp> {
    match op {
        OP_I64_ADD => Some(ScalarOp::I64Add),
        OP_I64_SUB => Some(ScalarOp::I64Sub),
        OP_I64_EQ => Some(ScalarOp::I64Eq),
        OP_I64_LT_S => Some(ScalarOp::I64LtS),
        _ => None,
    }
}

const fn object_kind(code: u32) -> Option<u8> {
    match code {
        KIND_TUPLE => Some(kind::TUPLE),
        KIND_ARRAY => Some(kind::ARRAY),
        KIND_RECORD => Some(kind::RECORD),
        KIND_ENUM => Some(kind::ENUM),
        KIND_WRAPPER => Some(kind::WRAPPER),
        KIND_UNION => Some(kind::UNION),
        _ => None,
    }
}

/// A non-tail call of a known function: the consumed operand list becomes the
/// callee's parameters, and the caller continues at block `resume` with the
/// result in slot `dst`. A checkpoint: answer `STATUS_CONTINUE` and return from
/// the step.
///
/// # Safety
///
/// The contract of the module; `function` must take `argc` parameters and no
/// environment, or the routine records a defect.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nexus_call(
    ctx: *mut NexusContext,
    function: u32,
    args: *const u32,
    argc: u32,
    dst: u32,
    resume: u32,
) -> u32 {
    routine(ctx, |session| {
        let args = session.operands(args, argc)?;
        let info = session.direct(function, args.len())?;
        let resume = session.block(resume)?;
        session.ctx.call(function, info.slots, args, dst, resume)?;
        Ok(STATUS_CONTINUE)
    })
}

/// A non-tail call through the function value in the borrowed slot `callee`.
/// A checkpoint. A callee that is no function value, or takes another number
/// of operands, is a defect.
///
/// # Safety
///
/// The contract of the module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nexus_call_value(
    ctx: *mut NexusContext,
    callee: u32,
    args: *const u32,
    argc: u32,
    dst: u32,
    resume: u32,
) -> u32 {
    routine(ctx, |session| {
        let args = session.operands(args, argc)?;
        let resume = session.block(resume)?;
        session.ctx.call_value(callee, args, dst, resume)?;
        Ok(STATUS_CONTINUE)
    })
}

/// A tail transfer to a known function: the callee replaces the running
/// frame. A checkpoint.
///
/// # Safety
///
/// As [`nexus_call`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nexus_tail_call(
    ctx: *mut NexusContext,
    function: u32,
    args: *const u32,
    argc: u32,
) -> u32 {
    routine(ctx, |session| {
        let args = session.operands(args, argc)?;
        let info = session.direct(function, args.len())?;
        session.ctx.tail_call(function, info.slots, args)?;
        Ok(STATUS_CONTINUE)
    })
}

/// A tail transfer through the function value in the borrowed slot `callee`.
/// A checkpoint.
///
/// # Safety
///
/// The contract of the module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nexus_tail_call_value(
    ctx: *mut NexusContext,
    callee: u32,
    args: *const u32,
    argc: u32,
) -> u32 {
    routine(ctx, |session| {
        let args = session.operands(args, argc)?;
        session.ctx.tail_call_value(callee, args)?;
        Ok(STATUS_CONTINUE)
    })
}

/// Leaves the running frame with the consumed value of `slot`. A checkpoint.
/// Answers `STATUS_DONE` when that frame was the entry's (the step returns it
/// as it is), and `STATUS_CONTINUE` otherwise.
///
/// # Safety
///
/// The contract of the module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nexus_ret(ctx: *mut NexusContext, slot: u32) -> u32 {
    routine(ctx, |session| {
        Ok(if session.ctx.ret(slot)? {
            STATUS_DONE
        } else {
            STATUS_CONTINUE
        })
    })
}

/// One host operation, performed exactly once: `symbol` indexes the program's
/// symbol table, `origin` is an origin ordinal (`0` for none). A checkpoint.
///
/// # Safety
///
/// The contract of the module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nexus_host(
    ctx: *mut NexusContext,
    symbol: u32,
    args: *const u32,
    argc: u32,
    dst: u32,
    resume: u32,
    origin: u32,
) -> u32 {
    routine(ctx, |session| {
        let args = session.operands(args, argc)?;
        let origin = session.origin(origin)?;
        let resume = session.block(resume)?;
        let index = usize::try_from(symbol).unwrap_or(usize::MAX);
        let Some(row) = session.code.rows.get(index).copied() else {
            return Err(session.defect());
        };
        session.ctx.host(row, args, dst, resume, origin)?;
        Ok(STATUS_CONTINUE)
    })
}

/// Reads module value `global` into `dst`. Answers `STATUS_CONTINUE` when the
/// first read pushed the initializer (a checkpoint: return from the step), and
/// `STATUS_PROCEED` when the value was ready and is now in `dst` (carry on at
/// block `resume` inside the step).
///
/// # Safety
///
/// The contract of the module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nexus_global(
    ctx: *mut NexusContext,
    global: u32,
    dst: u32,
    resume: u32,
) -> u32 {
    routine(ctx, |session| {
        let index = usize::try_from(global).unwrap_or(usize::MAX);
        let Some(initializer) = session.code.globals.get(index).copied() else {
            return Err(session.defect());
        };
        let info = session.function(initializer)?;
        let resume = session.block(resume)?;
        Ok(
            if session
                .ctx
                .global(global, initializer, info.slots, dst, resume)?
            {
                STATUS_CONTINUE
            } else {
                STATUS_PROCEED
            },
        )
    })
}

/// Records the trap `code` (a `TrapCode::code()`) at origin ordinal `origin`.
/// Always answers `STATUS_TRAP`; an unknown code is a defect instead.
///
/// # Safety
///
/// The contract of the module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nexus_trap(
    ctx: *mut NexusContext,
    code: u32,
    origin: u32,
) -> u32 {
    routine(ctx, |session| {
        let code = i32::try_from(code).ok().and_then(TrapCode::from_code);
        let origin = session.origin(origin)?;
        Err(match code {
            Some(code) => session.ctx.trap(code, origin),
            None => session.defect(),
        })
    })
}

/// Writes a scalar of `class` (`CLASS_*`) with `bits` to `dst`.
///
/// # Safety
///
/// The contract of the module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nexus_scalar(
    ctx: *mut NexusContext,
    dst: u32,
    class: u32,
    bits: u64,
) -> u32 {
    routine(ctx, |session| {
        let Some(class) = scalar_class(class) else {
            return Err(session.defect());
        };
        session.ctx.scalar(dst, class, bits)?;
        Ok(STATUS_CONTINUE)
    })
}

/// Builds constant `constant` of the program in the value store and writes a
/// reference to it to `dst`.
///
/// # Safety
///
/// The contract of the module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nexus_constant(
    ctx: *mut NexusContext,
    dst: u32,
    constant: u32,
) -> u32 {
    routine(ctx, |session| {
        let index = usize::try_from(constant).unwrap_or(usize::MAX);
        let code = session.code;
        match code.constants.get(index) {
            Some(ConstantData::Str(text)) => {
                session.ctx.constant_text(dst, kind::STR, text)?;
            }
            Some(ConstantData::Atom(text)) => {
                session.ctx.constant_text(dst, kind::ATOM, text)?;
            }
            Some(ConstantData::Bytes(bytes)) => {
                session.ctx.constant_bytes(dst, bytes)?;
            }
            None => return Err(session.defect()),
        }
        Ok(STATUS_CONTINUE)
    })
}

/// Writes another reference to the value of the borrowed slot `src` to `dst`.
///
/// # Safety
///
/// The contract of the module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nexus_copy(ctx: *mut NexusContext, dst: u32, src: u32) -> u32 {
    routine(ctx, |session| {
        session.ctx.copy(dst, src)?;
        Ok(STATUS_CONTINUE)
    })
}

/// Moves the value of `src` to `dst`, leaving `src` empty.
///
/// # Safety
///
/// The contract of the module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nexus_move(ctx: *mut NexusContext, dst: u32, src: u32) -> u32 {
    routine(ctx, |session| {
        session.ctx.move_slot(dst, src)?;
        Ok(STATUS_CONTINUE)
    })
}

/// Releases whatever `slot` holds and leaves it empty.
///
/// # Safety
///
/// The contract of the module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nexus_drop(ctx: *mut NexusContext, slot: u32) -> u32 {
    routine(ctx, |session| {
        session.ctx.drop_slot(slot)?;
        Ok(STATUS_CONTINUE)
    })
}

/// A scalar operation (`OP_*`) over the borrowed slots `left` and `right`,
/// written to `dst`.
///
/// # Safety
///
/// The contract of the module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nexus_op(
    ctx: *mut NexusContext,
    dst: u32,
    op: u32,
    left: u32,
    right: u32,
) -> u32 {
    routine(ctx, |session| {
        let Some(op) = scalar_op(op) else {
            return Err(session.defect());
        };
        session.ctx.scalar_op(dst, op, left, right)?;
        Ok(STATUS_CONTINUE)
    })
}

/// Assembles an object of `kind` (`KIND_*`) with `variant` from the consumed
/// operand list and writes a reference to it to `dst`.
///
/// # Safety
///
/// The contract of the module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nexus_build(
    ctx: *mut NexusContext,
    dst: u32,
    kind: u32,
    variant: u32,
    cells: *const u32,
    count: u32,
) -> u32 {
    routine(ctx, |session| {
        let cells = session.operands(cells, count)?;
        let Some(kind) = object_kind(kind) else {
            return Err(session.defect());
        };
        session.ctx.build(dst, kind, variant, cells)?;
        Ok(STATUS_CONTINUE)
    })
}

/// Writes another reference to cell `index` of the borrowed object in `object`
/// to `dst`.
///
/// # Safety
///
/// The contract of the module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nexus_cell(
    ctx: *mut NexusContext,
    dst: u32,
    object: u32,
    index: u32,
) -> u32 {
    routine(ctx, |session| {
        session.ctx.cell(dst, object, index)?;
        Ok(STATUS_CONTINUE)
    })
}

/// Writes the variant (or member) index of the borrowed object in `object` to
/// `dst` as an `I32`.
///
/// # Safety
///
/// The contract of the module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nexus_variant(
    ctx: *mut NexusContext,
    dst: u32,
    object: u32,
) -> u32 {
    routine(ctx, |session| {
        session.ctx.variant_of(dst, object)?;
        Ok(STATUS_CONTINUE)
    })
}

/// Builds a function value over `function`, holding the consumed capture list,
/// and writes it to `dst`. `function` must take an environment.
///
/// # Safety
///
/// The contract of the module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nexus_function(
    ctx: *mut NexusContext,
    dst: u32,
    function: u32,
    captures: *const u32,
    count: u32,
) -> u32 {
    routine(ctx, |session| {
        let captures = session.operands(captures, count)?;
        let info = session.function(function)?;
        if !info.env {
            return Err(session.defect());
        }
        session
            .ctx
            .make_function(dst, function, info.slots, info.params, captures)?;
        Ok(STATUS_CONTINUE)
    })
}

/// The block a `Branch` continues at: `then_block` when the borrowed scalar in
/// `cond` is nonzero, `else_block` otherwise. Answers a `u64`: the block in the
/// low 32 bits and a status in the high 32 (`STATUS_CONTINUE`, or a trap or
/// exhaustion, in which case the step returns that status).
///
/// # Safety
///
/// The contract of the module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nexus_branch(
    ctx: *mut NexusContext,
    cond: u32,
    then_block: u32,
    else_block: u32,
) -> u64 {
    selector(ctx, |session| {
        let block = session.ctx.branch(cond, then_block, else_block)?;
        session.block(block)
    })
}

/// The block a `Switch` continues at: `targets[value]` for the borrowed scalar
/// in `value`, `default` past the list. Answers as [`nexus_branch`].
///
/// # Safety
///
/// The contract of the module; `targets` is read like an operand list, with
/// `count` bounded by the running frame.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nexus_switch(
    ctx: *mut NexusContext,
    value: u32,
    targets: *const u32,
    count: u32,
    default: u32,
) -> u64 {
    selector(ctx, |session| {
        let targets = session.list(targets, count, MAX_SWITCH_TARGETS)?;
        let block = session.ctx.switch(value, targets, default)?;
        session.block(block)
    })
}
