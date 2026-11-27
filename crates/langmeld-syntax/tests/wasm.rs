//! The core types work when compiled to WebAssembly (run in Node and in a browser).
//! The same tests also run natively, so both builds are held to the same results.

use langmeld_syntax::{
    Diagnostic, FileId, Fix, Label, LineCol, Severity, SourceFile, SyntaxKind, SyntaxLanguage,
    TextEdit, TextRange, TextSize, Utf16LineCol,
};
use rowan::{GreenNodeBuilder, Language, SyntaxNode};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_test::wasm_bindgen_test;

fn range(start: u32, end: u32) -> TextRange {
    TextRange::new(TextSize::new(start), TextSize::new(end))
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn source_file_keeps_its_parts() {
    let file = SourceFile::new(FileId::new(3), "a.calc", "x = 1\n").expect("text is small");
    assert_eq!(file.id(), FileId::new(3));
    assert_eq!(file.name(), "a.calc");
    assert_eq!(file.text(), "x = 1\n");
    assert_eq!(file.line_index().line_count(), 2);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn line_index_converts_emoji_and_crlf() {
    let file = SourceFile::new(FileId::new(0), "a.calc", "a😀\r\nb").expect("text is small");
    let index = file.line_index();

    let b = index.line_col(TextSize::new(7));
    assert_eq!(b, Some(LineCol { line: 1, col: 0 }));
    let end_of_first = LineCol { line: 0, col: 5 };
    assert_eq!(index.offset(end_of_first), Some(TextSize::new(5)));
    assert_eq!(
        index.to_utf16(end_of_first),
        Some(Utf16LineCol { line: 0, col: 3 })
    );
    assert_eq!(
        index.to_utf8(Utf16LineCol { line: 0, col: 3 }),
        Some(end_of_first)
    );
    // The middle of the emoji is not a position.
    assert_eq!(index.line_col(TextSize::new(2)), None);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn diagnostics_are_built_with_details() {
    let file = FileId::new(0);
    let diagnostic = Diagnostic::warning("TEST-001", file, range(0, 1), "unused")
        .label(Label::new(file, range(2, 3), "declared here"))
        .note("remove it")
        .fix(Fix::new(
            "remove",
            vec![TextEdit::new(file, range(0, 1), "")],
        ));
    assert_eq!(diagnostic.severity, Severity::Warning);
    assert_eq!(diagnostic.labels.len(), 1);
    assert_eq!(diagnostic.notes, ["remove it"]);
    assert_eq!(diagnostic.fixes[0].edits[0].replacement, "");
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn syntax_tree_is_lossless() {
    const FILE: SyntaxKind = SyntaxKind(0);
    const WORD: SyntaxKind = SyntaxKind(1);
    const SPACE: SyntaxKind = SyntaxKind(2);

    let mut builder = GreenNodeBuilder::new();
    builder.start_node(SyntaxLanguage::kind_to_raw(FILE));
    for (kind, text) in [(WORD, "héllo"), (SPACE, " \r\n"), (WORD, "😀")] {
        builder.token(SyntaxLanguage::kind_to_raw(kind), text);
    }
    builder.finish_node();
    let root = SyntaxNode::<SyntaxLanguage>::new_root(builder.finish());

    assert_eq!(root.kind(), FILE);
    assert_eq!(root.text().to_string(), "héllo \r\n😀");
    assert_eq!(root.children_with_tokens().count(), 3);
}
