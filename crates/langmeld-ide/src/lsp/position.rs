//! Positions of the protocol, in lines and the units of the negotiated encoding.

use langmeld_syntax::{LineCol, SourceFile, TextRange, TextSize};
use lsp_types::{Position, Range};

/// How a column is counted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Encoding {
    Utf8,
    Utf16,
}

impl Encoding {
    /// How many units `c` takes.
    pub(crate) fn width(self, c: char) -> u32 {
        let width = match self {
            Self::Utf8 => c.len_utf8(),
            Self::Utf16 => c.len_utf16(),
        };
        // A character takes at most four units.
        width as u32
    }

    /// How many units `text` takes.
    pub(crate) fn length(self, text: &str) -> u32 {
        text.chars()
            .fold(0, |total, c| total.saturating_add(self.width(c)))
    }
}

/// The offset of a position. A column past the end of its line means that end, and a
/// line past the last means the end of the text. A position inside a character means
/// its start.
pub(crate) fn offset(file: &SourceFile, position: Position, encoding: Encoding) -> TextSize {
    let end = TextSize::of(file.text());
    let Some(line) = file.line_index().line_range(position.line) else {
        return end;
    };
    let text = file
        .text()
        .get(usize::from(line.start())..usize::from(line.end()));
    let text = text.unwrap_or_default().trim_end_matches(['\r', '\n']);
    let mut units = 0u32;
    for (at, c) in text.char_indices() {
        units = units.saturating_add(encoding.width(c));
        if units > position.character {
            return line.start() + TextSize::of(text.get(..at).unwrap_or_default());
        }
    }
    line.start() + TextSize::of(text)
}

/// The position of an offset of the text.
pub(crate) fn position(file: &SourceFile, offset: TextSize, encoding: Encoding) -> Position {
    let text = file.text();
    let mut at = usize::from(offset).min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    // A boundary of a text that fits has a line.
    let at = u32::try_from(at).unwrap_or(u32::MAX);
    let LineCol { line, col } = file
        .line_index()
        .line_col(TextSize::new(at))
        .unwrap_or(LineCol { line: 0, col: 0 });
    let start = at.saturating_sub(col) as usize;
    let character = text
        .get(start..at as usize)
        .map_or(0, |before| encoding.length(before));
    Position { line, character }
}

pub(crate) fn range(file: &SourceFile, range: TextRange, encoding: Encoding) -> Range {
    Range {
        start: position(file, range.start(), encoding),
        end: position(file, range.end(), encoding),
    }
}
