//! The execution IR: checked Vibra IR lowered to explicit blocks, frame slots,
//! transfers, and resume points, verified before anything executes it.
//!
//! `docs/roadmap/nexus/design.md`, "Execution IR", is the design this crate
//! implements, and `docs/roadmap/nexus/decisions.md` records why it has this
//! shape. In short:
//!
//! - a [`Program`] is a flat list of [`Function`]s; a function is a list of
//!   [`Block`]s over the **slots** of one runtime frame;
//! - every value that lives across a block lives in a slot, so a function can
//!   stop at the end of any block and continue later from the frame alone;
//! - a block ends in exactly one [`Terminator`]. The terminators that hand
//!   control to the runtime ([`Terminator::Call`], [`Terminator::TailCall`],
//!   [`Terminator::Host`], [`Terminator::Global`], and [`Terminator::Return`])
//!   are the **checkpoints**: the only places where execution may yield, and
//!   the only places a frame is pushed, replaced, or popped; and
//! - [`verify`] proves the structural and ownership rules below, so neither
//!   the reference stepper nor a code generator has to defend against a
//!   malformed program.
//!
//! # Ownership of slots
//!
//! A slot is *empty* or *full*. An instruction that writes a slot requires it
//! to be empty; an instruction that reads one requires it to be full. Operands
//! are **consumed** (moved out, leaving the slot empty) by [`Inst::Move`],
//! [`Inst::Build`], [`Inst::Function`], [`Terminator::Call`],
//! [`Terminator::TailCall`], and [`Terminator::Return`], and **borrowed** by
//! everything else. [`Inst::Drop`] accepts a slot in any state and leaves it
//! empty, which is how a lowering reconciles the two arms of a branch. A frame
//! that ends, by a return or a tail transfer, releases whatever it still owns.
//!
//! This crate knows no value layout, no runtime, and no target. It depends on
//! `vibra-ir` for source origins and the closed registries only.

mod text;
mod verify;

use std::collections::BTreeSet;

use vibra_ir::SourceOrigin;
use vibra_ir::boundary::TrapCode;
use vibra_ir::external::host::{HostRow, derived_registry_minor};
use vibra_ir::external::{CompilerIntrinsic, NumericType};

pub use verify::{Verified, VerifyError, VerifyErrorKind, verified, verify};

macro_rules! index_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub u32);

        impl $name {
            /// The index as a `usize`, or `usize::MAX` where it cannot fit.
            #[must_use]
            pub fn index(self) -> usize {
                usize::try_from(self.0).unwrap_or(usize::MAX)
            }
        }
    };
}

index_type! {
    /// A function of a [`Program`], by position.
    FuncId
}
index_type! {
    /// A block of a [`Function`], by position. Block `0` is the entry block.
    BlockId
}
index_type! {
    /// A slot of the current frame, by position.
    Slot
}
index_type! {
    /// A module value of a [`Program`], by position.
    GlobalId
}
index_type! {
    /// A constant of a [`Program`], by position.
    ConstId
}

/// The most slots a [`Function`]'s frame may declare. A toolchain limit, not a
/// language rule: [`verify`] rejects a larger frame before it allocates any
/// state for it, so verification is total in bounded memory.
pub const MAX_SLOTS: u32 = 65_536;

/// A source origin of a [`Program`], by ordinal. Ordinal `0` is "no origin",
/// so ordinal `n` names `Program::origins[n - 1]`, exactly as the origin table
/// of the runtime chapter's "Traps" section numbers them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OriginId(pub u32);

impl OriginId {
    /// No origin.
    pub const NONE: Self = Self(0);
}

/// The width class of a scalar cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ScalarClass {
    /// `char`, `void`, and the integers up to 32 bits.
    I32,
    /// The 64-bit integers.
    I64,
    /// `f32`, as its bits.
    F32,
    /// `f64`, as its bits.
    F64,
}

/// A constant that is built in the value store when it is first needed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Constant {
    /// A `str`.
    Str(String),
    /// A `bytes`.
    Bytes(Vec<u8>),
    /// An atom, without its `@`.
    Atom(String),
}

/// The kinds of object [`Inst::Build`] assembles from cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ObjectKind {
    /// A tuple.
    Tuple,
    /// An array.
    Array,
    /// A record, its fields in the order the layout fixes.
    Record,
    /// An enum value: no cell for a `void` payload, one otherwise.
    Enum,
    /// A wrapper-type value: one cell.
    Wrapper,
    /// A union value: one cell and the member index as its variant.
    Union,
}

/// An operation over two integers of one type. A shift takes a `u32` amount as
/// its second operand.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IntOp {
    /// The sum, saturated at the bounds of the type.
    Add,
    /// The difference, saturated at the bounds of the type.
    Sub,
    /// The product, saturated at the bounds of the type.
    Mul,
    /// The quotient truncated toward zero, saturated, so the signed minimum
    /// divided by `-1` is the maximum. A zero divisor has no answer.
    Div,
    /// The remainder with the dividend's sign, so the signed minimum and `-1`
    /// give `0`. A zero divisor has no answer.
    Rem,
    /// `left * 2^amount`, saturated, for every amount: zero stays zero.
    ShiftLeft,
    /// `floor(left / 2^amount)`, for every amount.
    ShiftRight,
    /// Equality, answering an `I32` `0` or `1`.
    Equal,
    /// Less-than in the order of the type, answering an `I32` `0` or `1`.
    Less,
}

impl IntOp {
    /// Every operation, in declaration order.
    pub const ALL: [Self; 9] = [
        Self::Add,
        Self::Sub,
        Self::Mul,
        Self::Div,
        Self::Rem,
        Self::ShiftLeft,
        Self::ShiftRight,
        Self::Equal,
        Self::Less,
    ];
}

/// The integer types an [`IntOp`] may name, in the order of the wire encoding.
pub const INTEGER_TYPES: [NumericType; 8] = [
    NumericType::I8,
    NumericType::I16,
    NumericType::I32,
    NumericType::I64,
    NumericType::U8,
    NumericType::U16,
    NumericType::U32,
    NumericType::U64,
];

/// A machine-level operation over scalar cells. Each is total over the values
/// of its operand types: none traps, none allocates, and none is a registry
/// row. They are what a lowering needs to test a discriminant or count, and to
/// compute the unchecked integer family; a registry row is an
/// [`Inst::Intrinsic`].
///
/// # Integer operations
///
/// [`ScalarOp::Int`] names one operation at one integer type, so the type is
/// part of the instruction. That is the smallest table that keeps the
/// operand classes checkable: the type fixes the cell class of both operands
/// and of the result (`I32` up to 32 bits, `I64` at 64, and `I32` for a shift
/// amount or a comparison), and the width, signedness, and saturation bounds the
/// runtime applies. The alternative, generic 32-bit and 64-bit operations
/// followed by explicit normalization, needs an extra instruction after every
/// narrow result, and a program that leaves it out is valid and wrong.
///
/// A zero divisor has no answer, so the lowering tests a divisor and ends the
/// block in a [`Terminator::Trap`] before it divides. The runtime treats a
/// division it is asked to do by zero as an invalid program.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ScalarOp {
    /// Wrapping 64-bit addition.
    I64Add,
    /// Wrapping 64-bit subtraction.
    I64Sub,
    /// 64-bit equality, answering an `I32` `0` or `1`.
    I64Eq,
    /// Signed 64-bit less-than, answering an `I32` `0` or `1`.
    I64LtS,
    /// An operation of the unchecked integer family at an integer type.
    Int(IntOp, NumericType),
}

impl ScalarOp {
    /// The number of operands.
    #[must_use]
    pub const fn arity(self) -> usize {
        2
    }

    /// Every operation, in a fixed order: the four 64-bit operations, then each
    /// [`IntOp`] at each of [`INTEGER_TYPES`].
    #[must_use]
    pub fn all() -> Vec<Self> {
        let mut all = vec![Self::I64Add, Self::I64Sub, Self::I64Eq, Self::I64LtS];
        for numeric in INTEGER_TYPES {
            for op in IntOp::ALL {
                all.push(Self::Int(op, numeric));
            }
        }
        all
    }
}

/// One instruction. It never transfers control and never yields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Inst {
    /// Writes a scalar.
    Scalar {
        /// The slot written.
        dst: Slot,
        /// The scalar's class.
        class: ScalarClass,
        /// Its bits, zero-extended.
        bits: u64,
    },
    /// Writes a reference to a constant.
    Constant {
        /// The slot written.
        dst: Slot,
        /// The constant.
        constant: ConstId,
    },
    /// Writes another reference to the value of `src`, which stays full.
    Copy {
        /// The slot written.
        dst: Slot,
        /// The slot read.
        src: Slot,
    },
    /// Moves the value of `src`, which becomes empty.
    Move {
        /// The slot written.
        dst: Slot,
        /// The slot consumed.
        src: Slot,
    },
    /// Releases whatever the slot holds and leaves it empty.
    Drop {
        /// The slot emptied.
        slot: Slot,
    },
    /// A scalar operation over borrowed operands.
    Op {
        /// The slot written.
        dst: Slot,
        /// The operation.
        op: ScalarOp,
        /// Its operands.
        args: Vec<Slot>,
    },
    /// A row of the closed `@compiler` registry over borrowed operands.
    Intrinsic {
        /// The slot written.
        dst: Slot,
        /// The registry row.
        intrinsic: CompilerIntrinsic,
        /// Its operands, in the registry's order.
        args: Vec<Slot>,
        /// The origin of the call, for a row that can trap.
        origin: OriginId,
    },
    /// Assembles an object from consumed cells.
    Build {
        /// The slot written.
        dst: Slot,
        /// The object's kind.
        kind: ObjectKind,
        /// The variant index of an enum or the member index of a union, and
        /// `0` for every other kind.
        variant: u32,
        /// The cells, in order.
        cells: Vec<Slot>,
    },
    /// Writes another reference to one cell of a borrowed object.
    Cell {
        /// The slot written.
        dst: Slot,
        /// The object read.
        object: Slot,
        /// The cell's position.
        index: u32,
    },
    /// Writes the variant or member index of a borrowed object as an `I32`.
    Variant {
        /// The slot written.
        dst: Slot,
        /// The object read.
        object: Slot,
    },
    /// Builds a function value: the function and the consumed captures.
    Function {
        /// The slot written.
        dst: Slot,
        /// The body, a function that takes an environment.
        function: FuncId,
        /// The captured values, in order.
        captures: Vec<Slot>,
    },
}

/// What a call invokes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Callee {
    /// A function known at lowering time. It takes no environment.
    Direct(FuncId),
    /// The function value in a borrowed slot.
    Indirect(Slot),
}

/// How a block ends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Terminator {
    /// Continues at another block of this function.
    Jump(BlockId),
    /// Continues at `then_block` when the borrowed scalar is nonzero.
    Branch {
        /// The condition.
        cond: Slot,
        /// The block for a nonzero condition.
        then_block: BlockId,
        /// The block for zero.
        else_block: BlockId,
    },
    /// Continues at `targets[value]`, or at `default` past the list.
    Switch {
        /// The borrowed scalar selector.
        value: Slot,
        /// One block per value from zero.
        targets: Vec<BlockId>,
        /// The block for every other value.
        default: BlockId,
    },
    /// A non-tail call: pushes a frame, and continues at `resume` with the
    /// result in `dst` once the callee returns. A checkpoint.
    Call {
        /// The callee.
        callee: Callee,
        /// The consumed operands, in parameter order.
        args: Vec<Slot>,
        /// The slot the result is written to.
        dst: Slot,
        /// The block that continues.
        resume: BlockId,
    },
    /// A tail transfer: the callee replaces this frame. A checkpoint.
    TailCall {
        /// The callee.
        callee: Callee,
        /// The consumed operands, in parameter order.
        args: Vec<Slot>,
    },
    /// One host operation of the closed registry, performed exactly once, and
    /// then `resume`. A checkpoint.
    Host {
        /// The registry row.
        row: &'static HostRow,
        /// The borrowed operands, in the row's parameter order.
        args: Vec<Slot>,
        /// The slot the `(result t e)` value is written to.
        dst: Slot,
        /// The block that continues.
        resume: BlockId,
        /// The origin of the call.
        origin: OriginId,
    },
    /// Reads a module value, evaluating its initializer first if this is the
    /// first read of the execution. A checkpoint when it evaluates.
    Global {
        /// The module value.
        global: GlobalId,
        /// The slot the value is written to.
        dst: Slot,
        /// The block that continues.
        resume: BlockId,
    },
    /// Leaves the frame with the consumed value. A checkpoint.
    Return {
        /// The returned value.
        value: Slot,
    },
    /// Stops execution with a registered trap.
    Trap {
        /// The trap code.
        code: TrapCode,
        /// Its origin.
        origin: OriginId,
    },
}

/// A straight line of instructions and the terminator that ends it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    /// The instructions, in order.
    pub insts: Vec<Inst>,
    /// How the block ends.
    pub term: Terminator,
}

/// One function: the frame it needs and its blocks.
///
/// The frame is `[parameters][environment][everything else]`: slots
/// `0..params` are the operands, slot `params` is the function value itself
/// when `env` is set, and the rest are the lowering's bindings and temporaries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Function {
    /// A name for diagnostics of the toolchain itself. It is not an identity.
    pub name: String,
    /// The number of operands.
    pub params: u32,
    /// Whether the function is entered through a function value, which it
    /// receives in the slot after its parameters.
    pub env: bool,
    /// The size of the frame in slots.
    pub slots: u32,
    /// The blocks. Block `0` is the entry block.
    pub blocks: Vec<Block>,
}

impl Function {
    /// The slots a caller fills: the parameters and the environment.
    #[must_use]
    pub const fn entry_slots(&self) -> u32 {
        self.params.saturating_add(if self.env { 1 } else { 0 })
    }
}

/// A complete program in execution IR.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    /// Every function.
    pub functions: Vec<Function>,
    /// The entry: a function with no parameter and no environment.
    pub entry: FuncId,
    /// The initializer of each module value: a function with no parameter and
    /// no environment that returns the value.
    pub globals: Vec<FuncId>,
    /// The constants.
    pub constants: Vec<Constant>,
    /// The origin table: ordinal `n` is `origins[n - 1]`.
    pub origins: Vec<SourceOrigin>,
}

impl Program {
    /// The function an ID names.
    #[must_use]
    pub fn function(&self, id: FuncId) -> Option<&Function> {
        self.functions.get(id.index())
    }

    /// The origin an ordinal names, and `None` for [`OriginId::NONE`].
    #[must_use]
    pub fn origin(&self, id: OriginId) -> Option<&SourceOrigin> {
        let index = usize::try_from(id.0).ok()?.checked_sub(1)?;
        self.origins.get(index)
    }

    /// The registry symbols the program's host calls name, sorted.
    #[must_use]
    pub fn host_symbols(&self) -> BTreeSet<&'static str> {
        self.functions
            .iter()
            .flat_map(|function| &function.blocks)
            .filter_map(|block| match &block.term {
                Terminator::Host { row, .. } => Some(row.symbol),
                _ => None,
            })
            .collect()
    }

    /// The derived registry minor of the runtime chapter's "Registry
    /// versioning": the greatest `since` among the required symbols.
    #[must_use]
    pub fn registry_minor(&self) -> u32 {
        // Every symbol comes from a registry row, so the minor always exists.
        derived_registry_minor(self.host_symbols()).unwrap_or(0)
    }
}
