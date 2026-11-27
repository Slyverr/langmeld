//! `LineIndex` round trips between byte offsets, UTF-8 positions and UTF-16
//! positions, over arbitrary text. The number of cases comes from the
//! `PROPTEST_CASES` environment variable.

#![cfg(not(target_arch = "wasm32"))]

use langmeld_syntax::{LineCol, LineIndex, TextSize, Utf16LineCol};
use proptest::prelude::*;

/// Text built from pieces that stress line endings and character widths:
/// emoji, CJK, `\r\n`, a lone `\r`, a byte order mark, and arbitrary characters.
fn text() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        Just("a".to_string()),
        Just("é".to_string()),
        Just("日本語".to_string()),
        Just("😀".to_string()),
        Just("\r\n".to_string()),
        Just("\r".to_string()),
        Just("\n".to_string()),
        Just("\u{FEFF}".to_string()),
        Just(" ".to_string()),
        ".{0,4}",
    ];
    prop::collection::vec(piece, 0..24).prop_map(|pieces| pieces.concat())
}

proptest! {
    #[test]
    fn every_valid_offset_round_trips(text in text()) {
        let index = LineIndex::new(&text).expect("test text is small");
        for offset in 0..=text.len() {
            let size = TextSize::new(u32::try_from(offset).expect("test text is small"));
            if !text.is_char_boundary(offset) {
                prop_assert_eq!(index.line_col(size), None);
                continue;
            }
            let position = index.line_col(size).expect("a boundary has a position");
            prop_assert_eq!(index.offset(position), Some(size));

            let wide = index.to_utf16(position).expect("a valid position converts");
            prop_assert_eq!(wide.line, position.line);
            prop_assert_eq!(index.to_utf8(wide), Some(position));

            // Compare with a direct count from the start of the line.
            let line = index.line_range(position.line).expect("the line exists");
            let line_start = usize::from(line.start());
            prop_assert_eq!(position.col as usize, offset - line_start);
            let units = text[line_start..offset].encode_utf16().count();
            prop_assert_eq!(wide.col as usize, units);
        }
    }

    #[test]
    fn every_valid_position_round_trips(text in text()) {
        let index = LineIndex::new(&text).expect("test text is small");
        for line in 0..index.line_count() {
            let range = index.line_range(line).expect("the line exists");
            let (start, end) = (usize::from(range.start()), usize::from(range.end()));
            // A terminator belongs to its line, so only the last line has a
            // column at its end.
            let last = line + 1 == index.line_count();
            let columns = if last { start..=end } else { start..=end.saturating_sub(1) };
            for offset in columns {
                if !text.is_char_boundary(offset) {
                    continue;
                }
                let position = LineCol { line, col: u32::try_from(offset - start).expect("small") };
                let size = index.offset(position).expect("inside the line");
                prop_assert_eq!(usize::from(size), offset);
                let wide = index.to_utf16(position).expect("inside the line");
                prop_assert_eq!(index.to_utf8(wide), Some(position));
            }
            // One column past the end of the line does not exist.
            let past = LineCol { line, col: u32::try_from(end - start + 1).expect("small") };
            prop_assert_eq!(index.offset(past), None);
        }
    }

    #[test]
    fn utf16_columns_inside_a_surrogate_pair_do_not_exist(text in text()) {
        let index = LineIndex::new(&text).expect("test text is small");
        for line in 0..index.line_count() {
            let range = index.line_range(line).expect("the line exists");
            let line_text = &text[usize::from(range.start())..usize::from(range.end())];
            let mut col = 0u32;
            for c in line_text.chars() {
                if c.len_utf16() == 2 {
                    let inside = Utf16LineCol { line, col: col + 1 };
                    prop_assert_eq!(index.to_utf8(inside), None);
                }
                col += u32::try_from(c.len_utf16()).expect("at most two");
            }
        }
    }

    #[test]
    fn positions_past_the_end_do_not_exist(text in text(), extra in 1u32..8) {
        let index = LineIndex::new(&text).expect("test text is small");
        let len = u32::try_from(text.len()).expect("test text is small");
        prop_assert_eq!(index.line_col(TextSize::new(len + extra)), None);
        let line = index.line_count() + extra - 1;
        prop_assert_eq!(index.offset(LineCol { line, col: 0 }), None);
        prop_assert_eq!(index.to_utf8(Utf16LineCol { line, col: 0 }), None);
    }
}
