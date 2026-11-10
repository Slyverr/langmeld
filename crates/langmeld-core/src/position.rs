use crate::source::SourceFile;

/// Character unit used by a [`LineIndex`] when converting text positions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PositionEncoding {
    /// UTF-8 code units, equivalent to byte offsets within a line.
    Utf8,
    /// UTF-16 code units, the default encoding in the Language Server Protocol.
    Utf16,
    /// Unicode scalar values, also known as UTF-32 code units.
    Utf32,
}

/// A zero-based line and encoded character position within a source snapshot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourcePosition {
    /// Zero-based line number.
    pub line: usize,
    /// Zero-based character offset in the selected [`PositionEncoding`].
    pub character: usize,
}

/// Converts positions in an immutable source snapshot to UTF-8 byte offsets.
///
/// Construct one index per source snapshot and reuse it for editor requests.
/// Lines are split on LF; a CR immediately before LF is excluded from the
/// preceding line's character count. Positions beyond a line are clamped to
/// that line's end. Positions inside a multi-unit character are clamped to its
/// start so returned offsets are always valid UTF-8 boundaries.
#[derive(Clone, Debug)]
pub struct LineIndex<'a> {
    text: &'a str,
    starts: Vec<usize>,
}

impl<'a> LineIndex<'a> {
    /// Indexes line starts in `source`.
    #[must_use]
    pub fn new(source: &'a SourceFile) -> Self {
        let mut starts = vec![0];
        starts.extend(
            source
                .text
                .match_indices('\n')
                .map(|(offset, _)| offset + 1),
        );
        Self {
            text: &source.text,
            starts,
        }
    }

    /// Converts a zero-based line and encoded character position to a byte
    /// offset. Returns `None` when `line` does not exist.
    #[must_use]
    pub fn offset(
        &self,
        line: usize,
        character: usize,
        encoding: PositionEncoding,
    ) -> Option<usize> {
        let (start, end) = self.line_range(line)?;
        let mut units = 0;
        for (relative, value) in self.text[start..end].char_indices() {
            if character <= units {
                return Some(start + relative);
            }
            let next_units = units + encoded_len(value, encoding);
            if character < next_units {
                return Some(start + relative);
            }
            units = next_units;
        }
        Some(end)
    }

    /// Converts a byte offset to a zero-based line and encoded character
    /// position. Returns `None` when the offset is beyond the source text.
    /// Offsets inside a UTF-8 code point are clamped to its start.
    #[must_use]
    pub fn position(&self, offset: usize, encoding: PositionEncoding) -> Option<SourcePosition> {
        if offset > self.text.len() {
            return None;
        }
        let mut offset = offset;
        while !self.text.is_char_boundary(offset) {
            offset -= 1;
        }

        let line = self.starts.partition_point(|start| *start <= offset) - 1;
        let (start, end) = self.line_range(line)?;
        let end_offset = offset.min(end);
        let character = self.text[start..end_offset]
            .chars()
            .map(|value| encoded_len(value, encoding))
            .sum();
        Some(SourcePosition { line, character })
    }

    fn line_range(&self, line: usize) -> Option<(usize, usize)> {
        let start = *self.starts.get(line)?;
        let mut end = self
            .starts
            .get(line + 1)
            .copied()
            .unwrap_or(self.text.len());
        if end > start && self.text.as_bytes()[end - 1] == b'\n' {
            end -= 1;
            if end > start && self.text.as_bytes()[end - 1] == b'\r' {
                end -= 1;
            }
        }
        Some((start, end))
    }
}

fn encoded_len(value: char, encoding: PositionEncoding) -> usize {
    match encoding {
        PositionEncoding::Utf8 => value.len_utf8(),
        PositionEncoding::Utf16 => value.len_utf16(),
        PositionEncoding::Utf32 => 1,
    }
}
