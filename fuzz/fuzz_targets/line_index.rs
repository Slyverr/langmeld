//! `LineIndex` conversions agree with a direct walk of the text, and each one
//! can be undone. Arbitrary positions never cause a panic.

#![no_main]

use langmeld_syntax::{LineCol, LineIndex, TextSize, Utf16LineCol};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: (&str, u32, u32)| {
    let (text, first, second) = input;
    let Ok(index) = LineIndex::new(text) else {
        return;
    };
    assert_eq!(index.len(), TextSize::of(text));
    walk(&index, text);
    probe(&index, first, second);
    probe(&index, second, first);
});

/// Checks every offset of the text, and one past the end.
fn walk(index: &LineIndex, text: &str) {
    let (mut line, mut col, mut col16) = (0, 0, 0);
    let mut line_start = 0;
    let mut chars = text.char_indices().peekable();
    loop {
        let next = chars.next();
        let offset = next.map_or(text.len(), |(offset, _)| offset) as u32;
        let position = LineCol { line, col };
        let utf16 = Utf16LineCol { line, col: col16 };

        assert_eq!(index.line_col(TextSize::new(offset)), Some(position));
        assert_eq!(index.offset(position), Some(TextSize::new(offset)));
        assert_eq!(index.to_utf16(position), Some(utf16));
        assert_eq!(index.to_utf8(utf16), Some(position));

        let Some((_, c)) = next else { break };
        for inside in offset + 1..offset + c.len_utf8() as u32 {
            assert_eq!(index.line_col(TextSize::new(inside)), None);
        }
        let ends_line =
            c == '\n' || (c == '\r' && chars.peek().map(|&(_, next)| next) != Some('\n'));
        if ends_line {
            let end = offset + 1;
            let range = index.line_range(line).expect("the line was walked");
            assert_eq!(u32::from(range.start()), line_start);
            assert_eq!(u32::from(range.end()), end);
            (line, col, col16, line_start) = (line + 1, 0, 0, end);
        } else {
            col += c.len_utf8() as u32;
            col16 += c.len_utf16() as u32;
        }
    }
    assert_eq!(index.line_count(), line + 1);
    let last = index.line_range(line).expect("the last line exists");
    assert_eq!(u32::from(last.start()), line_start);
    assert_eq!(last.end(), TextSize::of(text));
    assert_eq!(index.line_col(TextSize::of(text) + TextSize::new(1)), None);
}

/// Any answer for an arbitrary position must convert back to that position.
fn probe(index: &LineIndex, first: u32, second: u32) {
    if let Some(position) = index.line_col(TextSize::new(first)) {
        assert_eq!(index.offset(position), Some(TextSize::new(first)));
    }
    // Small numbers hit real lines far more often than arbitrary ones.
    for (line, col) in [(first, second), (first % 16, second % 64)] {
        let position = LineCol { line, col };
        if let Some(offset) = index.offset(position) {
            assert_eq!(index.line_col(offset), Some(position));
        }
        assert_eq!(
            index.to_utf16(position).is_some(),
            index.offset(position).is_some()
        );
        let utf16 = Utf16LineCol { line, col };
        if let Some(position) = index.to_utf8(utf16) {
            assert_eq!(index.to_utf16(position), Some(utf16));
        }
        let _ = index.line_range(line);
    }
}
