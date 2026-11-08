//! Bounded proofs of the backend-neutral algorithms, checked by `cargo kani`.
//!
//! Every other build leaves this module out. A harness states a property for
//! every input up to a small size, and Kani checks it exhaustively, together
//! with the absence of panics and arithmetic overflow. The `kani` job of
//! `.github/workflows/ci.yml` runs them; the unit tests in `lib.rs` keep the
//! concrete examples.

use vibra_ir::external::NumericType;

use crate::number::{Parsed, parse_integer};
use crate::{Sink, Source, decode, encode};

/// The units of a slice.
struct Units<'a, U>(&'a [U]);

impl<U: Copy> Source for Units<'_, U> {
    type Unit = U;
    type Iter<'b>
        = std::iter::Copied<std::slice::Iter<'b, U>>
    where
        Self: 'b;

    fn len(&self) -> u64 {
        u64::try_from(self.0.len()).unwrap_or(u64::MAX)
    }

    fn range(&self, from: u64, to: u64) -> Self::Iter<'_> {
        let from = usize::try_from(from).unwrap_or(usize::MAX);
        let to = usize::try_from(to).unwrap_or(usize::MAX);
        self.0.get(from..to).unwrap_or(&[]).iter().copied()
    }
}

/// The units put so far.
struct Collect<U>(Vec<U>);

impl<U: Copy> Sink for Collect<U> {
    type Unit = U;

    fn put(&mut self, unit: U) {
        self.0.push(unit);
    }
}

/// A prefix of `units` of nondeterministic length.
fn any_prefix<U, const N: usize>(units: &[U; N]) -> &[U] {
    let len: usize = kani::any_where(|len| *len <= N);
    units.get(..len).unwrap_or(&[])
}

/// Every Unicode scalar encodes to the bytes the standard library writes for
/// it, and those bytes decode back to the scalar alone.
#[kani::proof]
#[kani::unwind(6)]
fn encoding_a_scalar_matches_the_standard_library_and_decodes_back() {
    let scalar: u32 = kani::any();
    let Some(expected) = char::from_u32(scalar) else {
        return;
    };
    let mut bytes = Collect(Vec::new());
    encode(scalar, &mut bytes);
    let mut buffer = [0; 4];
    assert_eq!(bytes.0, expected.encode_utf8(&mut buffer).as_bytes());

    let mut scalars = Collect(Vec::new());
    assert!(decode(&Units(&bytes.0), &mut scalars));
    assert_eq!(scalars.0, [scalar]);
}

/// The decoder accepts exactly the byte strings the standard library accepts
/// as UTF-8, and reads the same scalars from them.
#[kani::proof]
#[kani::unwind(7)]
fn decoding_agrees_with_the_standard_library() {
    let bytes: [u8; 5] = kani::any();
    let input = any_prefix(&bytes);
    let mut scalars = Collect(Vec::new());
    let accepted = decode(&Units(input), &mut scalars);
    match std::str::from_utf8(input) {
        Ok(text) => {
            assert!(accepted);
            assert!(scalars.0.iter().copied().eq(text.chars().map(u32::from)));
        }
        // Kani's `assert!` ends in a semicolon, so it cannot be a bare arm.
        Err(_) => {
            assert!(!accepted);
        }
    }
}

/// Reading any short text as an integer of any type answers a value within
/// that type's range, or one of the two failures, and never panics.
#[kani::proof]
#[kani::unwind(6)]
fn parsing_an_integer_stays_within_the_type() {
    let index: usize = kani::any();
    let Some(&numeric) = NumericType::ALL.get(index) else {
        return;
    };
    kani::assume(numeric.is_integer());
    let scalars: [u32; 4] = kani::any();
    if let Parsed::Value(value) = parse_integer(numeric, &Units(any_prefix(&scalars))) {
        let (low, high) = numeric.range();
        assert!(low <= value && value <= high);
    }
}
