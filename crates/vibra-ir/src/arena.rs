//! What toolchain-owned native code knows of an arena object of a transition
//! module.
//!
//! `docs/spec/06-runtime.md`, "Native implementations" and "Wasm transition":
//! a native routine reads and writes the module's linear memory directly,
//! through the toolchain-reserved export, so it must know the header of an
//! arena object, the layouts of its payload, and the codes of its kinds and
//! cell classes. They are written once here, beside the closed tables of the
//! registry, because the emitter that lays objects out (`vibra-wasm`, whose
//! layout chapter explains the representation) and the native code that
//! reads them (`vibra-native`) depend on `vibra-ir` and on neither each
//! other. The codes are constants of this boundary alone; the native value
//! arena has its own private kinds. No offset of this module reaches typed
//! IR, a canonical encoding, an audit event, or a snapshot.

/// The object header: 24 bytes at the start of every arena block.
pub mod header {
    /// The header's size in bytes.
    pub const SIZE: u32 = 24;
    /// `count` (u32): the reference count.
    pub const COUNT: u32 = 0;
    /// `size` (u32): the block's size in bytes, a power of two.
    pub const BLOCK_SIZE: u32 = 4;
    /// `kind` (u8): the kind code of [`super::kind`].
    pub const KIND: u32 = 8;
    /// `stride` (u8): the byte width of one component.
    pub const STRIDE: u32 = 9;
    /// `len` (u32): the component count.
    pub const LEN: u32 = 12;
    /// `variant` (u32): the case index of a variant value.
    pub const VARIANT: u32 = 16;
    /// `reserved` (u32): always `0`. No host value enters a transition
    /// module, so no value is ever unsealed.
    pub const RESERVED: u32 = 20;
}

/// The stride of the cells layout: `len` class bytes, padded to 8, and then
/// `len` cells of 8 bytes.
pub const STRIDE_CELLS: u32 = 8;
/// The stride of the chars layout: `len` Unicode scalars of 4 bytes.
pub const STRIDE_CHARS: u32 = 4;
/// The stride of the bytes layout: `len` bytes.
pub const STRIDE_BYTES: u32 = 1;

/// The offset of the first cell of a cells-layout object of `len` cells, from
/// the start of the block: the header and the class bytes padded to 8.
#[must_use]
pub const fn cells_offset(len: u32) -> u32 {
    header::SIZE + len.div_ceil(8) * 8
}

/// The class bytes of a cell: what the cell holds. `0` is a scalar, so a block
/// whose cells have not been written yet releases safely.
pub mod class {
    /// A 32-bit integer slot: `Char` and the integers up to 32 bits.
    pub const I32: u8 = 0;
    /// A 64-bit integer slot.
    pub const I64: u8 = 1;
    /// A binary32 value.
    pub const F32: u8 = 2;
    /// A binary64 value.
    pub const F64: u8 = 3;
    /// A reference to an arena value: the referenced offset in the low 32 bits.
    pub const REF: u8 = 4;
}

/// The kind codes, in the order of the specification's "Value arena".
pub mod kind {
    /// An atom.
    pub const ATOM: u8 = 0;
    /// A `Str`.
    pub const STR: u8 = 1;
    /// A `Bytes`.
    pub const BYTES: u8 = 2;
    /// A tuple.
    pub const TUPLE: u8 = 3;
    /// A `List`.
    pub const LIST: u8 = 4;
    /// A `Dict`: its entries, each a reference to a two-component tuple.
    pub const DICT: u8 = 5;
    /// A record.
    pub const RECORD: u8 = 6;
    /// A value of a variant type, which is also every `Bool`.
    pub const VARIANT: u8 = 7;
    /// A wrapper-type value.
    pub const WRAPPER: u8 = 8;
    /// A function value.
    pub const FUNCTION: u8 = 9;
}
