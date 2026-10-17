//! Canonical equality of values whose types are known only at run time.
//!
//! [`crate::canon`] compares two values of one static type from the arena
//! alone. When a value can hold an interface value, the types of the parts are
//! not static: an interface value is the value it holds, so two interface values
//! that hold different types are equal exactly when their canonical encodings
//! are (`docs/spec/06-runtime.md`, "Canonical value encoding"). Two empty
//! arrays of different element types are equal, and so are the `none` of two
//! `option`s of different payloads; two values of different declared types are
//! not, because the encoding names the type, and neither are two unions whose
//! selected members are different types, or two primitives of different types.
//!
//! The walk therefore follows each value with the descriptor of its type, in
//! lockstep. A descriptor says what its constructor is and its arguments; the
//! type table of [`crate::lower`]'s `typetab` says what the encoding of a value
//! of a constructor is made of, and for a declared type gives the templates of
//! its components, which the walk instantiates at the arguments of the
//! descriptor (`inst`) and gives back at the end, with everything else it
//! allocated. Like the walk of [`crate::canon`], it keeps no state on the
//! engine's stack, however deep the values are.

use wasm_encoder::{BlockType, Instruction as I, ValType};

use crate::canon::{Code, nan_replaced};
use crate::layout::{self, header};
use crate::lower::typetab::{ROW, kind as tk};
use crate::runtime::{
    Ins, Routine, Routines, c32, c64, get, ld8, ld32, ld64, set, st8, st32, st64,
};
use vibra_ir::arena::class;

const REF: i32 = class::REF as i32;
/// The bytes of an entry of the walk's stack: the two values (the bits of a
/// cell), the two descriptors, and the two class bytes.
const ENTRY_SHIFT: i32 = 5;
const FIRST_CAPACITY: i32 = 16;
/// The fields of the state block: the stack, its capacity in entries, and the
/// entries on it.
const STACK_AT: u32 = header::SIZE;
const CAPACITY_AT: u32 = header::SIZE + 4;
const COUNT_AT: u32 = header::SIZE + 8;

/// The routines and the table the walk calls.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Typed {
    /// The routine that pushes an entry.
    pub(crate) push: u32,
    /// The routine that instantiates a template.
    pub(crate) inst: u32,
    /// The routine that compares two descriptors.
    pub(crate) desc_equal: u32,
    /// The passive data segment that holds the type table.
    pub(crate) segment: u32,
    /// The bytes of the table.
    pub(crate) table_size: u32,
}

/// The address of the cells of the tuple in the local `object`.
fn cells(object: u32) -> Vec<Ins> {
    vec![
        get(object),
        get(object),
        ld32(header::LEN),
        c32(7),
        I::I32Add,
        c32(-8),
        I::I32And,
        I::I32Add,
    ]
}

/// Pushes the argument whose index the instructions in `at` push, of the
/// descriptor in the local `d`: its offset, or `0` when the argument is not a
/// reference, which is a type nothing fixed.
fn argument(d: u32, at: &[Ins]) -> Vec<Ins> {
    let mut code = cells(d);
    code.extend_from_slice(at);
    code.extend([
        c32(3),
        I::I32Shl,
        I::I32Add,
        ld32(header::SIZE + 8),
        c32(0),
        get(d),
    ]);
    code.extend_from_slice(at);
    code.extend([
        I::I32Add,
        ld8(header::SIZE + 1),
        c32(REF),
        I::I32Eq,
        I::Select,
    ]);
    code
}

/// Pushes the head of the descriptor in the local `d`.
fn head(d: u32) -> Vec<Ins> {
    let mut code = cells(d);
    code.push(ld32(header::SIZE));
    code
}

/// Pushes the address of the row of the constructor of the descriptor in `d`.
fn row(tab: u32, d: u32) -> Vec<Ins> {
    let mut code = vec![get(tab), c32(4), I::I32Add];
    code.extend(head(d));
    code.extend([
        c32(8),
        I::I32ShrU,
        c32(ROW.cast_signed()),
        I::I32Mul,
        I::I32Add,
    ]);
    code
}

// -- push ---------------------------------------------------------------------

/// `(state, a, b, da, db, ca, cb) -> ()`: pushes an entry on the stack the state
/// block holds, doubling the stack first when it is full.
pub(crate) fn push_routine(fns: &Routines) -> Routine {
    // Params: 0 state, 1 a (i64), 2 b (i64), 3 da, 4 db, 5 ca, 6 cb.
    // Locals: 7 stack, 8 capacity, 9 count, 10 new stack, 11 entry.
    let (state, stack, cap, count, fresh, entry) = (0, 7, 8, 9, 10, 11);
    let mut c = Code::new();
    c.emit(&[
        get(state),
        ld32(STACK_AT),
        set(stack),
        get(state),
        ld32(CAPACITY_AT),
        set(cap),
        get(state),
        ld32(COUNT_AT),
        set(count),
        get(count),
        get(cap),
        I::I32Eq,
    ]);
    c.open("grow", I::If(BlockType::Empty));
    c.emit(&[
        get(cap),
        c32(1),
        I::I32Shl,
        set(cap),
        get(cap),
        I::I64ExtendI32U,
        c64(i64::from(ENTRY_SHIFT)),
        I::I64Shl,
        c64(i64::from(header::SIZE)),
        I::I64Add,
        I::Call(fns.alloc),
        set(fresh),
        get(fresh),
        c32(header::SIZE.cast_signed()),
        I::I32Add,
        get(stack),
        c32(header::SIZE.cast_signed()),
        I::I32Add,
        get(count),
        c32(ENTRY_SHIFT),
        I::I32Shl,
        I::MemoryCopy {
            src_mem: 0,
            dst_mem: 0,
        },
        get(stack),
        I::Call(fns.free),
        get(fresh),
        set(stack),
        get(state),
        get(stack),
        st32(STACK_AT),
        get(state),
        get(cap),
        st32(CAPACITY_AT),
    ]);
    c.close();
    c.emit(&[
        get(stack),
        c32(header::SIZE.cast_signed()),
        I::I32Add,
        get(count),
        c32(ENTRY_SHIFT),
        I::I32Shl,
        I::I32Add,
        set(entry),
        get(entry),
        get(1),
        st64(0),
        get(entry),
        get(2),
        st64(8),
        get(entry),
        get(3),
        st32(16),
        get(entry),
        get(4),
        st32(20),
        get(entry),
        get(5),
        st32(24),
        get(entry),
        get(6),
        st32(28),
        get(state),
        get(count),
        c32(1),
        I::I32Add,
        st32(COUNT_AT),
        I::End,
    ]);
    Routine {
        params: vec![
            ValType::I32,
            ValType::I64,
            ValType::I64,
            ValType::I32,
            ValType::I32,
            ValType::I32,
            ValType::I32,
        ],
        results: Vec::new(),
        locals: vec![ValType::I32; 5],
        body: c.ins,
        export: None,
    }
}

// -- instantiate --------------------------------------------------------------

/// `(table, template, env) -> i32`: the descriptor a template stands for at the
/// arguments of the descriptor `env`, or `0` for an argument that is no
/// reference. A parameter is the argument it names, borrowed from `env`; a
/// constructor is a new descriptor, with a count of one, of its head and the
/// instances of its arguments. A template is as deep as a type is written, so
/// it recurses.
pub(crate) fn inst_routine(fns: &Routines, own: u32) -> Routine {
    // Params: 0 table, 1 template (an absolute address), 2 env.
    // Locals: 3 tag, 4 x, 5 n, 6 object, 7 index, 8 child, 9 cells, 10 child template.
    let (table, template, env, tag, x, n, object, index, child, base, below) =
        (0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10);
    let mut c = Code::new();
    c.emit(&[
        get(template),
        ld32(0),
        set(tag),
        get(template),
        ld32(4),
        set(x),
        get(template),
        ld32(8),
        set(n),
        get(tag),
        I::I32Eqz,
    ]);
    c.open("parameter", I::If(BlockType::Empty));
    // An argument past the arguments, or one that is no reference, is no type.
    c.emit(&[
        get(x),
        c32(1),
        I::I32Add,
        get(env),
        ld32(header::LEN),
        I::I32GeU,
    ]);
    c.open("absent", I::If(BlockType::Empty));
    c.emit(&[c32(0), I::Return]);
    c.close();
    c.emit(&[
        get(env),
        get(x),
        I::I32Add,
        ld8(header::SIZE + 1),
        c32(REF),
        I::I32Ne,
    ]);
    c.open("scalar", I::If(BlockType::Empty));
    c.emit(&[c32(0), I::Return]);
    c.close();
    c.emit(&cells(env));
    c.emit(&[
        get(x),
        c32(3),
        I::I32Shl,
        I::I32Add,
        ld32(header::SIZE + 8),
        I::Return,
    ]);
    c.close();
    // A constructor: the descriptor of the head and the arguments.
    c.emit(&[
        c32(layout::Kind::Tuple.code().cast_signed()),
        c32(layout::STRIDE_CELLS.cast_signed()),
        get(n),
        c32(1),
        I::I32Add,
        c32(0),
        I::Call(fns.new),
        set(object),
    ]);
    c.emit(&cells(object));
    c.emit(&[
        set(base),
        get(base),
        get(x),
        I::I64ExtendI32U,
        st64(header::SIZE),
        c32(0),
        set(index),
    ]);
    c.open("done", I::Block(BlockType::Empty));
    c.open("next", I::Loop(BlockType::Empty));
    c.emit(&[get(index), get(n), I::I32GeU]);
    c.br_if("done");
    c.emit(&[
        get(table),
        get(template),
        get(index),
        c32(2),
        I::I32Shl,
        I::I32Add,
        ld32(12),
        I::I32Add,
        set(below),
        get(table),
        get(below),
        get(env),
        I::Call(own),
        set(child),
        get(child),
    ]);
    c.open("present", I::If(BlockType::Empty));
    // A parameter is borrowed, so the new descriptor takes a count of its own.
    c.emit(&[get(below), ld32(0), I::I32Eqz]);
    c.open("borrowed", I::If(BlockType::Empty));
    c.emit(&[get(child), I::Call(fns.dup)]);
    c.close();
    c.emit(&[
        get(base),
        get(index),
        c32(3),
        I::I32Shl,
        I::I32Add,
        get(child),
        I::I64ExtendI32U,
        st64(header::SIZE + 8),
        get(object),
        get(index),
        I::I32Add,
        c32(REF),
        st8(header::SIZE + 1),
    ]);
    c.close();
    c.emit(&[get(index), c32(1), I::I32Add, set(index)]);
    c.br("next");
    c.close();
    c.close();
    c.emit(&[get(object), I::End]);
    Routine {
        params: vec![ValType::I32; 3],
        results: vec![ValType::I32],
        locals: vec![ValType::I32; 8],
        body: c.ins,
        export: None,
    }
}

// -- the walk -----------------------------------------------------------------

// Params.
const A: u32 = 0;
const B: u32 = 1;
const DESC: u32 = 2;
// Locals.
const BLOCK: u32 = 3;
const TAB: u32 = 4;
const STATE: u32 = 5;
const RESULT: u32 = 6;
const SCRATCH: u32 = 7;
const ENTRY_AT: u32 = 8;
const DA: u32 = 9;
const DB: u32 = 10;
const CA: u32 = 11;
const CB: u32 = 12;
const ROW_A: u32 = 13;
const ROW_B: u32 = 14;
const KIND_A: u32 = 15;
const KIND_B: u32 = 16;
const OA: u32 = 17;
const OB: u32 = 18;
const LEN: u32 = 19;
const AT: u32 = 20;
const TA: u32 = 21;
const TB: u32 = 22;
const VA: u32 = 23;
const VB: u32 = 24;
const EA: u32 = 25;
const EB: u32 = 26;
const TPL: u32 = 28;
const TMP: u32 = 30;
const VALUE_A: u32 = 31;
const VALUE_B: u32 = 32;
const ENTRY_A: u32 = 33;
const BITS_A: u32 = 34;
const BITS_B: u32 = 35;
const I32_LOCALS: usize = 31;

/// Pushes the arguments of a call of `push` for one entry.
#[allow(clippy::too_many_arguments)]
fn push_entry(
    c: &mut Code,
    calls: Typed,
    a: &[Ins],
    b: &[Ins],
    da: &[Ins],
    db: &[Ins],
    ca: &[Ins],
    cb: &[Ins],
) {
    c.emit(&[get(STATE)]);
    for part in [a, b, da, db, ca, cb] {
        c.emit(part);
    }
    c.emit(&[I::Call(calls.push)]);
}

/// Sets `out` to the descriptor of the component at the position the
/// instructions in `pos` push, of the type the descriptor in `d` stands for,
/// whose row is `row_local`: the template instantiated at its arguments when the
/// type is declared (and linked on the scratch list, to be given back), and the
/// argument itself when it is not.
fn component(
    c: &mut Code,
    calls: Typed,
    d: u32,
    row_local: u32,
    out: u32,
    pos: &[Ins],
) {
    c.emit(&[get(row_local), ld32(4)]);
    c.open("declared", I::If(BlockType::Empty));
    c.emit(&[get(TAB), get(TAB), get(row_local), ld32(12), I::I32Add]);
    c.emit(pos);
    c.emit(&[
        c32(2),
        I::I32Shl,
        I::I32Add,
        ld32(4),
        I::I32Add,
        set(TPL),
        get(TAB),
        get(TPL),
        get(d),
        I::Call(calls.inst),
        set(out),
        // A new descriptor is linked through its reserved word; a borrowed
        // parameter is not.
        get(TPL),
        ld32(0),
        c32(1),
        I::I32Eq,
        get(out),
        c32(0),
        I::I32Ne,
        I::I32And,
    ]);
    c.open("fresh", I::If(BlockType::Empty));
    c.emit(&[
        get(out),
        get(SCRATCH),
        st32(header::SIZE - 4),
        get(out),
        set(SCRATCH),
    ]);
    c.close();
    c.otherwise();
    c.emit(&argument(d, pos));
    c.emit(&[set(out)]);
    c.close();
}

/// The cell `index` of the object whose cells are at the local `base`, as the
/// bits of a cell, and its class byte.
fn cell_bits(base: u32, index: &[Ins]) -> Vec<Ins> {
    let mut code = vec![get(base)];
    code.extend_from_slice(index);
    code.extend([c32(3), I::I32Shl, I::I32Add, ld64(header::SIZE)]);
    code
}

fn cell_class(object: u32, index: &[Ins]) -> Vec<Ins> {
    let mut code = vec![get(object)];
    code.extend_from_slice(index);
    code.extend([I::I32Add, ld8(header::SIZE)]);
    code
}

/// `(a, b, descriptor) -> i32`: `1` when the arena values `a` and `b`, both of
/// the type the descriptor says, have the same canonical encoding, and `0`
/// otherwise.
#[allow(clippy::too_many_lines)]
pub(crate) fn typed_routine(fns: &Routines, calls: Typed) -> Routine {
    let mut c = Code::new();
    let table = i64::from(calls.table_size);
    // The type table, and the state of the stack.
    c.emit(&[
        c64(i64::from(header::SIZE) + table),
        I::Call(fns.alloc),
        set(BLOCK),
        get(BLOCK),
        c32(header::SIZE.cast_signed()),
        I::I32Add,
        set(TAB),
        get(TAB),
        c32(0),
        c32(calls.table_size.cast_signed()),
        I::MemoryInit {
            mem: 0,
            data_index: calls.segment,
        },
        c64(i64::from(header::SIZE) + 12),
        I::Call(fns.alloc),
        set(STATE),
        get(STATE),
        c64(i64::from(header::SIZE) + (i64::from(FIRST_CAPACITY) << ENTRY_SHIFT)),
        I::Call(fns.alloc),
        st32(STACK_AT),
        get(STATE),
        c32(FIRST_CAPACITY),
        st32(CAPACITY_AT),
        get(STATE),
        c32(0),
        st32(COUNT_AT),
        c32(1),
        set(RESULT),
        c32(0),
        set(SCRATCH),
        get(STATE),
        get(A),
        I::I64ExtendI32U,
        get(B),
        I::I64ExtendI32U,
        get(DESC),
        get(DESC),
        c32(REF),
        c32(REF),
        I::Call(calls.push),
    ]);
    c.open("done", I::Block(BlockType::Empty));
    c.open("next", I::Loop(BlockType::Empty));
    // Pop an entry.
    c.emit(&[get(STATE), ld32(COUNT_AT), I::I32Eqz]);
    c.br_if("done");
    c.emit(&[
        get(STATE),
        ld32(COUNT_AT),
        c32(1),
        I::I32Sub,
        set(TMP),
        get(STATE),
        get(TMP),
        st32(COUNT_AT),
        get(STATE),
        ld32(STACK_AT),
        c32(header::SIZE.cast_signed()),
        I::I32Add,
        get(TMP),
        c32(ENTRY_SHIFT),
        I::I32Shl,
        I::I32Add,
        set(ENTRY_AT),
        get(ENTRY_AT),
        ld64(0),
        set(BITS_A),
        get(ENTRY_AT),
        ld64(8),
        set(BITS_B),
        get(ENTRY_AT),
        ld32(16),
        set(DA),
        get(ENTRY_AT),
        ld32(20),
        set(DB),
        get(ENTRY_AT),
        ld32(24),
        set(CA),
        get(ENTRY_AT),
        ld32(28),
        set(CB),
    ]);
    c.open("visit", I::Block(BlockType::Empty));

    // An interface value is the value it holds: open pairs until neither side
    // is one. A type nothing fixed agrees with the other side's.
    c.open("unwrap", I::Loop(BlockType::Empty));
    c.emit(&[get(DA), I::I32Eqz]);
    c.open("a-open", I::If(BlockType::Empty));
    c.emit(&[get(DB), set(DA)]);
    c.close();
    c.emit(&[get(DB), I::I32Eqz]);
    c.open("b-open", I::If(BlockType::Empty));
    c.emit(&[get(DA), set(DB)]);
    c.close();
    c.emit(&[get(DA), I::I32Eqz]);
    c.br_if("visit");
    c.emit(&row(TAB, DA));
    c.emit(&[set(ROW_A), get(ROW_A), ld32(0), set(KIND_A)]);
    c.emit(&row(TAB, DB));
    c.emit(&[set(ROW_B), get(ROW_B), ld32(0), set(KIND_B)]);
    for (kind_local, bits, descriptor, class_byte, object) in
        [(KIND_A, BITS_A, DA, CA, OA), (KIND_B, BITS_B, DB, CB, OB)]
    {
        c.emit(&[get(kind_local), c32(tk::INTERFACE.cast_signed()), I::I32Eq]);
        c.open("pair", I::If(BlockType::Empty));
        c.emit(&[
            get(bits),
            I::I32WrapI64,
            set(object),
            get(object),
            ld32(header::SIZE + 8),
            set(descriptor),
            get(object),
            ld8(header::SIZE + 1),
            set(class_byte),
            get(object),
            ld64(header::SIZE + 16),
            set(bits),
        ]);
        c.br("unwrap");
        c.close();
    }
    c.close();

    // The same encoding.
    c.emit(&[get(KIND_A), get(KIND_B), I::I32Ne]);
    c.open("kinds", I::If(BlockType::Empty));
    c.differ_at(RESULT);
    c.close();
    c.emit(&[
        get(BITS_A),
        I::I32WrapI64,
        set(OA),
        get(BITS_B),
        I::I32WrapI64,
        set(OB),
    ]);

    // Nothing to compare.
    c.emit(&[
        get(KIND_A),
        c32(tk::NONE.cast_signed()),
        I::I32Eq,
        get(KIND_A),
        c32(tk::FUNCTION.cast_signed()),
        I::I32Eq,
        I::I32Or,
        get(KIND_A),
        c32(tk::VOID.cast_signed()),
        I::I32Eq,
        I::I32Or,
    ]);
    c.br_if("visit");

    // A primitive: the same type and the same bits.
    c.emit(&[get(KIND_A), c32(tk::SCALAR.cast_signed()), I::I32Eq]);
    c.open("scalar", I::If(BlockType::Empty));
    c.emit(&head(DA));
    c.emit(&head(DB));
    c.emit(&[I::I32Ne, get(CA), get(CB), I::I32Ne, I::I32Or]);
    c.open("unlike", I::If(BlockType::Empty));
    c.differ_at(RESULT);
    c.close();
    c.emit(&[get(CA), c32(1), I::I32And, I::I32Eqz]);
    c.open("narrow", I::If(BlockType::Empty));
    c.emit(&[
        get(BITS_A),
        c64(0xFFFF_FFFF),
        I::I64And,
        set(BITS_A),
        get(BITS_B),
        c64(0xFFFF_FFFF),
        I::I64And,
        set(BITS_B),
    ]);
    c.close();
    c.emit(&[get(CA), c32(2), I::I32And]);
    c.open("float", I::If(BlockType::Empty));
    c.emit(&[get(CA), c32(1), I::I32And, I::I32Eqz]);
    c.open("single", I::If(BlockType::Empty));
    for local in [BITS_A, BITS_B] {
        c.emit(&nan_replaced(local, false));
    }
    c.otherwise();
    for local in [BITS_A, BITS_B] {
        c.emit(&nan_replaced(local, true));
    }
    c.close();
    c.close();
    c.emit(&[get(BITS_A), get(BITS_B), I::I64Ne]);
    c.open("unlike", I::If(BlockType::Empty));
    c.differ_at(RESULT);
    c.close();
    c.br("visit");
    c.close();

    // `bool`: the variant.
    c.emit(&[get(KIND_A), c32(tk::BOOL.cast_signed()), I::I32Eq]);
    c.open("bool", I::If(BlockType::Empty));
    c.emit(&[
        get(OA),
        ld32(header::VARIANT),
        get(OB),
        ld32(header::VARIANT),
        I::I32Ne,
    ]);
    c.open("unlike", I::If(BlockType::Empty));
    c.differ_at(RESULT);
    c.close();
    c.br("visit");
    c.close();

    // Text, an atom, and bytes: the payload.
    c.emit(&[
        get(KIND_A),
        c32(tk::TEXT.cast_signed()),
        I::I32Eq,
        get(KIND_A),
        c32(tk::ATOM.cast_signed()),
        I::I32Eq,
        I::I32Or,
        get(KIND_A),
        c32(tk::BYTES.cast_signed()),
        I::I32Eq,
        I::I32Or,
    ]);
    c.open("data", I::If(BlockType::Empty));
    c.emit(&[get(OA), get(OB), I::Call(fns.equal), I::I32Eqz]);
    c.open("unlike", I::If(BlockType::Empty));
    c.differ_at(RESULT);
    c.close();
    c.br("visit");
    c.close();

    // A compound: the same declared type, the same length, and, for an
    // anonymous record, the same field names, which its head says.
    c.emit(&[
        get(ROW_A),
        ld32(4),
        get(ROW_B),
        ld32(4),
        I::I32Ne,
        get(OA),
        ld32(header::LEN),
        get(OB),
        ld32(header::LEN),
        I::I32Ne,
        I::I32Or,
    ]);
    c.open("unlike", I::If(BlockType::Empty));
    c.differ_at(RESULT);
    c.close();
    c.emit(&[
        get(KIND_A),
        c32(tk::RECORD.cast_signed()),
        I::I32Eq,
        get(ROW_A),
        ld32(4),
        I::I32Eqz,
        I::I32And,
    ]);
    c.open("record", I::If(BlockType::Empty));
    c.emit(&head(DA));
    c.emit(&head(DB));
    c.emit(&[I::I32Ne]);
    c.open("unlike", I::If(BlockType::Empty));
    c.differ_at(RESULT);
    c.close();
    c.close();
    c.emit(&[
        get(OA),
        ld32(header::LEN),
        set(LEN),
        get(OA),
        ld32(header::VARIANT),
        set(VA),
        get(OB),
        ld32(header::VARIANT),
        set(VB),
    ]);
    c.emit(&cells(OA));
    c.emit(&[set(EA)]);
    c.emit(&cells(OB));
    c.emit(&[set(EB)]);

    // An enum: the variant chosen has the same name, and a payload is the one
    // component. A union: the members selected are the same type, and the value
    // is the one component. The component is at the variant's position.
    c.emit(&[
        get(KIND_A),
        c32(tk::ENUM.cast_signed()),
        I::I32Eq,
        get(KIND_A),
        c32(tk::UNION.cast_signed()),
        I::I32Eq,
        I::I32Or,
    ]);
    c.open("variants", I::If(BlockType::Empty));
    c.emit(&[get(KIND_A), c32(tk::ENUM.cast_signed()), I::I32Eq]);
    c.open("enum", I::If(BlockType::Empty));
    for (name, row_local, variant) in [(TA, ROW_A, VA), (TB, ROW_B, VB)] {
        c.emit(&[
            get(TAB),
            get(row_local),
            ld32(8),
            I::I32Add,
            get(variant),
            c32(2),
            I::I32Shl,
            I::I32Add,
            ld32(4),
            set(name),
        ]);
    }
    c.emit(&[get(TA), get(TB), I::I32Ne]);
    c.open("unlike", I::If(BlockType::Empty));
    c.differ_at(RESULT);
    c.close();
    // No payload to follow.
    c.emit(&[get(LEN), I::I32Eqz]);
    c.br_if("visit");
    c.close();
    component(&mut c, calls, DA, ROW_A, TA, &[get(VA)]);
    component(&mut c, calls, DB, ROW_B, TB, &[get(VB)]);
    c.emit(&[get(KIND_A), c32(tk::UNION.cast_signed()), I::I32Eq]);
    c.open("union", I::If(BlockType::Empty));
    c.emit(&[get(TA), get(TB), I::Call(calls.desc_equal), I::I32Eqz]);
    c.open("unlike", I::If(BlockType::Empty));
    c.differ_at(RESULT);
    c.close();
    c.close();
    push_entry(
        &mut c,
        calls,
        &[get(EA), ld64(header::SIZE)],
        &[get(EB), ld64(header::SIZE)],
        &[get(TA)],
        &[get(TB)],
        &[get(OA), ld8(header::SIZE)],
        &[get(OB), ld8(header::SIZE)],
    );
    c.br("visit");
    c.close();

    // A dict: the key and the value of each entry, which is a tuple of two.
    c.emit(&[get(KIND_A), c32(tk::DICT.cast_signed()), I::I32Eq]);
    c.open("dict", I::If(BlockType::Empty));
    component(&mut c, calls, DA, ROW_A, TA, &[c32(0)]);
    component(&mut c, calls, DB, ROW_B, TB, &[c32(0)]);
    component(&mut c, calls, DA, ROW_A, VALUE_A, &[c32(1)]);
    component(&mut c, calls, DB, ROW_B, VALUE_B, &[c32(1)]);
    c.emit(&[c32(0), set(AT)]);
    c.open("entries", I::Block(BlockType::Empty));
    c.open("entry", I::Loop(BlockType::Empty));
    c.emit(&[get(AT), get(LEN), I::I32GeU]);
    c.br_if("entries");
    // The tuple of each entry, then its two cells.
    for (base, entry_local, object) in [(EA, ENTRY_A, OA), (EB, TMP, OB)] {
        let _ = object;
        c.emit(&cell_bits(base, &[get(AT)]));
        c.emit(&[I::I32WrapI64, set(entry_local)]);
    }
    push_entry(
        &mut c,
        calls,
        &[get(ENTRY_A), ld64(header::SIZE + 8)],
        &[get(TMP), ld64(header::SIZE + 8)],
        &[get(TA)],
        &[get(TB)],
        &[get(ENTRY_A), ld8(header::SIZE)],
        &[get(TMP), ld8(header::SIZE)],
    );
    push_entry(
        &mut c,
        calls,
        &[get(ENTRY_A), ld64(header::SIZE + 16)],
        &[get(TMP), ld64(header::SIZE + 16)],
        &[get(VALUE_A)],
        &[get(VALUE_B)],
        &[get(ENTRY_A), ld8(header::SIZE + 1)],
        &[get(TMP), ld8(header::SIZE + 1)],
    );
    c.emit(&[get(AT), c32(1), I::I32Add, set(AT)]);
    c.br("entry");
    c.close();
    c.close();
    c.br("visit");
    c.close();

    // Tuples, records, wrappers, and arrays: component by component. An array's
    // elements are all of the one argument.
    c.emit(&[c32(0), set(AT)]);
    c.emit(&[get(KIND_A), c32(tk::ARRAY.cast_signed()), I::I32Eq]);
    c.open("array", I::If(BlockType::Empty));
    component(&mut c, calls, DA, ROW_A, TA, &[c32(0)]);
    component(&mut c, calls, DB, ROW_B, TB, &[c32(0)]);
    c.close();
    c.open("elements", I::Block(BlockType::Empty));
    c.open("element", I::Loop(BlockType::Empty));
    c.emit(&[get(AT), get(LEN), I::I32GeU]);
    c.br_if("elements");
    c.emit(&[get(KIND_A), c32(tk::ARRAY.cast_signed()), I::I32Ne]);
    c.open("positional", I::If(BlockType::Empty));
    component(&mut c, calls, DA, ROW_A, TA, &[get(AT)]);
    component(&mut c, calls, DB, ROW_B, TB, &[get(AT)]);
    c.close();
    push_entry(
        &mut c,
        calls,
        &cell_bits(EA, &[get(AT)]),
        &cell_bits(EB, &[get(AT)]),
        &[get(TA)],
        &[get(TB)],
        &cell_class(OA, &[get(AT)]),
        &cell_class(OB, &[get(AT)]),
    );
    c.emit(&[get(AT), c32(1), I::I32Add, set(AT)]);
    c.br("element");
    c.close();
    c.close();

    // Back to the next entry.
    c.close();
    c.br("next");
    c.close();
    c.close();

    // Give back what the walk allocated: the descriptors it instantiated, the
    // stack, the state, and the table.
    c.open("release", I::Block(BlockType::Empty));
    c.open("each", I::Loop(BlockType::Empty));
    c.emit(&[get(SCRATCH), I::I32Eqz]);
    c.br_if("release");
    c.emit(&[
        get(SCRATCH),
        ld32(header::SIZE - 4),
        set(TMP),
        get(SCRATCH),
        c32(0),
        st32(header::SIZE - 4),
        get(SCRATCH),
        I::Call(fns.drop),
        get(TMP),
        set(SCRATCH),
    ]);
    c.br("each");
    c.close();
    c.close();
    c.emit(&[
        get(STATE),
        ld32(STACK_AT),
        I::Call(fns.free),
        get(STATE),
        I::Call(fns.free),
        get(BLOCK),
        I::Call(fns.free),
        get(RESULT),
        I::End,
    ]);
    let mut locals = vec![ValType::I32; I32_LOCALS];
    locals.extend([ValType::I64, ValType::I64]);
    Routine {
        params: vec![ValType::I32; 3],
        results: vec![ValType::I32],
        locals,
        body: c.ins,
        export: None,
    }
}
