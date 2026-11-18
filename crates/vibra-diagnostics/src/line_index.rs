//! Deriving display positions from byte offsets.
//!
//! `docs/spec/07-diagnostics-and-conformance.md` stores spans as half-open
//! UTF-8 byte ranges and treats "one-based line and Unicode-scalar column" as
//! derived display data. This module performs that derivation, and it is the
//! reason the milestone 1 exit gate can require Unicode byte and display
//! spans to agree.

use crate::ByteSpan;

/// A one-based display position.
///
/// The column counts Unicode scalar values, not bytes and not grapheme
/// clusters. A scalar column is what the specification fixes; it is also the
/// only one of the three that is cheap and unambiguous. An editor that wants
/// grapheme columns can compute them from the line text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Position {
    /// One-based line number.
    pub line: usize,
    /// One-based column, counted in Unicode scalar values.
    pub column: usize,
}

impl Position {
    /// A position at `line` and `column`, both one-based.
    #[must_use]
    pub const fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

impl std::fmt::Display for Position {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}:{}", self.line, self.column)
    }
}

/// Maps byte offsets in one document to display positions.
///
/// The index borrows its document so a position can never be derived against
/// a different source than the one the offset came from.
///
/// # Line terminators
///
/// `\n` terminates a line, and a `\r` immediately preceding it is part of that
/// terminator rather than content. A lone `\r` is ordinary content: the
/// canonical format is LF, CRLF is accepted because editors and Windows
/// checkouts produce it, and inventing a third convention would make columns
/// disagree with what an editor shows.
#[derive(Clone, Debug)]
pub struct LineIndex<'source> {
    source: &'source str,
    /// Byte offset at which each line begins. Never empty: every document,
    /// including the empty one, has a first line starting at zero.
    line_starts: Vec<usize>,
}

impl<'source> LineIndex<'source> {
    /// Indexes `source`.
    #[must_use]
    pub fn new(source: &'source str) -> Self {
        let mut line_starts = vec![0];
        line_starts.extend(
            source
                .bytes()
                .enumerate()
                .filter(|(_, byte)| *byte == b'\n')
                .map(|(offset, _)| offset.saturating_add(1)),
        );

        Self {
            source,
            line_starts,
        }
    }

    /// The indexed document.
    #[must_use]
    pub const fn source(&self) -> &'source str {
        self.source
    }

    /// How many lines the document has.
    ///
    /// A document ending in a newline does not gain an extra empty line, which
    /// matches how editors number lines. The empty document has one line.
    #[must_use]
    pub fn line_count(&self) -> usize {
        if self.source.ends_with('\n') && self.line_starts.len() > 1 {
            self.line_starts.len().saturating_sub(1)
        } else {
            self.line_starts.len()
        }
    }

    /// The display position of `offset`.
    ///
    /// An offset past the end of the document is clamped to its end, and an
    /// offset inside a scalar is attributed to the start of that scalar.
    /// Neither can panic: this runs on spans produced while recovering from
    /// malformed input.
    #[must_use]
    pub fn position(&self, offset: usize) -> Position {
        let offset = self.floor_char_boundary(offset.min(self.source.len()));

        // The last line whose start is at or before the offset.
        let line = match self.line_starts.binary_search(&offset) {
            Ok(exact) => exact,
            Err(next) => next.saturating_sub(1),
        };

        let line_start = self.line_starts.get(line).copied().unwrap_or(0);
        let column = self
            .source
            .get(line_start..offset)
            .map_or(0, |prefix| prefix.chars().count());

        Position::new(line.saturating_add(1), column.saturating_add(1))
    }

    /// The span of `line`, excluding its terminator. `line` is one-based.
    #[must_use]
    pub fn line_span(&self, line: usize) -> Option<ByteSpan> {
        let index = line.checked_sub(1)?;
        let start = *self.line_starts.get(index)?;

        let end = match self.line_starts.get(index.saturating_add(1)) {
            // Step back over the "\n", then over a "\r" that belongs to it.
            Some(next_start) => {
                let without_newline = next_start.saturating_sub(1);
                if self
                    .source
                    .get(..without_newline)
                    .is_some_and(|text| text.ends_with('\r'))
                {
                    without_newline.saturating_sub(1)
                } else {
                    without_newline
                }
            }
            None => self.source.len(),
        };

        Some(ByteSpan::new(start, end.max(start)))
    }

    /// The text of `line`, excluding its terminator. `line` is one-based.
    #[must_use]
    pub fn line_text(&self, line: usize) -> Option<&'source str> {
        self.line_span(line)?.text(self.source)
    }

    /// The largest char boundary at or below `offset`.
    ///
    /// `str::floor_char_boundary` is still unstable, so this open-codes it.
    /// A UTF-8 scalar is at most four bytes, so the loop runs at most three
    /// times.
    fn floor_char_boundary(&self, mut offset: usize) -> usize {
        while offset > 0 && !self.source.is_char_boundary(offset) {
            offset = offset.saturating_sub(1);
        }
        offset
    }
}

#[cfg(test)]
mod tests {
    use super::{LineIndex, Position};
    use crate::ByteSpan;

    #[test]
    fn the_empty_document_has_one_line() {
        let index = LineIndex::new("");
        assert_eq!(index.line_count(), 1);
        assert_eq!(index.position(0), Position::new(1, 1));
    }

    #[test]
    fn the_first_byte_is_line_one_column_one() {
        let index = LineIndex::new("(a)");
        assert_eq!(index.position(0), Position::new(1, 1));
        assert_eq!(index.position(1), Position::new(1, 2));
        assert_eq!(index.position(3), Position::new(1, 4));
    }

    #[test]
    fn a_newline_starts_the_next_line() {
        let index = LineIndex::new("(a)\n(b)");
        assert_eq!(index.line_count(), 2);
        assert_eq!(index.position(3), Position::new(1, 4), "at the newline");
        assert_eq!(index.position(4), Position::new(2, 1), "after it");
        assert_eq!(index.position(5), Position::new(2, 2));
    }

    #[test]
    fn a_trailing_newline_does_not_add_an_empty_line() {
        // Canonical Vibra output always ends in one newline, so this is the
        // common case and must not report a phantom final line.
        assert_eq!(LineIndex::new("(a)\n").line_count(), 1);
        assert_eq!(LineIndex::new("(a)\n(b)\n").line_count(), 2);
    }

    #[test]
    fn a_blank_line_between_forms_is_counted() {
        // One blank line between top-level forms is canonical formatting.
        let index = LineIndex::new("(a)\n\n(b)\n");
        assert_eq!(index.line_count(), 3);
        assert_eq!(index.line_text(2), Some(""));
        assert_eq!(index.line_text(3), Some("(b)"));
    }

    #[test]
    fn columns_count_scalars_not_bytes() {
        // "→" is three bytes but one scalar, so the symbol after it is at
        // column 2 even though it is at byte offset 3.
        let source = "→x";
        assert_eq!(source.len(), 4);
        let index = LineIndex::new(source);
        assert_eq!(index.position(0), Position::new(1, 1));
        assert_eq!(index.position(3), Position::new(1, 2));
        assert_eq!(index.position(4), Position::new(1, 3));
    }

    #[test]
    fn an_astral_scalar_counts_as_one_column() {
        // Four bytes, one scalar, one column. A UTF-16 column would say two.
        let source = "🌱x";
        assert_eq!(source.len(), 5);
        let index = LineIndex::new(source);
        assert_eq!(index.position(4), Position::new(1, 2));
        assert_eq!(index.position(5), Position::new(1, 3));
    }

    #[test]
    fn a_combining_mark_counts_as_its_own_column() {
        // "e" + U+0301. One grapheme, two scalars. The specification fixes
        // scalar columns, so this is two columns, not one.
        let source = "e\u{301}x";
        assert_eq!(source.len(), 4);
        let index = LineIndex::new(source);
        assert_eq!(index.position(1), Position::new(1, 2), "the mark");
        assert_eq!(index.position(3), Position::new(1, 3), "after it");
    }

    #[test]
    fn an_offset_inside_a_scalar_is_attributed_to_its_start() {
        let index = LineIndex::new("→x");
        // Offsets 1 and 2 are interior bytes of the three-byte scalar.
        assert_eq!(index.position(1), Position::new(1, 1));
        assert_eq!(index.position(2), Position::new(1, 1));
    }

    #[test]
    fn an_offset_past_the_end_clamps_to_the_end() {
        let index = LineIndex::new("(a)");
        assert_eq!(index.position(999), Position::new(1, 4));
    }

    #[test]
    fn carriage_return_before_newline_is_part_of_the_terminator() {
        let index = LineIndex::new("(a)\r\n(b)");
        assert_eq!(index.line_count(), 2);
        assert_eq!(index.line_text(1), Some("(a)"), "the CR is not content");
        assert_eq!(index.position(5), Position::new(2, 1));
    }

    #[test]
    fn a_lone_carriage_return_is_ordinary_content() {
        let index = LineIndex::new("a\rb");
        assert_eq!(index.line_count(), 1);
        assert_eq!(index.position(2), Position::new(1, 3));
    }

    #[test]
    fn line_spans_exclude_the_terminator() {
        let index = LineIndex::new("ab\ncd\n");
        assert_eq!(index.line_span(1), Some(ByteSpan::new(0, 2)));
        assert_eq!(index.line_span(2), Some(ByteSpan::new(3, 5)));
        assert_eq!(index.line_text(1), Some("ab"));
        assert_eq!(index.line_text(2), Some("cd"));
    }

    #[test]
    fn line_lookup_rejects_line_zero_and_lines_past_the_end() {
        let index = LineIndex::new("ab\ncd");
        assert_eq!(index.line_span(0), None, "lines are one-based");
        assert_eq!(index.line_span(3), None);
        assert!(index.line_span(2).is_some());
    }

    #[test]
    fn the_last_line_without_a_terminator_reaches_the_end() {
        let index = LineIndex::new("ab\ncd");
        assert_eq!(index.line_text(2), Some("cd"));
    }

    #[test]
    fn positions_and_line_text_agree_on_a_multiline_unicode_document() {
        // The property the exit gate cares about: for every char boundary,
        // the derived column indexes the same scalar in the derived line.
        let source = "(defn 🌱 →)\n  \"e\u{301}\"\r\n(b)";
        let index = LineIndex::new(source);

        for (offset, expected) in source.char_indices() {
            let position = index.position(offset);
            let line = index
                .line_text(position.line)
                .unwrap_or_else(|| unreachable!("derived line must exist"));

            // The terminator scalars are not in any line's text.
            if expected == '\n'
                || (expected == '\r' && source[offset..].starts_with("\r\n"))
            {
                continue;
            }

            let found = line
                .chars()
                .nth(position.column.saturating_sub(1))
                .unwrap_or_else(|| {
                    unreachable!("derived column must exist in the line")
                });
            assert_eq!(found, expected, "at byte offset {offset}");
        }
    }

    #[test]
    fn position_display_is_line_colon_column() {
        assert_eq!(Position::new(3, 12).to_string(), "3:12");
    }
}
