//! The natives over a module's linear memory.
//!
//! This is what the runner calls for an import of `vibra_native_v1`. It
//! implements [`Source`] and [`Sink`] over the arena objects of
//! `vibra_ir::arena` and runs the one set of algorithms of this crate, so the
//! module and the reference interpreter call the same code.
//!
//! Every offset the module hands over is checked before it is read: an object
//! must lie inside the memory, have the kind and stride the row expects, and
//! hold the payload its header claims. A violation is a [`Fault`], which the
//! runner reports as a defect of the toolchain, and never a read or a write
//! outside the arena.

use std::cell::Cell;
use std::fmt;

use vibra_ir::arena::{self, class, header, kind};
use vibra_ir::external::{
    Answer, CompilerIntrinsic, FloatOp, IntegerOp, NativeRow, NumericType, ObjectKind,
    Operand,
};

use crate::number::Parsed;
use crate::{Count, Element, Sink, Source};

/// A module's linear memory, as bytes that a source and a sink can share.
pub type Memory<'m> = &'m [Cell<u8>];

/// Views a mutable byte slice as [`Memory`].
#[must_use]
pub fn memory(bytes: &mut [u8]) -> Memory<'_> {
    Cell::from_mut(bytes).as_slice_of_cells()
}

/// Why a native import refused its operands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    /// An offset or a payload lies outside the memory.
    OutOfBounds,
    /// An object is not of the kind or layout the row takes.
    WrongObject,
    /// The call does not match the row's signature.
    Signature,
    /// The destination is not sized to what the row builds.
    Destination,
}

impl fmt::Display for Fault {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::OutOfBounds => "an object lies outside the memory",
            Self::WrongObject => "an object is not of the kind the native takes",
            Self::Signature => "the call does not match the native's signature",
            Self::Destination => "the destination is not sized to the result",
        })
    }
}

impl std::error::Error for Fault {}

fn read<const N: usize>(memory: Memory<'_>, at: usize) -> Result<[u8; N], Fault> {
    let end = at.checked_add(N).ok_or(Fault::OutOfBounds)?;
    let bytes = memory.get(at..end).ok_or(Fault::OutOfBounds)?;
    let mut out = [0_u8; N];
    for (slot, byte) in out.iter_mut().zip(bytes) {
        *slot = byte.get();
    }
    Ok(out)
}

fn write(memory: Memory<'_>, at: usize, bytes: &[u8]) -> Result<(), Fault> {
    let end = at.checked_add(bytes.len()).ok_or(Fault::OutOfBounds)?;
    let target = memory.get(at..end).ok_or(Fault::OutOfBounds)?;
    for (slot, byte) in target.iter().zip(bytes) {
        slot.set(*byte);
    }
    Ok(())
}

fn read_u32(memory: Memory<'_>, at: usize) -> Result<u32, Fault> {
    read::<4>(memory, at).map(u32::from_le_bytes)
}

fn read_u64(memory: Memory<'_>, at: usize) -> Result<u64, Fault> {
    read::<8>(memory, at).map(u64::from_le_bytes)
}

fn offset(value: u32) -> Result<usize, Fault> {
    usize::try_from(value).map_err(|_| Fault::OutOfBounds)
}

/// The layout of the payload a row expects of an operand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Layout {
    /// The Unicode scalars of a `str`.
    Chars,
    /// The bytes of `bytes`.
    Bytes,
    /// The cells of an `array` or a `dict`.
    Cells,
}

/// An arena object whose header and payload have been checked.
#[derive(Clone, Copy, Debug)]
pub struct Object<'m> {
    memory: Memory<'m>,
    at: usize,
    len: u64,
}

impl<'m> Object<'m> {
    fn open(memory: Memory<'m>, at: u32, layout: Layout) -> Result<Self, Fault> {
        let at = offset(at)?;
        let stored_kind = read::<1>(memory, at + offset(header::KIND)?)?[0];
        let stride = u32::from(read::<1>(memory, at + offset(header::STRIDE)?)?[0]);
        let len = read_u32(memory, at + offset(header::LEN)?)?;
        let (kinds, want_stride, payload): (&[u8], u32, u64) = match layout {
            Layout::Chars => (
                &[kind::STR],
                arena::STRIDE_CHARS,
                u64::from(header::SIZE) + 4 * u64::from(len),
            ),
            Layout::Bytes => (
                &[kind::BYTES],
                arena::STRIDE_BYTES,
                u64::from(header::SIZE) + u64::from(len),
            ),
            Layout::Cells => (
                &[kind::ARRAY, kind::DICT],
                arena::STRIDE_CELLS,
                u64::from(arena::cells_offset(len)) + 8 * u64::from(len),
            ),
        };
        if !kinds.contains(&stored_kind) || stride != want_stride {
            return Err(Fault::WrongObject);
        }
        let end = u64::try_from(at)
            .map_err(|_| Fault::OutOfBounds)?
            .checked_add(payload)
            .ok_or(Fault::OutOfBounds)?;
        let block = u64::from(read_u32(memory, at + offset(header::BLOCK_SIZE)?)?);
        if end > u64::try_from(memory.len()).unwrap_or(u64::MAX) || payload > block {
            return Err(Fault::OutOfBounds);
        }
        Ok(Self {
            memory,
            at,
            len: u64::from(len),
        })
    }

    /// The component count.
    #[must_use]
    pub const fn len(&self) -> u64 {
        self.len
    }

    /// Whether there is no component.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn payload(&self, stride: u64, index: u64) -> usize {
        // The payload was checked to lie inside the memory, so this cannot
        // overflow `usize` for an index below the length.
        self.at
            .wrapping_add(usize::try_from(header::SIZE).unwrap_or(0))
            .wrapping_add(usize::try_from(stride * index).unwrap_or(0))
    }
}

/// Walks a range of the units of an object.
#[derive(Debug)]
pub struct Walk<'m, U> {
    object: Object<'m>,
    next: u64,
    end: u64,
    read: fn(&Object<'m>, u64) -> Option<U>,
}

impl<U> Iterator for Walk<'_, U> {
    type Item = U;

    fn next(&mut self) -> Option<U> {
        if self.next >= self.end {
            return None;
        }
        let unit = (self.read)(&self.object, self.next)?;
        self.next += 1;
        Some(unit)
    }
}

fn walk<'m, U>(
    object: Object<'m>,
    from: u64,
    to: u64,
    read: fn(&Object<'m>, u64) -> Option<U>,
) -> Walk<'m, U> {
    Walk {
        object,
        next: from,
        end: to.min(object.len),
        read,
    }
}

fn read_char(object: &Object<'_>, index: u64) -> Option<u32> {
    read_u32(object.memory, object.payload(4, index)).ok()
}

fn read_byte(object: &Object<'_>, index: u64) -> Option<u8> {
    read::<1>(object.memory, object.payload(1, index))
        .ok()
        .map(|[byte]| byte)
}

fn read_cell(object: &Object<'_>, index: u64) -> Option<Element> {
    let class_at = object
        .at
        .checked_add(offset(header::SIZE).ok()?)?
        .checked_add(usize::try_from(index).ok()?)?;
    let cell_at = object
        .at
        .checked_add(
            offset(arena::cells_offset(u32::try_from(object.len).ok()?)).ok()?,
        )?
        .checked_add(usize::try_from(index.checked_mul(8)?).ok()?)?;
    Some(Element {
        class: read::<1>(object.memory, class_at).ok().map(|[byte]| byte)?,
        bits: read_u64(object.memory, cell_at).ok()?,
    })
}

/// The scalars of a `str`.
#[derive(Clone, Copy, Debug)]
pub struct Chars<'m>(Object<'m>);

impl<'m> Source for Chars<'m> {
    type Unit = u32;
    type Iter<'a>
        = Walk<'m, u32>
    where
        Self: 'a;

    fn len(&self) -> u64 {
        self.0.len
    }

    fn range(&self, from: u64, to: u64) -> Walk<'m, u32> {
        walk(self.0, from, to, read_char)
    }
}

/// The bytes of `bytes`.
#[derive(Clone, Copy, Debug)]
pub struct ByteSeq<'m>(Object<'m>);

impl<'m> Source for ByteSeq<'m> {
    type Unit = u8;
    type Iter<'a>
        = Walk<'m, u8>
    where
        Self: 'a;

    fn len(&self) -> u64 {
        self.0.len
    }

    fn range(&self, from: u64, to: u64) -> Walk<'m, u8> {
        walk(self.0, from, to, read_byte)
    }
}

/// The cells of an `array` or a `dict`.
#[derive(Clone, Copy, Debug)]
pub struct Cells<'m>(Object<'m>);

impl<'m> Source for Cells<'m> {
    type Unit = Element;
    type Iter<'a>
        = Walk<'m, Element>
    where
        Self: 'a;

    fn len(&self) -> u64 {
        self.0.len
    }

    fn range(&self, from: u64, to: u64) -> Walk<'m, Element> {
        walk(self.0, from, to, read_cell)
    }
}

/// Reads the cells of an array as the scalars they hold in their low bits: the
/// `char`s of an `(array char)` or the bytes of an `(array u8)`.
#[derive(Clone, Copy, Debug)]
pub struct Narrow<S, U>(S, std::marker::PhantomData<U>);

fn narrow<U: TryFrom<u64> + Default>(element: Element) -> U {
    U::try_from(element.bits).unwrap_or_default()
}

impl<S: Source<Unit = Element>, U: TryFrom<u64> + Copy + Default> Source
    for Narrow<S, U>
{
    type Unit = U;
    type Iter<'a>
        = std::iter::Map<S::Iter<'a>, fn(Element) -> U>
    where
        Self: 'a;

    fn len(&self) -> u64 {
        self.0.len()
    }

    fn range(&self, from: u64, to: u64) -> Self::Iter<'_> {
        self.0.range(from, to).map(narrow::<U> as fn(Element) -> U)
    }
}

/// A destination object being filled, which checks it received exactly the
/// units its header counts.
pub trait Built: Sink {
    /// Whether the object was filled exactly, and every reference it took was
    /// valid.
    ///
    /// # Errors
    ///
    /// A [`Fault`] when units are missing or in excess, or a reference lies
    /// outside the memory.
    fn finish(self) -> Result<u64, Fault>;
}

/// The bookkeeping of a destination: where the next unit goes.
#[derive(Debug)]
struct Fill<'m> {
    object: Object<'m>,
    next: u64,
    fault: Option<Fault>,
}

impl<'m> Fill<'m> {
    fn open(memory: Memory<'m>, at: u32, layout: Layout) -> Result<Self, Fault> {
        Ok(Self {
            object: Object::open(memory, at, layout)?,
            next: 0,
            fault: None,
        })
    }

    /// Takes the next place, or records that there is none.
    fn place(&mut self) -> Option<u64> {
        if self.next >= self.object.len {
            self.fault = Some(Fault::Destination);
            return None;
        }
        let place = self.next;
        self.next += 1;
        Some(place)
    }

    fn finish(self) -> Result<u64, Fault> {
        match self.fault {
            Some(fault) => Err(fault),
            None if self.next == self.object.len => Ok(self.next),
            None => Err(Fault::Destination),
        }
    }

    fn note(&mut self, result: Result<(), Fault>) {
        if let Err(fault) = result {
            self.fault.get_or_insert(fault);
        }
    }
}

/// The scalars of a new `str`.
#[derive(Debug)]
pub struct CharSink<'m>(Fill<'m>);

impl Sink for CharSink<'_> {
    type Unit = u32;

    fn put(&mut self, unit: u32) {
        if let Some(place) = self.0.place() {
            let at = self.0.object.payload(4, place);
            let written = write(self.0.object.memory, at, &unit.to_le_bytes());
            self.0.note(written);
        }
    }
}

impl Built for CharSink<'_> {
    fn finish(self) -> Result<u64, Fault> {
        self.0.finish()
    }
}

/// The bytes of a new `bytes`.
#[derive(Debug)]
pub struct ByteSink<'m>(Fill<'m>);

impl Sink for ByteSink<'_> {
    type Unit = u8;

    fn put(&mut self, unit: u8) {
        if let Some(place) = self.0.place() {
            let at = self.0.object.payload(1, place);
            let written = write(self.0.object.memory, at, &[unit]);
            self.0.note(written);
        }
    }
}

impl Built for ByteSink<'_> {
    fn finish(self) -> Result<u64, Fault> {
        self.0.finish()
    }
}

/// The cells of a new `array`. A cell that holds a reference takes one `dup` of
/// the object it names, which is the count the new array owns.
#[derive(Debug)]
pub struct CellSink<'m>(Fill<'m>);

impl CellSink<'_> {
    fn store(&mut self, place: u64, element: Element) -> Result<(), Fault> {
        let object = &self.0.object;
        let class_at = object.at
            + usize::try_from(header::SIZE).map_err(|_| Fault::OutOfBounds)?
            + usize::try_from(place).map_err(|_| Fault::OutOfBounds)?;
        let cell_at = object.at
            + usize::try_from(arena::cells_offset(
                u32::try_from(object.len).map_err(|_| Fault::OutOfBounds)?,
            ))
            .map_err(|_| Fault::OutOfBounds)?
            + usize::try_from(8 * place).map_err(|_| Fault::OutOfBounds)?;
        if element.class == class::REF {
            // The count lives in the first word of the referenced block.
            let child =
                offset(u32::try_from(element.bits).map_err(|_| Fault::WrongObject)?)?;
            let count = read_u32(object.memory, child + offset(header::COUNT)?)?;
            write(
                object.memory,
                child + offset(header::COUNT)?,
                &count.wrapping_add(1).to_le_bytes(),
            )?;
        }
        write(object.memory, class_at, &[element.class])?;
        write(object.memory, cell_at, &element.bits.to_le_bytes())
    }
}

impl Sink for CellSink<'_> {
    type Unit = Element;

    fn put(&mut self, unit: Element) {
        if let Some(place) = self.0.place() {
            let stored = self.store(place, unit);
            self.0.note(stored);
        }
    }
}

impl Built for CellSink<'_> {
    fn finish(self) -> Result<u64, Fault> {
        self.0.finish()
    }
}

/// A sink of scalars into the cells of a new array: the scalar is a cell of the
/// 32-bit integer class, holding it zero-extended.
#[derive(Debug)]
pub struct AsCells<K, U>(K, std::marker::PhantomData<U>);

impl<K: Sink<Unit = Element>, U: Into<u64> + Copy> Sink for AsCells<K, U> {
    type Unit = U;

    fn put(&mut self, unit: U) {
        self.0.put(Element {
            class: class::I32,
            bits: unit.into(),
        });
    }
}

impl<K: Built<Unit = Element>, U: Into<u64> + Copy> Built for AsCells<K, U> {
    fn finish(self) -> Result<u64, Fault> {
        self.0.finish()
    }
}

/// One operand of a call, as the row's signature reads it.
#[derive(Clone, Copy, Debug)]
enum Argument {
    Object(u32),
    Index(u64),
    Element(Element),
    Number(u64),
}

/// Reads each operand of `row` from the scalars of a call.
fn arguments(row: &NativeRow, scalars: &[i64]) -> Result<Vec<Argument>, Fault> {
    let mut taken = scalars.iter().copied();
    let mut next = || taken.next().ok_or(Fault::Signature);
    let narrow = |value: i64| u32::try_from(value).map_err(|_| Fault::OutOfBounds);
    let mut out = Vec::with_capacity(row.operands.len());
    for operand in row.operands {
        out.push(match operand {
            Operand::Text | Operand::Bytes | Operand::Cells => {
                Argument::Object(narrow(next()?)?)
            }
            Operand::Index => Argument::Index(next()?.cast_unsigned()),
            Operand::Number(_) => Argument::Number(next()?.cast_unsigned()),
            Operand::Element => {
                let bits = next()?.cast_unsigned();
                let class = u8::try_from(next()?).map_err(|_| Fault::Signature)?;
                Argument::Element(Element { class, bits })
            }
        });
    }
    Ok(out)
}

/// Runs a native import of `row` against `memory`.
///
/// `scalars` are the import's parameters in order, each widened to an `i64`, and
/// the result is the import's result: an `ordering` as `-1`, `0`, or `1`, or the
/// component count of what a building row built or would build, or `-1` for
/// the absence of one. See `vibra_ir::external::NativeRow` for the protocol of
/// the destination.
///
/// # Errors
///
/// A [`Fault`] when an operand or the destination is not an object the row
/// takes, or the call does not match the row's signature.
pub fn call(
    row: &NativeRow,
    memory: Memory<'_>,
    scalars: &[i64],
) -> Result<i64, Fault> {
    let operands = arguments(row, scalars)?;
    let destination = match row.answer {
        Answer::Object { .. } | Answer::Parsed => {
            let last = scalars.last().copied().ok_or(Fault::Signature)?;
            Some(u32::try_from(last).map_err(|_| Fault::OutOfBounds)?)
        }
        Answer::Order | Answer::Length | Answer::Same | Answer::Operand => None,
    };
    let object = |position: usize, layout: Layout| -> Result<Object<'_>, Fault> {
        match operands.get(position) {
            Some(Argument::Object(at)) => Object::open(memory, *at, layout),
            _ => Err(Fault::Signature),
        }
    };
    let index = |position: usize| match operands.get(position) {
        Some(Argument::Index(value)) => Ok(*value),
        _ => Err(Fault::Signature),
    };
    let element = |position: usize| match operands.get(position) {
        Some(Argument::Element(value)) => Ok(*value),
        _ => Err(Fault::Signature),
    };
    let number = |position: usize| match operands.get(position) {
        Some(Argument::Number(bits)) => Ok(*bits),
        _ => Err(Fault::Signature),
    };
    use CompilerIntrinsic as C;
    let built = match row.intrinsic {
        C::Integer(numeric, IntegerOp::ToStr) => {
            let value = decode_integer(numeric, number(0)?);
            build(
                destination,
                |at| Fill::open(memory, at, Layout::Chars).map(CharSink),
                |out: &mut dyn Sink<Unit = u32>| {
                    crate::number::integer_to_str(value, out);
                    true
                },
            )?
        }
        C::Float(numeric, FloatOp::ToStr) => {
            let bits = number(0)?;
            build(
                destination,
                |at| Fill::open(memory, at, Layout::Chars).map(CharSink),
                |out: &mut dyn Sink<Unit = u32>| {
                    crate::number::float_to_str(numeric, bits, out);
                    true
                },
            )?
        }
        C::Integer(numeric, IntegerOp::Parse) => {
            let text = Chars(object(0, Layout::Chars)?);
            let parsed = match crate::number::parse_integer(numeric, &text) {
                Parsed::Value(value) => Parsed::Value(encode_integer(numeric, value)),
                Parsed::InvalidFormat => Parsed::InvalidFormat,
                Parsed::OutOfRange => Parsed::OutOfRange,
            };
            return answer_parsed(memory, destination, parsed);
        }
        C::Float(numeric, FloatOp::Parse) => {
            let text = Chars(object(0, Layout::Chars)?);
            let parsed = crate::number::parse_float(numeric, &text);
            return answer_parsed(memory, destination, parsed);
        }
        C::TextLength => {
            return i64::try_from(crate::length(&Chars(object(0, Layout::Chars)?)))
                .map_err(|_| Fault::OutOfBounds);
        }
        C::BytesLength => {
            return i64::try_from(crate::length(&ByteSeq(object(0, Layout::Bytes)?)))
                .map_err(|_| Fault::OutOfBounds);
        }
        C::TextEqual => {
            let (a, b) = (
                Chars(object(0, Layout::Chars)?),
                Chars(object(1, Layout::Chars)?),
            );
            return Ok(i64::from(crate::same(&a, &b)));
        }
        C::BytesEqual => {
            let (a, b) = (
                ByteSeq(object(0, Layout::Bytes)?),
                ByteSeq(object(1, Layout::Bytes)?),
            );
            return Ok(i64::from(crate::same(&a, &b)));
        }
        // The packed tail is the collection: the answer is the operand, once
        // it is known to be an object of the layout.
        C::ArrayOf | C::DictOf => {
            let packed = object(0, Layout::Cells)?;
            return i64::try_from(crate::identity(packed.at))
                .map_err(|_| Fault::OutOfBounds);
        }
        C::TextCompare => {
            let (a, b) = (
                Chars(object(0, Layout::Chars)?),
                Chars(object(1, Layout::Chars)?),
            );
            return Ok(i64::from(crate::order_code(crate::compare(&a, &b))));
        }
        C::BytesCompare => {
            let (a, b) = (
                ByteSeq(object(0, Layout::Bytes)?),
                ByteSeq(object(1, Layout::Bytes)?),
            );
            return Ok(i64::from(crate::order_code(crate::compare(&a, &b))));
        }
        C::TextConcat => {
            let (a, b) = (
                Chars(object(0, Layout::Chars)?),
                Chars(object(1, Layout::Chars)?),
            );
            build(
                destination,
                |at| Fill::open(memory, at, Layout::Chars).map(CharSink),
                |out: &mut dyn Sink<Unit = u32>| {
                    crate::concat(&a, &b, out);
                    true
                },
            )?
        }
        C::TextSlice => {
            let a = Chars(object(0, Layout::Chars)?);
            let (start, end) = (index(1)?, index(2)?);
            build(
                destination,
                |at| Fill::open(memory, at, Layout::Chars).map(CharSink),
                |out: &mut dyn Sink<Unit = u32>| crate::slice(&a, start, end, out),
            )?
        }
        C::TextToChars => {
            let a = Chars(object(0, Layout::Chars)?);
            build(
                destination,
                |at| {
                    Fill::open(memory, at, Layout::Cells)
                        .map(|fill| AsCells(CellSink(fill), std::marker::PhantomData))
                },
                |out: &mut dyn Sink<Unit = u32>| {
                    crate::copy(&a, out);
                    true
                },
            )?
        }
        C::TextFromChars => {
            let a = Narrow::<_, u32>(
                Cells(object(0, Layout::Cells)?),
                std::marker::PhantomData,
            );
            build(
                destination,
                |at| Fill::open(memory, at, Layout::Chars).map(CharSink),
                |out: &mut dyn Sink<Unit = u32>| {
                    crate::copy(&a, out);
                    true
                },
            )?
        }
        C::TextToUtf8 => {
            let a = Chars(object(0, Layout::Chars)?);
            build(
                destination,
                |at| Fill::open(memory, at, Layout::Bytes).map(ByteSink),
                |out: &mut dyn Sink<Unit = u8>| {
                    crate::to_utf8(&a, out);
                    true
                },
            )?
        }
        C::TextFromUtf8 => {
            let a = ByteSeq(object(0, Layout::Bytes)?);
            build(
                destination,
                |at| Fill::open(memory, at, Layout::Chars).map(CharSink),
                |out: &mut dyn Sink<Unit = u32>| crate::from_utf8(&a, out),
            )?
        }
        C::BytesConcat => {
            let (a, b) = (
                ByteSeq(object(0, Layout::Bytes)?),
                ByteSeq(object(1, Layout::Bytes)?),
            );
            build(
                destination,
                |at| Fill::open(memory, at, Layout::Bytes).map(ByteSink),
                |out: &mut dyn Sink<Unit = u8>| {
                    crate::concat(&a, &b, out);
                    true
                },
            )?
        }
        C::BytesSlice => {
            let a = ByteSeq(object(0, Layout::Bytes)?);
            let (start, end) = (index(1)?, index(2)?);
            build(
                destination,
                |at| Fill::open(memory, at, Layout::Bytes).map(ByteSink),
                |out: &mut dyn Sink<Unit = u8>| crate::slice(&a, start, end, out),
            )?
        }
        C::BytesToArray => {
            let a = ByteSeq(object(0, Layout::Bytes)?);
            build(
                destination,
                |at| {
                    Fill::open(memory, at, Layout::Cells)
                        .map(|fill| AsCells(CellSink(fill), std::marker::PhantomData))
                },
                |out: &mut dyn Sink<Unit = u8>| {
                    crate::copy(&a, out);
                    true
                },
            )?
        }
        C::BytesFromArray => {
            let a = Narrow::<_, u8>(
                Cells(object(0, Layout::Cells)?),
                std::marker::PhantomData,
            );
            build(
                destination,
                |at| Fill::open(memory, at, Layout::Bytes).map(ByteSink),
                |out: &mut dyn Sink<Unit = u8>| {
                    crate::copy(&a, out);
                    true
                },
            )?
        }
        C::DictEntries => {
            let a = Cells(object(0, Layout::Cells)?);
            build(
                destination,
                |at| Fill::open(memory, at, Layout::Cells).map(CellSink),
                |out: &mut dyn Sink<Unit = Element>| {
                    crate::copy(&a, out);
                    true
                },
            )?
        }
        C::ArrayAppend => {
            let a = Cells(object(0, Layout::Cells)?);
            let item = element(1)?;
            build(
                destination,
                |at| Fill::open(memory, at, Layout::Cells).map(CellSink),
                |out: &mut dyn Sink<Unit = Element>| {
                    crate::append(&a, item, out);
                    true
                },
            )?
        }
        C::ArrayConcat => {
            let (a, b) = (
                Cells(object(0, Layout::Cells)?),
                Cells(object(1, Layout::Cells)?),
            );
            build(
                destination,
                |at| Fill::open(memory, at, Layout::Cells).map(CellSink),
                |out: &mut dyn Sink<Unit = Element>| {
                    crate::concat(&a, &b, out);
                    true
                },
            )?
        }
        C::ArraySlice => {
            let a = Cells(object(0, Layout::Cells)?);
            let (start, end) = (index(1)?, index(2)?);
            build(
                destination,
                |at| Fill::open(memory, at, Layout::Cells).map(CellSink),
                |out: &mut dyn Sink<Unit = Element>| crate::slice(&a, start, end, out),
            )?
        }
        _ => return Err(Fault::Signature),
    };
    Ok(built)
}

/// The integer a cell of `numeric` holds: a type of at most 32 bits is its
/// 32-bit pattern, zero-extended, and a 64-bit type is its 64 bits.
fn decode_integer(numeric: NumericType, bits: u64) -> i128 {
    let low = u32::try_from(bits & 0xFFFF_FFFF).unwrap_or(0);
    match numeric {
        NumericType::I8 | NumericType::I16 | NumericType::I32 => {
            i128::from(low.cast_signed())
        }
        NumericType::U8 | NumericType::U16 | NumericType::U32 => i128::from(low),
        NumericType::I64 => i128::from(bits.cast_signed()),
        NumericType::U64 | NumericType::F32 | NumericType::F64 => i128::from(bits),
    }
}

/// The cell of `numeric` that holds `value`, which the type contains.
fn encode_integer(numeric: NumericType, value: i128) -> u64 {
    let pattern = u64::try_from(value & i128::from(u64::MAX)).unwrap_or(0);
    if numeric.bits() == 64 {
        pattern
    } else {
        pattern & 0xFFFF_FFFF
    }
}

/// Answers a row that reads a number: writes the cell at the destination when
/// the text spelled one, and answers the status of the row's protocol.
fn answer_parsed(
    memory: Memory<'_>,
    destination: Option<u32>,
    parsed: Parsed<u64>,
) -> Result<i64, Fault> {
    let at = match destination {
        Some(at) if at != 0 => offset(at)?,
        _ => return Err(Fault::Destination),
    };
    match parsed {
        Parsed::Value(bits) => {
            write(memory, at, &bits.to_le_bytes())?;
            Ok(0)
        }
        Parsed::InvalidFormat => Ok(1),
        Parsed::OutOfRange => Ok(2),
    }
}

/// Measures what a building row makes, or fills the destination with it.
///
/// With no destination (`0`) the row only counts and answers the count, or `-1`
/// when the row has no object. With one, the destination is opened (which
/// checks its kind and that its length is what it was allocated to), the row
/// fills it, and the answer is its length.
fn build<K: Built>(
    destination: Option<u32>,
    open: impl FnOnce(u32) -> Result<K, Fault>,
    run: impl Fn(&mut dyn Sink<Unit = K::Unit>) -> bool,
) -> Result<i64, Fault> {
    let at = destination.unwrap_or(0);
    if at == 0 {
        let mut count = Count::<K::Unit>::default();
        if !run(&mut count) {
            return Ok(-1);
        }
        return i64::try_from(count.units).map_err(|_| Fault::Destination);
    }
    let mut sink = open(at)?;
    if !run(&mut sink) {
        return Ok(-1);
    }
    let length = sink.finish()?;
    i64::try_from(length).map_err(|_| Fault::Destination)
}

/// The kinds of object a row builds, for the module's allocation.
#[must_use]
pub const fn object_code(kind: ObjectKind) -> u8 {
    match kind {
        ObjectKind::Str => kind::STR,
        ObjectKind::Bytes => kind::BYTES,
        ObjectKind::Array => kind::ARRAY,
    }
}
