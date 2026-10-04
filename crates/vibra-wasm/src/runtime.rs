//! The runtime support routines every v1 module carries: the allocator, the
//! reference counts, the worklist release, the handle table, and the exported
//! accessors.
//!
//! The routines are built once, in a fixed order, from the layout of
//! [`crate::layout`], so emission stays deterministic and a routine a program
//! does not use is left out. A routine never names a value kind: it reads the
//! header, the stride, and the masks of the kind table. The release routine
//! walks an explicit worklist threaded through the dying blocks themselves, so
//! the engine stack stays bounded however deeply a value is nested and the
//! worklist needs no storage of its own.

use wasm_encoder::{BlockType, Instruction as I, MemArg, ValType};

use crate::layout::{
    self, CellClass, MAX_CLASS, MIN_CLASS, TABLE_ENTRY_SIZE, TABLE_FIRST_CAPACITY,
    header, state,
};
use vibra_ir::boundary::{
    LENGTH_EXPORT, LIVE_SIZE_EXPORT, ORIGIN_EXPORT, READ_F32_EXPORT, READ_F64_EXPORT,
    READ_I32_EXPORT, READ_I64_EXPORT, READ_ID_EXPORT, RELEASE_EXPORT, RESULT_EXPORT,
    STATUS_EXPORT, TRAP_CODE_EXPORT, TrapCode, VARIANT_EXPORT,
};

/// An instruction with no borrowed data.
pub(crate) type Ins = I<'static>;

/// What a value leaves on the operand stack: nothing for `void`, one number for
/// a scalar, and an arena offset for a reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueClass {
    /// `void`: no slot.
    Void,
    /// A 32-bit integer slot.
    I32,
    /// A 64-bit integer slot.
    I64,
    /// A binary32 slot.
    F32,
    /// A binary64 slot.
    F64,
    /// An offset of an arena value, which the holder owns one count of.
    Ref,
}

impl ValueClass {
    /// The WebAssembly type of the slot, `None` for `void`.
    #[must_use]
    pub const fn val_type(self) -> Option<ValType> {
        match self {
            Self::Void => None,
            Self::I32 | Self::Ref => Some(ValType::I32),
            Self::I64 => Some(ValType::I64),
            Self::F32 => Some(ValType::F32),
            Self::F64 => Some(ValType::F64),
        }
    }
}

/// The index of every routine, in the fixed order they are emitted.
#[derive(Clone, Copy, Debug)]
pub struct Routines {
    /// `(trap code) -> ()`: records the trap and stops.
    pub stop_trap: u32,
    /// `() -> ()`: records the memory host event and stops.
    pub stop_memory: u32,
    /// `() -> ()`: clears the record at the start of a stoppable call.
    pub begin: u32,
    /// `(bytes: i64) -> i32`: allocates a block.
    pub alloc: u32,
    /// `(block) -> ()`: frees a block.
    pub free: u32,
    /// `(object) -> ()`: takes a reference.
    pub dup: u32,
    /// `(object) -> ()`: drops a reference, releasing at zero.
    pub drop: u32,
    /// `(object) -> ()`: releases an object whose count reached zero.
    pub release: u32,
    /// `(object) -> i64`: issues an ID holding a new reference.
    pub new_id: u32,
    /// `(id) -> i32`: the handle-table entry of a valid ID, or the trap.
    pub entry_of: u32,
    /// The first of the exported accessors, in the order they are exported.
    pub first_accessor: u32,
    /// `() -> ()`: the `vibra_v1_entry` export.
    pub entry_export: u32,
    /// `(kind, stride, len, variant) -> i32`: allocates and initializes an
    /// object, with a count of one. It is the last routine and is present only
    /// when the module builds an object.
    pub new: u32,
}

/// The accessors in the order they are emitted and exported.
const ACCESSORS: [Accessor; 13] = [
    Accessor::Status,
    Accessor::TrapCode,
    Accessor::Origin,
    Accessor::Result,
    Accessor::LiveSize,
    Accessor::Release,
    Accessor::Variant,
    Accessor::Length,
    Accessor::Read(Read::I32),
    Accessor::Read(Read::I64),
    Accessor::Read(Read::F32),
    Accessor::Read(Read::F64),
    Accessor::Read(Read::Id),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Accessor {
    Status,
    TrapCode,
    Origin,
    Result,
    LiveSize,
    Release,
    Variant,
    Length,
    Read(Read),
}

/// The five ways a component is read: four scalars and a reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Read {
    I32,
    I64,
    F32,
    F64,
    Id,
}

impl Accessor {
    const fn export(self) -> &'static str {
        match self {
            Self::Status => STATUS_EXPORT,
            Self::TrapCode => TRAP_CODE_EXPORT,
            Self::Origin => ORIGIN_EXPORT,
            Self::Result => RESULT_EXPORT,
            Self::LiveSize => LIVE_SIZE_EXPORT,
            Self::Release => RELEASE_EXPORT,
            Self::Variant => VARIANT_EXPORT,
            Self::Length => LENGTH_EXPORT,
            Self::Read(Read::I32) => READ_I32_EXPORT,
            Self::Read(Read::I64) => READ_I64_EXPORT,
            Self::Read(Read::F32) => READ_F32_EXPORT,
            Self::Read(Read::F64) => READ_F64_EXPORT,
            Self::Read(Read::Id) => READ_ID_EXPORT,
        }
    }
}

/// The routines before the accessors.
const CORE: u32 = 10;

impl Routines {
    /// Assigns indices from `base`: the core routines, the accessors, the entry
    /// export, and last the object constructor, which a module includes only
    /// when it builds an object.
    pub(crate) const fn plan(base: u32) -> Self {
        let first_accessor = base + CORE;
        let accessors = ACCESSORS.len() as u32;
        Self {
            stop_trap: base,
            stop_memory: base + 1,
            begin: base + 2,
            alloc: base + 3,
            free: base + 4,
            dup: base + 5,
            drop: base + 6,
            release: base + 7,
            new_id: base + 8,
            entry_of: base + 9,
            first_accessor,
            entry_export: first_accessor + accessors,
            new: first_accessor + accessors + 1,
        }
    }
}

/// One routine: its signature, locals, body, and the name it is exported as.
#[derive(Debug)]
pub(crate) struct Routine {
    pub(crate) params: Vec<ValType>,
    pub(crate) results: Vec<ValType>,
    pub(crate) locals: Vec<ValType>,
    pub(crate) body: Vec<Ins>,
    pub(crate) export: Option<&'static str>,
}

/// The core routines and the accessors, in index order from
/// [`Routines::plan`]'s base.
pub(crate) fn core_routines(fns: &Routines) -> Vec<Routine> {
    let mut list = vec![
        stop_trap(),
        stop_memory(),
        begin(),
        alloc(fns),
        free(),
        dup(),
        drop_routine(fns),
        release(fns),
        new_id(fns),
        entry_of(fns),
    ];
    for accessor in ACCESSORS {
        list.push(accessor_routine(fns, accessor));
    }
    list
}

/// The exported accessors, in order, with the routine index of each.
pub(crate) fn accessor_exports(fns: &Routines) -> Vec<(&'static str, u32)> {
    ACCESSORS
        .iter()
        .zip(fns.first_accessor..)
        .map(|(accessor, index)| (accessor.export(), index))
        .collect()
}

// -- instruction helpers ----------------------------------------------------

const fn mem(offset: u32, align: u32) -> MemArg {
    MemArg {
        offset: offset as u64,
        align,
        memory_index: 0,
    }
}

const fn c32(value: i32) -> Ins {
    I::I32Const(value)
}

const fn c64(value: i64) -> Ins {
    I::I64Const(value)
}

const fn get(local: u32) -> Ins {
    I::LocalGet(local)
}

const fn set(local: u32) -> Ins {
    I::LocalSet(local)
}

const fn tee(local: u32) -> Ins {
    I::LocalTee(local)
}

const fn ld32(offset: u32) -> Ins {
    I::I32Load(mem(offset, 2))
}

const fn st32(offset: u32) -> Ins {
    I::I32Store(mem(offset, 2))
}

const fn ld64(offset: u32) -> Ins {
    I::I64Load(mem(offset, 3))
}

const fn st64(offset: u32) -> Ins {
    I::I64Store(mem(offset, 3))
}

const fn ld8(offset: u32) -> Ins {
    I::I32Load8U(mem(offset, 0))
}

const fn st8(offset: u32) -> Ins {
    I::I32Store8(mem(offset, 0))
}

/// A signed constant for an unsigned layout offset or size.
const fn index(value: u32) -> i32 {
    value.cast_signed()
}

const EMPTY: BlockType = BlockType::Empty;

/// `[0]` as the base address of a state field.
const ZERO: Ins = c32(0);

/// Instructions that push the value of a 32-bit state field.
fn load_state32(address: u32) -> [Ins; 2] {
    [ZERO, ld32(address)]
}

/// Instructions that push the value of a 64-bit state field.
fn load_state64(address: u32) -> [Ins; 2] {
    [ZERO, ld64(address)]
}

fn make(
    params: &[ValType],
    results: &[ValType],
    locals: &[ValType],
    body: Vec<Ins>,
) -> Routine {
    Routine {
        params: params.to_vec(),
        results: results.to_vec(),
        locals: locals.to_vec(),
        body,
        export: None,
    }
}

const I32: ValType = ValType::I32;
const I64: ValType = ValType::I64;

/// Appends `parts` to `code`.
fn push(code: &mut Vec<Ins>, parts: &[Ins]) {
    code.extend_from_slice(parts);
}

/// The trap `@runtime.invalid-host-value`, as the instructions that raise it.
fn invalid_host_value(fns: &Routines) -> [Ins; 2] {
    [
        c32(TrapCode::InvalidHostValue.code()),
        I::Call(fns.stop_trap),
    ]
}

// -- the routines -----------------------------------------------------------

/// `(code) -> ()`: records a trap with no origin and stops.
fn stop_trap() -> Routine {
    let mut code = Vec::new();
    push(&mut code, &[ZERO, c32(1), st32(state::STATUS)]);
    push(&mut code, &[ZERO, get(0), st32(state::TRAP_CODE)]);
    push(&mut code, &[ZERO, c32(0), st32(state::ORIGIN)]);
    code.push(I::Unreachable);
    code.push(I::End);
    make(&[I32], &[], &[], code)
}

/// `() -> ()`: records the memory host event, which is not a trap, and stops.
fn stop_memory() -> Routine {
    let mut code = Vec::new();
    push(&mut code, &[ZERO, c32(2), st32(state::STATUS)]);
    code.push(I::Unreachable);
    code.push(I::End);
    make(&[], &[], &[], code)
}

/// `() -> ()`: clears the record, so the record is what the last call made.
fn begin() -> Routine {
    let mut code = Vec::new();
    for address in [state::STATUS, state::TRAP_CODE, state::ORIGIN] {
        push(&mut code, &[ZERO, c32(0), st32(address)]);
    }
    code.push(I::End);
    make(&[], &[], &[], code)
}

/// `(bytes: i64) -> i32`: the block of the smallest size class that holds
/// `bytes`, from its free list or from fresh bytes, with a count of one.
fn alloc(fns: &Routines) -> Routine {
    // Locals: 1 class, 2 block, 3 bytes (i64), 4 end (i64), 5 have (i64),
    // 6 list offset.
    let (class, block, bytes, end, have, list) = (1, 2, 3, 4, 5, 6);
    let mut code = Vec::new();
    // No class holds more than 2^30 bytes.
    push(
        &mut code,
        &[
            get(0),
            c64(1 << MAX_CLASS),
            I::I64GtU,
            I::If(EMPTY),
            I::Call(fns.stop_memory),
            I::End,
        ],
    );
    // class = max(MIN_CLASS, 64 - clz(size - 1))
    push(
        &mut code,
        &[
            c32(64),
            get(0),
            c64(1),
            I::I64Sub,
            I::I64Clz,
            I::I32WrapI64,
            I::I32Sub,
            set(class),
            get(class),
            c32(index(MIN_CLASS)),
            I::I32LtU,
            I::If(EMPTY),
            c32(index(MIN_CLASS)),
            set(class),
            I::End,
        ],
    );
    // bytes = 1 << class, and the list head's offset is class * 4.
    push(
        &mut code,
        &[
            c64(1),
            get(class),
            I::I64ExtendI32U,
            I::I64Shl,
            set(bytes),
            get(class),
            c32(2),
            I::I32Shl,
            set(list),
            get(list),
            ld32(state::FREE_LISTS),
            set(block),
            get(block),
            I::If(EMPTY),
            // A block of the class is free: unlink it.
            get(list),
            get(block),
            ld32(0),
            st32(state::FREE_LISTS),
            I::Else,
        ],
    );
    // Fresh bytes at the bump pointer, growing the memory when they pass its end.
    push(&mut code, &[c64(i64::from(layout::ARENA_START))]);
    push(&mut code, &load_state32(state::ARENA_USED));
    push(
        &mut code,
        &[
            I::I64ExtendI32U,
            I::I64Add,
            get(bytes),
            I::I64Add,
            set(end),
            I::MemorySize(0),
            I::I64ExtendI32U,
            c64(16),
            I::I64Shl,
            set(have),
            get(end),
            get(have),
            I::I64GtU,
            I::If(EMPTY),
            get(end),
            c64(1 << 32),
            I::I64GtU,
            I::If(EMPTY),
            I::Call(fns.stop_memory),
            I::End,
            // pages = ceil((end - have) / 65536)
            get(end),
            get(have),
            I::I64Sub,
            c64(65535),
            I::I64Add,
            c64(16),
            I::I64ShrU,
            I::I32WrapI64,
            I::MemoryGrow(0),
            c32(-1),
            I::I32Eq,
            I::If(EMPTY),
            I::Call(fns.stop_memory),
            I::End,
            I::End,
        ],
    );
    push(&mut code, &load_state32(state::ARENA_USED));
    push(
        &mut code,
        &[c32(index(layout::ARENA_START)), I::I32Add, set(block)],
    );
    push(&mut code, &[ZERO]);
    push(&mut code, &load_state32(state::ARENA_USED));
    push(
        &mut code,
        &[
            get(bytes),
            I::I32WrapI64,
            I::I32Add,
            st32(state::ARENA_USED),
        ],
    );
    code.push(I::End);
    // live_size += bytes; the header's count and size.
    push(&mut code, &[ZERO]);
    push(&mut code, &load_state64(state::LIVE_SIZE));
    push(&mut code, &[get(bytes), I::I64Add, st64(state::LIVE_SIZE)]);
    push(
        &mut code,
        &[
            get(block),
            c32(1),
            st32(header::COUNT),
            get(block),
            get(bytes),
            I::I32WrapI64,
            st32(header::BLOCK_SIZE),
            get(block),
            I::End,
        ],
    );
    make(&[I64], &[I32], &[I32, I32, I64, I64, I64, I32], code)
}

/// `(block) -> ()`: returns a block to the free list of its class.
fn free() -> Routine {
    // Local 1: the list head's offset.
    let list = 1;
    let mut code = Vec::new();
    push(
        &mut code,
        &[
            get(0),
            ld32(header::BLOCK_SIZE),
            I::I32Ctz,
            c32(2),
            I::I32Shl,
            set(list),
            // *block = head; head = block
            get(0),
            get(list),
            ld32(state::FREE_LISTS),
            st32(0),
            get(list),
            get(0),
            st32(state::FREE_LISTS),
        ],
    );
    // live_size -= size
    push(&mut code, &[ZERO]);
    push(&mut code, &load_state64(state::LIVE_SIZE));
    push(
        &mut code,
        &[
            get(0),
            ld32(header::BLOCK_SIZE),
            I::I64ExtendI32U,
            I::I64Sub,
            st64(state::LIVE_SIZE),
            I::End,
        ],
    );
    make(&[I32], &[], &[I32], code)
}

/// `(kind, stride, len, variant) -> i32`: an object with a count of one, its
/// header written, and, for the cells layout, every class byte a scalar.
pub(crate) fn new_object(fns: &Routines) -> Routine {
    // Params: 0 kind, 1 stride, 2 len, 3 variant. Locals: 4 object,
    // 5 padded class bytes (i64), 6 class bytes (i64), 7 total (i64).
    let (object, padded, classes, total) = (4, 5, 6, 7);
    let mut code = Vec::new();
    push(
        &mut code,
        &[
            // padded = (len + 7) & !7
            get(2),
            I::I64ExtendI32U,
            c64(7),
            I::I64Add,
            c64(-8),
            I::I64And,
            set(padded),
            // classes = stride == 8 ? padded : 0
            get(1),
            c32(index(layout::STRIDE_CELLS)),
            I::I32Eq,
            I::If(BlockType::Result(I64)),
            get(padded),
            I::Else,
            c64(0),
            I::End,
            set(classes),
            // total = header + classes + len * stride
            c64(i64::from(header::SIZE)),
            get(classes),
            I::I64Add,
            get(2),
            I::I64ExtendI32U,
            get(1),
            I::I64ExtendI32U,
            I::I64Mul,
            I::I64Add,
            set(total),
            get(total),
            I::Call(fns.alloc),
            set(object),
            get(object),
            get(0),
            st8(header::KIND),
            get(object),
            get(1),
            st8(header::STRIDE),
            get(object),
            get(2),
            st32(header::LEN),
            get(object),
            get(3),
            st32(header::VARIANT),
            get(object),
            c32(0),
            st32(20),
            // The class bytes start as scalars, so an unwritten cell is no
            // reference.
            get(object),
            c32(index(header::SIZE)),
            I::I32Add,
            c32(0),
            get(classes),
            I::I32WrapI64,
            I::MemoryFill(0),
            get(object),
            I::End,
        ],
    );
    make(&[I32, I32, I32, I32], &[I32], &[I32, I64, I64, I64], code)
}

/// `(object) -> ()`: one more reference.
fn dup() -> Routine {
    let code = vec![
        get(0),
        get(0),
        ld32(header::COUNT),
        c32(1),
        I::I32Add,
        st32(header::COUNT),
        I::End,
    ];
    make(&[I32], &[], &[], code)
}

/// `(object) -> ()`: one reference fewer, releasing the object at zero.
fn drop_routine(fns: &Routines) -> Routine {
    // Local 1: the new count.
    let code = vec![
        get(0),
        ld32(header::COUNT),
        c32(1),
        I::I32Sub,
        set(1),
        get(0),
        get(1),
        st32(header::COUNT),
        get(1),
        I::I32Eqz,
        I::If(EMPTY),
        get(0),
        I::Call(fns.release),
        I::End,
        I::End,
    ];
    make(&[I32], &[], &[I32], code)
}

/// `(object) -> ()`: frees an object whose count reached zero and, with it,
/// every object only it held, walking a worklist that is threaded through the
/// `count` words of the blocks that are waiting.
fn release(fns: &Routines) -> Routine {
    // Locals: 1 head, 2 object, 3 index, 4 len, 5 child, 6 count, 7 classes,
    // 8 cells.
    let (head, object, i, len, child, count, classes, cells) = (1, 2, 3, 4, 5, 6, 7, 8);
    let mut code = Vec::new();
    push(
        &mut code,
        &[
            get(0),
            set(head),
            get(0),
            c32(0),
            st32(header::COUNT),
            I::Block(EMPTY),
            I::Loop(EMPTY),
            get(head),
            I::I32Eqz,
            I::BrIf(1),
            get(head),
            set(object),
            get(object),
            ld32(header::COUNT),
            set(head),
            // Only the cells layout holds references.
            get(object),
            ld8(header::STRIDE),
            c32(index(layout::STRIDE_CELLS)),
            I::I32Eq,
            I::If(EMPTY),
            get(object),
            ld32(header::LEN),
            set(len),
            get(object),
            c32(index(header::SIZE)),
            I::I32Add,
            set(classes),
            get(classes),
            get(len),
            c32(7),
            I::I32Add,
            c32(-8),
            I::I32And,
            I::I32Add,
            set(cells),
            c32(0),
            set(i),
            I::Block(EMPTY),
            I::Loop(EMPTY),
            get(i),
            get(len),
            I::I32GeU,
            I::BrIf(1),
            get(classes),
            get(i),
            I::I32Add,
            ld8(0),
            c32(i32::from(CellClass::Ref.code())),
            I::I32Eq,
            I::If(EMPTY),
            get(cells),
            get(i),
            c32(3),
            I::I32Shl,
            I::I32Add,
            ld32(0),
            set(child),
            get(child),
            get(child),
            ld32(header::COUNT),
            c32(1),
            I::I32Sub,
            tee(count),
            st32(header::COUNT),
            get(count),
            I::I32Eqz,
            I::If(EMPTY),
            // The child is dead too: push it on the worklist.
            get(child),
            get(head),
            st32(header::COUNT),
            get(child),
            set(head),
            I::End,
            I::End,
            get(i),
            c32(1),
            I::I32Add,
            set(i),
            I::Br(0),
            I::End,
            I::End,
            I::End,
            get(object),
            I::Call(fns.free),
            I::Br(0),
            I::End,
            I::End,
            I::End,
        ],
    );
    make(&[I32], &[], &[I32; 8], code)
}

/// `(object) -> i64`: an ID whose handle-table entry holds a new reference.
fn new_id(fns: &Routines) -> Routine {
    // Locals: 1 counter (i64), 2 slot, 3 entry, 4 capacity, 5 new capacity,
    // 6 new table, 7 id (i64).
    let (counter, slot, entry, capacity, new_capacity, new_table, id) =
        (1, 2, 3, 4, 5, 6, 7);
    let shift = c32(4); // TABLE_ENTRY_SIZE is 16.
    debug_assert_eq!(TABLE_ENTRY_SIZE, 16);
    let mut code = Vec::new();
    // The counter is 32 bits. An ID that would exceed them is the host event.
    push(&mut code, &load_state64(state::ID_COUNTER));
    push(
        &mut code,
        &[
            c64(1),
            I::I64Add,
            set(counter),
            get(counter),
            c64(0xFFFF_FFFF),
            I::I64GtU,
            I::If(EMPTY),
            I::Call(fns.stop_memory),
            I::End,
        ],
    );
    push(&mut code, &load_state32(state::TABLE_FREE));
    push(&mut code, &[set(slot), get(slot), I::If(EMPTY)]);
    // Reuse a released slot.
    push(&mut code, &[get(slot), c32(1), I::I32Sub, set(slot)]);
    push(&mut code, &load_state32(state::TABLE));
    push(
        &mut code,
        &[
            get(slot),
            shift.clone(),
            I::I32Shl,
            I::I32Add,
            set(entry),
            ZERO,
            get(entry),
            ld32(12),
            st32(state::TABLE_FREE),
        ],
    );
    code.push(I::Else);
    // Take the next unused slot, growing the table when it is full or absent.
    push(&mut code, &load_state32(state::TABLE_USED));
    push(&mut code, &[set(slot)]);
    push(&mut code, &load_state32(state::TABLE_CAPACITY));
    push(
        &mut code,
        &[
            set(capacity),
            get(slot),
            get(capacity),
            I::I32Eq,
            I::If(EMPTY),
        ],
    );
    push(
        &mut code,
        &[
            get(capacity),
            I::I32Eqz,
            I::If(BlockType::Result(I32)),
            c32(index(TABLE_FIRST_CAPACITY)),
            I::Else,
            get(capacity),
            c32(1),
            I::I32Shl,
            I::End,
            set(new_capacity),
            get(new_capacity),
            I::I64ExtendI32U,
            c64(4),
            I::I64Shl,
            I::Call(fns.alloc),
            set(new_table),
            get(capacity),
            I::If(EMPTY),
            // Copy the used entries (every slot of a full table is used).
            get(new_table),
        ],
    );
    push(&mut code, &load_state32(state::TABLE));
    push(
        &mut code,
        &[
            get(slot),
            shift.clone(),
            I::I32Shl,
            I::MemoryCopy {
                src_mem: 0,
                dst_mem: 0,
            },
        ],
    );
    push(&mut code, &load_state32(state::TABLE));
    push(
        &mut code,
        &[
            I::Call(fns.free),
            I::End,
            ZERO,
            get(new_table),
            st32(state::TABLE),
            ZERO,
            get(new_capacity),
            st32(state::TABLE_CAPACITY),
            I::End,
        ],
    );
    push(
        &mut code,
        &[ZERO, get(slot), c32(1), I::I32Add, st32(state::TABLE_USED)],
    );
    push(&mut code, &load_state32(state::TABLE));
    push(
        &mut code,
        &[get(slot), shift, I::I32Shl, I::I32Add, set(entry), I::End],
    );
    // id = counter << 32 | slot; the entry holds the ID, the offset, and no link.
    push(
        &mut code,
        &[
            get(counter),
            c64(32),
            I::I64Shl,
            get(slot),
            I::I64ExtendI32U,
            I::I64Or,
            set(id),
            get(entry),
            get(id),
            st64(0),
            get(entry),
            get(0),
            st32(8),
            get(entry),
            c32(0),
            st32(12),
            get(0),
            I::Call(fns.dup),
            ZERO,
            get(counter),
            st64(state::ID_COUNTER),
            ZERO,
        ],
    );
    push(&mut code, &load_state32(state::TABLE_COUNT));
    push(
        &mut code,
        &[c32(1), I::I32Add, st32(state::TABLE_COUNT), get(id), I::End],
    );
    make(&[I32], &[I64], &[I64, I32, I32, I32, I32, I32, I64], code)
}

/// `(id) -> i32`: the handle-table entry of a valid ID. A zero ID, a slot the
/// table never handed out, and an entry that holds another ID (a released or
/// never-issued one) are `@runtime.invalid-host-value`.
fn entry_of(fns: &Routines) -> Routine {
    // Locals: 1 slot, 2 entry.
    let (slot, entry) = (1, 2);
    let mut code = Vec::new();
    push(&mut code, &[get(0), I::I64Eqz, I::If(EMPTY)]);
    push(&mut code, &invalid_host_value(fns));
    push(
        &mut code,
        &[I::End, get(0), I::I32WrapI64, set(slot), get(slot)],
    );
    push(&mut code, &load_state32(state::TABLE_USED));
    push(&mut code, &[I::I32GeU, I::If(EMPTY)]);
    push(&mut code, &invalid_host_value(fns));
    code.push(I::End);
    push(&mut code, &load_state32(state::TABLE));
    push(
        &mut code,
        &[
            get(slot),
            c32(4),
            I::I32Shl,
            I::I32Add,
            set(entry),
            get(entry),
            ld64(0),
            get(0),
            I::I64Ne,
            I::If(EMPTY),
        ],
    );
    push(&mut code, &invalid_host_value(fns));
    push(&mut code, &[I::End, get(entry), I::End]);
    make(&[I64], &[I32], &[I32, I32], code)
}

/// Instructions that trap `@runtime.invalid-host-value` unless the kind of the
/// object in `object` is in `mask`.
fn require_kind(fns: &Routines, object: u32, mask: u32) -> Vec<Ins> {
    let mut code = vec![
        c32(mask.cast_signed()),
        get(object),
        ld8(header::KIND),
        I::I32ShrU,
        c32(1),
        I::I32And,
        I::I32Eqz,
        I::If(EMPTY),
    ];
    code.extend(invalid_host_value(fns));
    code.push(I::End);
    code
}

fn accessor_routine(fns: &Routines, accessor: Accessor) -> Routine {
    let mut built = match accessor {
        Accessor::Status => word(state::STATUS),
        Accessor::TrapCode => word(state::TRAP_CODE),
        Accessor::Origin => word(state::ORIGIN),
        Accessor::Result => wide(state::RESULT),
        Accessor::LiveSize => wide(state::LIVE_SIZE),
        Accessor::Release => release_id(fns),
        Accessor::Variant => variant(fns),
        Accessor::Length => length(fns),
        Accessor::Read(read) => read_component(fns, read),
    };
    built.export = Some(accessor.export());
    built
}

/// `() -> i32` over a state word.
fn word(address: u32) -> Routine {
    let mut code = Vec::new();
    push(&mut code, &load_state32(address));
    code.push(I::End);
    make(&[], &[I32], &[], code)
}

/// `() -> i64` over a state double word.
fn wide(address: u32) -> Routine {
    let mut code = Vec::new();
    push(&mut code, &load_state64(address));
    code.push(I::End);
    make(&[], &[I64], &[], code)
}

/// `(id) -> ()`: the host drops its hold on an ID.
fn release_id(fns: &Routines) -> Routine {
    // Locals: 1 entry, 2 object, 3 slot, 4 live IDs.
    let (entry, object, slot, live) = (1, 2, 3, 4);
    let mut code = vec![
        I::Call(fns.begin),
        get(0),
        I::Call(fns.entry_of),
        set(entry),
        get(entry),
        ld32(8),
        set(object),
        get(0),
        I::I32WrapI64,
        set(slot),
        // The entry is free: no ID, and a link to the free-slot list.
        get(entry),
        c64(0),
        st64(0),
        get(entry),
    ];
    code.extend(load_state32(state::TABLE_FREE));
    code.extend([
        st32(12),
        ZERO,
        get(slot),
        c32(1),
        I::I32Add,
        st32(state::TABLE_FREE),
    ]);
    code.extend(load_state32(state::TABLE_COUNT));
    code.extend([
        c32(1),
        I::I32Sub,
        set(live),
        ZERO,
        get(live),
        st32(state::TABLE_COUNT),
        // The last ID released: the table needs no storage.
        get(live),
        I::I32Eqz,
        I::If(EMPTY),
    ]);
    code.extend(load_state32(state::TABLE));
    code.push(I::Call(fns.free));
    for address in [
        state::TABLE,
        state::TABLE_CAPACITY,
        state::TABLE_USED,
        state::TABLE_FREE,
    ] {
        code.extend([ZERO, c32(0), st32(address)]);
    }
    code.extend([I::End, get(object), I::Call(fns.drop), I::End]);
    make(&[I64], &[], &[I32, I32, I32, I32], code)
}

/// `(id) -> i32`: the variant index of an enum or member index of a union.
fn variant(fns: &Routines) -> Routine {
    // Local 1: the object.
    let mut code = vec![
        I::Call(fns.begin),
        get(0),
        I::Call(fns.entry_of),
        ld32(8),
        set(1),
    ];
    code.extend(require_kind(fns, 1, layout::variant_mask()));
    code.extend([get(1), ld32(header::VARIANT), I::End]);
    make(&[I64], &[I32], &[I32], code)
}

/// `(id) -> i64`: the scalar, byte, element, entry, or component count.
fn length(fns: &Routines) -> Routine {
    let mut code = vec![
        I::Call(fns.begin),
        get(0),
        I::Call(fns.entry_of),
        ld32(8),
        set(1),
    ];
    code.extend(require_kind(fns, 1, layout::length_mask()));
    code.extend([get(1), ld32(header::LEN), I::I64ExtendI32U, I::End]);
    make(&[I64], &[I64], &[I32], code)
}

/// `(id, index) -> T`: the scalar component of a cells object whose class is
/// `T`, or, for an `i32` read only, a character or a byte, or the reference
/// component of a cells object as a new ID.
fn read_component(fns: &Routines, read: Read) -> Routine {
    // Params: 0 id, 1 index. Locals: 2 object, 3 position, 4 element address.
    let (object, position, address) = (2, 3, 4);
    let (class, result, load): (CellClass, ValType, Ins) = match read {
        Read::I32 => (CellClass::I32, I32, ld32(0)),
        Read::I64 => (CellClass::I64, I64, ld64(0)),
        Read::F32 => (CellClass::F32, ValType::F32, I::F32Load(mem(0, 2))),
        Read::F64 => (CellClass::F64, ValType::F64, I::F64Load(mem(0, 3))),
        Read::Id => (CellClass::Ref, I64, ld32(0)),
    };
    let mut code = vec![
        I::Call(fns.begin),
        get(0),
        I::Call(fns.entry_of),
        ld32(8),
        set(object),
    ];
    code.extend(require_kind(fns, object, layout::components_mask()));
    // The index is within the object's components.
    code.extend([
        get(1),
        get(object),
        ld32(header::LEN),
        I::I64ExtendI32U,
        I::I64GeU,
        I::If(EMPTY),
    ]);
    code.extend(invalid_host_value(fns));
    code.extend([I::End, get(1), I::I32WrapI64, set(position)]);
    // The element address and, for the cells layout, the class check.
    code.extend([
        get(object),
        ld8(header::STRIDE),
        c32(index(layout::STRIDE_CELLS)),
        I::I32Eq,
        I::If(BlockType::Result(result)),
        // The class byte of the component is the class read.
        get(object),
        c32(index(header::SIZE)),
        I::I32Add,
        get(position),
        I::I32Add,
        ld8(0),
        c32(i32::from(class.code())),
        I::I32Ne,
        I::If(EMPTY),
    ]);
    code.extend(invalid_host_value(fns));
    code.extend([
        I::End,
        // cell = object + header + padded classes + position * 8
        get(object),
        c32(index(header::SIZE)),
        I::I32Add,
        get(object),
        ld32(header::LEN),
        c32(7),
        I::I32Add,
        c32(-8),
        I::I32And,
        I::I32Add,
        get(position),
        c32(3),
        I::I32Shl,
        I::I32Add,
        set(address),
        get(address),
        load,
    ]);
    if read == Read::Id {
        code.push(I::Call(fns.new_id));
    }
    code.push(I::Else);
    if read == Read::I32 {
        // A character is 4 bytes and a byte is 1.
        code.extend([
            get(object),
            ld8(header::STRIDE),
            c32(index(layout::STRIDE_CHARS)),
            I::I32Eq,
            I::If(BlockType::Result(I32)),
            get(object),
            c32(index(header::SIZE)),
            I::I32Add,
            get(position),
            c32(2),
            I::I32Shl,
            I::I32Add,
            ld32(0),
            I::Else,
            get(object),
            c32(index(header::SIZE)),
            I::I32Add,
            get(position),
            I::I32Add,
            ld8(0),
            I::End,
        ]);
    } else {
        // Characters and bytes are `i32` slots only.
        code.extend(invalid_host_value(fns));
        code.push(I::Unreachable);
    }
    code.extend([I::End, I::End]);
    make(&[I64, I64], &[result], &[I32, I32, I32], code)
}

/// The entry export: clears the record and the result, runs the program's
/// entry, and stores its result in the result slot. A reference is registered
/// in the handle table, which takes its own count, and the entry's count is
/// dropped.
pub(crate) fn entry_export(fns: &Routines, entry: u32, class: ValueClass) -> Routine {
    // Locals: 0 slot (i64), 1 object (i32).
    let (slot, object) = (0, 1);
    let mut code = vec![
        I::Call(fns.begin),
        ZERO,
        c64(0),
        st64(state::RESULT),
        I::Call(entry),
    ];
    match class {
        ValueClass::Void => {}
        ValueClass::I32 => code.extend([I::I64ExtendI32U, set(slot)]),
        ValueClass::I64 => code.push(set(slot)),
        ValueClass::F32 => {
            code.extend([I::I32ReinterpretF32, I::I64ExtendI32U, set(slot)])
        }
        ValueClass::F64 => code.extend([I::I64ReinterpretF64, set(slot)]),
        ValueClass::Ref => code.extend([
            set(object),
            get(object),
            I::Call(fns.new_id),
            set(slot),
            get(object),
            I::Call(fns.drop),
        ]),
    }
    if class != ValueClass::Void {
        code.extend([ZERO, get(slot), st64(state::RESULT)]);
    }
    code.push(I::End);
    make(&[], &[], &[I64, I32], code)
}
