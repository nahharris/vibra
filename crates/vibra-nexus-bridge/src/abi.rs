//! The data layout of a native program: [`VibraProgram`] and the types it
//! points to, and the status and code constants of the routines.
//!
//! Everything here is `#[repr(C)]`, fixed-width, and free of Rust-only types,
//! so a code generator can emit a descriptor as a data section. The crate
//! documentation gives the layout field by field; the size and offset
//! assertions below and the layout test keep that table honest.

use std::fmt;

/// The version of the boundary between generated code and the runtime. It is
/// the one [`vibra_nexus::admit`] checks.
pub const ABI_VERSION: u32 = vibra_nexus::ABI_VERSION;

/// The symbol the launcher looks for: the one data symbol a program defines.
pub const PROGRAM_SYMBOL: &str = "vibra_program";

/// Status of a step function and of a routine: the step reached a checkpoint
/// and the context has a frame to continue. For a routine that is not a
/// checkpoint it means "done, carry on".
pub const STATUS_CONTINUE: u32 = 0;
/// Status: the entry returned and its result is in the context.
pub const STATUS_DONE: u32 = 1;
/// Status: a trap was recorded in the context.
pub const STATUS_TRAP: u32 = 2;
/// Status: the memory limit was reached, the host event.
pub const STATUS_EXHAUSTED: u32 = 3;
/// Status reserved for `Wait` (decision N23, v0.0.4). Nothing returns it
/// today, and the dispatcher treats it as a defect of the program.
pub const STATUS_WAIT: u32 = 4;
/// Status of [`crate::nexus_global`] only: the module value was already
/// initialized and is now in `dst`. No frame was pushed, so the step carries
/// on at its `resume` block instead of returning.
pub const STATUS_PROCEED: u32 = 5;

/// `nexus_scalar` class: `char`, `void`, and integers up to 32 bits.
pub const CLASS_I32: u32 = 0;
/// `nexus_scalar` class: the 64-bit integers.
pub const CLASS_I64: u32 = 1;
/// `nexus_scalar` class: `f32` as its bits.
pub const CLASS_F32: u32 = 2;
/// `nexus_scalar` class: `f64` as its bits.
pub const CLASS_F64: u32 = 3;

/// `nexus_op`: wrapping 64-bit addition.
pub const OP_I64_ADD: u32 = 0;
/// `nexus_op`: wrapping 64-bit subtraction.
pub const OP_I64_SUB: u32 = 1;
/// `nexus_op`: 64-bit equality, an `I32` `0` or `1`.
pub const OP_I64_EQ: u32 = 2;
/// `nexus_op`: signed 64-bit less-than, an `I32` `0` or `1`.
pub const OP_I64_LT_S: u32 = 3;

/// `nexus_build` kind: tuple.
pub const KIND_TUPLE: u32 = 0;
/// `nexus_build` kind: array.
pub const KIND_ARRAY: u32 = 1;
/// `nexus_build` kind: record.
pub const KIND_RECORD: u32 = 2;
/// `nexus_build` kind: enum (no cell for a `void` payload).
pub const KIND_ENUM: u32 = 3;
/// `nexus_build` kind: wrapper.
pub const KIND_WRAPPER: u32 = 4;
/// `nexus_build` kind: union (the variant is the member index).
pub const KIND_UNION: u32 = 5;

/// Constant kind in [`VibraConstant`]: a `str`, UTF-8.
pub const CONSTANT_STR: u32 = 0;
/// Constant kind in [`VibraConstant`]: a `bytes`.
pub const CONSTANT_BYTES: u32 = 1;
/// Constant kind in [`VibraConstant`]: an atom without its `@`, UTF-8.
pub const CONSTANT_ATOM: u32 = 2;

/// The opaque handle a step function receives and passes back to every
/// routine. It is the address of a runtime structure that lives for one step;
/// generated code must not read it, keep it past the step, or pass it to
/// another thread.
#[repr(C)]
#[derive(Debug)]
pub struct NexusContext {
    _private: [u8; 0],
}

/// A generated step function: runs the top frame from block `resume` up to and
/// including its next checkpoint, and answers a `STATUS_*` code.
pub type VibraStep = unsafe extern "C" fn(*mut NexusContext, u32) -> u32;

/// A pointer and an element count. A table of `len` elements of `T` starting
/// at `ptr`; `ptr` may be null only when `len` is zero. 16 bytes, 8-aligned.
#[repr(C)]
pub struct VibraTable<T> {
    /// The address of the first element.
    pub ptr: *const T,
    /// The number of elements, not bytes.
    pub len: u64,
}

impl<T> VibraTable<T> {
    /// The empty table.
    pub const EMPTY: Self = Self {
        ptr: std::ptr::null(),
        len: 0,
    };

    /// A table over `items`. The table does not keep `items` alive.
    #[must_use]
    pub const fn new(items: &[T]) -> Self {
        Self {
            ptr: items.as_ptr(),
            len: items.len() as u64,
        }
    }
}

impl<T> Clone for VibraTable<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for VibraTable<T> {}

impl<T> fmt::Debug for VibraTable<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VibraTable")
            .field("ptr", &self.ptr)
            .field("len", &self.len)
            .finish()
    }
}

/// A function of the program. 24 bytes, 8-aligned.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct VibraFunction {
    /// The step function, never null.
    pub step: Option<VibraStep>,
    /// The slots of the function's frame: at most `MAX_FRAME_SLOTS`.
    pub slots: u32,
    /// The number of parameters.
    pub params: u32,
    /// `1` when the function takes an environment (the function value itself,
    /// in the slot after the parameters), `0` when not. Nothing else.
    pub env: u32,
    /// The number of resume blocks of the function, at least `1`. A resume
    /// or branch target is below it.
    pub blocks: u32,
}

/// A constant of the program. 24 bytes, 8-aligned.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct VibraConstant {
    /// `CONSTANT_STR`, `CONSTANT_BYTES`, or `CONSTANT_ATOM`.
    pub kind: u32,
    /// Zero.
    pub reserved: u32,
    /// The bytes. Valid UTF-8 for a `str` and an atom.
    pub data: VibraTable<u8>,
}

/// One entry of the origin table: ordinal `n` is entry `n - 1`. 24 bytes,
/// 8-aligned.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct VibraOrigin {
    /// The source's identity, UTF-8, not NUL-terminated.
    pub source: VibraTable<u8>,
    /// The first byte of the span.
    pub start: u32,
    /// One past the last byte of the span, at least `start`.
    pub end: u32,
}

/// The descriptor of a native program: the one thing the launcher reads, found
/// under the symbol [`PROGRAM_SYMBOL`]. 96 bytes, 8-aligned, on a 64-bit
/// target.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct VibraProgram {
    /// The [`ABI_VERSION`] the program was generated for. Checked first: a
    /// program of another version is refused before any other field is read.
    pub abi: u32,
    /// The derived registry minor the program needs.
    pub registry_minor: u32,
    /// The index of the entry function.
    pub entry: u32,
    /// What the launcher may conclude from the entry's result: `0` when the
    /// entry's result is not a `result` (the launcher never reports
    /// `program-failed`), and `n > 0` when the entry returns a `result`
    /// whose `err` variant index is `n - 1`.
    pub entry_outcome: u32,
    /// The functions, at least one.
    pub functions: VibraTable<VibraFunction>,
    /// For each module value, the index of its initializer function.
    pub globals: VibraTable<u32>,
    /// The constants.
    pub constants: VibraTable<VibraConstant>,
    /// The host registry symbols the program names, each a UTF-8 string.
    /// `nexus_host` names one by its index.
    pub symbols: VibraTable<VibraTable<u8>>,
    /// The origin table.
    pub origins: VibraTable<VibraOrigin>,
}

// The descriptor is laid out for 64-bit targets: pointers are 8 bytes.
const _: () = assert!(size_of::<usize>() == 8);
const _: () = assert!(size_of::<VibraTable<u8>>() == 16);
const _: () = assert!(size_of::<VibraFunction>() == 24);
const _: () = assert!(size_of::<VibraConstant>() == 24);
const _: () = assert!(size_of::<VibraOrigin>() == 24);
const _: () = assert!(size_of::<VibraProgram>() == 96);
const _: () = assert!(align_of::<VibraProgram>() == 8);
