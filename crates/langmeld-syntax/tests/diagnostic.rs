//! `Diagnostic`: one problem, where it is, and how to fix it.

use langmeld_syntax::{Diagnostic, FileId, Fix, Label, Severity, TextEdit, TextRange, TextSize};

fn range(start: u32, end: u32) -> TextRange {
    TextRange::new(TextSize::new(start), TextSize::new(end))
}

#[test]
fn new_diagnostic_has_no_extras() {
    let diagnostic = Diagnostic::new(
        Severity::Error,
        "TEST-001",
        FileId::new(1),
        range(2, 5),
        "something is wrong",
    );
    assert_eq!(diagnostic.severity, Severity::Error);
    assert_eq!(diagnostic.code, "TEST-001");
    assert_eq!(diagnostic.file, FileId::new(1));
    assert_eq!(diagnostic.range, range(2, 5));
    assert_eq!(diagnostic.message, "something is wrong");
    assert!(diagnostic.labels.is_empty());
    assert!(diagnostic.notes.is_empty());
    assert!(diagnostic.fixes.is_empty());
}

#[test]
fn constructors_set_the_severity() {
    let (file, at) = (FileId::new(0), range(0, 1));
    assert_eq!(
        Diagnostic::error("T-1", file, at, "m").severity,
        Severity::Error
    );
    assert_eq!(
        Diagnostic::warning("T-2", file, at, "m").severity,
        Severity::Warning
    );
    assert_eq!(
        Diagnostic::info("T-3", file, at, "m").severity,
        Severity::Info
    );
}

#[test]
fn labels_notes_and_fixes_can_point_at_other_files() {
    let diagnostic = Diagnostic::warning(
        "TEST-002",
        FileId::new(1),
        range(0, 1),
        String::from("`x` is defined twice"),
    )
    .label(Label::new(
        FileId::new(2),
        range(8, 9),
        "first definition is here",
    ))
    .note("names are shared across files")
    .fix(Fix::new(
        "rename to `x2`",
        vec![TextEdit::new(FileId::new(1), range(0, 1), "x2")],
    ));

    assert_eq!(diagnostic.labels[0].file, FileId::new(2));
    assert_eq!(diagnostic.labels[0].message, "first definition is here");
    assert_eq!(diagnostic.notes, ["names are shared across files"]);
    assert_eq!(diagnostic.fixes[0].title, "rename to `x2`");
    assert_eq!(diagnostic.fixes[0].edits[0].replacement, "x2");
    assert_eq!(diagnostic.clone(), diagnostic);
}

#[test]
fn builders_keep_the_order_they_were_called_in() {
    let diagnostic = Diagnostic::error("TEST-003", FileId::new(0), range(0, 0), "m")
        .note("first")
        .note("second");
    assert_eq!(diagnostic.notes, ["first", "second"]);
}
