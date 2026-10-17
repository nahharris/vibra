//! Canonical equality and observability of arena values.
//!
//! A test assertion compares the canonical encodings of its operands
//! (`docs/spec/06-runtime.md`, "Test assertions" and "Canonical value
//! encoding"), and a value that holds a function has none. Two routines of a
//! module that runs tests answer both from the arena alone, with no type: a
//! value in the arena carries what its encoding is made of.
//!
//! - A scalar cell is its class and its bits. Two scalars encode alike exactly
//!   when their classes and bits agree, with one exception the specification
//!   makes: every NaN of a width is the one quiet NaN there (decision D11.1),
//!   so a NaN equals a NaN, while a negative zero is still not a positive one.
//! - A reference cell is an arena value, compared by its kind, its length, its
//!   variant (an enum's variant, a union's member), and its components. Text
//!   and bytes compare their payload, a dict compares its entries in the
//!   canonical key order it keeps, and an interface value, which is a pair of
//!   the descriptor of the type it holds and the value, compares the two
//!   descriptors, which are tuples themselves, and the two values. A value
//!   that is the same object as the other is equal to it without a walk.
//! - A function value has no encoding, so reaching one is the answer of
//!   [`WALK_SCAN`]: the value is not observable.
//!
//! The walk is iterative, so a value nested as deep as memory admits uses the
//! engine's stack for nothing: the pairs still to visit are a stack in a block
//! of the arena, which doubles when full and is freed on every exit, so the live
//! size returns to where it was. Exhausting memory while it grows is the host
//! event, as for any allocation.

use wasm_encoder::{BlockType, Instruction as I, ValType};

use crate::layout::header;
use crate::runtime::{
    Ins, Routine, Routines, c32, c64, get, ld8, ld32, ld64, set, st32,
};
use vibra_ir::arena::{class, kind};

/// The `pair` argument of a walk that looks for a function in one value.
pub(crate) const WALK_SCAN: i32 = 0;
/// The `pair` argument of a walk that compares two values.
pub(crate) const WALK_EQUAL: i32 = 1;

/// The entries the walk's stack holds before it first grows.
const FIRST_CAPACITY: i32 = 16;

// The parameters and locals of the routine.
const A: u32 = 0;
const B: u32 = 1;
const PAIR: u32 = 2;
const STACK: u32 = 3;
const CAP: u32 = 4;
const SP: u32 = 5;
const RESULT: u32 = 6;
const ENTRY: u32 = 7;
const X: u32 = 8;
const Y: u32 = 9;
const AT: u32 = 10;
const LEN: u32 = 11;
const CX: u32 = 12;
const KX: u32 = 13;
const CY: u32 = 14;
const KY: u32 = 15;
const CLASS: u32 = 16;
const NEW: u32 = 17;
const BX: u32 = 18;
const BY: u32 = 19;

/// Instructions, with the labels of the structured control flow they open, so
/// a branch names its target and never counts it.
struct Code {
    ins: Vec<Ins>,
    labels: Vec<&'static str>,
}

impl Code {
    fn new() -> Self {
        Self {
            ins: Vec::new(),
            labels: Vec::new(),
        }
    }

    fn emit(&mut self, instructions: &[Ins]) {
        self.ins.extend_from_slice(instructions);
    }

    fn open(&mut self, label: &'static str, instruction: Ins) {
        self.labels.push(label);
        self.ins.push(instruction);
    }

    fn close(&mut self) {
        self.labels.pop();
        self.ins.push(I::End);
    }

    fn otherwise(&mut self) {
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

    fn br(&mut self, label: &'static str) {
        let depth = self.depth(label);
        self.ins.push(I::Br(depth));
    }

    fn br_if(&mut self, label: &'static str) {
        let depth = self.depth(label);
        self.ins.push(I::BrIf(depth));
    }

    /// Answers `0` and leaves the walk.
    fn differ(&mut self) {
        self.emit(&[c32(0), set(RESULT)]);
        self.br("done");
    }
}

/// The address of entry `SP` of the stack.
fn entry_address() -> [Ins; 6] {
    [
        get(STACK),
        get(SP),
        c32(3),
        I::I32Shl,
        I::I32Add,
        set(ENTRY),
    ]
}

/// `(a, b, pair) -> i32`: `1` when the walk finds what it looks for nothing of,
/// `0` when it finds a function (scan) or a difference (equal).
///
/// `pair` is [`WALK_SCAN`] for the values reachable from `a`, which must hold
/// no function, and [`WALK_EQUAL`] for `a` and `b`, which must be equal.
#[allow(clippy::too_many_lines)]
pub(crate) fn walk_routine(fns: &Routines) -> Routine {
    let mut c = Code::new();
    let reference = i32::from(class::REF);
    // The stack: entries of two offsets, after the 24 bytes every block may
    // keep for its header.
    c.emit(&[
        c32(FIRST_CAPACITY),
        set(CAP),
        c64(i64::from(header::SIZE) + 8 * i64::from(FIRST_CAPACITY)),
        I::Call(fns.alloc),
        set(STACK),
        c32(0),
        set(SP),
        c32(1),
        set(RESULT),
        get(STACK),
        get(A),
        st32(header::SIZE),
        get(STACK),
        get(B),
        st32(header::SIZE + 4),
        c32(1),
        set(SP),
    ]);
    c.open("done", I::Block(BlockType::Empty));
    c.open("next", I::Loop(BlockType::Empty));
    c.emit(&[get(SP), I::I32Eqz]);
    c.br_if("done");
    c.emit(&[get(SP), c32(1), I::I32Sub, set(SP)]);
    c.emit(&entry_address());
    c.emit(&[
        get(ENTRY),
        ld32(header::SIZE),
        set(X),
        get(ENTRY),
        ld32(header::SIZE + 4),
        set(Y),
    ]);
    c.open("visit", I::Block(BlockType::Empty));

    // What the walk checks of the two objects themselves.
    c.emit(&[get(PAIR), I::I32Eqz]);
    c.open("mode", I::If(BlockType::Empty));
    // A function has no encoding.
    c.emit(&[
        get(X),
        ld8(header::KIND),
        c32(i32::from(kind::FUNCTION)),
        I::I32Eq,
    ]);
    c.open("hidden", I::If(BlockType::Empty));
    c.differ();
    c.close();
    c.otherwise();
    // The same object is equal to itself.
    c.emit(&[get(X), get(Y), I::I32Eq]);
    c.br_if("visit");
    // The same kind, length, and variant.
    c.emit(&[
        get(X),
        ld8(header::KIND),
        get(Y),
        ld8(header::KIND),
        I::I32Ne,
        get(X),
        ld32(header::LEN),
        get(Y),
        ld32(header::LEN),
        I::I32Ne,
        I::I32Or,
        get(X),
        ld32(header::VARIANT),
        get(Y),
        ld32(header::VARIANT),
        I::I32Ne,
        I::I32Or,
    ]);
    c.open("unlike", I::If(BlockType::Empty));
    c.differ();
    c.close();
    c.close();

    // Text and bytes hold no reference.
    c.emit(&[
        get(X),
        ld8(header::STRIDE),
        c32(crate::layout::STRIDE_CELLS.cast_signed()),
        I::I32Ne,
    ]);
    c.open("flat", I::If(BlockType::Empty));
    c.emit(&[get(PAIR)]);
    c.open("compare", I::If(BlockType::Empty));
    c.emit(&[get(X), get(Y), I::Call(fns.equal), I::I32Eqz]);
    c.open("text", I::If(BlockType::Empty));
    c.differ();
    c.close();
    c.close();
    c.br("visit");
    c.close();

    // The cells: a class byte for each, and then the cells.
    c.emit(&[
        get(X),
        ld32(header::LEN),
        set(LEN),
        get(X),
        c32(index_of(header::SIZE)),
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
        get(Y),
        c32(index_of(header::SIZE)),
        I::I32Add,
        set(CY),
        get(CY),
        get(LEN),
        c32(7),
        I::I32Add,
        c32(-8),
        I::I32And,
        I::I32Add,
        set(KY),
        c32(0),
        set(AT),
    ]);
    c.open("cells", I::Block(BlockType::Empty));
    c.open("cell", I::Loop(BlockType::Empty));
    c.emit(&[get(AT), get(LEN), I::I32GeU]);
    c.br_if("cells");
    c.emit(&[get(CX), get(AT), I::I32Add, ld8(0), set(CLASS)]);
    // Two cells hold the same class of thing.
    c.emit(&[get(PAIR)]);
    c.open("classes", I::If(BlockType::Empty));
    c.emit(&[get(CLASS), get(CY), get(AT), I::I32Add, ld8(0), I::I32Ne]);
    c.open("unlike", I::If(BlockType::Empty));
    c.differ();
    c.close();
    c.close();

    c.emit(&[get(CLASS), c32(reference), I::I32Eq]);
    c.open("reference", I::If(BlockType::Empty));
    // Push the children, growing the stack first when it is full.
    c.emit(&[get(SP), get(CAP), I::I32Eq]);
    c.open("grow", I::If(BlockType::Empty));
    c.emit(&[
        get(CAP),
        c32(1),
        I::I32Shl,
        set(CAP),
        get(CAP),
        I::I64ExtendI32U,
        c64(3),
        I::I64Shl,
        c64(i64::from(header::SIZE)),
        I::I64Add,
        I::Call(fns.alloc),
        set(NEW),
        get(NEW),
        c32(index_of(header::SIZE)),
        I::I32Add,
        get(STACK),
        c32(index_of(header::SIZE)),
        I::I32Add,
        get(SP),
        c32(3),
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
    c.emit(&entry_address());
    c.emit(&[
        get(ENTRY),
        get(KX),
        get(AT),
        c32(3),
        I::I32Shl,
        I::I32Add,
        ld32(0),
        st32(header::SIZE),
        get(ENTRY),
        get(PAIR),
        I::If(BlockType::Result(ValType::I32)),
        get(KY),
        get(AT),
        c32(3),
        I::I32Shl,
        I::I32Add,
        ld32(0),
        I::Else,
        c32(0),
        I::End,
        st32(header::SIZE + 4),
        get(SP),
        c32(1),
        I::I32Add,
        set(SP),
    ]);
    c.otherwise();
    // A scalar: compared when two values are.
    c.emit(&[get(PAIR)]);
    c.open("scalar", I::If(BlockType::Empty));
    scalars(&mut c);
    c.close();
    c.close();
    c.emit(&[get(AT), c32(1), I::I32Add, set(AT)]);
    c.br("cell");
    c.close();
    c.close();
    c.close();
    // Back to the next pair.
    c.br("next");
    c.close();
    c.close();
    c.emit(&[get(STACK), I::Call(fns.free), get(RESULT), I::End]);

    Routine {
        params: vec![ValType::I32; 3],
        results: vec![ValType::I32],
        locals: {
            let mut locals = vec![ValType::I32; 15];
            locals.extend([ValType::I64, ValType::I64]);
            locals
        },
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

/// Compares the scalar cells at `AT` of the two objects by their class: the
/// bits that class holds, with every NaN replaced by the quiet NaN of its
/// width.
fn scalars(c: &mut Code) {
    c.emit(&[
        get(KX),
        get(AT),
        c32(3),
        I::I32Shl,
        I::I32Add,
        ld64(0),
        set(BX),
        get(KY),
        get(AT),
        c32(3),
        I::I32Shl,
        I::I32Add,
        ld64(0),
        set(BY),
    ]);
    // The classes of 32 bits are the even ones; only their low half is a value.
    c.emit(&[get(CLASS), c32(1), I::I32And, I::I32Eqz]);
    c.open("narrow", I::If(BlockType::Empty));
    c.emit(&[
        get(BX),
        c64(0xFFFF_FFFF),
        I::I64And,
        set(BX),
        get(BY),
        c64(0xFFFF_FFFF),
        I::I64And,
        set(BY),
    ]);
    c.close();
    // The floats are the classes with the second bit.
    c.emit(&[get(CLASS), c32(2), I::I32And]);
    c.open("float", I::If(BlockType::Empty));
    c.emit(&[get(CLASS), c32(1), I::I32And, I::I32Eqz]);
    c.open("single", I::If(BlockType::Empty));
    for local in [BX, BY] {
        c.emit(&nan_replaced(local, false));
    }
    c.otherwise();
    for local in [BX, BY] {
        c.emit(&nan_replaced(local, true));
    }
    c.close();
    c.close();
    c.emit(&[get(BX), get(BY), I::I64Ne]);
    c.open("unlike", I::If(BlockType::Empty));
    c.differ();
    c.close();
}

const fn index_of(value: u32) -> i32 {
    value.cast_signed()
}
