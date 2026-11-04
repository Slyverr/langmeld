use langmeld_core::{LexicalSpec, SourceFile};
use langmeld_language::{LanguageTokenKind, ParseLimits, lex_source, lex_source_with_lexical_spec};

#[test]
fn built_in_lexer_preserves_utf8_crlf_comment_and_string_spans() {
    let source = SourceFile::new("scene.source", "node α -> \"hello\\nworld\" # keep\r\n");
    let lexed = lex_source(&source);

    assert_eq!(
        lexed
            .tokens
            .iter()
            .map(|token| token.kind.clone())
            .collect::<Vec<_>>(),
        [
            LanguageTokenKind::Identifier("node".into()),
            LanguageTokenKind::Identifier("α".into()),
            LanguageTokenKind::Arrow,
            LanguageTokenKind::Text("hello\nworld".into()),
            LanguageTokenKind::Comment,
            LanguageTokenKind::Newline,
        ]
    );
    assert_eq!(
        &source.text[lexed.tokens[1].span.start..lexed.tokens[1].span.end],
        "α"
    );
    let newline = lexed.tokens.last().expect("newline token");
    assert_eq!(&source.text[newline.span.start..newline.span.end], "\r\n");
    assert!(lexed.diagnostics.is_empty());
}

#[test]
fn malformed_lexemes_return_spanned_diagnostics_and_unknown_tokens() {
    let source = SourceFile::new("broken.source", "\"unfinished\n\"bad\\q\" §");
    let lexed = lex_source(&source);

    assert!(
        lexed
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "LM1101")
    );
    assert!(
        lexed
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "LM1102")
    );
    assert!(
        lexed
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "LM1100")
    );
    assert!(
        lexed
            .tokens
            .iter()
            .any(|token| token.kind == LanguageTokenKind::Unknown('§'))
    );
    for diagnostic in &lexed.diagnostics {
        let span = diagnostic.span.expect("lexer diagnostic has a span");
        assert!(source.text.is_char_boundary(span.start));
        assert!(source.text.is_char_boundary(span.end));
        assert!(span.start <= span.end);
    }
}

#[test]
fn configured_comment_rules_preserve_line_breaks_inside_block_comments() {
    let source = SourceFile::new("scene.source", "root /* first\r\n second */ end // tail\n");
    let lexical = LexicalSpec::new()
        .line_comment("//")
        .block_comment("/*", "*/");
    let lexed = lex_source_with_lexical_spec(&source, &lexical);

    assert_eq!(
        lexed
            .tokens
            .iter()
            .map(|token| token.kind.clone())
            .collect::<Vec<_>>(),
        [
            LanguageTokenKind::Identifier("root".into()),
            LanguageTokenKind::Comment,
            LanguageTokenKind::Newline,
            LanguageTokenKind::Comment,
            LanguageTokenKind::Identifier("end".into()),
            LanguageTokenKind::Comment,
            LanguageTokenKind::Newline,
        ]
    );
    let newline = &lexed.tokens[2];
    assert_eq!(&source.text[newline.span.start..newline.span.end], "\r\n");
    assert!(lexed.diagnostics.is_empty());
}

#[test]
fn comment_markers_inside_strings_are_not_lexed_as_comments() {
    let source = SourceFile::new("scene.source", "text \"// /* literal */\"\n");
    let lexical = LexicalSpec::new()
        .line_comment("//")
        .block_comment("/*", "*/");
    let lexed = lex_source_with_lexical_spec(&source, &lexical);

    assert_eq!(
        lexed
            .tokens
            .iter()
            .map(|token| token.kind.clone())
            .collect::<Vec<_>>(),
        [
            LanguageTokenKind::Identifier("text".into()),
            LanguageTokenKind::Text("// /* literal */".into()),
            LanguageTokenKind::Newline,
        ]
    );
    assert!(lexed.diagnostics.is_empty());
}

#[test]
fn an_explicit_policy_can_disable_the_legacy_line_comment() {
    let source = SourceFile::new("scene.source", "# is punctuation\n");
    let lexed = lex_source_with_lexical_spec(&source, &LexicalSpec::new());

    assert_eq!(lexed.tokens[0].kind, LanguageTokenKind::Punctuation('#'));
    assert!(
        !lexed
            .tokens
            .iter()
            .any(|token| token.kind == LanguageTokenKind::Comment)
    );
    assert!(lexed.diagnostics.is_empty());
}

#[test]
fn parser_limits_have_bounded_defaults() {
    let limits = ParseLimits::default();

    assert_eq!(limits.max_source_bytes, 4 * 1024 * 1024);
    assert_eq!(limits.max_tokens, 250_000);
    assert_eq!(limits.max_ast_nodes, 250_000);
    assert_eq!(limits.max_nesting_depth, 128);
    assert_eq!(limits.max_pattern_steps, 1_000_000);
}
