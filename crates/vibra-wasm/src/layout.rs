//! The memory layout of a v1 module: the instance state, the arena, the object
//! header, the per-kind payloads, the handle table, and the frame storage.
//!
//! This is the one place the layout is written. The runtime routines
//! (`runtime`), the lowering (`lower`), the host tests, and every later step
//! read the constants and the kind table below, so a kind or a field is added
//! here and nowhere else. `docs/spec/06-runtime.md`, "The value arena" and
//! "Reclamation", state the rules; this chapter states the representation.
//! Everything is little-endian, as WebAssembly memory is, and every offset in
//! this chapter is a byte offset into the module's one linear memory. No offset
//! below reaches typed IR, a canonical encoding, an audit event, or a snapshot.
//!
//! # Memory map
//!
//! ```text
//! 0      instance state          (STATE_SIZE bytes, fixed addresses)
//! 128    free-list heads         (one u32 per size class, FREE_LISTS)
//! 256    the arena               (ARENA_START; grows by `memory.grow`)
//! ```
//!
//! # Instance state
//!
//! The state is a block of fixed addresses, so generated code reaches a field
//! with a constant address and a host test can read or set one through
//! `vibra_v1_memory`, which only toolchain-owned code does. Zero is every
//! field's initial value, so a fresh instance needs no start function.
//!
//! | Address | Field | Meaning |
//! | --- | --- | --- |
//! | 0 | `status` (u32) | What the last stoppable call recorded: `0` nothing, `1` trap, `2` memory host event, `3` failed assertion |
//! | 4 | `trap_code` (u32) | After status `1`: the trap code |
//! | 8 | `origin` (u32) | After status `1` or `3`: the origin ordinal, `0` for none |
//! | 12 | `arena_used` (u32) | Bytes of the arena handed out so far, a bump pointer relative to [`ARENA_START`] |
//! | 16 | `result` (u64) | After a completed entry: the entry's result slot (see below) |
//! | 24 | `live_size` (u64) | Bytes of live blocks: allocated minus released |
//! | 32 | `id_counter` (u64) | The last ID counter issued; an ID is never reused because it never decreases |
//! | 40 | `table` (u32) | Offset of the handle-table block, `0` when the table is empty |
//! | 44 | `table_capacity` (u32) | Entries the block holds |
//! | 48 | `table_used` (u32) | The high-water mark of slots ever handed out of this block |
//! | 52 | `table_count` (u32) | Live IDs |
//! | 56 | `table_free` (u32) | Head of the free-slot list, as slot plus one, `0` for none |
//! | 60 | `module_values` (u32) | Offset of the module-value state (see below), `0` until the entry allocates it |
//! | 64 | `frame_segment` (u32) | Offset of the frame segment holding the top activation, `0` for none |
//! | 68 | `frame_top` (u32) | Offset of the top activation inside that segment, `0` for none |
//! | 72 | `frame_limit` (u32) | End of that segment as a 32-bit address; a segment that ends at the 4 GiB edge reads `0`, which makes it full |
//! | 76 | `frame_depth` (u32) | Activations on the frame stack |
//! | 80 | `ret` (u64) | The cell a returning activation hands to its caller (see "Activations and the dispatcher") |
//!
//! # The arena
//!
//! The arena is a sequence of blocks. A block's size is a power of two from
//! 2^5 to 2^30 bytes, its size class. An allocation takes the head of the free
//! list of its class when one exists. Otherwise it takes fresh bytes at the
//! bump pointer and grows the memory by whole pages when the pointer would pass
//! its end. A request that no class can hold, or a growth the engine refuses,
//! is the host event `@runtime.memory-exhausted` (status `2`), never a trap. A
//! freed block goes to the head of its class's list through its first word, and
//! a block never moves, so an offset is stable for the life of its value. A
//! list of one class never satisfies another class, so a free list can hold a
//! block that is too small for a request and still leave the request to fresh
//! bytes.
//!
//! # The object header
//!
//! Every arena value is one block that starts with a 24-byte header:
//!
//! | Offset | Field | Meaning |
//! | --- | --- | --- |
//! | 0 | `count` (u32) | The reference count. While the block is being released it is the link of the release worklist, and while it is free it is the link of its free list |
//! | 4 | `size` (u32) | The block's size in bytes, a power of two, which is also its class |
//! | 8 | `kind` (u8) | The kind code of [`KINDS`] |
//! | 9 | `stride` (u8) | The byte width of one component: `8` for cells, `4` for scalars of `str` and `atom`, `1` for the bytes of `bytes` |
//! | 12 | `len` (u32) | The component count |
//! | 16 | `variant` (u32) | The variant index of an enum or the member index of a union, `0` for every other kind |
//! | 20 | reserved (u32) | Zero |
//!
//! # Payloads
//!
//! The payload follows the header and depends only on `stride`, so a routine
//! never branches on a kind to find a component:
//!
//! - stride 8, the **cells** layout: `len` class bytes, padded to a multiple of
//!   8, and then `len` cells of 8 bytes. A class byte says what its cell holds,
//!   as a [`CellClass`]: a scalar by its WebAssembly type or a reference to an
//!   arena value. A reference cell holds the referenced offset in its low 32
//!   bits and zero in its high 32 bits. A scalar cell holds the scalar's bits
//!   in its low bytes: an `i32` slot is the value a narrower integer
//!   sign-extends or zero-extends to, as the boundary requires. Class `0` is a
//!   scalar, so a block whose cells have not been written yet releases safely.
//! - stride 4, the **chars** layout: `len` Unicode scalars of 4 bytes each, and
//!   no class bytes.
//! - stride 1, the **bytes** layout: `len` bytes, and no class bytes.
//!
//! A kind's row of [`KINDS`] gives its stride and which accessors admit it.
//! Release, the accessors, and the allocator read the header and the stride and
//! never the kind, except where an accessor checks that its kind is admitted,
//! which it does with a mask the table computes. A later step adds a kind by
//! adding a row.
//!
//! | Kind | Stride | Components |
//! | --- | --- | --- |
//! | `atom`, `str` | 4 | the characters |
//! | `bytes` | 1 | the bytes |
//! | `tuple`, `record`, `array` | 8 | the components or elements, in order |
//! | `dict` | 8 | the entries, each a reference to a two-component `tuple`, so a dict's entry reads like an array element |
//! | `enum` | 8 | `len` is `0`, or `1` for a non-`void` payload |
//! | `wrapper`, `union` | 8 | one cell, the representation or the member value |
//! | `function` | 8 | the captured values, which no host reads |
//!
//! # The handle table
//!
//! The handle table is one block of 16-byte entries from the arena itself,
//! `[id: u64][offset: u32][next_free: u32]`, grown by allocating a block of
//! twice the capacity, copying, and freeing the old one. Entry 0 is the block's
//! own header (its `count` and `size` words, like every block's), so the first
//! slot is 1 and the first block of 16 entries holds 15 IDs. The table is freed
//! when its last ID is released, so a balanced program returns the live size to
//! its start. An ID is `counter << 32 | slot`: the counter is the instance's
//! `id_counter` after an increment, so it is at least `1`, which makes `0`
//! invalid and the IDs strictly increasing, and the slot is the entry's index.
//! A released slot is reused by a later ID, but the counter is not, so an ID is
//! never reused. A lookup checks that the slot is neither the header nor out of
//! range and that the entry holds exactly that ID, which rejects a zero, a
//! stale, a never-issued, and a released ID. The counter has 32 bits: an ID
//! that would exceed them is the host event, like exhausted memory. Each live
//! entry holds one reference to its value.
//!
//! The IDs of two instances are not told apart by the instance, which cannot
//! know of the other. The host does: it tags each ID with its instance and
//! rejects another instance's ID before calling a module
//! (`docs/spec/06-runtime.md`, "WebAssembly boundary").
//!
//! # The entry's result slot
//!
//! After a completed entry `result` holds the entry's result as a 64-bit slot,
//! and `vibra_v1_result` returns it. For a `void` result it is `0`. For a
//! result that is an arena value it is the ID of the value, which the host
//! releases. For a scalar result it is the scalar's bits, zero-extended from
//! 32 bits for an `i32`-slot type, and the host reads it by the entry's result
//! type, as a failed assertion's operands are read (ledger D2.8).
//!
//! # Cells
//!
//! One representation serves every place a value is stored: an **8-byte cell**
//! with a **class byte** that says whether the cell owns a reference. A cell
//! holds a scalar's bits zero-extended to 64 bits (an `i32`-slot value is its
//! 32-bit two's complement pattern, an `f32` its bit pattern), or the offset of
//! an arena value in its low 32 bits. The cells of an object, of a frame, and of
//! the module-value state are all laid out as in "Payloads", so copying a value
//! from one place to another is a copy of 8 bytes and a class byte, whatever the
//! value's type is, and no routine branches on a type. A `void` value is a
//! scalar cell holding `0`. The class bytes of an object are written as the host
//! reads them; for a frame and the module-value state only the reference class
//! matters, because it is what a scan drops.
//!
//! # Activations and the dispatcher
//!
//! A language activation lives in the arena and never on the engine's stack.
//! This is the one mechanism of every call, and Step 6 extends it: indirect
//! calls, closures, tail calls to every kind of callee, and generics are other
//! ways to choose the frame to push or replace, not other mechanisms.
//!
//! **Frame.** A frame is `[resume: u32][function: u32][slots: u32][caller: u32]`
//! followed by the cells layout of its `slots`: the class bytes padded to 8, and
//! then the cells ([`frame`]). A function's slots are its parameters in
//! parameter order, then its `let` bindings, then the temporaries of its own
//! lowering. A slot's class byte is the reference class exactly while the slot
//! **owns** a count: a value is moved out of a slot by copying its cell and
//! clearing the byte, and leaving a frame drops every cell whose byte is still
//! the reference class, by one scan. That scan is what balances an early
//! `return`, which leaves temporaries half-built. A new frame has every class
//! byte clear. No frame is a value, and none has a header or an ID.
//!
//! **Segments.** Frames are bump-allocated in **segments**, which are ordinary
//! arena blocks (so `vibra_v1_live_size` counts them and a deep recursion
//! exhausts memory as the host event). A segment is a block header, then
//! `[previous: u32][end: u32]` (`previous` is the segment below it), then its
//! frames from [`segment::HEADER`]. A new frame goes after the top frame, which
//! knows its own size from `slots`, and a frame that does not fit takes a new
//! segment of at least [`segment::MIN_SIZE`] bytes. Returning from the first
//! frame of a segment frees the segment. The frame stack is therefore a list of
//! blocks whose total size is the only bound on the depth of the recursion.
//!
//! **Function.** Every language function, and every module-value initializer, is
//! one WebAssembly function `(frame: i32) -> ()` with the same signature, in
//! the one function table at the position of its function index. It runs the
//! activation from the resume point the frame holds until the activation calls
//! or returns, and then it returns to the dispatcher. Its body is a set of
//! **basic blocks**, numbered, in a `loop` over a `br_table` on a `pc` local, so
//! that any block can be entered from the top: a block ends in a jump, a branch
//! on a `bool`, a call, or a return, and a call ends the Wasm function, because
//! nothing on the engine's operand stack or in its locals survives a return to
//! the dispatcher. Every value that lives across a block boundary lives in a
//! frame slot, which is why every intermediate value does.
//!
//! **Call.** A non-tail call evaluates its operands into temporaries of the
//! caller's frame, stores the number of the block that continues the caller in
//! the caller's `resume`, and asks the runtime to push a callee frame
//! (`push_frame(function, slots)`, which links `caller`, makes it the top, and
//! counts the depth). It moves each operand into the callee's parameter slot
//! (copying the cell and the class byte, and clearing the temporary's byte), and
//! returns to the dispatcher. The dispatcher runs `table[top.function](top)`
//! until no frame is left. The caller's continuation block first stores `ret`
//! into the result's slot.
//!
//! **Return.** A function moves its result's cell into the state's `ret`,
//! clears the byte of that slot, and leaves the frame: it drops what the frame
//! still owns, pops it, and frees its segment when it was the segment's first
//! frame. The caller owns what `ret` holds from the moment it is resumed.
//!
//! **Tail call (Step 6).** The callee replaces the current frame: after the
//! operands are in temporaries, the scan runs over the slots that are not
//! operands, and the frame is rewritten in place (`function`, `slots`, class
//! bytes) with the operands moved into its parameter slots; when the callee's
//! frame is larger than the room left in the segment it is popped and pushed.
//! `caller` is kept, so the depth does not grow.
//!
//! **Module values.** The module-value state is one cells object of two cells
//! per module value (a flag, then the value), allocated by the entry and held by
//! the instance until it is dropped. A read tests the flag. When it is clear the
//! reader pushes the value's initializer as a frame like any callee, and its
//! continuation moves the returned cell into the table, sets the flag, and keeps
//! its own count with a `dup`, so the initializer runs once and a later read
//! only copies the cell. An initializer cycle is rejected before a program
//! exists, so a flag is never read while its initializer is running.
//!
//! **Engine stack.** The dispatcher, one function, the runtime routines, and the
//! release worklist are the whole engine call chain at any language depth, so
//! the engine stack use is constant.

/// The frame layout, see "Activations and the dispatcher".
pub mod frame {
    /// The bytes before a frame's class bytes.
    pub const HEADER: u32 = 16;
    /// `resume`: the block that continues the activation.
    pub const RESUME: u32 = 0;
    /// `function`: the function index, which is the position in the table.
    pub const FUNCTION: u32 = 4;
    /// `slots`: the number of slots.
    pub const SLOTS: u32 = 8;
    /// `caller`: the offset of the frame below, `0` for the first.
    pub const CALLER: u32 = 12;

    /// The class bytes of `slots` slots, padded to a multiple of 8.
    #[must_use]
    pub const fn classes(slots: u32) -> u32 {
        slots.div_ceil(8) * 8
    }

    /// The size in bytes of a frame of `slots` slots.
    #[must_use]
    pub const fn size(slots: u32) -> u32 {
        HEADER + classes(slots) + 8 * slots
    }

    /// The offset of a slot's class byte from the start of the frame.
    #[must_use]
    pub const fn class_offset(slot: u32) -> u32 {
        HEADER + slot
    }

    /// The offset of a slot's cell from the start of a frame of `slots` slots.
    #[must_use]
    pub const fn cell_offset(slots: u32, slot: u32) -> u32 {
        HEADER + classes(slots) + 8 * slot
    }
}

/// The frame segments, see "Activations and the dispatcher".
pub mod segment {
    use super::header;

    /// `previous`: the segment below, `0` for the first.
    pub const PREVIOUS: u32 = header::SIZE;
    /// `end`: the address just past the segment's block.
    pub const END: u32 = header::SIZE + 4;
    /// The offset of a segment's first frame.
    pub const HEADER: u32 = header::SIZE + 8;
    /// The least size of a segment's block.
    pub const MIN_SIZE: u32 = 16 * 1024;
}

/// The first byte of the arena. Everything below is the instance state and the
/// free-list heads.
pub const ARENA_START: u32 = 256;

/// The state fields, as constant addresses.
pub mod state {
    /// `status`.
    pub const STATUS: u32 = 0;
    /// `trap_code`.
    pub const TRAP_CODE: u32 = 4;
    /// `origin`.
    pub const ORIGIN: u32 = 8;
    /// `arena_used`.
    pub const ARENA_USED: u32 = 12;
    /// `result`.
    pub const RESULT: u32 = 16;
    /// `live_size`.
    pub const LIVE_SIZE: u32 = 24;
    /// `id_counter`.
    pub const ID_COUNTER: u32 = 32;
    /// `table`.
    pub const TABLE: u32 = 40;
    /// `table_capacity`.
    pub const TABLE_CAPACITY: u32 = 44;
    /// `table_used`.
    pub const TABLE_USED: u32 = 48;
    /// `table_count`.
    pub const TABLE_COUNT: u32 = 52;
    /// `table_free`.
    pub const TABLE_FREE: u32 = 56;
    /// `module_values`.
    pub const MODULE_VALUES: u32 = 60;
    /// `frame_segment`.
    pub const FRAME_SEGMENT: u32 = 64;
    /// `frame_top`.
    pub const FRAME_TOP: u32 = 68;
    /// `frame_limit`.
    pub const FRAME_LIMIT: u32 = 72;
    /// `frame_depth`.
    pub const FRAME_DEPTH: u32 = 76;
    /// `ret`.
    pub const RET: u32 = 80;
    /// The end of the fields.
    pub const END: u32 = 88;
    /// The free-list heads: one `u32` per size class, indexed by the class.
    pub const FREE_LISTS: u32 = 128;
}

/// The smallest size class, 32 bytes: the header and one cell.
pub const MIN_CLASS: u32 = 5;
/// The largest size class, one gibibyte. A request above it is the memory host
/// event.
pub const MAX_CLASS: u32 = 30;

/// The object header.
pub mod header {
    /// The header's size in bytes.
    pub const SIZE: u32 = 24;
    /// `count`.
    pub const COUNT: u32 = 0;
    /// `size`.
    pub const BLOCK_SIZE: u32 = 4;
    /// `kind`.
    pub const KIND: u32 = 8;
    /// `stride`.
    pub const STRIDE: u32 = 9;
    /// `len`.
    pub const LEN: u32 = 12;
    /// `variant`.
    pub const VARIANT: u32 = 16;
}

/// The stride of the cells layout.
pub const STRIDE_CELLS: u32 = 8;
/// The stride of the chars layout.
pub const STRIDE_CHARS: u32 = 4;
/// The stride of the bytes layout.
pub const STRIDE_BYTES: u32 = 1;

/// The handle table's entry size in bytes.
pub const TABLE_ENTRY_SIZE: u32 = 16;
/// The entry count of the first handle-table block, entry 0 (its header)
/// included.
pub const TABLE_FIRST_CAPACITY: u32 = 16;

/// What a cell of the cells layout holds. The codes are the class bytes, and
/// `0` is a scalar, so a block that is not yet written releases safely.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellClass {
    /// A 32-bit integer slot: `char` and the integers up to 32 bits.
    I32,
    /// A 64-bit integer slot.
    I64,
    /// A binary32 value.
    F32,
    /// A binary64 value.
    F64,
    /// A reference to an arena value.
    Ref,
}

impl CellClass {
    /// The class byte.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::I32 => 0,
            Self::I64 => 1,
            Self::F32 => 2,
            Self::F64 => 3,
            Self::Ref => 4,
        }
    }
}

/// The closed kinds of arena values, with the codes of the kind byte, in the
/// order of `docs/spec/06-runtime.md`, "The value arena".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// An atom.
    Atom,
    /// A string.
    Str,
    /// A byte sequence.
    Bytes,
    /// A tuple.
    Tuple,
    /// An array.
    Array,
    /// A dict.
    Dict,
    /// A record.
    Record,
    /// An enum value, which is also every `bool`.
    Enum,
    /// A wrapper-type value.
    Wrapper,
    /// A union value.
    Union,
    /// A function value.
    Function,
}

impl Kind {
    /// The kind byte.
    #[must_use]
    pub const fn code(self) -> u32 {
        self as u32
    }
}

/// One row of the kind table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KindRow {
    /// The kind.
    pub kind: Kind,
    /// The kind's name, as the specification spells it.
    pub name: &'static str,
    /// The byte width of one component.
    pub stride: u32,
    /// Whether `vibra_v1_length` admits the kind.
    pub length: bool,
    /// Whether `vibra_v1_variant` admits the kind.
    pub variant: bool,
    /// Whether the `read` accessors admit the kind.
    pub components: bool,
}

/// The kind table. Every value kind is a row; no routine names a kind.
pub const KINDS: [KindRow; 11] = [
    KindRow {
        kind: Kind::Atom,
        name: "atom",
        stride: STRIDE_CHARS,
        length: true,
        variant: false,
        components: true,
    },
    KindRow {
        kind: Kind::Str,
        name: "str",
        stride: STRIDE_CHARS,
        length: true,
        variant: false,
        components: true,
    },
    KindRow {
        kind: Kind::Bytes,
        name: "bytes",
        stride: STRIDE_BYTES,
        length: true,
        variant: false,
        components: true,
    },
    KindRow {
        kind: Kind::Tuple,
        name: "tuple",
        stride: STRIDE_CELLS,
        length: true,
        variant: false,
        components: true,
    },
    KindRow {
        kind: Kind::Array,
        name: "array",
        stride: STRIDE_CELLS,
        length: true,
        variant: false,
        components: true,
    },
    KindRow {
        kind: Kind::Dict,
        name: "dict",
        stride: STRIDE_CELLS,
        length: true,
        variant: false,
        components: true,
    },
    KindRow {
        kind: Kind::Record,
        name: "record",
        stride: STRIDE_CELLS,
        length: true,
        variant: false,
        components: true,
    },
    KindRow {
        kind: Kind::Enum,
        name: "enum",
        stride: STRIDE_CELLS,
        length: false,
        variant: true,
        components: true,
    },
    KindRow {
        kind: Kind::Wrapper,
        name: "wrapper",
        stride: STRIDE_CELLS,
        length: false,
        variant: false,
        components: true,
    },
    KindRow {
        kind: Kind::Union,
        name: "union",
        stride: STRIDE_CELLS,
        length: false,
        variant: true,
        components: true,
    },
    KindRow {
        kind: Kind::Function,
        name: "function",
        stride: STRIDE_CELLS,
        length: false,
        variant: false,
        components: false,
    },
];

/// The row of a kind.
#[must_use]
#[allow(clippy::indexing_slicing)] // The table is in kind order, which a test holds it to.
pub fn row(kind: Kind) -> &'static KindRow {
    &KINDS[kind as usize]
}

fn mask(admits: fn(&KindRow) -> bool) -> u32 {
    KINDS
        .iter()
        .filter(|row| admits(row))
        .fold(0, |mask, row| mask | (1 << row.kind.code()))
}

/// The kinds `vibra_v1_length` admits, as a bit mask over kind codes.
#[must_use]
pub fn length_mask() -> u32 {
    mask(|row| row.length)
}

/// The kinds `vibra_v1_variant` admits, as a bit mask over kind codes.
#[must_use]
pub fn variant_mask() -> u32 {
    mask(|row| row.variant)
}

/// The kinds the `read` accessors admit, as a bit mask over kind codes.
#[must_use]
pub fn components_mask() -> u32 {
    mask(|row| row.components)
}

/// The offset of the first cell of a cells-layout object of `len` cells, from
/// the start of the block: the header and the class bytes padded to 8.
#[must_use]
pub const fn cells_offset(len: u32) -> u32 {
    header::SIZE + len.div_ceil(8) * 8
}

/// The size in bytes of an object of `len` components and a stride, header
/// included, before it is rounded to a size class.
#[must_use]
pub const fn object_size(stride: u32, len: u32) -> u64 {
    let len = len as u64;
    let classes = if stride == STRIDE_CELLS {
        len.div_ceil(8) * 8
    } else {
        0
    };
    header::SIZE as u64 + classes + len * stride as u64
}

/// The size class of a request of `bytes`, or `None` when no class holds it.
#[must_use]
pub const fn size_class(bytes: u64) -> Option<u32> {
    if bytes > 1 << MAX_CLASS {
        return None;
    }
    let mut class = MIN_CLASS;
    while (1_u64 << class) < bytes {
        class += 1;
    }
    Some(class)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kind_table_is_in_kind_order() {
        for (index, row) in KINDS.iter().enumerate() {
            assert_eq!(row.kind as usize, index, "{}", row.name);
        }
        assert_eq!(super::row(Kind::Enum).name, "enum");
    }

    #[test]
    fn size_classes_round_up_to_a_power_of_two() {
        assert_eq!(size_class(1), Some(5));
        assert_eq!(size_class(32), Some(5));
        assert_eq!(size_class(33), Some(6));
        assert_eq!(size_class(1 << 30), Some(30));
        assert_eq!(size_class((1 << 30) + 1), None);
    }

    #[test]
    fn payload_sizes_follow_the_stride() {
        assert_eq!(object_size(STRIDE_CELLS, 0), 24);
        assert_eq!(object_size(STRIDE_CELLS, 1), 24 + 8 + 8);
        assert_eq!(object_size(STRIDE_CELLS, 9), 24 + 16 + 72);
        assert_eq!(object_size(STRIDE_CHARS, 3), 24 + 12);
        assert_eq!(object_size(STRIDE_BYTES, 5), 24 + 5);
        assert_eq!(cells_offset(1), 32);
        assert_eq!(cells_offset(8), 32);
        assert_eq!(cells_offset(9), 40);
    }

    #[test]
    fn the_state_fields_do_not_overlap_the_free_lists_or_the_arena() {
        const {
            assert!(state::END <= state::FREE_LISTS);
            assert!(
                state::FREE_LISTS + 4 * (MAX_CLASS + 1) <= ARENA_START,
                "a head for every class fits below the arena"
            );
            assert!(ARENA_START.is_multiple_of(32), "blocks stay aligned");
        }
    }

    #[test]
    fn the_masks_name_the_kinds_the_specification_lists() {
        let admitted = |mask: u32| {
            KINDS
                .iter()
                .filter(|row| mask & (1 << row.kind.code()) != 0)
                .map(|row| row.name)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            admitted(length_mask()),
            ["atom", "str", "bytes", "tuple", "array", "dict", "record"]
        );
        assert_eq!(admitted(variant_mask()), ["enum", "union"]);
        assert_eq!(
            admitted(components_mask()).len(),
            10,
            "every kind but function has components"
        );
    }
}
