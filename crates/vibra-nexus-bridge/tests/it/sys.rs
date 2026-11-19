//! Safe-looking wrappers over the routines, for the step functions that stand in
//! for generated code. Each wrapper is one `unsafe` call, with the invariant
//! that makes it sound: the handle is the one the dispatcher passed to the
//! running step function, and every operand list is an array that outlives the
//! call.

use vibra_nexus_bridge as bridge;
use vibra_nexus_bridge::NexusContext;

pub type Ctx = *mut NexusContext;

fn ptr(list: &[u32]) -> *const u32 {
    if list.is_empty() {
        std::ptr::null()
    } else {
        list.as_ptr()
    }
}

fn count(list: &[u32]) -> u32 {
    u32::try_from(list.len()).unwrap_or(u32::MAX)
}

pub fn call(ctx: Ctx, function: u32, args: &[u32], dst: u32, resume: u32) -> u32 {
    // SAFETY: `ctx` is the running step's handle; `args` outlives the call.
    unsafe { bridge::nexus_call(ctx, function, ptr(args), count(args), dst, resume) }
}

pub fn call_value(ctx: Ctx, callee: u32, args: &[u32], dst: u32, resume: u32) -> u32 {
    // SAFETY: `ctx` is the running step's handle; `args` outlives the call.
    unsafe {
        bridge::nexus_call_value(ctx, callee, ptr(args), count(args), dst, resume)
    }
}

pub fn tail_call(ctx: Ctx, function: u32, args: &[u32]) -> u32 {
    // SAFETY: `ctx` is the running step's handle; `args` outlives the call.
    unsafe { bridge::nexus_tail_call(ctx, function, ptr(args), count(args)) }
}

pub fn tail_call_value(ctx: Ctx, callee: u32, args: &[u32]) -> u32 {
    // SAFETY: `ctx` is the running step's handle; `args` outlives the call.
    unsafe { bridge::nexus_tail_call_value(ctx, callee, ptr(args), count(args)) }
}

pub fn ret(ctx: Ctx, slot: u32) -> u32 {
    // SAFETY: `ctx` is the running step's handle.
    unsafe { bridge::nexus_ret(ctx, slot) }
}

pub fn host(
    ctx: Ctx,
    symbol: u32,
    args: &[u32],
    dst: u32,
    resume: u32,
    origin: u32,
) -> u32 {
    // SAFETY: `ctx` is the running step's handle; `args` outlives the call.
    unsafe {
        bridge::nexus_host(ctx, symbol, ptr(args), count(args), dst, resume, origin)
    }
}

pub fn global(ctx: Ctx, global: u32, dst: u32, resume: u32) -> u32 {
    // SAFETY: `ctx` is the running step's handle.
    unsafe { bridge::nexus_global(ctx, global, dst, resume) }
}

pub fn trap(ctx: Ctx, code: u32, origin: u32) -> u32 {
    // SAFETY: `ctx` is the running step's handle.
    unsafe { bridge::nexus_trap(ctx, code, origin) }
}

pub fn scalar(ctx: Ctx, dst: u32, class: u32, bits: u64) -> u32 {
    // SAFETY: `ctx` is the running step's handle.
    unsafe { bridge::nexus_scalar(ctx, dst, class, bits) }
}

pub fn constant(ctx: Ctx, dst: u32, constant: u32) -> u32 {
    // SAFETY: `ctx` is the running step's handle.
    unsafe { bridge::nexus_constant(ctx, dst, constant) }
}

pub fn copy(ctx: Ctx, dst: u32, src: u32) -> u32 {
    // SAFETY: `ctx` is the running step's handle.
    unsafe { bridge::nexus_copy(ctx, dst, src) }
}

pub fn move_slot(ctx: Ctx, dst: u32, src: u32) -> u32 {
    // SAFETY: `ctx` is the running step's handle.
    unsafe { bridge::nexus_move(ctx, dst, src) }
}

pub fn drop_slot(ctx: Ctx, slot: u32) -> u32 {
    // SAFETY: `ctx` is the running step's handle.
    unsafe { bridge::nexus_drop(ctx, slot) }
}

pub fn op(ctx: Ctx, dst: u32, op: u32, left: u32, right: u32) -> u32 {
    // SAFETY: `ctx` is the running step's handle.
    unsafe { bridge::nexus_op(ctx, dst, op, left, right) }
}

pub fn build(ctx: Ctx, dst: u32, kind: u32, variant: u32, cells: &[u32]) -> u32 {
    // SAFETY: `ctx` is the running step's handle; `cells` outlives the call.
    unsafe { bridge::nexus_build(ctx, dst, kind, variant, ptr(cells), count(cells)) }
}

pub fn cell(ctx: Ctx, dst: u32, object: u32, index: u32) -> u32 {
    // SAFETY: `ctx` is the running step's handle.
    unsafe { bridge::nexus_cell(ctx, dst, object, index) }
}

pub fn variant(ctx: Ctx, dst: u32, object: u32) -> u32 {
    // SAFETY: `ctx` is the running step's handle.
    unsafe { bridge::nexus_variant(ctx, dst, object) }
}

pub fn function(ctx: Ctx, dst: u32, function: u32, captures: &[u32]) -> u32 {
    // SAFETY: `ctx` is the running step's handle; `captures` outlives the call.
    unsafe {
        bridge::nexus_function(ctx, dst, function, ptr(captures), count(captures))
    }
}

pub fn branch(ctx: Ctx, cond: u32, then_block: u32, else_block: u32) -> u64 {
    // SAFETY: `ctx` is the running step's handle.
    unsafe { bridge::nexus_branch(ctx, cond, then_block, else_block) }
}

pub fn switch(ctx: Ctx, value: u32, targets: &[u32], default: u32) -> u64 {
    // SAFETY: `ctx` is the running step's handle; `targets` outlives the call.
    unsafe { bridge::nexus_switch(ctx, value, ptr(targets), count(targets), default) }
}

/// `Ok` for `STATUS_CONTINUE`, and the status otherwise, so a step can use `?`.
pub fn ok(status: u32) -> Result<(), u32> {
    if status == bridge::STATUS_CONTINUE {
        Ok(())
    } else {
        Err(status)
    }
}

/// The block a selector routine answered, or the status that ends the step.
pub fn block(packed: u64) -> Result<u32, u32> {
    ok(u32::try_from(packed >> 32).unwrap_or(u32::MAX))?;
    Ok(u32::try_from(packed & 0xFFFF_FFFF).unwrap_or(u32::MAX))
}

/// The status a step answers for the result of its body.
pub fn status(result: Result<u32, u32>) -> u32 {
    result.unwrap_or_else(|status| status)
}
