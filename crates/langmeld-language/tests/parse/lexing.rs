use super::*;

#[test]
fn ascii_integers_and_unicode_identifiers_have_consistent_token_boundaries() {
    let source = SourceFile::new("tokens", "123abc abc123 fox١23 ١12");
    let output = DefaultLexer.lex(&source);
    let tokens = output
        .tokens
        .iter()
        .map(|token| (token.text.as_str(), token.kind))
        .collect::<Vec<_>>();

    assert!(output.diagnostics.is_empty());
    assert_eq!(
        tokens,
        [
            ("123", TokenKind::Integer),
            ("abc", TokenKind::Identifier),
            ("abc123", TokenKind::Identifier),
            ("fox١23", TokenKind::Identifier),
            ("١", TokenKind::Punctuation),
            ("12", TokenKind::Integer),
        ]
    );
}

#[test]
fn configured_line_and_block_comments_are_skipped_without_changing_string_tokens() {
    let grammar = LanguageSpec::new(
        "Comments",
        [ConstructSpec::new(
            "print",
            [
                PatternPart::literal("print"),
                PatternPart::string("value"),
                PatternPart::literal(";"),
            ],
        )],
    )
    .unwrap()
    .with_lexical_rules(
        LexicalRules::default()
            .line_comment("#")
            .block_comment("/*", "*/"),
    )
    .unwrap();
    let source = SourceFile::new(
        "comments",
        "# before\nprint \"/* string */\"; /* between */ print \"// string\";",
    );

    let parsed = parse(&source, &grammar);

    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert_eq!(parsed.constructs().len(), 2);
    assert_eq!(
        parsed.constructs()[0].fields[0].value,
        CapturedValue::String("\"/* string */\"".into())
    );
    assert_eq!(
        parsed.constructs()[1].fields[0].value,
        CapturedValue::String("\"// string\"".into())
    );
}

#[test]
fn unterminated_block_comments_report_a_source_located_lexical_error() {
    let grammar = LanguageSpec::new(
        "Comments",
        [ConstructSpec::new("print", [PatternPart::literal("print")])],
    )
    .unwrap()
    .with_lexical_rules(LexicalRules::default().block_comment("/*", "*/"))
    .unwrap();
    let source = SourceFile::new("comments", "print /* never closed");

    let parsed = parse(&source, &grammar);

    let diagnostic = parsed
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.code == "LM-LEX-003")
        .expect("unterminated comment diagnostic");
    assert_eq!(
        diagnostic.location.as_ref().unwrap().span(),
        Span::new(6, source.text().len())
    );
}

#[test]
fn longest_comment_opener_wins_when_comment_markers_share_a_prefix() {
    let grammar = LanguageSpec::new(
        "Comments",
        [ConstructSpec::new("item", [PatternPart::literal("item")])],
    )
    .unwrap()
    .with_lexical_rules(
        LexicalRules::default()
            .line_comment("#")
            .block_comment("##", "!!"),
    )
    .unwrap();

    let parsed = parse(&SourceFile::new("comments", "## ignored !! item"), &grammar);

    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert_eq!(parsed.constructs().len(), 1);
}

#[test]
fn rejects_empty_and_duplicate_comment_markers() {
    for marker in [" ", "\"", "#\n"] {
        let construct = ConstructSpec::new("item", [PatternPart::literal("item")]);
        assert!(matches!(
            LanguageSpec::new("Comments", [construct])
                .unwrap()
                .with_lexical_rules(LexicalRules::default().line_comment(marker)),
            Err(langmeld_language::SpecError::InvalidLineComment(_))
        ));
    }
    let construct = ConstructSpec::new("item", [PatternPart::literal("item")]);
    assert!(matches!(
        LanguageSpec::new("Comments", [construct])
            .unwrap()
            .with_lexical_rules(
                LexicalRules::default()
                    .line_comment("#")
                    .block_comment("#", "!"),
            ),
        Err(langmeld_language::SpecError::DuplicateCommentStart(_))
    ));
}

#[test]
fn reports_unmatched_tokens_and_keeps_parsing_after_them() {
    let source = SourceFile::new("main", "? print value;");
    let result = parse(&source, &language());

    assert_eq!(result.constructs().len(), 1);
    assert_eq!(result.diagnostics().len(), 1);
    assert_eq!(result.diagnostics()[0].severity, Severity::Error);
    assert_eq!(
        result.diagnostics()[0]
            .location
            .as_ref()
            .unwrap()
            .span()
            .start,
        0
    );
}

#[test]
fn keeps_escaped_line_breaks_inside_string_tokens() {
    let grammar = LanguageSpec::new(
        "Strings",
        [ConstructSpec::new(
            "say",
            [
                PatternPart::literal("say"),
                PatternPart::string("value"),
                PatternPart::literal(";"),
            ],
        )],
    )
    .unwrap();
    let source = SourceFile::new("strings", "say \"first\\\nsecond\";");

    let parsed = parse(&source, &grammar);

    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert_eq!(
        parsed.constructs()[0].fields[0].value,
        CapturedValue::String("\"first\\\nsecond\"".into())
    );
}

#[test]
fn keeps_unmapped_punctuation_as_single_character_tokens() {
    let grammar = LanguageSpec::new(
        "Punctuation",
        [ConstructSpec::new(
            "symbols",
            [
                PatternPart::literal("@"),
                PatternPart::literal("#"),
                PatternPart::literal(";"),
            ],
        )
        .variant([PatternPart::literal("+-")])],
    )
    .unwrap();

    let separate_symbols = SourceFile::new("punctuation", "@#;");
    let parsed_separate = parse(&separate_symbols, &grammar);

    assert!(
        parsed_separate.diagnostics().is_empty(),
        "{:?}",
        parsed_separate.diagnostics()
    );
    assert_eq!(parsed_separate.constructs().len(), 1);

    let grouped_symbols = SourceFile::new("punctuation", "+-");
    let parsed_grouped = parse(&grouped_symbols, &grammar);

    assert!(
        parsed_grouped.diagnostics().is_empty(),
        "{:?}",
        parsed_grouped.diagnostics()
    );
    assert_eq!(parsed_grouped.constructs().len(), 1);
}

#[test]
fn reports_unterminated_string_literals() {
    let grammar = LanguageSpec::new(
        "Strings",
        [ConstructSpec::new(
            "say",
            [
                PatternPart::literal("say"),
                PatternPart::string("value"),
                PatternPart::literal(";"),
            ],
        )],
    )
    .unwrap();
    let source = SourceFile::new("main", "say \"unfinished");
    let result = parse(&source, &grammar);

    assert_eq!(result.diagnostics().len(), 2);
    assert_eq!(result.diagnostics()[0].code, "LM-LEX-001");
    assert_eq!(result.diagnostics()[1].code, "LM-PARSE-002");
}

#[test]
fn custom_lexer_can_supply_host_specific_string_tokens() {
    struct RawStringLexer;

    impl Lexer for RawStringLexer {
        fn lex(&self, source: &SourceFile) -> LexOutput {
            let string_start = source.text().find("r#\"").unwrap();
            LexOutput {
                tokens: vec![
                    LexedToken {
                        text: "emit".into(),
                        kind: TokenKind::Identifier,
                        span: Span::new(0, 4),
                    },
                    LexedToken {
                        text: source.text()[string_start..].into(),
                        kind: TokenKind::String,
                        span: Span::new(string_start, source.text().len()),
                    },
                ],
                diagnostics: Vec::new(),
            }
        }
    }

    let grammar = LanguageSpec::new(
        "RawStrings",
        [ConstructSpec::new(
            "emit",
            [PatternPart::literal("emit"), PatternPart::string("text")],
        )],
    )
    .unwrap();
    let source = SourceFile::new("raw", "emit r#\"hello\"#");

    let parsed = parse_with_lexer(&source, &grammar, &RawStringLexer);

    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert_eq!(
        parsed.constructs()[0].fields[0].value,
        CapturedValue::String("r#\"hello\"#".into())
    );
}

#[test]
fn generic_token_capture_preserves_custom_lexer_text_and_kind() {
    struct TaggedTokenLexer;

    impl Lexer for TaggedTokenLexer {
        fn lex(&self, _source: &SourceFile) -> LexOutput {
            LexOutput {
                tokens: vec![
                    LexedToken {
                        text: "emit".into(),
                        kind: TokenKind::Identifier,
                        span: Span::new(0, 4),
                    },
                    LexedToken {
                        text: "@tag".into(),
                        kind: TokenKind::Punctuation,
                        span: Span::new(5, 9),
                    },
                    LexedToken {
                        text: ";".into(),
                        kind: TokenKind::Punctuation,
                        span: Span::new(9, 10),
                    },
                ],
                diagnostics: Vec::new(),
            }
        }
    }

    let grammar = LanguageSpec::new(
        "TaggedTokens",
        [ConstructSpec::new(
            "emit",
            [
                PatternPart::literal("emit"),
                PatternPart::token("value"),
                PatternPart::literal(";"),
            ],
        )],
    )
    .unwrap();
    let source = SourceFile::new("tags", "emit @tag;");

    let parsed = parse_with_lexer(&source, &grammar, &TaggedTokenLexer);

    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert_eq!(
        parsed.constructs()[0].fields[0].value,
        CapturedValue::Token {
            text: "@tag".into(),
            kind: TokenKind::Punctuation,
        }
    );
}

#[test]
fn finite_choice_fields_capture_only_declared_tokens() {
    let grammar = LanguageSpec::new(
        "Movement",
        [ConstructSpec::new(
            "move",
            [
                PatternPart::literal("move"),
                PatternPart::choice("direction", ["left", "right"]),
                PatternPart::literal(";"),
            ],
        )],
    )
    .unwrap();

    let parsed = parse(&SourceFile::new("program", "move left;"), &grammar);
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert_eq!(
        parsed.constructs()[0].fields[0].value,
        CapturedValue::Token {
            text: "left".into(),
            kind: TokenKind::Identifier,
        }
    );

    let rejected = parse(&SourceFile::new("program", "move up;"), &grammar);
    let diagnostic = rejected
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.code == "LM-PARSE-002")
        .expect("undeclared choice values are rejected");
    assert!(diagnostic.message.contains("`left`, `right`"));
}

#[test]
fn finite_choice_fields_reject_empty_duplicate_and_multitoken_values() {
    for values in [vec![], vec!["left", "left"], vec!["two words"]] {
        assert!(matches!(
            LanguageSpec::new(
                "InvalidChoice",
                [ConstructSpec::new(
                    "move",
                    [PatternPart::choice("direction", values)],
                )],
            ),
            Err(langmeld_language::SpecError::EmptyChoiceValues(_)
                | langmeld_language::SpecError::InvalidChoiceValue { .. }
                | langmeld_language::SpecError::DuplicateChoiceValue { .. })
        ));
    }
}

#[test]
fn rejects_custom_tokens_that_do_not_match_their_source_spans() {
    struct IncorrectLexer;

    impl Lexer for IncorrectLexer {
        fn lex(&self, _source: &SourceFile) -> LexOutput {
            LexOutput {
                tokens: vec![LexedToken {
                    text: "wrong".into(),
                    kind: TokenKind::Identifier,
                    span: Span::new(0, 4),
                }],
                diagnostics: Vec::new(),
            }
        }
    }

    let grammar = LanguageSpec::new(
        "Custom",
        [ConstructSpec::new("item", [PatternPart::literal("item")])],
    )
    .unwrap();
    let source = SourceFile::new("source", "item");

    let parsed = parse_with_lexer(&source, &grammar, &IncorrectLexer);

    assert_eq!(parsed.diagnostics()[0].code, "LM-LEX-002");
    assert!(parsed.constructs().is_empty());
}

#[test]
fn suppresses_custom_lexer_diagnostics_without_valid_source_locations() {
    struct DiagnosticsLexer {
        diagnostics: Vec<Diagnostic>,
    }

    impl Lexer for DiagnosticsLexer {
        fn lex(&self, _source: &SourceFile) -> LexOutput {
            LexOutput {
                tokens: Vec::new(),
                diagnostics: self.diagnostics.clone(),
            }
        }
    }

    let source = SourceFile::new("source", "item");
    let valid = Diagnostic::warning("HOST-001", "valid finding", &source, Span::new(0, 4));
    let foreign = SourceFile::new("source", "item");
    let lexer = DiagnosticsLexer {
        diagnostics: vec![
            valid.clone(),
            Diagnostic::error("HOST-002", "foreign snapshot", &foreign, Span::new(0, 4)),
            Diagnostic::error("HOST-003", "invalid range", &source, Span::new(1, 5)),
            Diagnostic::spec_info("HOST-004", "missing source location"),
        ],
    };
    let grammar = LanguageSpec::new(
        "Custom",
        [ConstructSpec::new("item", [PatternPart::literal("item")])],
    )
    .unwrap();

    let parsed = parse_with_lexer(&source, &grammar, &lexer);

    assert_eq!(parsed.diagnostics().len(), 2);
    assert_eq!(parsed.diagnostics()[0], valid);
    let malformed = &parsed.diagnostics()[1];
    assert_eq!(malformed.code, "LM-LEX-004");
    let location = malformed
        .location
        .as_ref()
        .expect("framework diagnostics must use the parsed source");
    assert_eq!(location.source_identity(), source.identity());
    assert_eq!(location.span(), Span::new(0, 0));
}

#[test]
fn reports_source_located_diagnostics_for_invalid_custom_token_ranges() {
    struct InvalidSpanLexer;

    impl Lexer for InvalidSpanLexer {
        fn lex(&self, _source: &SourceFile) -> LexOutput {
            LexOutput {
                tokens: vec![
                    LexedToken {
                        text: "x".into(),
                        kind: TokenKind::Identifier,
                        span: Span::new(1, 2),
                    },
                    LexedToken {
                        text: "y".into(),
                        kind: TokenKind::Identifier,
                        span: Span::new(99, 100),
                    },
                    LexedToken {
                        text: "z".into(),
                        kind: TokenKind::Identifier,
                        span: Span::new(3, 4),
                    },
                ],
                diagnostics: Vec::new(),
            }
        }
    }

    let grammar = LanguageSpec::new(
        "Unicode",
        [ConstructSpec::new(
            "item",
            [PatternPart::identifier("name")],
        )],
    )
    .unwrap();
    let source = SourceFile::new("unicode", "🦀");

    let parsed = parse_with_lexer(&source, &grammar, &InvalidSpanLexer);

    assert_eq!(parsed.diagnostics()[0].code, "LM-LEX-002");
    let utf8_location = parsed.diagnostics()[0]
        .location
        .as_ref()
        .expect("invalid UTF-8 ranges should retain the source identity");
    assert_eq!(utf8_location.source_name(), "unicode");
    assert_eq!(utf8_location.source_identity(), source.identity());
    assert_eq!(utf8_location.span(), Span::new(0, 0));
    let out_of_bounds_location = parsed.diagnostics()[1]
        .location
        .as_ref()
        .expect("out-of-bounds ranges should retain the source identity");
    assert_eq!(out_of_bounds_location.source_identity(), source.identity());
    assert_eq!(out_of_bounds_location.span(), Span::new(4, 4));
    assert_eq!(
        parsed.diagnostics()[2]
            .location
            .as_ref()
            .expect("invalid interior offsets should retain the source identity")
            .span(),
        Span::new(4, 4)
    );
    assert!(parsed.constructs().is_empty());
}
