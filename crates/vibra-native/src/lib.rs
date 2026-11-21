//! Toolchain-owned native code, written once against the shared value-storage
//! interface.
//!
//! `docs/spec/06-runtime.md`, "Native implementations": every native
//! implementation, and every primitive row whose algorithm needs a loop, is
//! native code that the toolchain owns. A runtime calls these functions over
//! the value arena it owns. The Vibra body of a native stays its meaning, and
//! the shared sample vectors hold this code to it.
//!
//! # The value-access interface
//!
//! The algorithms here know no value representation. They read a sequence
//! through [`Source`] and build one through [`Sink`], and a backend implements
//! the two for its own values:
//!
//! - [`arena`] implements them over the arena objects of a runtime's value
//!   arena, which is what a runtime calls. A `Str` is read as its Unicode
//!   scalars, a `Bytes` as its bytes, and a `List` or `Dict` as its cells, so
//!   a function never branches on a value kind;
//! - the host tests implement them over slices.
//!
//! [`number`] is the same code for the rows that print and read numbers: it
//! reads the scalars of a `Str` through a [`Source`] and writes them through a
//! [`Sink`], and takes and answers a number as a value (an integer as an `i128`,
//! a float as its IEEE bits), so it is the one implementation of integer and
//! float `toStr` and `parse`.
//!
//! A [`Sink`] can also only count, so a row that builds an object of a size it
//! does not know in advance runs once to measure and once to fill, which is how
//! the arena's own allocator stays the one allocator. The code never allocates
//! in an arena, never drops, and calls back into nothing.
//!
//! This crate depends on `vibra-ir` for the closed table of native rows and
//! the shape of an arena object, and on nothing else.

pub mod arena;
pub mod number;
mod utf8;

use std::marker::PhantomData;

pub use utf8::{decode, encode};

/// One cell of the cells layout: what the cell holds, and its 64 bits. A cell of
/// the reference class holds the offset of an arena object in its low 32 bits.
/// This is the unit of a `List`, and of the entries of a `Dict`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Element {
    /// The class byte of `vibra_ir::arena::class`.
    pub class: u8,
    /// The cell's bits.
    pub bits: u64,
}

/// A sequence of units to read: the scalars of a `Str`, the bytes of a
/// `Bytes`, or the elements of a `List`.
pub trait Source {
    /// One unit.
    type Unit: Copy;

    /// The iterator over a range of units.
    type Iter<'a>: Iterator<Item = Self::Unit>
    where
        Self: 'a;

    /// The number of units.
    fn len(&self) -> u64;

    /// Whether there is no unit.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The units in `from..to`, in order. The caller keeps both within
    /// [`Source::len`]; a range outside it yields nothing.
    fn range(&self, from: u64, to: u64) -> Self::Iter<'_>;
}

/// A sequence of units to build.
pub trait Sink {
    /// One unit.
    type Unit: Copy;

    /// Appends a unit.
    fn put(&mut self, unit: Self::Unit);
}

/// A sink that only counts, for measuring what a row would build.
#[derive(Debug)]
pub struct Count<U> {
    /// The units put so far.
    pub units: u64,
    unit: PhantomData<U>,
}

impl<U> Default for Count<U> {
    fn default() -> Self {
        Self {
            units: 0,
            unit: PhantomData,
        }
    }
}

impl<U: Copy> Sink for Count<U> {
    type Unit = U;

    fn put(&mut self, _unit: U) {
        self.units = self.units.saturating_add(1);
    }
}

/// All of `source`: `str.toChars`, `str.fromChars`, `bytes.toList`,
/// `bytes.fromList`, and `dict.entries` copy a sequence into the unit of
/// another kind of object.
pub fn copy<S: Source, K: Sink<Unit = S::Unit> + ?Sized>(source: &S, out: &mut K) {
    for unit in source.range(0, source.len()) {
        out.put(unit);
    }
}

/// `str.concat`, `bytes.concat`, and `list.concat`: the units of `first`, then
/// those of `second`.
pub fn concat<S, T, K>(first: &S, second: &T, out: &mut K)
where
    S: Source,
    T: Source<Unit = S::Unit>,
    K: Sink<Unit = S::Unit> + ?Sized,
{
    copy(first, out);
    copy(second, out);
}

/// `list.append`: the units of `source`, then one more.
pub fn append<S: Source, K: Sink<Unit = S::Unit> + ?Sized>(
    source: &S,
    unit: S::Unit,
    out: &mut K,
) {
    copy(source, out);
    out.put(unit);
}

/// `str.slice`, `bytes.slice`, and `list.slice`: the units in the half-open
/// range `start..end`, or `false` and nothing when `start` exceeds `end` or
/// `end` exceeds the length.
pub fn slice<S: Source, K: Sink<Unit = S::Unit> + ?Sized>(
    source: &S,
    start: u64,
    end: u64,
    out: &mut K,
) -> bool {
    if start > end || end > source.len() {
        return false;
    }
    for unit in source.range(start, end) {
        out.put(unit);
    }
    true
}

/// `str.length`, `bytes.length`: the number of units.
pub fn length<S: Source>(source: &S) -> u64 {
    source.len()
}

/// `str.equal`, `bytes.equal`: the same units in the same order.
pub fn same<S, T>(first: &S, second: &T) -> bool
where
    S: Source,
    T: Source<Unit = S::Unit>,
    S::Unit: PartialEq,
{
    first.len() == second.len()
        && first
            .range(0, first.len())
            .zip(second.range(0, second.len()))
            .all(|(left, right)| left == right)
}

/// `str.toUtf8`: the UTF-8 encoding of the scalars of `source`.
pub fn to_utf8<S, K>(source: &S, out: &mut K)
where
    S: Source<Unit = u32>,
    K: Sink<Unit = u8> + ?Sized,
{
    for scalar in source.range(0, source.len()) {
        encode(scalar, out);
    }
}

/// `str.fromUtf8`: the scalars of well-formed UTF-8, or `false` for an
/// ill-formed sequence: an overlong form, a surrogate, a value above U+10FFFF, a
/// stray continuation byte, or a truncated sequence. What a sink received before
/// `false` is of no use.
pub fn from_utf8<S, K>(source: &S, out: &mut K) -> bool
where
    S: Source<Unit = u8>,
    K: Sink<Unit = u32> + ?Sized,
{
    decode(source, out)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::indexing_slicing, clippy::unwrap_used)]
mod tests {
    use super::*;

    struct Slice<'a, U>(&'a [U]);

    impl<U: Copy> Source for Slice<'_, U> {
        type Unit = U;
        type Iter<'a>
            = std::iter::Copied<std::slice::Iter<'a, U>>
        where
            Self: 'a;

        fn len(&self) -> u64 {
            self.0.len() as u64
        }

        fn range(&self, from: u64, to: u64) -> Self::Iter<'_> {
            let (from, to) =
                (usize::try_from(from).unwrap(), usize::try_from(to).unwrap());
            self.0.get(from..to).unwrap_or(&[]).iter().copied()
        }
    }

    struct Collect<U>(Vec<U>);

    impl<U: Copy> Sink for Collect<U> {
        type Unit = U;
        fn put(&mut self, unit: U) {
            self.0.push(unit);
        }
    }

    fn run<U: Copy>(f: impl FnOnce(&mut Collect<U>)) -> Vec<U> {
        let mut out = Collect(Vec::new());
        f(&mut out);
        out.0
    }

    #[test]
    fn concat_and_append_keep_the_order_of_their_units() {
        let (a, b) = (Slice(&[1_u8, 2]), Slice(&[3_u8]));
        assert_eq!(run::<u8>(|out| concat(&a, &b, out)), [1, 2, 3]);
        assert_eq!(run::<u8>(|out| append(&a, 9, out)), [1, 2, 9]);
        let empty = Slice::<u8>(&[]);
        assert_eq!(run::<u8>(|out| concat(&empty, &empty, out)), []);
    }

    #[test]
    fn a_slice_is_none_when_start_exceeds_end_or_end_exceeds_the_length() {
        let a = Slice(&[1_u32, 2, 3]);
        let mut out = Collect(Vec::new());
        assert!(slice(&a, 1, 3, &mut out));
        assert_eq!(out.0, [2, 3]);
        for (start, end) in
            [(2, 1), (0, 4), (4, 4), (0, u64::MAX), (u64::MAX, u64::MAX)]
        {
            let mut out = Collect(Vec::new());
            assert!(!slice(&a, start, end, &mut out), "{start}..{end}");
            assert!(out.0.is_empty());
        }
        let mut out = Collect(Vec::new());
        assert!(
            slice(&a, 3, 3, &mut out),
            "an empty range at the end is some"
        );
        assert!(out.0.is_empty());
    }

    #[test]
    fn utf8_agrees_with_the_standard_library_on_every_scalar_and_on_ill_formed_input() {
        for scalar in (0..=0x10_FFFF_u32).filter_map(char::from_u32) {
            let mut buffer = [0_u8; 4];
            let want = scalar.encode_utf8(&mut buffer).as_bytes().to_vec();
            let got = run::<u8>(|out| {
                to_utf8(&Slice(&[u32::from(scalar)]), out);
            });
            assert_eq!(got, want, "{scalar:?}");
            let mut decoded = Collect(Vec::new());
            assert!(from_utf8(&Slice(&want), &mut decoded));
            assert_eq!(decoded.0, [u32::from(scalar)]);
        }
        // Every one- and two-byte input, and a sample of three- and four-byte
        // sequences, is valid exactly when the standard library says so.
        let mut inputs: Vec<Vec<u8>> = Vec::new();
        for first in 0..=255_u8 {
            inputs.push(vec![first]);
            for second in 0..=255_u8 {
                inputs.push(vec![first, second]);
            }
        }
        for first in [0xE0, 0xE1, 0xED, 0xEF, 0xF0, 0xF1, 0xF4, 0xF5, 0xC0, 0xC2] {
            for second in [0x7F, 0x80, 0x8F, 0x90, 0x9F, 0xA0, 0xBF, 0xC0] {
                for third in [0x7F, 0x80, 0xBF, 0xC0] {
                    inputs.push(vec![first, second, third]);
                    inputs.push(vec![first, second, third, 0x80]);
                    inputs.push(vec![first, second, third, 0xBF]);
                }
            }
        }
        for input in inputs {
            let mut decoded = Collect(Vec::new());
            let valid = from_utf8(&Slice(&input), &mut decoded);
            let want = std::str::from_utf8(&input);
            assert_eq!(valid, want.is_ok(), "{input:?}");
            if let Ok(text) = want {
                let scalars = text.chars().map(u32::from).collect::<Vec<_>>();
                assert_eq!(decoded.0, scalars, "{input:?}");
            }
        }
    }
}
