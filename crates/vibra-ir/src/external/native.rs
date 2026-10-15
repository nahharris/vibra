//! The closed list of native imports: the registry rows that a WebAssembly
//! module reaches through the `vibra_native_v1` import module.
//!
//! `docs/spec/06-runtime.md`, "Native implementations": every native
//! implementation, and every primitive row whose algorithm needs a loop, is
//! toolchain-owned native code written once. The reference interpreter calls it
//! directly; a module imports it, by the names and scalar signatures this table
//! fixes, and the runner supplies it over the module's linear memory. The
//! emitter depends on this table and never on the native code; the native code
//! (`vibra-native`) and the runner depend on it too, so a signature is written
//! once and cannot drift.
//!
//! # The protocol
//!
//! An import is pure: it takes scalars and offsets of arena objects, reads and
//! writes only the module's memory, never allocates, and never calls back into
//! the module. A row that **builds** an object cannot know the object's size
//! before it runs, and the arena's allocator is the module's own, so it takes
//! the destination as its last operand and is called twice by the code the
//! emitter writes:
//!
//! 1. with a destination of `0`, it measures: it returns the component count of
//!    the object it would build, or `-1` when the row's result is the absence of
//!    one (`none`, or the `invalid-format` error), and writes nothing; and
//! 2. with the destination the module allocated to that size, it fills it and
//!    returns the same count. The cells of an object that hold references take
//!    one `dup` each, so the counts stay exact, and nothing is dropped.
//!
//! A row that answers a scalar returns it directly: an `ordering` as `-1`, `0`,
//! or `1`.
//!
//! A native whose meaning applies a function value, such as `array.fold`, is
//! not in this table, because a pure import cannot call back into the module:
//! its module runs the Vibra body.
//!
//! # What the table holds
//!
//! One property decides how a row is lowered. A **native implementation** (the
//! standard library declares it with `native:`) and a **looping primitive row**
//! are imports, and both are rows of this table; a native that applies a function
//! value is neither, and a module runs its body. Only a primitive row that does
//! not loop is a few instructions the emitter writes inline (`array.length` and
//! the integer and `char` rows): [`CompilerIntrinsic::is_native`] says which side a
//! row is on, and the emitter's inline table holds no native.
//!
//! `array.of` and `dict.of` are natives whose meaning is the packed tail they
//! receive, so their import answers its operand; `text.length`, `bytes.length`,
//! `text.equal`, and `bytes.equal` answer a count and a `bool`.

use super::CompilerIntrinsic;

/// A WebAssembly scalar of an import's signature.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scalar {
    /// `i32`: an offset of an arena object, a class byte, or an ordering.
    I32,
    /// `i64`: an index, or a component count.
    I64,
}

/// What an operand of a native import is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operand {
    /// A `str` object, as its offset (`i32`).
    Text,
    /// A `bytes` object, as its offset (`i32`).
    Bytes,
    /// An `array` or `dict` object, which share the cells layout, as its
    /// offset (`i32`).
    Cells,
    /// A `u64` index (`i64`).
    Index,
    /// One cell: its 64 bits (`i64`) and then its class byte (`i32`).
    Element,
}

impl Operand {
    /// The scalars the operand is passed as.
    #[must_use]
    pub const fn scalars(self) -> &'static [Scalar] {
        match self {
            Self::Text | Self::Bytes | Self::Cells => &[Scalar::I32],
            Self::Index => &[Scalar::I64],
            Self::Element => &[Scalar::I64, Scalar::I32],
        }
    }
}

/// The kind of object a building row makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectKind {
    /// A `str`.
    Str,
    /// `bytes`.
    Bytes,
    /// An `array`.
    Array,
}

/// When a building row has no object to build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Totality {
    /// Never: the row always builds an object.
    Total,
    /// For some operands: the result is an `option`, and `none` is the count
    /// `-1`.
    Option,
    /// For some operands: the result is a `(result t conversion-error)`, and
    /// `err invalid-format` is the count `-1`.
    Conversion,
}

/// What a native import answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    /// An `ordering` of the two operands, as `-1`, `0`, or `1` (`i32`).
    Order,
    /// The number of components of the operand (`i64`).
    Length,
    /// Whether the two operands hold the same components, as `0` or `1` (`i32`).
    Same,
    /// The operand itself, as the offset it was given (`i32`): the native whose
    /// meaning is the packed tail it receives.
    Operand,
    /// An object, built into the destination the last operand names.
    Object {
        /// The kind of object.
        kind: ObjectKind,
        /// Whether the row can have no object.
        totality: Totality,
    },
}

/// One row of the table: a registry row and the native import that implements
/// it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeRow {
    /// The registry row. The import's name is its symbol.
    pub intrinsic: CompilerIntrinsic,
    /// The operands, in the order of the row's signature.
    pub operands: &'static [Operand],
    /// What the import answers.
    pub answer: Answer,
}

impl NativeRow {
    /// The name of the import, which is the registry symbol.
    #[must_use]
    pub fn name(&self) -> &'static str {
        self.intrinsic.symbol()
    }

    /// The parameters of the import: each operand's scalars, then the
    /// destination (`i32`) of a building row.
    #[must_use]
    pub fn params(&self) -> Vec<Scalar> {
        let mut params = self
            .operands
            .iter()
            .flat_map(|operand| operand.scalars().iter().copied())
            .collect::<Vec<_>>();
        if matches!(self.answer, Answer::Object { .. }) {
            params.push(Scalar::I32);
        }
        params
    }

    /// The scalar the import returns.
    #[must_use]
    pub const fn result(&self) -> Scalar {
        match self.answer {
            Answer::Order | Answer::Same | Answer::Operand => Scalar::I32,
            Answer::Length | Answer::Object { .. } => Scalar::I64,
        }
    }
}

const fn row(
    intrinsic: CompilerIntrinsic,
    operands: &'static [Operand],
    answer: Answer,
) -> NativeRow {
    NativeRow {
        intrinsic,
        operands,
        answer,
    }
}

const fn object(kind: ObjectKind, totality: Totality) -> Answer {
    Answer::Object { kind, totality }
}

use ObjectKind::{Array, Bytes as BytesObject, Str};
use Operand::{Bytes, Cells, Element, Index, Text};
use Totality::{Conversion, Option as Maybe, Total};

/// Every native import, in canonical registry order. The row's own place in this
/// list is nothing a module can see: a module imports the rows it uses, in this
/// order.
pub const NATIVE_ROWS: [NativeRow; 22] = [
    row(
        CompilerIntrinsic::TextConcat,
        &[Text, Text],
        object(Str, Total),
    ),
    row(CompilerIntrinsic::TextLength, &[Text], Answer::Length),
    row(CompilerIntrinsic::TextEqual, &[Text, Text], Answer::Same),
    row(CompilerIntrinsic::TextCompare, &[Text, Text], Answer::Order),
    row(
        CompilerIntrinsic::TextSlice,
        &[Text, Index, Index],
        object(Str, Maybe),
    ),
    row(
        CompilerIntrinsic::TextToChars,
        &[Text],
        object(Array, Total),
    ),
    row(
        CompilerIntrinsic::TextFromChars,
        &[Cells],
        object(Str, Total),
    ),
    row(
        CompilerIntrinsic::TextToUtf8,
        &[Text],
        object(BytesObject, Total),
    ),
    row(
        CompilerIntrinsic::TextFromUtf8,
        &[Bytes],
        object(Str, Conversion),
    ),
    row(
        CompilerIntrinsic::BytesConcat,
        &[Bytes, Bytes],
        object(BytesObject, Total),
    ),
    row(CompilerIntrinsic::BytesLength, &[Bytes], Answer::Length),
    row(CompilerIntrinsic::BytesEqual, &[Bytes, Bytes], Answer::Same),
    row(
        CompilerIntrinsic::BytesCompare,
        &[Bytes, Bytes],
        Answer::Order,
    ),
    row(
        CompilerIntrinsic::BytesSlice,
        &[Bytes, Index, Index],
        object(BytesObject, Maybe),
    ),
    row(
        CompilerIntrinsic::BytesToArray,
        &[Bytes],
        object(Array, Total),
    ),
    row(
        CompilerIntrinsic::BytesFromArray,
        &[Cells],
        object(BytesObject, Total),
    ),
    row(CompilerIntrinsic::ArrayOf, &[Cells], Answer::Operand),
    row(CompilerIntrinsic::DictOf, &[Cells], Answer::Operand),
    row(
        CompilerIntrinsic::DictEntries,
        &[Cells],
        object(Array, Total),
    ),
    row(
        CompilerIntrinsic::ArrayAppend,
        &[Cells, Element],
        object(Array, Total),
    ),
    row(
        CompilerIntrinsic::ArrayConcat,
        &[Cells, Cells],
        object(Array, Total),
    ),
    row(
        CompilerIntrinsic::ArraySlice,
        &[Cells, Index, Index],
        object(Array, Maybe),
    ),
];

impl CompilerIntrinsic {
    /// The native import that implements this row, or `None` for a row that is
    /// lowered inline, whose meaning is the packed tail, or whose meaning
    /// applies a function value.
    #[must_use]
    pub fn native_import(self) -> Option<NativeRow> {
        NATIVE_ROWS
            .iter()
            .find(|row| row.intrinsic == self)
            .copied()
    }
}
