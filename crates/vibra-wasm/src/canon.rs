//! Observability of arena values, and the helpers the equality walk shares.
//!
//! A test assertion compares the canonical encodings of its operands
//! (`docs/spec/06-runtime.md`, "Test assertions" and "Canonical value
//! encoding"), and a value that holds a function has none. The comparison is the
//! one walk of [`crate::canon_typed`]; this module holds the routine that looks
//! for a function in a value, and the small builder both walks are written with.
//!
//! The scan reaches every arena value a value holds and answers `0` when one is
//! a function value. It is iterative, so a value nested as deep as memory admits
//! uses the engine's stack for nothing: the values still to visit are a stack in
//! a block of the arena, which doubles when full and is freed on every exit, so
//! the live size returns to where it was. Exhausting memory while it grows is the
//! host event, as for any allocation.

use wasm_encoder::{BlockType, Instruction as I, ValType};

use crate::layout::header;
use crate::runtime::{Ins, Routine, Routines, c32, c64, get, ld8, ld32, set, st32};
use vibra_ir::arena::{class, kind};

/// The entries the scan's stack holds before it first grows.
const FIRST_CAPACITY: i32 = 16;

// The parameter and locals of the scan.
const A: u32 = 0;
const STACK: u32 = 1;
const CAP: u32 = 2;
const SP: u32 = 3;
const RESULT: u32 = 4;
const X: u32 = 5;
const AT: u32 = 6;
const LEN: u32 = 7;
const CX: u32 = 8;
const KX: u32 = 9;
const CLASS: u32 = 10;
const NEW: u32 = 11;

/// Instructions, with the labels of the structured control flow they open, so
/// a branch names its target and never counts it.
pub(crate) struct Code {
    pub(crate) ins: Vec<Ins>,
    pub(crate) labels: Vec<&'static str>,
}

impl Code {
    pub(crate) fn new() -> Self {
        Self {
            ins: Vec::new(),
            labels: Vec::new(),
        }
    }

    pub(crate) fn emit(&mut self, instructions: &[Ins]) {
        self.ins.extend_from_slice(instructions);
    }

    pub(crate) fn open(&mut self, label: &'static str, instruction: Ins) {
        self.labels.push(label);
        self.ins.push(instruction);
    }

    pub(crate) fn close(&mut self) {
        self.labels.pop();
        self.ins.push(I::End);
    }

    pub(crate) fn otherwise(&mut self) {
        self.ins.push(I::Else);
    }

    /// The depth of the nearest enclosing construct called `label`.
    fn depth(&self, label: &'static str) -> u32 {
        let position = self
            .labels
            .iter()
            .rev()
            .position(|known| *known == label)
            .unwrap_or(0);
        u32::try_from(position).unwrap_or(0)
    }

    pub(crate) fn br(&mut self, label: &'static str) {
        let depth = self.depth(label);
        self.ins.push(I::Br(depth));
    }

    pub(crate) fn br_if(&mut self, label: &'static str) {
        let depth = self.depth(label);
        self.ins.push(I::BrIf(depth));
    }

    /// Sets the local `result` to `0` and leaves the walk.
    pub(crate) fn differ_at(&mut self, result: u32) {
        self.emit(&[c32(0), set(result)]);
        self.br("done");
    }
}

/// `(value) -> i32`: `1` when no function value is reachable from the arena
/// value `value`, and `0` when one is.
#[allow(clippy::too_many_lines)]
pub(crate) fn scan_routine(fns: &Routines) -> Routine {
    let mut c = Code::new();
    let reference = i32::from(class::REF);
    c.emit(&[
        c32(FIRST_CAPACITY),
        set(CAP),
        c64(i64::from(header::SIZE) + 4 * i64::from(FIRST_CAPACITY)),
        I::Call(fns.alloc),
        set(STACK),
        get(STACK),
        get(A),
        st32(header::SIZE),
        c32(1),
        set(SP),
        c32(1),
        set(RESULT),
    ]);
    c.open("done", I::Block(BlockType::Empty));
    c.open("next", I::Loop(BlockType::Empty));
    c.emit(&[get(SP), I::I32Eqz]);
    c.br_if("done");
    c.emit(&[
        get(SP),
        c32(1),
        I::I32Sub,
        set(SP),
        get(STACK),
        get(SP),
        c32(2),
        I::I32Shl,
        I::I32Add,
        ld32(header::SIZE),
        set(X),
    ]);
    // A function has no encoding.
    c.emit(&[
        get(X),
        ld8(header::KIND),
        c32(i32::from(kind::FUNCTION)),
        I::I32Eq,
    ]);
    c.open("hidden", I::If(BlockType::Empty));
    c.differ_at(RESULT);
    c.close();
    // Text and bytes hold no reference.
    c.open("visit", I::Block(BlockType::Empty));
    c.emit(&[
        get(X),
        ld8(header::STRIDE),
        c32(crate::layout::STRIDE_CELLS.cast_signed()),
        I::I32Ne,
    ]);
    c.br_if("visit");
    c.emit(&[
        get(X),
        ld32(header::LEN),
        set(LEN),
        get(X),
        c32(header::SIZE.cast_signed()),
        I::I32Add,
        set(CX),
        get(CX),
        get(LEN),
        c32(7),
        I::I32Add,
        c32(-8),
        I::I32And,
        I::I32Add,
        set(KX),
        c32(0),
        set(AT),
    ]);
    c.open("cells", I::Block(BlockType::Empty));
    c.open("cell", I::Loop(BlockType::Empty));
    c.emit(&[get(AT), get(LEN), I::I32GeU]);
    c.br_if("cells");
    c.emit(&[get(CX), get(AT), I::I32Add, ld8(0), set(CLASS)]);
    c.emit(&[get(CLASS), c32(reference), I::I32Eq]);
    c.open("reference", I::If(BlockType::Empty));
    c.emit(&[get(SP), get(CAP), I::I32Eq]);
    c.open("grow", I::If(BlockType::Empty));
    c.emit(&[
        get(CAP),
        c32(1),
        I::I32Shl,
        set(CAP),
        get(CAP),
        I::I64ExtendI32U,
        c64(2),
        I::I64Shl,
        c64(i64::from(header::SIZE)),
        I::I64Add,
        I::Call(fns.alloc),
        set(NEW),
        get(NEW),
        c32(header::SIZE.cast_signed()),
        I::I32Add,
        get(STACK),
        c32(header::SIZE.cast_signed()),
        I::I32Add,
        get(SP),
        c32(2),
        I::I32Shl,
        I::MemoryCopy {
            src_mem: 0,
            dst_mem: 0,
        },
        get(STACK),
        I::Call(fns.free),
        get(NEW),
        set(STACK),
    ]);
    c.close();
    c.emit(&[
        get(STACK),
        get(SP),
        c32(2),
        I::I32Shl,
        I::I32Add,
        get(KX),
        get(AT),
        c32(3),
        I::I32Shl,
        I::I32Add,
        ld32(0),
        st32(header::SIZE),
        get(SP),
        c32(1),
        I::I32Add,
        set(SP),
    ]);
    c.close();
    c.emit(&[get(AT), c32(1), I::I32Add, set(AT)]);
    c.br("cell");
    c.close();
    c.close();
    c.close();
    c.br("next");
    c.close();
    c.close();
    c.emit(&[get(STACK), I::Call(fns.free), get(RESULT), I::End]);
    Routine {
        params: vec![ValType::I32],
        results: vec![ValType::I32],
        locals: vec![ValType::I32; 11],
        body: c.ins,
        export: None,
    }
}

/// The instructions that replace the float bits in the 64-bit local `local` by
/// the quiet NaN of their width when they are a NaN: the one replacement of
/// decision D11.1 that observation applies. A binary32 value is in the low half
/// of the local.
pub(crate) fn nan_replaced(local: u32, double: bool) -> Vec<Ins> {
    let (canonical, magnitude, infinity) = if double {
        (
            vibra_ir::CANONICAL_NAN_F64.cast_signed(),
            0x7FFF_FFFF_FFFF_FFFF,
            0x7FF0_0000_0000_0000,
        )
    } else {
        (
            i64::from(vibra_ir::CANONICAL_NAN_F32),
            0x7FFF_FFFF,
            0x7F80_0000,
        )
    };
    vec![
        c64(canonical),
        get(local),
        get(local),
        c64(magnitude),
        I::I64And,
        c64(infinity),
        I::I64GtU,
        I::Select,
        set(local),
    ]
}
