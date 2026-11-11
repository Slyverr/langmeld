use std::path::Path;

use langmeld_core::{SourceFile, Span};
use langmeld_custom_lexer_example::{RawStringLexer, load_language};
use langmeld_language::{CapturedValue, parse_with_lexer};

#[test]
fn combines_a_langmeld_grammar_with_a_host_lexer() {
    let language = load_language(Path::new(env!("CARGO_MANIFEST_DIR")))
        .expect("the example definition should load");
    let source = SourceFile::new(
        "main.rawsay",
        "say r#\"comments // inside the string stay text\"#; // ignored\n",
    );

    let parsed = parse_with_lexer(&source, &language, &RawStringLexer);

    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(parsed.constructs.len(), 1);
    assert_eq!(
        parsed.constructs[0].fields[0].value,
        CapturedValue::String("r#\"comments // inside the string stay text\"#".to_owned())
    );
}

#[test]
fn reports_an_unterminated_raw_string_at_its_source_span() {
    let language = load_language(Path::new(env!("CARGO_MANIFEST_DIR")))
        .expect("the example definition should load");
    let source = SourceFile::new("broken.rawsay", "say r#\"missing terminator");

    let parsed = parse_with_lexer(&source, &language, &RawStringLexer);

    let diagnostic = parsed
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "RAW-LEX-001")
        .expect("the custom lexer should report the malformed raw string");
    assert_eq!(
        diagnostic.location.as_ref().unwrap().span(),
        Span::new(4, source.text().len())
    );
}
