use langmeld_core::{SourceFile, Span};
use langmeld_language::LanguageParser;
use language::SayParser;

#[test]
fn parses_a_valid_say_statement() {
    let source = SourceFile::new("valid.say", "say hello\n");
    let parsed = SayParser.parse(&source);

    assert!(parsed.is_valid(), "{:?}", parsed.diagnostics());
    assert_eq!(parsed.tree().len(), 1);
    assert_eq!(parsed.tree()[0], "hello");
}

#[test]
fn reports_byte_correct_diagnostics_after_crlf_lines() {
    let source = SourceFile::new("example.say", "say ok\r\ninvalid\r\n");
    let parsed = SayParser.parse(&source);
    let diagnostic = parsed
        .diagnostics()
        .first()
        .expect("invalid line should be diagnosed");
    let location = diagnostic
        .location
        .as_ref()
        .expect("diagnostic should have a source span");

    assert_eq!(location.span(), Span::new(8, 15));
    assert_eq!(source.line_column(location.span().start), (2, 1));
}
