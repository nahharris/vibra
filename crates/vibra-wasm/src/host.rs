//! The builders of the host boundary and the routine that adopts a result.
//!
//! `docs/spec/06-runtime.md`, "Host calls and the builders": a module that
//! imports a `vibra_v1` operation also exports seven builders, with which the
//! host builds a compound value in place, starting unsealed, writing each slot
//! once it knows what it holds, and sealing it. The host never receives a
//! pointer. Every misuse is the trap `@runtime.invalid-host-value`: a write to
//! a sealed value, an index outside the value, a class the kind does not hold,
//! a byte outside `0..255`, a `char` that is not a Unicode scalar, a child that
//! is not sealed, a seal with a slot unwritten, a kind the builders do not
//! build, or an ID that is not valid.
//!
//! An unsealed value is an ordinary arena object with the header word
//! `unsealed` set to `1` (`vibra_ir::arena::header::UNSEALED`), so every value
//! a module builds itself is sealed by construction. An unsealed value also
//! carries a **written map** after its payload, one bit for each component,
//! which the writers set and the seal checks; the map is part of the same
//! block, so releasing the value releases it.

use wasm_encoder::{BlockType, Instruction as I, ValType};

use crate::layout::{self, CellClass, header};
use crate::runtime::{
    EMPTY, I32, I64, Ins, Routine, Routines, c32, c64, get, invalid_host_value, ld8,
    ld32, make, push, set, st8, st32, st64,
};
use vibra_ir::boundary::{
    NEW_EXPORT, SEAL_EXPORT, WRITE_F32_EXPORT, WRITE_F64_EXPORT, WRITE_I32_EXPORT,
    WRITE_I64_EXPORT, WRITE_ID_EXPORT,
};

/// The host routines after the object constructor, in this order: the seven
/// builders and the adoption of a result.
pub(crate) const HOST_ROUTINES: u32 = 8;

/// The position of the adoption among the host routines.
pub(crate) const HOST_ADOPT: u32 = 7;

/// The kinds `vibra_v1_new` builds: every kind but `dict` and `function`.
const BUILDABLE: u32 = 0b11_1101_1111;

/// The largest component count a builder accepts before it is the memory host
/// event: no block holds more.
const MAX_LEN: i64 = 1 << 28;

const KIND_ENUM: i32 = 7;
const KIND_WRAPPER: i32 = 8;
const KIND_UNION: i32 = 9;

fn trap(fns: &Routines) -> [Ins; 2] {
    invalid_host_value(fns)
}

/// Traps when the stack top is true.
fn trap_if(fns: &Routines, condition: &[Ins]) -> Vec<Ins> {
    let mut code = condition.to_vec();
    code.push(I::If(EMPTY));
    code.extend(trap(fns));
    code.push(I::End);
    code
}

/// Pushes the bytes of class bytes an object of `object` has: its length
/// rounded up to 8 for the cells layout, and none otherwise.
fn classes_of(object: u32) -> Vec<Ins> {
    vec![
        get(object),
        ld8(header::STRIDE),
        c32(layout::STRIDE_CELLS.cast_signed()),
        I::I32Eq,
        get(object),
        ld32(header::LEN),
        c32(7),
        I32_ADD,
        c32(-8),
        I::I32And,
        I::I32Mul,
    ]
}

const I32_ADD: Ins = I::I32Add;

/// Pushes the address of the written map of the object in `object`: just past
/// its payload.
fn written_map(object: u32) -> Vec<Ins> {
    let mut code = vec![get(object), c32(header::SIZE.cast_signed()), I::I32Add];
    code.extend(classes_of(object));
    code.extend([
        I::I32Add,
        get(object),
        ld32(header::LEN),
        get(object),
        ld8(header::STRIDE),
        I::I32Mul,
        I::I32Add,
    ]);
    code
}

/// The routines, in index order from `fns.new + 1`.
pub(crate) fn host_routines(fns: &Routines) -> Vec<Routine> {
    let mut list = vec![
        new_builder(fns),
        write_scalar(fns, Write::I32),
        write_scalar(fns, Write::I64),
        write_scalar(fns, Write::F32),
        write_scalar(fns, Write::F64),
        write_id(fns),
        seal(fns),
        crate::runtime::adopt(fns),
    ];
    let exports = [
        Some(NEW_EXPORT),
        Some(WRITE_I32_EXPORT),
        Some(WRITE_I64_EXPORT),
        Some(WRITE_F32_EXPORT),
        Some(WRITE_F64_EXPORT),
        Some(WRITE_ID_EXPORT),
        Some(SEAL_EXPORT),
        None,
    ];
    for (routine, export) in list.iter_mut().zip(exports) {
        routine.export = export;
    }
    list
}

/// `(kind, len: i64, variant) -> i64`: a new unsealed value, as an ID the host
/// holds.
fn new_builder(fns: &Routines) -> Routine {
    // Params: 0 kind, 1 len (i64), 2 variant. Locals: 3 stride, 4 object,
    // 5 total (i64), 6 padded (i64), 7 id (i64).
    let (stride, object, total, padded, id) = (3, 4, 5, 6, 7);
    let mut code = vec![I::Call(fns.begin)];
    code.extend(trap_if(fns, &[get(0), c32(9), I::I32GtU]));
    code.extend(trap_if(
        fns,
        &[
            c32(BUILDABLE.cast_signed()),
            get(0),
            I::I32ShrU,
            c32(1),
            I::I32And,
            I::I32Eqz,
        ],
    ));
    // stride: 4 for atom and str, 1 for bytes, 8 for the rest.
    push(
        &mut code,
        &[
            get(0),
            c32(1),
            I::I32LeU,
            I::If(BlockType::Result(ValType::I32)),
            c32(layout::STRIDE_CHARS.cast_signed()),
            I::Else,
            get(0),
            c32(2),
            I::I32Eq,
            I::If(BlockType::Result(ValType::I32)),
            c32(layout::STRIDE_BYTES.cast_signed()),
            I::Else,
            c32(layout::STRIDE_CELLS.cast_signed()),
            I::End,
            I::End,
            set(stride),
        ],
    );
    // No block holds more than this many components: the memory host event.
    push(
        &mut code,
        &[
            get(1),
            c64(MAX_LEN),
            I::I64GtU,
            I::If(EMPTY),
            I::Call(fns.stop_memory),
            I::End,
        ],
    );
    // An enum holds `0` or `1` components, and a wrapper or a union exactly `1`.
    code.extend(trap_if(
        fns,
        &[
            get(0),
            c32(KIND_ENUM),
            I::I32Eq,
            get(1),
            c64(1),
            I::I64GtU,
            I::I32And,
        ],
    ));
    code.extend(trap_if(
        fns,
        &[
            get(0),
            c32(KIND_WRAPPER),
            I::I32GeU,
            get(1),
            c64(1),
            I::I64Ne,
            I::I32And,
        ],
    ));
    // Only an enum and a union have a variant.
    code.extend(trap_if(
        fns,
        &[
            get(2),
            c32(0),
            I::I32Ne,
            get(0),
            c32(KIND_ENUM),
            I::I32Ne,
            get(0),
            c32(KIND_UNION),
            I::I32Ne,
            I::I32And,
            I::I32And,
        ],
    ));
    push(
        &mut code,
        &[
            // padded = (len + 7) & !7
            get(1),
            c64(7),
            I::I64Add,
            c64(-8),
            I::I64And,
            set(padded),
            // total = header + classes + len * stride + the written map
            c64(i64::from(header::SIZE)),
            get(stride),
            c32(layout::STRIDE_CELLS.cast_signed()),
            I::I32Eq,
            I::If(BlockType::Result(ValType::I64)),
            get(padded),
            I::Else,
            c64(0),
            I::End,
            I::I64Add,
            get(1),
            get(stride),
            I::I64ExtendI32U,
            I::I64Mul,
            I::I64Add,
            get(padded),
            c64(3),
            I::I64ShrU,
            I::I64Add,
            set(total),
            get(total),
            I::Call(fns.alloc),
            set(object),
            get(object),
            get(0),
            st8(header::KIND),
            get(object),
            get(stride),
            st8(header::STRIDE),
            get(object),
            get(1),
            I::I32WrapI64,
            st32(header::LEN),
            get(object),
            get(2),
            st32(header::VARIANT),
            get(object),
            c32(1),
            st32(header::UNSEALED),
            // The class bytes start as scalars, and nothing is written yet.
            get(object),
            c32(header::SIZE.cast_signed()),
            I::I32Add,
            c32(0),
        ],
    );
    code.extend(classes_of(object));
    code.push(I::MemoryFill(0));
    code.extend(written_map(object));
    code.extend([
        c32(0),
        get(padded),
        c64(3),
        I::I64ShrU,
        I::I32WrapI64,
        I::MemoryFill(0),
        // The table's entry holds the one count; the constructor's is dropped.
        get(object),
        I::Call(fns.new_id),
        set(id),
        get(object),
        I::Call(fns.drop),
        get(id),
        I::End,
    ]);
    make(&[I32, I64, I32], &[I64], &[I32, I32, I64, I64, I64], code)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Write {
    I32,
    I64,
    F32,
    F64,
}

/// The checks every writer makes first: the ID names a live, unsealed value,
/// and the index is inside it. Leaves the object in local 3 and the position in
/// local 4.
fn writer_prefix(fns: &Routines) -> Vec<Ins> {
    let mut code = vec![
        I::Call(fns.begin),
        get(0),
        I::Call(fns.entry_of),
        ld32(8),
        set(3),
    ];
    code.extend(trap_if(fns, &[get(3), ld32(header::UNSEALED), I::I32Eqz]));
    code.extend(trap_if(
        fns,
        &[
            get(1),
            get(3),
            ld32(header::LEN),
            I::I64ExtendI32U,
            I::I64GeU,
        ],
    ));
    code.extend([get(1), I::I32WrapI64, set(4)]);
    code
}

/// Records that the component in local 4 of the object in local 3 is written.
/// Uses local 5.
fn mark_written() -> Vec<Ins> {
    let mut code = written_map(3);
    code.extend([
        get(4),
        c32(3),
        I::I32ShrU,
        I::I32Add,
        set(5),
        get(5),
        get(5),
        ld8(0),
        c32(1),
        get(4),
        c32(7),
        I::I32And,
        I::I32Shl,
        I::I32Or,
        st8(0),
    ]);
    code
}

/// Stores a scalar of `class` into the cell at local 4 of the object in local
/// 3, dropping the child the cell replaced. `bits` pushes the 64-bit cell.
/// Uses locals 5 and 6.
fn write_cell(fns: &Routines, class: CellClass, bits: &[Ins]) -> Vec<Ins> {
    let mut code = vec![
        // The cell: object + header + padded class bytes + position * 8.
        get(3),
        c32(header::SIZE.cast_signed()),
        I::I32Add,
        get(3),
        ld32(header::LEN),
        c32(7),
        I::I32Add,
        c32(-8),
        I::I32And,
        I::I32Add,
        get(4),
        c32(3),
        I::I32Shl,
        I::I32Add,
        set(5),
        // The class byte it replaces: a reference is dropped.
        get(3),
        c32(header::SIZE.cast_signed()),
        I::I32Add,
        get(4),
        I::I32Add,
        ld8(0),
        c32(i32::from(CellClass::Ref.code())),
        I::I32Eq,
        I::If(EMPTY),
        get(5),
        ld32(0),
        I::Call(fns.drop),
        I::End,
        get(3),
        c32(header::SIZE.cast_signed()),
        I::I32Add,
        get(4),
        I::I32Add,
        c32(i32::from(class.code())),
        st8(0),
        get(5),
    ];
    code.extend_from_slice(bits);
    code.push(st64(0));
    code
}

/// `(id, index, value)`: writes a scalar of one width.
fn write_scalar(fns: &Routines, write: Write) -> Routine {
    let mut code = writer_prefix(fns);
    let (class, bits, value): (CellClass, Vec<Ins>, ValType) = match write {
        Write::I32 => (CellClass::I32, vec![get(2), I::I64ExtendI32U], ValType::I32),
        Write::I64 => (CellClass::I64, vec![get(2)], ValType::I64),
        Write::F32 => (
            CellClass::F32,
            vec![get(2), I::I32ReinterpretF32, I::I64ExtendI32U],
            ValType::F32,
        ),
        Write::F64 => (
            CellClass::F64,
            vec![get(2), I::I64ReinterpretF64],
            ValType::F64,
        ),
    };
    if write == Write::I32 {
        // A character, a byte, or an `i32`-slot scalar, by the layout.
        code.extend([
            get(3),
            ld8(header::STRIDE),
            c32(layout::STRIDE_CHARS.cast_signed()),
            I::I32Eq,
            I::If(EMPTY),
        ]);
        // A Unicode scalar: at most U+10FFFF and not a surrogate.
        code.extend(trap_if(
            fns,
            &[
                get(2),
                c32(0x0010_FFFF),
                I::I32GtU,
                get(2),
                c32(0xD800),
                I::I32Sub,
                c32(0x800),
                I::I32LtU,
                I::I32Or,
            ],
        ));
        code.extend([
            get(3),
            c32(header::SIZE.cast_signed()),
            I::I32Add,
            get(4),
            c32(2),
            I::I32Shl,
            I::I32Add,
            get(2),
            st32(0),
            I::Else,
            get(3),
            ld8(header::STRIDE),
            c32(layout::STRIDE_BYTES.cast_signed()),
            I::I32Eq,
            I::If(EMPTY),
        ]);
        code.extend(trap_if(fns, &[get(2), c32(255), I::I32GtU]));
        code.extend([
            get(3),
            c32(header::SIZE.cast_signed()),
            I::I32Add,
            get(4),
            I::I32Add,
            get(2),
            st8(0),
            I::Else,
        ]);
        code.extend(write_cell(fns, class, &bits));
        code.extend([I::End, I::End]);
    } else {
        // Only the cells layout holds a scalar of a width other than `i32`.
        code.extend(trap_if(
            fns,
            &[
                get(3),
                ld8(header::STRIDE),
                c32(layout::STRIDE_CELLS.cast_signed()),
                I::I32Ne,
            ],
        ));
        code.extend(write_cell(fns, class, &bits));
    }
    code.extend(mark_written());
    code.push(I::End);
    make(&[I64, I64, value], &[], &[I32, I32, I32, I32], code)
}

/// `(id, index, child)`: writes the value of a sealed ID, taking a reference.
fn write_id(fns: &Routines) -> Routine {
    // Locals: 3 object, 4 position, 5 address, 6 scratch, 7 child.
    let mut code = writer_prefix(fns);
    code.extend(trap_if(
        fns,
        &[
            get(3),
            ld8(header::STRIDE),
            c32(layout::STRIDE_CELLS.cast_signed()),
            I::I32Ne,
        ],
    ));
    code.extend([get(2), I::Call(fns.entry_of), ld32(8), set(7)]);
    code.extend(trap_if(fns, &[get(7), ld32(header::UNSEALED)]));
    // The count is taken before the replaced child is dropped, so writing the
    // same child again changes nothing.
    code.extend([get(7), I::Call(fns.dup)]);
    code.extend(write_cell(fns, CellClass::Ref, &[get(7), I::I64ExtendI32U]));
    code.extend(mark_written());
    code.push(I::End);
    make(&[I64, I64, I64], &[], &[I32, I32, I32, I32, I32], code)
}

/// `(id)`: seals a value whose every slot is written.
fn seal(fns: &Routines) -> Routine {
    // Locals: 1 object, 2 position, 3 length, 4 written map.
    let (object, position, length, map) = (1, 2, 3, 4);
    let mut code = vec![
        I::Call(fns.begin),
        get(0),
        I::Call(fns.entry_of),
        ld32(8),
        set(object),
    ];
    code.extend(trap_if(
        fns,
        &[get(object), ld32(header::UNSEALED), I::I32Eqz],
    ));
    code.extend([get(object), ld32(header::LEN), set(length)]);
    code.extend(written_map(object));
    code.extend([
        set(map),
        c32(0),
        set(position),
        I::Block(EMPTY),
        I::Loop(EMPTY),
    ]);
    code.extend([get(position), get(length), I::I32GeU, I::BrIf(1)]);
    code.extend(trap_if(
        fns,
        &[
            get(map),
            get(position),
            c32(3),
            I::I32ShrU,
            I::I32Add,
            ld8(0),
            get(position),
            c32(7),
            I::I32And,
            I::I32ShrU,
            c32(1),
            I::I32And,
            I::I32Eqz,
        ],
    ));
    code.extend([
        get(position),
        c32(1),
        I::I32Add,
        set(position),
        I::Br(0),
        I::End,
        I::End,
        get(object),
        c32(0),
        st32(header::UNSEALED),
        I::End,
    ]);
    make(&[I64], &[], &[I32, I32, I32, I32], code)
}
