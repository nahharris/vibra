//! UTF-8 encoding and decoding over a [`Source`] and a [`Sink`].
//!
//! The decoder accepts exactly the well-formed sequences of Unicode, Table 3-7,
//! the ones the Vibra body of `text.from-utf8` accepts: a scalar at its
//! shortest form, no surrogate, nothing above U+10FFFF.

use crate::{Sink, Source};

/// Appends the UTF-8 encoding of `scalar`, which is a Unicode scalar value.
pub fn encode<K: Sink<Unit = u8> + ?Sized>(scalar: u32, out: &mut K) {
    // Each byte is masked to the bits it carries, so the narrowing is exact.
    let byte = |value: u32| u8::try_from(value & 0xFF).unwrap_or(0);
    if scalar < 0x80 {
        out.put(byte(scalar));
    } else if scalar < 0x800 {
        out.put(byte(0xC0 | (scalar >> 6)));
        out.put(byte(0x80 | (scalar & 0x3F)));
    } else if scalar < 0x1_0000 {
        out.put(byte(0xE0 | (scalar >> 12)));
        out.put(byte(0x80 | ((scalar >> 6) & 0x3F)));
        out.put(byte(0x80 | (scalar & 0x3F)));
    } else {
        out.put(byte(0xF0 | (scalar >> 18)));
        out.put(byte(0x80 | ((scalar >> 12) & 0x3F)));
        out.put(byte(0x80 | ((scalar >> 6) & 0x3F)));
        out.put(byte(0x80 | (scalar & 0x3F)));
    }
}

/// The bytes of the sequence that starts at `index`, at most four.
fn window<S: Source<Unit = u8>>(source: &S, index: u64) -> [Option<u8>; 4] {
    let mut bytes = [None; 4];
    let end = index.saturating_add(4).min(source.len());
    for (slot, byte) in bytes.iter_mut().zip(source.range(index, end)) {
        *slot = Some(byte);
    }
    bytes
}

/// The six payload bits of `byte` when it lies in `low..=high`.
fn continuation(byte: Option<u8>, low: u8, high: u8) -> Option<u32> {
    byte.filter(|byte| (low..=high).contains(byte))
        .map(|byte| u32::from(byte & 0x3F))
}

/// The scalar and the width of the sequence in `bytes`, or `None` when it is
/// ill-formed.
fn sequence(bytes: &[Option<u8>; 4]) -> Option<(u32, u64)> {
    let [Some(first), second, third, fourth] = *bytes else {
        return None;
    };
    let lead = u32::from(first);
    Some(match first {
        0x00..=0x7F => (lead, 1),
        0xC2..=0xDF => {
            let second = continuation(second, 0x80, 0xBF)?;
            (((lead & 0x1F) << 6) | second, 2)
        }
        0xE0..=0xEF => {
            // E0 starts above the overlong forms, and ED stops below the
            // surrogates.
            let (low, high) = match first {
                0xE0 => (0xA0, 0xBF),
                0xED => (0x80, 0x9F),
                _ => (0x80, 0xBF),
            };
            let second = continuation(second, low, high)?;
            let third = continuation(third, 0x80, 0xBF)?;
            (((lead & 0x0F) << 12) | (second << 6) | third, 3)
        }
        0xF0..=0xF4 => {
            // F0 starts above the overlong forms, and F4 stops at U+10FFFF.
            let (low, high) = match first {
                0xF0 => (0x90, 0xBF),
                0xF4 => (0x80, 0x8F),
                _ => (0x80, 0xBF),
            };
            let second = continuation(second, low, high)?;
            let third = continuation(third, 0x80, 0xBF)?;
            let fourth = continuation(fourth, 0x80, 0xBF)?;
            (
                ((lead & 0x07) << 18) | (second << 12) | (third << 6) | fourth,
                4,
            )
        }
        _ => return None,
    })
}

/// Appends the scalars of the UTF-8 in `source`, or returns `false` at the
/// first ill-formed sequence.
pub fn decode<S: Source<Unit = u8>, K: Sink<Unit = u32> + ?Sized>(
    source: &S,
    out: &mut K,
) -> bool {
    let mut index = 0;
    while index < source.len() {
        let Some((scalar, width)) = sequence(&window(source, index)) else {
            return false;
        };
        out.put(scalar);
        index += width;
    }
    true
}
