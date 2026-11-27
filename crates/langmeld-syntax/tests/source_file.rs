//! `FileId` and `SourceFile`: text with an identity.

use langmeld_syntax::{FileId, LineCol, SourceFile, TextSize};

#[test]
fn keeps_identity_and_text() {
    let file = SourceFile::new(FileId::new(7), "src/main.calc", "let x = 1;\n").expect("small");
    assert_eq!(file.id(), FileId::new(7));
    assert_eq!(file.id().index(), 7);
    assert_eq!(file.name(), "src/main.calc");
    assert_eq!(file.text(), "let x = 1;\n");
}

#[test]
fn text_is_kept_byte_for_byte() {
    let text = "\u{FEFF}a\r\nb\r😀\0";
    let file = SourceFile::new(FileId::new(0), "mixed", text).expect("small");
    assert_eq!(file.text(), text);
}

#[test]
fn line_index_belongs_to_the_text() {
    let file = SourceFile::new(FileId::new(0), "two-lines", "a\nbc").expect("small");
    assert_eq!(file.line_index().line_count(), 2);
    assert_eq!(file.line_index().len(), TextSize::of(file.text()));
    assert_eq!(
        file.line_index().line_col(TextSize::new(3)),
        Some(LineCol { line: 1, col: 1 })
    );
}

#[test]
fn file_ids_are_ordered_by_index() {
    assert!(FileId::new(1) < FileId::new(2));
}
