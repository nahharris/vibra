//! Source spans.
//!
//! `docs/spec/07-diagnostics-and-conformance.md` fixes the representation:
//! spans are half-open UTF-8 byte ranges, and line and column are derived
//! display data rather than stored. [`LineIndex`](crate::LineIndex) derives
//! them.

use std::fmt;

/// A half-open range of UTF-8 bytes in one source document.
///
/// The range is `start .. end`, so `end` is one past the last byte and an
/// empty span is `start == end`. Empty spans are meaningful: a missing
/// closing delimiter is reported at the position where it should appear.
///
/// # Offset width
///
/// Offsets are `usize`. A narrower offset would halve the memory a lossless
/// tree spends on spans, but it would also make construction fallible and
/// impose a document size limit on the whole reader API. That trade is not
/// worth taking without a measurement saying it matters; revisit it with one.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ByteSpan {
    start: usize,
    end: usize,
}

impl ByteSpan {
    /// A span covering `start .. end`.
    ///
    /// An inverted range yields the empty span at `start`. Normalizing here
    /// rather than asserting is deliberate: this type sits under a reader that
    /// consumes untrusted, incomplete input and whose milestone 1 exit gate
    /// fuzzes for panics, and one unconditional behaviour keeps debug and
    /// release builds identical. It also makes [`Self::len`] sound, since
    /// `end` can never be below `start`.
    #[must_use]
    pub const fn new(start: usize, end: usize) -> Self {
        Self {
            start,
            end: if end < start { start } else { end },
        }
    }

    /// The empty span at `offset`.
    #[must_use]
    pub const fn empty_at(offset: usize) -> Self {
        Self {
            start: offset,
            end: offset,
        }
    }

    /// A span of `length` bytes starting at `start`.
    ///
    /// Saturating, so no arithmetic here can overflow or wrap.
    #[must_use]
    pub const fn sized(start: usize, length: usize) -> Self {
        Self {
            start,
            end: start.saturating_add(length),
        }
    }

    /// The first byte offset in the span.
    #[must_use]
    pub const fn start(self) -> usize {
        self.start
    }

    /// One past the last byte offset in the span.
    #[must_use]
    pub const fn end(self) -> usize {
        self.end
    }

    /// The number of bytes the span covers.
    #[must_use]
    pub const fn len(self) -> usize {
        self.end - self.start
    }

    /// Whether the span covers no bytes.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// Whether `offset` falls inside the span.
    ///
    /// Half-open, so [`Self::end`] is not contained. An empty span contains
    /// nothing.
    #[must_use]
    pub const fn contains(self, offset: usize) -> bool {
        self.start <= offset && offset < self.end
    }

    /// Whether `other` lies entirely within this span.
    ///
    /// Unlike [`Self::contains`], this is reflexive: every span contains
    /// itself, and an empty span at a boundary is contained. A source-position
    /// query needs that reading to find the smallest node containing a caret
    /// sitting between two tokens.
    #[must_use]
    pub const fn contains_span(self, other: Self) -> bool {
        self.start <= other.start && other.end <= self.end
    }

    /// The smallest span covering both operands.
    #[must_use]
    pub fn join(self, other: Self) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    /// The text this span covers in `source`.
    ///
    /// Returns `None` when the span leaves the document or splits a UTF-8
    /// scalar, so a span from one document cannot silently slice another.
    #[must_use]
    pub fn text(self, source: &str) -> Option<&str> {
        source.get(self.start..self.end)
    }
}

impl fmt::Debug for ByteSpan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}..{}", self.start, self.end)
    }
}

impl From<ByteSpan> for std::ops::Range<usize> {
    fn from(span: ByteSpan) -> Self {
        span.start..span.end
    }
}

#[cfg(test)]
mod tests {
    use super::ByteSpan;

    #[test]
    fn a_span_reports_its_bounds_and_length() {
        let span = ByteSpan::new(3, 10);
        assert_eq!(span.start(), 3);
        assert_eq!(span.end(), 10);
        assert_eq!(span.len(), 7);
        assert!(!span.is_empty());
    }

    #[test]
    fn an_empty_span_covers_nothing() {
        let span = ByteSpan::empty_at(4);
        assert_eq!(span.len(), 0);
        assert!(span.is_empty());
        assert!(
            !span.contains(4),
            "half-open, so the start is not contained"
        );
    }

    #[test]
    fn a_sized_span_ends_after_its_length() {
        assert_eq!(ByteSpan::sized(2, 3), ByteSpan::new(2, 5));
    }

    #[test]
    fn a_sized_span_saturates_rather_than_overflowing() {
        let span = ByteSpan::sized(usize::MAX, 8);
        assert_eq!(span.end(), usize::MAX);
    }

    #[test]
    fn containment_is_half_open() {
        let span = ByteSpan::new(2, 5);
        assert!(!span.contains(1));
        assert!(span.contains(2));
        assert!(span.contains(4));
        assert!(!span.contains(5), "the end offset is one past the span");
    }

    #[test]
    fn span_containment_admits_the_boundaries() {
        let outer = ByteSpan::new(2, 8);
        assert!(outer.contains_span(outer), "containment is reflexive");
        assert!(outer.contains_span(ByteSpan::empty_at(2)));
        assert!(outer.contains_span(ByteSpan::empty_at(8)));
        assert!(!outer.contains_span(ByteSpan::new(1, 8)));
        assert!(!outer.contains_span(ByteSpan::new(2, 9)));
    }

    #[test]
    fn joining_covers_both_operands_in_either_order() {
        let left = ByteSpan::new(2, 4);
        let right = ByteSpan::new(9, 12);
        assert_eq!(left.join(right), ByteSpan::new(2, 12));
        assert_eq!(right.join(left), ByteSpan::new(2, 12));
    }

    #[test]
    fn joining_disjoint_spans_covers_the_gap_between_them() {
        assert_eq!(
            ByteSpan::new(0, 1).join(ByteSpan::new(20, 21)),
            ByteSpan::new(0, 21)
        );
    }

    #[test]
    fn text_returns_the_covered_source() {
        let source = "(defn greet)";
        assert_eq!(ByteSpan::new(1, 5).text(source), Some("defn"));
        assert_eq!(ByteSpan::empty_at(1).text(source), Some(""));
    }

    #[test]
    fn text_refuses_a_span_that_leaves_the_document() {
        let source = "(a)";
        assert_eq!(ByteSpan::new(0, 99).text(source), None);
    }

    #[test]
    fn text_refuses_a_span_that_splits_a_scalar() {
        // Three bytes, one scalar. Slicing inside it is never valid UTF-8.
        let source = "→";
        assert_eq!(source.len(), 3);
        assert_eq!(ByteSpan::new(0, 1).text(source), None);
        assert_eq!(ByteSpan::new(0, 3).text(source), Some("→"));
    }

    #[test]
    fn an_inverted_range_becomes_empty_rather_than_panicking() {
        let span = ByteSpan::new(9, 4);
        assert!(span.is_empty());
        assert_eq!(span.start(), 9);
        assert_eq!(span.end(), 9);
        assert_eq!(span.len(), 0, "len must not underflow");
    }

    #[test]
    fn a_span_converts_into_a_range() {
        let range: std::ops::Range<usize> = ByteSpan::new(2, 6).into();
        assert_eq!(range, 2..6);
    }

    #[test]
    fn debug_output_is_compact() {
        assert_eq!(format!("{:?}", ByteSpan::new(2, 6)), "2..6");
    }
}
