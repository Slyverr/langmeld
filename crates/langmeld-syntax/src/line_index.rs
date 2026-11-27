use std::error::Error;
use std::fmt;

use text_size::{TextRange, TextSize};

/// A zero-based line and a column counted in UTF-8 bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LineCol {
    /// Zero-based line number.
    pub line: u32,
    /// Zero-based column, in UTF-8 bytes from the start of the line.
    pub col: u32,
}

/// A zero-based line and a column counted in UTF-16 code units.
///
/// This is the position encoding editors use unless they agree on another one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Utf16LineCol {
    /// Zero-based line number.
    pub line: u32,
    /// Zero-based column, in UTF-16 code units from the start of the line.
    pub col: u32,
}

/// A text has too many bytes to be addressed with a [`TextSize`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextTooLarge {
    /// Length of the rejected text, in bytes.
    pub len: usize,
}

impl fmt::Display for TextTooLarge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "text is {} bytes long, but the largest supported text is {} bytes",
            self.len, MAX_LEN
        )
    }
}

impl Error for TextTooLarge {}

/// One byte short of `u32::MAX`, so that the number of lines also fits in a `u32`.
const MAX_LEN: u32 = u32::MAX - 1;

fn checked_len(len: usize) -> Result<u32, TextTooLarge> {
    match u32::try_from(len) {
        Ok(len) if len <= MAX_LEN => Ok(len),
        _ => Err(TextTooLarge { len }),
    }
}

/// A character that is not one byte in UTF-8 and one unit in UTF-16.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WideChar {
    /// Byte offset of the character.
    start: u32,
    /// How many more UTF-8 bytes than UTF-16 units the text holds up to and
    /// including this character.
    extra: u32,
    len_utf8: u8,
    len_utf16: u8,
}

impl WideChar {
    fn extra_here(self) -> u32 {
        u32::from(self.len_utf8 - self.len_utf16)
    }

    /// Offset of the character in UTF-16 units from the start of the text.
    fn start_utf16(self) -> u32 {
        self.start - (self.extra - self.extra_here())
    }
}

/// Converts between byte offsets, lines and columns.
///
/// A line ends after `\n`, `\r\n` or a lone `\r`. The terminator belongs to the
/// line it ends, so an offset between `\r` and `\n` is on that line. Every
/// conversion returns `None` for a position that does not exist: past the end,
/// past the end of its line, or inside a character.
///
/// Lines and columns start at zero. A byte order mark is an ordinary character.
///
/// ```
/// use langmeld_syntax::{LineCol, LineIndex, TextSize, Utf16LineCol};
///
/// let index = LineIndex::new("a\r\n😀b")?;
/// let b = index.line_col(TextSize::new(7));
/// assert_eq!(b, Some(LineCol { line: 1, col: 4 }));
/// assert_eq!(
///     b.and_then(|b| index.to_utf16(b)),
///     Some(Utf16LineCol { line: 1, col: 2 }),
/// );
/// # Ok::<(), langmeld_syntax::TextTooLarge>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineIndex {
    len: u32,
    /// Byte offset of the start of each line. The first is always zero.
    line_starts: Vec<u32>,
    /// In text order.
    wide_chars: Vec<WideChar>,
}

impl LineIndex {
    /// Indexes `text`. Fails if the text is too large to address.
    pub fn new(text: &str) -> Result<Self, TextTooLarge> {
        let len = checked_len(text.len())?;
        let mut line_starts = vec![0];
        let mut wide_chars = Vec::new();
        let mut extra = 0;
        let mut chars = text.char_indices().peekable();
        while let Some((offset, c)) = chars.next() {
            // `checked_len` guarantees that every offset fits.
            let start = offset as u32;
            match c {
                '\n' => line_starts.push(start + 1),
                '\r' if !matches!(chars.peek(), Some((_, '\n'))) => line_starts.push(start + 1),
                c if !c.is_ascii() => {
                    let (len_utf8, len_utf16) = (c.len_utf8() as u8, c.len_utf16() as u8);
                    extra += u32::from(len_utf8 - len_utf16);
                    wide_chars.push(WideChar {
                        start,
                        extra,
                        len_utf8,
                        len_utf16,
                    });
                }
                _ => {}
            }
        }
        Ok(Self {
            len,
            line_starts,
            wide_chars,
        })
    }

    /// Length of the indexed text, in bytes.
    // An empty text still has one line, so "is empty" has no obvious meaning here.
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> TextSize {
        TextSize::new(self.len)
    }

    /// Number of lines. This is at least one, and a final terminator starts a
    /// last, empty line.
    pub fn line_count(&self) -> u32 {
        // There is at most one line per byte, plus one, and `len < u32::MAX`.
        self.line_starts.len() as u32
    }

    /// The bytes of a line, including its terminator.
    pub fn line_range(&self, line: u32) -> Option<TextRange> {
        let (start, next) = self.line_bounds(line)?;
        let end = next.unwrap_or(self.len);
        Some(TextRange::new(TextSize::new(start), TextSize::new(end)))
    }

    /// The line and UTF-8 column of a byte offset.
    pub fn line_col(&self, offset: TextSize) -> Option<LineCol> {
        let offset = u32::from(offset);
        if !self.is_boundary(offset) {
            return None;
        }
        // The first line starts at zero, so at least one start is `<= offset`.
        let line = self.line_starts.partition_point(|&start| start <= offset) - 1;
        let start = *self.line_starts.get(line)?;
        Some(LineCol {
            line: line as u32,
            col: offset - start,
        })
    }

    /// The byte offset of a line and UTF-8 column.
    pub fn offset(&self, position: LineCol) -> Option<TextSize> {
        let (start, next) = self.line_bounds(position.line)?;
        let offset = start.checked_add(position.col)?;
        let on_this_line = next.is_none_or(|next| offset < next);
        (on_this_line && self.is_boundary(offset)).then(|| TextSize::new(offset))
    }

    /// Converts a UTF-8 column to a UTF-16 column.
    pub fn to_utf16(&self, position: LineCol) -> Option<Utf16LineCol> {
        let offset = u32::from(self.offset(position)?);
        let line_start = offset - position.col;
        let extra = self.extra_before(offset) - self.extra_before(line_start);
        Some(Utf16LineCol {
            line: position.line,
            col: position.col - extra,
        })
    }

    /// Converts a UTF-16 column to a UTF-8 column.
    pub fn to_utf8(&self, position: Utf16LineCol) -> Option<LineCol> {
        let (line_start, _) = self.line_bounds(position.line)?;
        let line_start_utf16 = line_start - self.extra_before(line_start);
        let target = line_start_utf16.checked_add(position.col)?;
        let count = self
            .wide_chars
            .partition_point(|wide| wide.start_utf16() < target);
        let extra = match self.wide_chars.get(..count)?.last() {
            // Inside a surrogate pair.
            Some(wide) if target < wide.start_utf16() + u32::from(wide.len_utf16) => return None,
            Some(wide) => wide.extra,
            None => 0,
        };
        let offset = target.checked_add(extra)?;
        let found = LineCol {
            line: position.line,
            col: offset - line_start,
        };
        // Rejects columns past the end of the line.
        self.offset(found).map(|_| found)
    }

    /// Start of a line, and start of the next one if there is one.
    fn line_bounds(&self, line: u32) -> Option<(u32, Option<u32>)> {
        let line = usize::try_from(line).ok()?;
        let start = *self.line_starts.get(line)?;
        Some((start, self.line_starts.get(line + 1).copied()))
    }

    /// The last wide character that starts before `offset`.
    fn wide_char_before(&self, offset: u32) -> Option<WideChar> {
        let count = self.wide_chars.partition_point(|wide| wide.start < offset);
        self.wide_chars.get(..count)?.last().copied()
    }

    fn extra_before(&self, offset: u32) -> u32 {
        self.wide_char_before(offset).map_or(0, |wide| wide.extra)
    }

    /// Whether `offset` is in the text and not inside a character.
    fn is_boundary(&self, offset: u32) -> bool {
        offset <= self.len
            && self
                .wide_char_before(offset)
                .is_none_or(|wide| offset >= wide.start + u32::from(wide.len_utf8))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_that_do_not_fit_are_rejected() {
        assert_eq!(checked_len(0), Ok(0));
        assert_eq!(checked_len(MAX_LEN as usize), Ok(MAX_LEN));
        for len in [u32::MAX as usize, usize::MAX] {
            assert_eq!(checked_len(len), Err(TextTooLarge { len }));
        }
    }

    #[test]
    fn too_large_message() {
        let error = TextTooLarge {
            len: u32::MAX as usize,
        };
        assert_eq!(
            error.to_string(),
            "text is 4294967295 bytes long, but the largest supported text is 4294967294 bytes"
        );
    }
}
