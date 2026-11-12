use langmeld_aster_language_example::AsterBackend;
use langmeld_core::{SourceFile, Span};
use langmeld_language::{CompletionInsertTextFormat, CompletionKind};
use langmeld_lsp::LanguageBackend;

#[test]
fn suggests_a_function_snippet_on_a_top_level_blank_line() {
    let text = "fn main() -> i32 { return 0; }\n    ";
    let source = SourceFile::new("main.aster", text);
    let document = AsterBackend.analyze(&source);
    let candidates = AsterBackend.complete(&source, &document, text.len());
    let line_start = text.find('\n').unwrap() + 1;

    assert_eq!(candidates.len(), 1);
    let candidate = &candidates[0];
    assert_eq!(
        candidate.text,
        "fn ${1:name}(${2:parameter}: ${3:i32}) -> ${4:i32} {\n    $0\n}"
    );
    assert_eq!(candidate.label.as_deref(), Some("Function declaration"));
    assert_eq!(candidate.kind, Some(CompletionKind::Snippet));
    assert_eq!(
        candidate.insert_text_format,
        CompletionInsertTextFormat::Snippet
    );
    assert_eq!(candidate.replace, Span::new(line_start, text.len()));
}

#[test]
fn does_not_suggest_a_top_level_snippet_inside_a_function_body() {
    let text = "fn main() -> i32 {\n    \n    return 0;\n}\n";
    let source = SourceFile::new("main.aster", text);
    let document = AsterBackend.analyze(&source);
    let offset = text.find("    \n").unwrap() + 4;

    assert!(AsterBackend.complete(&source, &document, offset).is_empty());
}

#[test]
fn does_not_suggest_a_function_snippet_on_nonempty_lines_or_invalid_offsets() {
    let text = "fn main() -> i32 { return 0; }\nf";
    let source = SourceFile::new("main.aster", text);
    let document = AsterBackend.analyze(&source);

    assert!(
        AsterBackend
            .complete(&source, &document, text.len())
            .is_empty()
    );
    assert!(
        AsterBackend
            .complete(&source, &document, text.len() + 1)
            .is_empty()
    );
}
