use std::ops::Range;

/// Half-open UTF-8 byte range in one source document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn range(self) -> Range<usize> {
        self.start..self.end
    }

    pub fn cover(self, other: Self) -> Self {
        Self::new(self.start.min(other.start), self.end.max(other.end))
    }
}

/// One-based display position. Columns count Unicode scalar values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub line: usize,
    pub column: usize,
    pub offset: usize,
}

/// Maps byte offsets to positions without treating UTF-8 continuation bytes
/// as columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineIndex {
    line_starts: Vec<usize>,
    source_len: usize,
}

impl LineIndex {
    pub fn new(source: &str) -> Self {
        let mut line_starts = vec![0];
        line_starts.extend(
            source
                .bytes()
                .enumerate()
                .filter_map(|(offset, byte)| (byte == b'\n').then_some(offset + 1)),
        );
        Self {
            line_starts,
            source_len: source.len(),
        }
    }

    pub fn position(&self, source: &str, offset: usize) -> Position {
        let offset = offset.min(self.source_len).min(source.len());
        let line = self.line_starts.partition_point(|start| *start <= offset) - 1;
        let line_start = self.line_starts[line];
        let boundary = floor_char_boundary(source, offset);
        let column = source[line_start..boundary].chars().count() + 1;
        Position {
            line: line + 1,
            column,
            offset: boundary,
        }
    }

    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }
}

fn floor_char_boundary(source: &str, mut offset: usize) -> usize {
    while !source.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexes_unicode_columns_and_crlf_lines() {
        let source = "αβ\r\nx\n";
        let index = LineIndex::new(source);
        assert_eq!(index.line_count(), 3);
        assert_eq!(
            index.position(source, "α".len()),
            Position {
                line: 1,
                column: 2,
                offset: 2
            }
        );
        assert_eq!(
            index.position(source, source.find('x').unwrap()),
            Position {
                line: 2,
                column: 1,
                offset: 6
            }
        );
    }

    #[test]
    fn clamps_offsets_and_utf8_continuation_bytes() {
        let source = "é";
        let index = LineIndex::new(source);
        assert_eq!(index.position(source, 1).column, 1);
        assert_eq!(index.position(source, 99).column, 2);
    }
}
