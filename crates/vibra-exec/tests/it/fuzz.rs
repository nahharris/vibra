//! Random programs, and random single-field mutations of them: the verifier
//! must answer every one with a verdict and never panic.
//!
//! The generator keeps a model of which slots are full, so most programs it
//! builds are valid and a mutation reaches the verifier's deep checks (the
//! data-flow rules) instead of failing the first one. The random numbers come
//! from a small generator written here, seeded by a constant, so a failure
//! names a seed that reproduces it.

use std::collections::BTreeSet;
use std::panic::{AssertUnwindSafe, catch_unwind};

use vibra_diagnostics::ByteSpan;
use vibra_exec::{
    Block, BlockId, Callee, ConstId, Constant, FuncId, Function, GlobalId, Inst,
    ObjectKind, OriginId, Program, ScalarClass, ScalarOp, Slot, Terminator, verify,
};
use vibra_ir::SourceOrigin;
use vibra_ir::boundary::TrapCode;
use vibra_ir::external::CompilerIntrinsic;
use vibra_ir::external::host::{HOST_ROWS, HostRow, host_row};

/// The seed every run starts from.
const SEED: u64 = 0x5EED_0000_F0E1;

/// How many programs are generated and mutated.
const MUTATIONS: u64 = 20_000;

/// A splitmix64 generator.
struct Rng(u64);

impl Rng {
    fn draw(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number below `bound`, or `0` when `bound` is `0`.
    fn below(&mut self, bound: usize) -> usize {
        let bound = u64::try_from(bound).unwrap_or(u64::MAX).max(1);
        usize::try_from(self.draw() % bound).unwrap_or(0)
    }

    fn below32(&mut self, bound: u32) -> u32 {
        u32::try_from(self.below(usize::try_from(bound).unwrap_or(usize::MAX)))
            .unwrap_or(0)
    }

    /// `true` once in `n`.
    fn one_in(&mut self, n: usize) -> bool {
        self.below(n) == 0
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> Option<T> {
        items.get(self.below(items.len())).copied()
    }

    /// A number that is usually in `0..bound`, sometimes just past it, and
    /// sometimes enormous.
    fn wild(&mut self, bound: u32) -> u32 {
        match self.below(20) {
            0 => u32::MAX,
            1 | 2 => bound.saturating_add(self.below32(3)),
            _ => self.below32(bound.max(1)),
        }
    }
}

/// What the generator knows about the program it is building.
struct Context {
    /// `(params, env)` of each function.
    signatures: Vec<(u32, bool)>,
    constants: u32,
    origins: u32,
    globals: u32,
}

impl Context {
    fn origin(&self, rng: &mut Rng) -> OriginId {
        OriginId(rng.below32(self.origins.saturating_add(1)))
    }

    /// The functions a direct call can name, or a function value cannot.
    fn without_env(&self) -> Vec<u32> {
        self.matching(false)
    }

    fn with_env(&self) -> Vec<u32> {
        self.matching(true)
    }

    fn matching(&self, env: bool) -> Vec<u32> {
        self.signatures
            .iter()
            .enumerate()
            .filter(|(_, signature)| signature.1 == env)
            .map(|(index, _)| u32::try_from(index).unwrap_or(u32::MAX))
            .collect()
    }

    fn params(&self, id: u32) -> u32 {
        let index = usize::try_from(id).unwrap_or(usize::MAX);
        self.signatures
            .get(index)
            .map_or(0, |signature| signature.0)
    }
}

/// One function body under construction, with a model of its frame.
struct Body {
    full: Vec<bool>,
    insts: Vec<Inst>,
    blocks: Vec<Block>,
}

impl Body {
    fn slots(&self, full: bool) -> Vec<Slot> {
        self.full
            .iter()
            .enumerate()
            .filter(|(_, state)| **state == full)
            .map(|(index, _)| Slot(u32::try_from(index).unwrap_or(u32::MAX)))
            .collect()
    }

    fn set(&mut self, slot: Slot, full: bool) {
        if let Some(state) = self.full.get_mut(slot.index()) {
            *state = full;
        }
    }

    fn pick(&self, rng: &mut Rng, full: bool) -> Option<Slot> {
        rng.pick(&self.slots(full))
    }

    /// Up to `count` distinct full slots.
    fn distinct(&self, rng: &mut Rng, count: usize) -> Vec<Slot> {
        let mut pool = self.slots(true);
        let mut out = Vec::new();
        while out.len() < count && !pool.is_empty() {
            let at = rng.below(pool.len());
            out.push(pool.swap_remove(at));
        }
        out
    }

    fn emit(&mut self, inst: Inst) {
        self.insts.push(inst);
    }

    /// Writes a scalar into an empty slot, if there is one.
    fn write_scalar(&mut self, rng: &mut Rng) -> Option<Slot> {
        let dst = self.pick(rng, false)?;
        self.emit(Inst::Scalar {
            dst,
            class: scalar_class(rng),
            bits: rng.draw(),
        });
        self.set(dst, true);
        Some(dst)
    }

    /// A full slot, writing a scalar to make one if none is.
    fn some_full(&mut self, rng: &mut Rng) -> Option<Slot> {
        match self.pick(rng, true) {
            Some(slot) => Some(slot),
            None => self.write_scalar(rng),
        }
    }

    /// Ends the current block with `term`, and answers its index.
    fn close(&mut self, term: Terminator) -> u32 {
        let insts = std::mem::take(&mut self.insts);
        self.blocks.push(Block { insts, term });
        u32::try_from(self.blocks.len()).unwrap_or(u32::MAX)
    }

    fn here(&self) -> u32 {
        u32::try_from(self.blocks.len()).unwrap_or(u32::MAX)
    }
}

fn scalar_class(rng: &mut Rng) -> ScalarClass {
    match rng.below(4) {
        0 => ScalarClass::I32,
        1 => ScalarClass::I64,
        2 => ScalarClass::F32,
        _ => ScalarClass::F64,
    }
}

fn object_kind(rng: &mut Rng) -> ObjectKind {
    match rng.below(6) {
        0 => ObjectKind::Tuple,
        1 => ObjectKind::Array,
        2 => ObjectKind::Record,
        3 => ObjectKind::Enum,
        4 => ObjectKind::Wrapper,
        _ => ObjectKind::Union,
    }
}

fn intrinsic(rng: &mut Rng) -> CompilerIntrinsic {
    match rng.below(4) {
        0 => CompilerIntrinsic::StrLength,
        1 => CompilerIntrinsic::StrConcat,
        2 => CompilerIntrinsic::CharToU32,
        _ => CompilerIntrinsic::StrEqual,
    }
}

fn origin(source: &str, start: usize, end: usize) -> SourceOrigin {
    SourceOrigin::new(source, ByteSpan::new(start, end))
}

fn host_symbols() -> Vec<&'static str> {
    HOST_ROWS.iter().map(|row| row.symbol).collect()
}

fn host(rng: &mut Rng) -> &'static HostRow {
    let symbols = host_symbols();
    let symbol = rng.pick(&symbols).unwrap_or("io.stdout-text");
    host_row(symbol).unwrap()
}

/// One instruction that is valid in the current state, if one can be made.
fn instruction(body: &mut Body, context: &Context, rng: &mut Rng) {
    match rng.below(11) {
        0 | 1 => {
            body.write_scalar(rng);
        }
        2 if context.constants > 0 => {
            if let Some(dst) = body.pick(rng, false) {
                let constant = ConstId(rng.below32(context.constants));
                body.emit(Inst::Constant { dst, constant });
                body.set(dst, true);
            }
        }
        3 => {
            if let Some(src) = body.pick(rng, true)
                && let Some(dst) = body.pick(rng, false)
            {
                body.emit(Inst::Copy { dst, src });
                body.set(dst, true);
            }
        }
        4 => {
            if let Some(src) = body.pick(rng, true)
                && let Some(dst) = body.pick(rng, false)
            {
                body.emit(Inst::Move { dst, src });
                body.set(src, false);
                body.set(dst, true);
            }
        }
        5 => {
            let slot = Slot(rng.below32(u32::try_from(body.full.len()).unwrap_or(0)));
            body.emit(Inst::Drop { slot });
            body.set(slot, false);
        }
        6 => {
            if let Some(a) = body.pick(rng, true)
                && let Some(b) = body.pick(rng, true)
                && let Some(dst) = body.pick(rng, false)
            {
                body.emit(Inst::Op {
                    dst,
                    op: ScalarOp::I64Add,
                    args: vec![a, b],
                });
                body.set(dst, true);
            }
        }
        7 => {
            let count = rng.below(3);
            let args: Vec<Slot> =
                (0..count).filter_map(|_| body.pick(rng, true)).collect();
            if let Some(dst) = body.pick(rng, false) {
                body.emit(Inst::Intrinsic {
                    dst,
                    intrinsic: intrinsic(rng),
                    args,
                    origin: context.origin(rng),
                });
                body.set(dst, true);
            }
        }
        8 => {
            let wanted = rng.below(4);
            let cells = body.distinct(rng, wanted);
            for cell in &cells {
                body.set(*cell, false);
            }
            if let Some(dst) = body.pick(rng, false) {
                body.emit(Inst::Build {
                    dst,
                    kind: object_kind(rng),
                    variant: rng.below32(3),
                    cells,
                });
                body.set(dst, true);
            } else {
                // Nothing was emitted, so the cells are still full.
                for cell in &cells {
                    body.set(*cell, true);
                }
            }
        }
        9 => {
            if let Some(object) = body.pick(rng, true)
                && let Some(dst) = body.pick(rng, false)
            {
                if rng.one_in(2) {
                    body.emit(Inst::Cell {
                        dst,
                        object,
                        index: rng.below32(3),
                    });
                } else {
                    body.emit(Inst::Variant { dst, object });
                }
                body.set(dst, true);
            }
        }
        _ => {
            let Some(function) = rng.pick(&context.with_env()) else {
                return;
            };
            let wanted = rng.below(3);
            let captures = body.distinct(rng, wanted);
            for capture in &captures {
                body.set(*capture, false);
            }
            if let Some(dst) = body.pick(rng, false) {
                body.emit(Inst::Function {
                    dst,
                    function: FuncId(function),
                    captures,
                });
                body.set(dst, true);
            } else {
                for capture in &captures {
                    body.set(*capture, true);
                }
            }
        }
    }
}

/// Ends the current block in a way that continues, and starts the next.
fn segment(body: &mut Body, context: &Context, rng: &mut Rng) {
    match rng.below(10) {
        0 | 1 => diamond(body, rng),
        2 | 3 => call(body, context, rng),
        4 => {
            let row = host(rng);
            let args: Vec<Slot> = (0..row.parameters.len())
                .filter_map(|_| body.some_full(rng))
                .collect();
            if args.len() != row.parameters.len() {
                return;
            }
            if let Some(dst) = body.pick(rng, false) {
                let resume = body.here().saturating_add(1);
                let origin = context.origin(rng);
                body.close(Terminator::Host {
                    row,
                    args,
                    dst,
                    resume: BlockId(resume),
                    origin,
                });
                body.set(dst, true);
            }
        }
        5 if context.globals > 0 => {
            if let Some(dst) = body.pick(rng, false) {
                let resume = body.here().saturating_add(1);
                body.close(Terminator::Global {
                    global: GlobalId(rng.below32(context.globals)),
                    dst,
                    resume: BlockId(resume),
                });
                body.set(dst, true);
            }
        }
        _ => {
            for _ in 0..=rng.below(3) {
                instruction(body, context, rng);
            }
        }
    }
}

/// A branch or a switch to two arms that join. An arm may leave a slot full
/// that the other leaves empty; the join then drops it.
fn diamond(body: &mut Body, rng: &mut Rng) {
    let Some(cond) = body.some_full(rng) else {
        return;
    };
    let leaked = if rng.one_in(2) {
        body.pick(rng, false)
    } else {
        None
    };
    let scratch = body.pick(rng, false).filter(|slot| Some(*slot) != leaked);
    let first = body.here().saturating_add(1);
    let term = if rng.one_in(2) {
        Terminator::Branch {
            cond,
            then_block: BlockId(first),
            else_block: BlockId(first.saturating_add(1)),
        }
    } else {
        Terminator::Switch {
            value: cond,
            targets: vec![BlockId(first), BlockId(first.saturating_add(1))],
            default: BlockId(first.saturating_add(1)),
        }
    };
    body.close(term);
    let join = BlockId(first.saturating_add(2));
    let write = |dst: Slot, bits: u64| Inst::Scalar {
        dst,
        class: ScalarClass::I64,
        bits,
    };
    body.insts = leaked.map(|slot| write(slot, 1)).into_iter().collect();
    body.close(Terminator::Jump(join));
    body.insts = scratch
        .into_iter()
        .flat_map(|slot| [write(slot, 2), Inst::Drop { slot }])
        .collect();
    body.close(Terminator::Jump(join));
    body.insts = leaked.into_iter().map(|slot| Inst::Drop { slot }).collect();
}

fn call(body: &mut Body, context: &Context, rng: &mut Rng) {
    let resume = BlockId(body.here().saturating_add(1));
    if rng.one_in(4) {
        // An indirect call: the callee's arity is the runtime's to check.
        let Some(held) = body.some_full(rng) else {
            return;
        };
        body.set(held, false);
        let wanted = rng.below(3);
        let args = body.distinct(rng, wanted);
        body.set(held, true);
        for arg in &args {
            body.set(*arg, false);
        }
        let Some(dst) = body.pick(rng, false) else {
            for arg in &args {
                body.set(*arg, true);
            }
            return;
        };
        body.close(Terminator::Call {
            callee: Callee::Indirect(held),
            args,
            dst,
            resume,
        });
        body.set(dst, true);
        return;
    }
    let Some(callee) = rng.pick(&context.without_env()) else {
        return;
    };
    let wanted = usize::try_from(context.params(callee)).unwrap_or(usize::MAX);
    while body.slots(true).len() < wanted {
        if body.write_scalar(rng).is_none() {
            return;
        }
    }
    let args = body.distinct(rng, wanted);
    for arg in &args {
        body.set(*arg, false);
    }
    let Some(dst) = body.pick(rng, false) else {
        for arg in &args {
            body.set(*arg, true);
        }
        return;
    };
    body.close(Terminator::Call {
        callee: Callee::Direct(FuncId(callee)),
        args,
        dst,
        resume,
    });
    body.set(dst, true);
}

/// Ends the function.
fn finish(body: &mut Body, context: &Context, rng: &mut Rng) {
    match rng.below(6) {
        0 => {
            let origin = context.origin(rng);
            let code = if rng.one_in(2) {
                TrapCode::DivisionByZero
            } else {
                TrapCode::InvalidHostValue
            };
            body.close(Terminator::Trap { code, origin });
        }
        1 => {
            if let Some(callee) = rng.pick(&context.without_env()) {
                let wanted =
                    usize::try_from(context.params(callee)).unwrap_or(usize::MAX);
                while body.slots(true).len() < wanted {
                    if body.write_scalar(rng).is_none() {
                        break;
                    }
                }
                let args = body.distinct(rng, wanted);
                if args.len() == wanted {
                    body.close(Terminator::TailCall {
                        callee: Callee::Direct(FuncId(callee)),
                        args,
                    });
                    return;
                }
            }
            returned(body, rng);
        }
        _ => returned(body, rng),
    }
}

fn returned(body: &mut Body, rng: &mut Rng) {
    let value = body.some_full(rng).unwrap_or(Slot(0));
    body.close(Terminator::Return { value });
}

fn function(context: &Context, index: usize, rng: &mut Rng) -> Function {
    let (params, env) = context.signatures.get(index).copied().unwrap_or((0, false));
    let entry_slots = params + u32::from(env);
    let slots = entry_slots + 3 + rng.below32(4);
    let full = (0..slots).map(|slot| slot < entry_slots).collect();
    let mut body = Body {
        full,
        insts: Vec::new(),
        blocks: Vec::new(),
    };
    for _ in 0..=rng.below(5) {
        segment(&mut body, context, rng);
    }
    finish(&mut body, context, rng);
    Function {
        name: format!("f{index}"),
        params,
        env,
        slots,
        blocks: body.blocks,
    }
}

/// A random program, and mostly a valid one.
fn generate(rng: &mut Rng) -> Program {
    let count = 1 + rng.below(4);
    let signatures: Vec<(u32, bool)> = (0..count)
        .map(|index| {
            if index == 0 {
                (0, false)
            } else {
                (rng.below32(3), rng.one_in(3))
            }
        })
        .collect();
    let roots: Vec<FuncId> = signatures
        .iter()
        .enumerate()
        .filter(|(_, signature)| **signature == (0, false))
        .map(|(index, _)| FuncId(u32::try_from(index).unwrap_or(u32::MAX)))
        .collect();
    let globals: Vec<FuncId> =
        (0..rng.below(3)).filter_map(|_| rng.pick(&roots)).collect();
    let constants: Vec<Constant> = (0..rng.below(3))
        .map(|index| match rng.below(3) {
            0 => Constant::Str(format!("s{index}")),
            1 => Constant::Bytes(vec![0, 1, 255]),
            _ => Constant::Atom(format!("a{index}")),
        })
        .collect();
    // Half of the programs have an origin table, so their origin ordinals
    // can name entries.
    let origins: Vec<SourceOrigin> = if rng.one_in(2) {
        (0..1 + rng.below(3))
            .map(|index| origin(&format!("src{index}.vib"), index, index + 3))
            .collect()
    } else {
        Vec::new()
    };
    let context = Context {
        signatures,
        constants: u32::try_from(constants.len()).unwrap_or(0),
        origins: u32::try_from(origins.len()).unwrap_or(0),
        globals: u32::try_from(globals.len()).unwrap_or(0),
    };
    let functions = (0..count)
        .map(|index| function(&context, index, rng))
        .collect();
    Program {
        functions,
        entry: FuncId(0),
        globals,
        constants,
        origins,
    }
}

/// The bounds a mutation draws names from.
#[derive(Clone, Copy)]
struct Bounds {
    slots: u32,
    blocks: u32,
    functions: u32,
    constants: u32,
    globals: u32,
    origins: u32,
}

fn count<T>(items: &[T]) -> u32 {
    u32::try_from(items.len()).unwrap_or(u32::MAX)
}

fn replace_slot(slot: &mut Slot, rng: &mut Rng, b: Bounds) {
    *slot = Slot(rng.wild(b.slots));
}

fn replace_block(block: &mut BlockId, rng: &mut Rng, b: Bounds) {
    *block = BlockId(rng.wild(b.blocks));
}

fn replace_origin(origin: &mut OriginId, rng: &mut Rng, b: Bounds) {
    *origin = OriginId(rng.wild(b.origins));
}

fn replace_callee(callee: &mut Callee, rng: &mut Rng, b: Bounds) {
    *callee = if rng.one_in(2) {
        Callee::Direct(FuncId(rng.wild(b.functions)))
    } else {
        Callee::Indirect(Slot(rng.wild(b.slots)))
    };
}

/// Changes one element of an operand list, or adds, drops, or repeats one.
fn tweak_slots(list: &mut Vec<Slot>, rng: &mut Rng, b: Bounds) {
    match rng.below(4) {
        0 => list.push(Slot(rng.wild(b.slots))),
        1 => {
            list.pop();
        }
        2 => {
            if let Some(first) = list.first().copied() {
                list.push(first);
            }
        }
        _ => {
            let at = rng.below(list.len());
            if let Some(slot) = list.get_mut(at) {
                replace_slot(slot, rng, b);
            }
        }
    }
}

fn tweak_inst(inst: &mut Inst, rng: &mut Rng, b: Bounds) {
    let field = rng.below(4);
    match inst {
        Inst::Scalar { dst, class, bits } => match field {
            0 | 1 => replace_slot(dst, rng, b),
            2 => *class = scalar_class(rng),
            _ => *bits = rng.draw(),
        },
        Inst::Constant { dst, constant } => {
            if field < 2 {
                replace_slot(dst, rng, b);
            } else {
                *constant = ConstId(rng.wild(b.constants));
            }
        }
        Inst::Copy { dst, src } | Inst::Move { dst, src } => {
            if field < 2 {
                replace_slot(dst, rng, b);
            } else {
                replace_slot(src, rng, b);
            }
        }
        Inst::Drop { slot } => replace_slot(slot, rng, b),
        Inst::Op { dst, args, .. } => {
            if field < 2 {
                replace_slot(dst, rng, b);
            } else {
                tweak_slots(args, rng, b);
            }
        }
        Inst::Intrinsic {
            dst,
            intrinsic: kind,
            args,
            origin,
        } => match field {
            0 => replace_slot(dst, rng, b),
            1 => *kind = intrinsic(rng),
            2 => tweak_slots(args, rng, b),
            _ => replace_origin(origin, rng, b),
        },
        Inst::Build {
            dst,
            kind,
            variant,
            cells,
        } => match field {
            0 => replace_slot(dst, rng, b),
            1 => {
                *kind = object_kind(rng);
                *variant = rng.below32(4);
            }
            _ => tweak_slots(cells, rng, b),
        },
        Inst::Cell { dst, object, index } => match field {
            0 => replace_slot(dst, rng, b),
            1 | 2 => replace_slot(object, rng, b),
            _ => *index = rng.wild(4),
        },
        Inst::Variant { dst, object } => {
            if field < 2 {
                replace_slot(dst, rng, b);
            } else {
                replace_slot(object, rng, b);
            }
        }
        Inst::Function {
            dst,
            function,
            captures,
        } => match field {
            0 => replace_slot(dst, rng, b),
            1 => *function = FuncId(rng.wild(b.functions)),
            _ => tweak_slots(captures, rng, b),
        },
    }
}

fn tweak_term(term: &mut Terminator, rng: &mut Rng, b: Bounds) {
    let field = rng.below(5);
    match term {
        Terminator::Jump(target) => replace_block(target, rng, b),
        Terminator::Branch {
            cond,
            then_block,
            else_block,
        } => match field {
            0 | 1 => replace_slot(cond, rng, b),
            2 | 3 => replace_block(then_block, rng, b),
            _ => replace_block(else_block, rng, b),
        },
        Terminator::Switch {
            value,
            targets,
            default,
        } => match field {
            0 => replace_slot(value, rng, b),
            1 => replace_block(default, rng, b),
            2 => targets.push(BlockId(rng.wild(b.blocks))),
            3 => {
                targets.pop();
            }
            _ => {
                let at = rng.below(targets.len());
                if let Some(target) = targets.get_mut(at) {
                    replace_block(target, rng, b);
                }
            }
        },
        Terminator::Call {
            callee,
            args,
            dst,
            resume,
        } => match field {
            0 => replace_callee(callee, rng, b),
            1 => tweak_slots(args, rng, b),
            2 => replace_slot(dst, rng, b),
            _ => replace_block(resume, rng, b),
        },
        Terminator::TailCall { callee, args } => {
            if field < 2 {
                replace_callee(callee, rng, b);
            } else {
                tweak_slots(args, rng, b);
            }
        }
        Terminator::Host {
            row,
            args,
            dst,
            resume,
            origin,
        } => match field {
            0 => *row = host(rng),
            1 => tweak_slots(args, rng, b),
            2 => replace_slot(dst, rng, b),
            3 => replace_block(resume, rng, b),
            _ => replace_origin(origin, rng, b),
        },
        Terminator::Global {
            global,
            dst,
            resume,
        } => match field {
            0 | 1 => *global = GlobalId(rng.wild(b.globals)),
            2 | 3 => replace_slot(dst, rng, b),
            _ => replace_block(resume, rng, b),
        },
        Terminator::Return { value } => replace_slot(value, rng, b),
        Terminator::Trap { code, origin } => {
            if field < 2 {
                *code = TrapCode::InvalidCheckedProgram;
            } else {
                replace_origin(origin, rng, b);
            }
        }
    }
}

/// Changes one field of `program`.
fn mutate(program: &mut Program, rng: &mut Rng) {
    let function_count = count(&program.functions);
    let which = rng.below(program.functions.len());
    let b = Bounds {
        slots: program.functions.get(which).map_or(0, |f| f.slots),
        blocks: program.functions.get(which).map_or(0, |f| count(&f.blocks)),
        functions: function_count,
        constants: count(&program.constants),
        globals: count(&program.globals),
        origins: count(&program.origins),
    };
    match rng.below(10) {
        0 => match rng.below(9) {
            7 => {
                let at = rng.below(program.origins.len());
                if let Some(entry) = program.origins.get_mut(at) {
                    let start = rng.below(20);
                    let end = start + rng.below(5);
                    let source = if rng.one_in(2) {
                        entry.source_id().to_owned()
                    } else {
                        format!("other{}.vib", rng.below(3))
                    };
                    *entry = origin(&source, start, end);
                }
            }
            8 => program.origins.push(origin("added.vib", 0, 1)),
            0 => program.entry = FuncId(rng.wild(function_count)),
            1 => program.globals.push(FuncId(rng.wild(function_count))),
            2 => {
                let at = rng.below(program.globals.len());
                if let Some(global) = program.globals.get_mut(at) {
                    *global = FuncId(rng.wild(function_count));
                }
            }
            3 => {
                program.functions.pop();
            }
            4 => {
                program.constants.pop();
            }
            5 => {
                program.origins.pop();
            }
            _ => program.functions.clear(),
        },
        1 => {
            if let Some(function) = program.functions.get_mut(which) {
                match rng.below(6) {
                    0 => function.params = rng.below32(4),
                    1 => function.env = !function.env,
                    2 => function.slots = rng.below32(10),
                    3 => {
                        function.blocks.pop();
                    }
                    4 => function.blocks.clear(),
                    _ => {
                        if let Some(copy) =
                            rng.pick(&(0..function.blocks.len()).collect::<Vec<_>>())
                            && let Some(block) = function.blocks.get(copy).cloned()
                        {
                            function.blocks.push(block);
                        }
                    }
                }
            }
        }
        2 => {
            if let Some(function) = program.functions.get_mut(which) {
                let at = rng.below(function.blocks.len());
                if let Some(block) = function.blocks.get_mut(at) {
                    match rng.below(4) {
                        0 => {
                            let drop = rng.below(block.insts.len());
                            if drop < block.insts.len() {
                                block.insts.remove(drop);
                            }
                        }
                        1 => {
                            let repeat = rng.below(block.insts.len());
                            if let Some(inst) = block.insts.get(repeat).cloned() {
                                block.insts.insert(repeat, inst);
                            }
                        }
                        2 => {
                            let swap = rng.below(block.insts.len());
                            let with = rng.below(block.insts.len());
                            if swap < block.insts.len() && with < block.insts.len() {
                                block.insts.swap(swap, with);
                            }
                        }
                        _ => block.term = Terminator::Jump(BlockId(rng.wild(b.blocks))),
                    }
                }
            }
        }
        3..=6 => {
            if let Some(function) = program.functions.get_mut(which) {
                let at = rng.below(function.blocks.len());
                if let Some(block) = function.blocks.get_mut(at) {
                    let inst = rng.below(block.insts.len());
                    if let Some(inst) = block.insts.get_mut(inst) {
                        tweak_inst(inst, rng, b);
                    }
                }
            }
        }
        _ => {
            if let Some(function) = program.functions.get_mut(which) {
                let at = rng.below(function.blocks.len());
                if let Some(block) = function.blocks.get_mut(at) {
                    tweak_term(&mut block.term, rng, b);
                }
            }
        }
    }
}

fn variant_name(error: &vibra_exec::VerifyError) -> String {
    let text = format!("{:?}", error.kind);
    text.split(['(', ' ']).next().unwrap_or("").to_owned()
}

#[test]
fn verify_never_panics_on_mutated_programs() {
    let mut seeds = Rng(SEED);
    let (mut valid, mut accepted, mut rejected) = (0_u64, 0_u64, 0_u64);
    let mut kinds = BTreeSet::new();
    for _ in 0..MUTATIONS {
        let seed = seeds.draw();
        let mut rng = Rng(seed);
        let original = generate(&mut rng);
        if verify(&original).is_ok() {
            valid += 1;
        }
        let mut mutated = original.clone();
        mutate(&mut mutated, &mut rng);
        let verdict = catch_unwind(AssertUnwindSafe(|| verify(&mutated)));
        assert!(
            verdict.is_ok(),
            "verify panicked: seed {seed:#x}\n{mutated}"
        );
        match verdict.unwrap() {
            Ok(()) => accepted += 1,
            Err(error) => {
                rejected += 1;
                kinds.insert(variant_name(&error));
            }
        }
        // A changed program prints different text.
        if mutated != original {
            assert_ne!(
                mutated.to_string(),
                original.to_string(),
                "seed {seed:#x}: a changed program printed the same text"
            );
        }
    }
    println!(
        "generated {MUTATIONS}: {valid} valid; mutated: {accepted} accepted, {rejected} rejected, {} kinds: {kinds:?}",
        kinds.len()
    );
    assert!(
        valid * 10 >= MUTATIONS * 9,
        "only {valid} of {MUTATIONS} generated programs verify"
    );
    assert!(accepted > 0, "no mutation was accepted");
    assert!(rejected > 0, "no mutation was rejected");
    assert!(
        kinds.len() >= 12,
        "mutations reached only {} kinds of rejection: {kinds:?}",
        kinds.len()
    );
}
