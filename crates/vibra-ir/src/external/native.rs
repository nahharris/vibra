//! The closed table of native rows: the registry rows that toolchain-owned
//! native code implements over the bytes of a value arena.
//!
//! `docs/spec/06-runtime.md`, "Native implementations": every native
//! implementation, and every primitive row whose algorithm needs a loop, is
//! toolchain-owned native code written once. A runtime calls it by the names
//! and scalar signatures this table fixes, over the arena it owns. A code
//! generator depends on this table and never on the native code; the native
//! code (`vibra-native`) depends on it too, so a signature is written once
//! and cannot drift.
//!
//! # The protocol
//!
//! A native call is pure: it takes scalars and offsets of arena objects, reads
//! and writes only the arena's bytes, never allocates, and never calls back
//! into its caller. A row that **builds** an object cannot know the object's
//! size before it runs, and the arena's allocator is its owner's, so it takes
//! the destination as its last operand and is called twice:
//!
//! 1. with a destination of `0`, it measures: it returns the component count of
//!    the object it would build, or `-1` when the row's result is the absence of
//!    one (`none`, or the `invalidFormat` error), and writes nothing; and
//! 2. with the destination the caller allocated to that size, it fills it and
//!    returns the same count. The cells of an object that hold references take
//!    one `dup` each, so the counts stay exact, and nothing is dropped.
//!
//! A row that answers a scalar returns it directly. A row that reads a number from text (`parse`) builds no object
//! and needs no allocation either: it takes the address of an 8-byte cell the
//! caller owns as its last operand, writes the number there in the cell format
//! of its type when the text spells one, and returns a status (`0` for a
//! number, `1` for `invalidFormat`, `2` for `outOfRange`). A row that takes a
//! number (`toStr`) receives the 64 bits of its cell as one `i64`, which its
//! type, fixed by the row, decodes; an `F32` or `F64` is its IEEE bit pattern,
//! so a NaN reaches the native code exactly as the caller holds it and the
//! native code decides what it observes.
//!
//! A row whose meaning applies a function value, such as `list.fold`, is not
//! in this table, because a native call cannot call back into its caller.
//!
//! # What the table holds
//!
//! The looping primitive rows: the collection rows that copy, and the 20 rows
//! that print and read numbers, `toStr` and `parse` of the eight integer types
//! and of `F32` and `F64`. A primitive row that does not loop is a few
//! instructions a code generator writes inline, and none of those is here.
use super::{CompilerIntrinsic, FloatOp, FloatType, IntegerOp, IntegerType};

/// A scalar of a native row's signature.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scalar {
    /// `i32`: an offset of an arena object, a class byte, or a status.
    I32,
    /// `i64`: an index, or a component count.
    I64,
}

/// A numeric type whose text a native row prints or reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Numeric {
    /// An integer type.
    Integer(IntegerType),
    /// A floating-point type.
    Float(FloatType),
}

impl Numeric {
    /// The bit width of the type.
    #[must_use]
    pub const fn bits(self) -> u32 {
        match self {
            Self::Integer(integer) => integer.bits(),
            Self::Float(FloatType::F32) => 32,
            Self::Float(FloatType::F64) => 64,
        }
    }
}

/// What an operand of a native row is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operand {
    /// A `Str` object, as its offset (`i32`).
    Text,
    /// A `Bytes` object, as its offset (`i32`).
    Bytes,
    /// A `List` or `Dict` object, which share the cells layout, as its offset
    /// (`i32`).
    Cells,
    /// A `U64` index (`i64`).
    Index,
    /// One cell: its 64 bits (`i64`) and then its class byte (`i32`).
    Element,
    /// A number of this type, as the 64 bits of its cell (`i64`): an integer
    /// of at most 32 bits is its 32-bit two's complement pattern zero-extended,
    /// and a float is its IEEE bit pattern, zero-extended for `F32`.
    Number(Numeric),
}

impl Operand {
    /// The scalars the operand is passed as.
    #[must_use]
    pub const fn scalars(self) -> &'static [Scalar] {
        match self {
            Self::Text | Self::Bytes | Self::Cells => &[Scalar::I32],
            Self::Index | Self::Number(_) => &[Scalar::I64],
            Self::Element => &[Scalar::I64, Scalar::I32],
        }
    }
}

/// The kind of object a building row makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectKind {
    /// A `Str`.
    Str,
    /// A `Bytes`.
    Bytes,
    /// A `List`.
    List,
}

/// When a building row has no object to build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Totality {
    /// Never: the row always builds an object.
    Total,
    /// For some operands: the result is an `Option`, and `none()` is the count
    /// `-1`.
    Option,
    /// For some operands: the result is a `Result(T ConversionError)`, and
    /// `err(error: ConversionError.invalidFormat())` is the count `-1`.
    Conversion,
}

/// What a native row answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    /// The number of components of the operand (`i64`).
    Length,
    /// Whether the two operands hold the same components, as `0` or `1`
    /// (`i32`).
    Same,
    /// A number the row reads from text, written into the 8-byte cell at the
    /// destination the last operand (`i32`) names, which is a cell the
    /// caller owns, and its status (`i32`): `0` when the text spells a
    /// number, which is then in the cell; `1` for `invalidFormat`; `2` for
    /// `outOfRange`. The cell is written only when the status is `0`.
    Parsed,
    /// An object, built into the destination the last operand names.
    Object {
        /// The kind of object.
        kind: ObjectKind,
        /// Whether the row can have no object.
        totality: Totality,
    },
}

/// One row of the table: a registry row and the signature of the native
/// routine that implements it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeRow {
    /// The registry row. The routine's name is its symbol.
    pub intrinsic: CompilerIntrinsic,
    /// The operands, in the order of the row's signature.
    pub operands: &'static [Operand],
    /// What the routine answers.
    pub answer: Answer,
}

impl NativeRow {
    /// The name of the routine, which is the registry symbol.
    #[must_use]
    pub fn name(&self) -> String {
        self.intrinsic.symbol()
    }

    /// The parameters of the routine: each operand's scalars, then the
    /// destination (`i32`) of a building row.
    #[must_use]
    pub fn params(&self) -> Vec<Scalar> {
        let mut params = self
            .operands
            .iter()
            .flat_map(|operand| operand.scalars().iter().copied())
            .collect::<Vec<_>>();
        if matches!(self.answer, Answer::Object { .. } | Answer::Parsed) {
            params.push(Scalar::I32);
        }
        params
    }

    /// The scalar the routine returns.
    #[must_use]
    pub const fn result(&self) -> Scalar {
        match self.answer {
            Answer::Same | Answer::Parsed => Scalar::I32,
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

use ObjectKind::{Bytes as BytesObject, List, Str};
use Operand::{Bytes, Cells, Element, Index, Number, Text};
use Totality::{Conversion, Option as Maybe, Total};

/// The table: for each numeric type, its `toStr` row (which builds a `Str`
/// from the number's cell) and its `parse` row (which reads a `Str` and writes
/// the number's cell), then the rows that follow the numeric ones.
macro_rules! table {
    (
        $(($kind:ident, $op:ident, $numeric:ident, $owner:ident)),* $(,)?;
        $($rest:expr),* $(,)?
    ) => {
        [
            $(
                row(
                    CompilerIntrinsic::$kind($owner::$numeric, $op::ToStr),
                    &[Number(Numeric::$kind($owner::$numeric))],
                    object(Str, Total),
                ),
                row(
                    CompilerIntrinsic::$kind($owner::$numeric, $op::Parse),
                    &[Text],
                    Answer::Parsed,
                ),
            )*
            $($rest,)*
        ]
    };
}

/// Every native row. A row's place in this list is nothing a program can
/// observe.
pub const NATIVE_ROWS: [NativeRow; 38] = table![
    (Integer, IntegerOp, I8, IntegerType),
    (Integer, IntegerOp, I16, IntegerType),
    (Integer, IntegerOp, I32, IntegerType),
    (Integer, IntegerOp, I64, IntegerType),
    (Integer, IntegerOp, U8, IntegerType),
    (Integer, IntegerOp, U16, IntegerType),
    (Integer, IntegerOp, U32, IntegerType),
    (Integer, IntegerOp, U64, IntegerType),
    (Float, FloatOp, F32, FloatType),
    (Float, FloatOp, F64, FloatType);
    row(CompilerIntrinsic::StrConcat, &[Text, Text], object(Str, Total)),
    row(CompilerIntrinsic::StrLength, &[Text], Answer::Length),
    row(CompilerIntrinsic::StrEqual, &[Text, Text], Answer::Same),
    row(
        CompilerIntrinsic::StrSlice,
        &[Text, Index, Index],
        object(Str, Maybe),
    ),
    row(CompilerIntrinsic::StrToChars, &[Text], object(List, Total)),
    row(CompilerIntrinsic::StrFromChars, &[Cells], object(Str, Total)),
    row(
        CompilerIntrinsic::StrToUtf8,
        &[Text],
        object(BytesObject, Total),
    ),
    row(
        CompilerIntrinsic::StrFromUtf8,
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
        CompilerIntrinsic::BytesSlice,
        &[Bytes, Index, Index],
        object(BytesObject, Maybe),
    ),
    row(CompilerIntrinsic::BytesToList, &[Bytes], object(List, Total)),
    row(
        CompilerIntrinsic::BytesFromList,
        &[Cells],
        object(BytesObject, Total),
    ),
    row(CompilerIntrinsic::DictEntries, &[Cells], object(List, Total)),
    row(
        CompilerIntrinsic::ListAppend,
        &[Cells, Element],
        object(List, Total),
    ),
    row(
        CompilerIntrinsic::ListConcat,
        &[Cells, Cells],
        object(List, Total),
    ),
    row(
        CompilerIntrinsic::ListSlice,
        &[Cells, Index, Index],
        object(List, Maybe),
    ),
];

impl CompilerIntrinsic {
    /// The native row that implements this row, or `None` for a row that is
    /// lowered inline or whose meaning applies a function value.
    #[must_use]
    pub fn native_import(self) -> Option<NativeRow> {
        NATIVE_ROWS
            .iter()
            .find(|row| row.intrinsic == self)
            .copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_native_row_is_a_pure_row_of_the_registry_once() {
        let all = CompilerIntrinsic::all();
        let mut seen = std::collections::BTreeSet::new();
        for native in &NATIVE_ROWS {
            assert!(all.contains(&native.intrinsic), "{}", native.name());
            assert!(native.intrinsic.is_pure(), "{}", native.name());
            assert!(seen.insert(native.name()), "{} is repeated", native.name());
            let (parameters, _) = native.intrinsic.signature();
            assert_eq!(native.operands.len(), parameters.len(), "{}", native.name());
        }
    }

    #[test]
    fn a_row_that_applies_a_function_value_is_not_native() {
        assert_eq!(CompilerIntrinsic::ListFold.native_import(), None);
        assert!(CompilerIntrinsic::StrConcat.native_import().is_some());
    }
}
