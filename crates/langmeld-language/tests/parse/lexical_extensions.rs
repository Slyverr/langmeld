use super::*;

struct RawStringExtension;

impl LexerExtension for RawStringExtension {
    fn lex_token(&self, source: &SourceFile, offset: usize) -> Option<LexicalMatch> {
        let text = source.text();
        let content_start = offset.checked_add(3)?;
        if !text.get(offset..)?.starts_with("r#\"") {
            return None;
        }

        let (end, kind) = match text[content_start..].find("\"#") {
            Some(relative_end) => (content_start + relative_end + 2, TokenKind::String),
            None => (text.len(), TokenKind::Invalid),
        };
        let span = Span::new(offset, end);
        let token = LexedToken {
            text: text[offset..end].to_owned(),
            kind,
            span,
        };
        let mut matched = LexicalMatch::new(token);

        if kind == TokenKind::Invalid {
            matched.diagnostics.push(Diagnostic::error(
                "TEST-LEX-001",
                "unterminated raw string",
                source,
                span,
            ));
        }

        Some(matched)
    }
}

#[test]
fn extension_tokens_compose_with_default_tokens_and_comment_rules() {
    let rules = LexicalRules::default().line_comment("//");
    let source = SourceFile::new("input", "name // ignored\nr#\"raw text\"# tail");
    let lexer = ExtensibleLexer::new(&rules).with_extension(&RawStringExtension);

    let output = lexer.lex(&source);
    let tokens = output
        .tokens
        .iter()
        .map(|token| (token.text.as_str(), token.kind))
        .collect::<Vec<_>>();

    assert!(output.diagnostics.is_empty());
    assert_eq!(
        tokens,
        [
            ("name", TokenKind::Identifier),
            ("r#\"raw text\"#", TokenKind::String),
            ("tail", TokenKind::Identifier),
        ]
    );
}

#[test]
fn extension_matches_share_the_configured_token_limit() {
    let source = SourceFile::new("input", "r#\"raw\"# tail");
    let rules = LexicalRules::default();
    let lexer = ExtensibleLexer::new(&rules).with_extension(&RawStringExtension);

    let output = lexer.lex_with_token_limit(&source, 1);

    assert_eq!(output.tokens.len(), 1);
    assert_eq!(output.tokens[0].text, "r#\"raw\"#");
    assert_eq!(output.diagnostics[0].code, "LM-PARSE-008");
    assert_eq!(
        output.diagnostics[0]
            .location
            .as_ref()
            .map(|location| location.span()),
        Some(Span::new(9, 10))
    );
}

#[test]
fn the_first_matching_extension_has_priority() {
    struct FirstExtension;

    impl LexerExtension for FirstExtension {
        fn lex_token(&self, source: &SourceFile, offset: usize) -> Option<LexicalMatch> {
            (offset == 0).then(|| {
                LexicalMatch::new(LexedToken {
                    text: source.text()[offset..offset + 1].to_owned(),
                    kind: TokenKind::String,
                    span: Span::new(offset, offset + 1),
                })
            })
        }
    }

    struct SecondExtension;

    impl LexerExtension for SecondExtension {
        fn lex_token(&self, source: &SourceFile, offset: usize) -> Option<LexicalMatch> {
            (offset == 0).then(|| {
                LexicalMatch::new(LexedToken {
                    text: source.text()[offset..offset + 1].to_owned(),
                    kind: TokenKind::Integer,
                    span: Span::new(offset, offset + 1),
                })
            })
        }
    }

    let source = SourceFile::new("input", "x");
    let rules = LexicalRules::default();
    let lexer = ExtensibleLexer::new(&rules)
        .with_extension(&FirstExtension)
        .with_extension(&SecondExtension);

    let output = lexer.lex(&source);

    assert_eq!(output.tokens[0].kind, TokenKind::String);
}

#[test]
fn extension_diagnostics_survive_and_invalid_tokens_are_rejected_safely() {
    let source = SourceFile::new("input", "r#\"unfinished");
    let rules = LexicalRules::default();
    let lexer = ExtensibleLexer::new(&rules).with_extension(&RawStringExtension);

    let output = lexer.lex(&source);

    assert_eq!(output.tokens.len(), 1);
    assert_eq!(output.tokens[0].kind, TokenKind::Invalid);
    assert_eq!(output.diagnostics[0].code, "TEST-LEX-001");

    struct InvalidRangeExtension;

    impl LexerExtension for InvalidRangeExtension {
        fn lex_token(&self, _source: &SourceFile, offset: usize) -> Option<LexicalMatch> {
            (offset == 0).then(|| {
                LexicalMatch::new(LexedToken {
                    text: "wrong".to_owned(),
                    kind: TokenKind::Identifier,
                    span: Span::new(offset, offset + 1),
                })
            })
        }
    }

    let invalid_source = SourceFile::new("input", "x");
    let invalid_lexer = ExtensibleLexer::new(&rules).with_extension(&InvalidRangeExtension);
    let invalid_output = invalid_lexer.lex(&invalid_source);

    assert!(invalid_output.tokens.is_empty());
    assert_eq!(invalid_output.diagnostics[0].code, "LM-LEX-002");
}
