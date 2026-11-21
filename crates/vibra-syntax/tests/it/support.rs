//! Helpers shared by the test modules.

use vibra_diagnostics::{ByteSpan, Diagnostic, DiagnosticCode};

/// A small deterministic generator, so a failing random input reproduces.
pub(crate) struct Random(u64);

impl Random {
    pub(crate) const fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    pub(crate) fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub(crate) fn below(&mut self, bound: usize) -> usize {
        usize::try_from(self.next() % u64::try_from(bound).unwrap()).unwrap()
    }

    /// A random valid UTF-8 text of up to `max` characters, drawn mostly from
    /// the characters the lexer treats specially.
    pub(crate) fn text(&mut self, max: usize) -> String {
        const ALPHABET: &[char] = &[
            ' ',
            '\t',
            '\r',
            '\n',
            '"',
            '\'',
            '\\',
            '/',
            '{',
            '}',
            '[',
            ']',
            '(',
            ')',
            ':',
            '.',
            '@',
            '_',
            '-',
            '+',
            '?',
            '!',
            '0',
            '1',
            '9',
            'a',
            'e',
            'E',
            'u',
            'i',
            'f',
            '8',
            '3',
            '2',
            '6',
            '4',
            'x',
            'z',
            'Z',
            'é',
            '€',
            '\u{1F600}',
            '\u{FEFF}',
            '\0',
            '\u{7F}',
            '{',
            '"',
        ];
        let len = self.below(max + 1);
        (0..len)
            .map(|_| {
                if self.below(16) == 0 {
                    char::from_u32(u32::try_from(self.below(0x11_0000)).unwrap())
                        .unwrap_or('?')
                } else {
                    ALPHABET[self.below(ALPHABET.len())]
                }
            })
            .collect()
    }
}

/// The diagnostics as `(code, start, end)`.
pub(crate) fn codes(diagnostics: &[Diagnostic]) -> Vec<(DiagnosticCode, usize, usize)> {
    diagnostics
        .iter()
        .map(|diagnostic| {
            let span: ByteSpan = diagnostic.primary_span();
            (diagnostic.code(), span.start(), span.end())
        })
        .collect()
}
