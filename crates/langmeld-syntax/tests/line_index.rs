//! `LineIndex`: offsets, lines and UTF-8 / UTF-16 columns.

use langmeld_syntax::{LineCol, LineIndex, TextRange, TextSize, Utf16LineCol};

fn index(text: &str) -> LineIndex {
    LineIndex::new(text).expect("test text is small")
}

fn at(offset: u32) -> TextSize {
    TextSize::new(offset)
}

fn lc(line: u32, col: u32) -> LineCol {
    LineCol { line, col }
}

fn wide(line: u32, col: u32) -> Utf16LineCol {
    Utf16LineCol { line, col }
}

#[test]
fn empty_text_has_one_empty_line() {
    let index = index("");
    assert_eq!(index.len(), at(0));
    assert_eq!(index.line_count(), 1);
    assert_eq!(index.line_range(0), Some(TextRange::empty(at(0))));
    assert_eq!(index.line_range(1), None);
    assert_eq!(index.line_col(at(0)), Some(lc(0, 0)));
    assert_eq!(index.line_col(at(1)), None);
    assert_eq!(index.offset(lc(0, 0)), Some(at(0)));
}

#[test]
fn line_feed_ends_a_line() {
    let index = index("ab\ncd\n");
    assert_eq!(index.line_count(), 3);
    assert_eq!(index.line_range(0), Some(TextRange::new(at(0), at(3))));
    assert_eq!(index.line_range(1), Some(TextRange::new(at(3), at(6))));
    assert_eq!(index.line_range(2), Some(TextRange::empty(at(6))));
    assert_eq!(index.line_col(at(2)), Some(lc(0, 2)));
    assert_eq!(index.line_col(at(3)), Some(lc(1, 0)));
    assert_eq!(index.line_col(at(6)), Some(lc(2, 0)));
}

#[test]
fn crlf_is_one_terminator() {
    let index = index("ab\r\ncd");
    assert_eq!(index.line_count(), 2);
    assert_eq!(index.line_range(0), Some(TextRange::new(at(0), at(4))));
    // The offset between `\r` and `\n` stays on the first line.
    assert_eq!(index.line_col(at(3)), Some(lc(0, 3)));
    assert_eq!(index.offset(lc(0, 3)), Some(at(3)));
    assert_eq!(index.line_col(at(4)), Some(lc(1, 0)));
}

#[test]
fn lone_carriage_return_ends_a_line() {
    let index = index("a\rb\r\rc\r");
    assert_eq!(index.line_count(), 5);
    assert_eq!(index.line_col(at(2)), Some(lc(1, 0)));
    assert_eq!(index.line_col(at(4)), Some(lc(2, 0)));
    assert_eq!(index.line_col(at(5)), Some(lc(3, 0)));
    assert_eq!(index.line_col(at(7)), Some(lc(4, 0)));
}

#[test]
fn line_feed_then_carriage_return_is_two_terminators() {
    assert_eq!(index("a\n\rb").line_count(), 3);
}

#[test]
fn unicode_line_separators_do_not_end_lines() {
    assert_eq!(index("a\u{2028}b\u{2029}c\u{0085}d").line_count(), 1);
}

#[test]
fn emoji_takes_four_bytes_and_two_utf16_units() {
    let index = index("a😀b");
    assert_eq!(index.line_col(at(5)), Some(lc(0, 5)));
    assert_eq!(index.to_utf16(lc(0, 1)), Some(wide(0, 1)));
    assert_eq!(index.to_utf16(lc(0, 5)), Some(wide(0, 3)));
    assert_eq!(index.to_utf16(lc(0, 6)), Some(wide(0, 4)));
    assert_eq!(index.to_utf8(wide(0, 3)), Some(lc(0, 5)));
    assert_eq!(index.to_utf8(wide(0, 4)), Some(lc(0, 6)));
}

#[test]
fn positions_inside_a_character_are_rejected() {
    let index = index("a😀b");
    for offset in 2..5 {
        assert_eq!(index.line_col(at(offset)), None, "offset {offset}");
        assert_eq!(index.offset(lc(0, offset)), None, "column {offset}");
        assert_eq!(index.to_utf16(lc(0, offset)), None, "column {offset}");
    }
    // Between the two halves of the surrogate pair.
    assert_eq!(index.to_utf8(wide(0, 2)), None);
}

#[test]
fn cjk_takes_three_bytes_and_one_utf16_unit() {
    let index = index("let 名前 = 1;\n値");
    assert_eq!(index.to_utf16(lc(0, 4)), Some(wide(0, 4)));
    assert_eq!(index.to_utf16(lc(0, 7)), Some(wide(0, 5)));
    assert_eq!(index.to_utf16(lc(0, 10)), Some(wide(0, 6)));
    assert_eq!(index.to_utf8(wide(0, 6)), Some(lc(0, 10)));
    assert_eq!(index.to_utf16(lc(1, 3)), Some(wide(1, 1)));
    assert_eq!(index.to_utf8(wide(1, 1)), Some(lc(1, 3)));
}

#[test]
fn utf16_columns_restart_on_every_line() {
    let index = index("😀é\r\né😀x");
    assert_eq!(index.to_utf16(lc(0, 6)), Some(wide(0, 3)));
    assert_eq!(index.to_utf16(lc(1, 0)), Some(wide(1, 0)));
    assert_eq!(index.to_utf16(lc(1, 6)), Some(wide(1, 3)));
    assert_eq!(index.to_utf8(wide(1, 3)), Some(lc(1, 6)));
}

#[test]
fn byte_order_mark_counts_as_a_character() {
    let index = index("\u{FEFF}ab\nc");
    assert_eq!(index.line_col(at(0)), Some(lc(0, 0)));
    assert_eq!(index.line_col(at(1)), None);
    assert_eq!(index.line_col(at(3)), Some(lc(0, 3)));
    assert_eq!(index.to_utf16(lc(0, 3)), Some(wide(0, 1)));
    assert_eq!(index.to_utf16(lc(1, 1)), Some(wide(1, 1)));
}

#[test]
fn positions_outside_the_text_are_rejected() {
    let index = index("ab\ncd");
    assert_eq!(index.line_col(at(6)), None);
    assert_eq!(index.line_col(at(u32::MAX)), None);
    // Column 3 of the first line would be the start of the second line.
    assert_eq!(index.offset(lc(0, 3)), None);
    assert_eq!(index.offset(lc(1, 2)), Some(at(5)));
    assert_eq!(index.offset(lc(1, 3)), None);
    assert_eq!(index.offset(lc(2, 0)), None);
    assert_eq!(index.offset(lc(1, u32::MAX)), None);
    assert_eq!(index.offset(lc(u32::MAX, u32::MAX)), None);
    assert_eq!(index.to_utf16(lc(2, 0)), None);
    assert_eq!(index.to_utf8(wide(0, 3)), None);
    assert_eq!(index.to_utf8(wide(1, 3)), None);
    assert_eq!(index.to_utf8(wide(2, 0)), None);
    assert_eq!(index.to_utf8(wide(1, u32::MAX)), None);
    assert_eq!(index.line_range(u32::MAX), None);
}

/// A direct, slow model of the rules: walk the text one character at a time.
fn model(text: &str) -> Vec<(u32, LineCol, Utf16LineCol)> {
    let mut positions = Vec::new();
    let (mut line, mut col, mut col16) = (0, 0, 0);
    let mut chars = text.char_indices().peekable();
    while let Some((offset, c)) = chars.next() {
        positions.push((offset as u32, lc(line, col), wide(line, col16)));
        let ends_line = c == '\n' || (c == '\r' && chars.peek().map(|&(_, n)| n) != Some('\n'));
        if ends_line {
            (line, col, col16) = (line + 1, 0, 0);
        } else {
            col += c.len_utf8() as u32;
            col16 += c.len_utf16() as u32;
        }
    }
    positions.push((text.len() as u32, lc(line, col), wide(line, col16)));
    positions
}

#[test]
fn agrees_with_a_model_on_every_short_text() {
    const ALPHABET: [char; 7] = ['a', '\n', '\r', 'é', '中', '😀', '\u{FEFF}'];
    const MAX_LEN: u32 = 5;

    for len in 0..=MAX_LEN {
        for mut number in 0..ALPHABET.len().pow(len) {
            let mut text = String::new();
            for _ in 0..len {
                text.push(ALPHABET[number % ALPHABET.len()]);
                number /= ALPHABET.len();
            }
            check_against_model(&text);
        }
    }
}

fn check_against_model(text: &str) {
    let index = index(text);
    let positions = model(text);
    let last_line = positions[positions.len() - 1].1.line;
    assert_eq!(index.line_count(), last_line + 1, "{text:?}");

    for offset in 0..=text.len() as u32 + 1 {
        let expected = positions.iter().find(|p| p.0 == offset);
        assert_eq!(
            index.line_col(at(offset)),
            expected.map(|p| p.1),
            "{text:?} at {offset}"
        );
    }
    for line in 0..=last_line + 1 {
        // Four bytes past the longest possible line.
        for col in 0..=text.len() as u32 + 4 {
            let by_utf8 = positions.iter().find(|p| p.1 == lc(line, col));
            assert_eq!(
                index.offset(lc(line, col)),
                by_utf8.map(|p| at(p.0)),
                "{text:?} {line}:{col}"
            );
            assert_eq!(
                index.to_utf16(lc(line, col)),
                by_utf8.map(|p| p.2),
                "{text:?} {line}:{col}"
            );
            let by_utf16 = positions.iter().find(|p| p.2 == wide(line, col));
            assert_eq!(
                index.to_utf8(wide(line, col)),
                by_utf16.map(|p| p.1),
                "{text:?} {line}:{col}"
            );
        }
    }
}
