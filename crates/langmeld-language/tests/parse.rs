use langmeld_core::{Severity, SourceDatabase, SourceFile, SourceIdentity, Span};
use langmeld_language::{
    CapturedValue, ConstructSpec, LanguageSpec, LexOutput, LexedToken, Lexer, PatternPart,
    TokenKind, parse, parse_with_lexer,
};

fn language() -> LanguageSpec {
    LanguageSpec::new(
        "Example",
        [
            ConstructSpec::new(
                "let",
                [
                    PatternPart::literal("let"),
                    PatternPart::identifier("name"),
                    PatternPart::literal("="),
                    PatternPart::integer("value"),
                    PatternPart::literal(";"),
                ],
            ),
            ConstructSpec::new(
                "print",
                [
                    PatternPart::literal("print"),
                    PatternPart::identifier("name"),
                    PatternPart::literal(";"),
                ],
            ),
        ],
    )
    .unwrap()
}

#[test]
fn parses_multiple_declarations_and_captures_values_with_spans() {
    let source = SourceFile::new("main", "let answer = 42; print answer;");
    let result = parse(&source, &language());

    assert!(result.diagnostics.is_empty());
    assert_eq!(result.constructs.len(), 2);
    assert_eq!(result.constructs[0].name, "let");
    assert_eq!(
        result.constructs[0].fields[0].value,
        CapturedValue::Identifier("answer".into())
    );
    assert_eq!(
        result.constructs[0].fields[1].value,
        CapturedValue::Integer("42".into())
    );
    assert_eq!(
        &source.text[result.constructs[0].span.start..result.constructs[0].span.end],
        "let answer = 42;"
    );
}

#[test]
fn reports_unmatched_tokens_and_keeps_parsing_after_them() {
    let source = SourceFile::new("main", "? print value;");
    let result = parse(&source, &language());

    assert_eq!(result.constructs.len(), 1);
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].severity, Severity::Error);
    assert_eq!(
        result.diagnostics[0].location.as_ref().unwrap().span.start,
        0
    );
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

    assert_eq!(result.diagnostics.len(), 2);
    assert_eq!(result.diagnostics[0].code, "LM-LEX-001");
    assert_eq!(result.diagnostics[1].code, "LM-PARSE-002");
}

#[test]
fn validates_duplicate_declarations_and_fields() {
    let repeated = ConstructSpec::new(
        "let",
        [PatternPart::identifier("x"), PatternPart::identifier("x")],
    );
    assert!(LanguageSpec::new("Example", [repeated]).is_err());
    let first = ConstructSpec::new("same", [PatternPart::literal("first")]);
    let second = ConstructSpec::new("same", [PatternPart::literal("second")]);
    assert!(matches!(
        LanguageSpec::new("Example", [first, second]),
        Err(langmeld_language::SpecError::DuplicateConstruct(_))
    ));
}

#[test]
fn root_construct_allowlist_preserves_nested_constructs() {
    let grammar = LanguageSpec::new(
        "ScopedRoot",
        [
            ConstructSpec::new("program", [PatternPart::literal("program")]).block(
                "{",
                "}",
                &["print"],
            ),
            ConstructSpec::new(
                "print",
                [
                    PatternPart::literal("print"),
                    PatternPart::identifier("name"),
                ],
            ),
        ],
    )
    .unwrap()
    .with_root_constructs(&["program"])
    .unwrap();
    let source = SourceFile::new("main", "print outside program { print inside }");

    let result = parse(&source, &grammar);

    assert_eq!(result.constructs.len(), 1);
    assert_eq!(result.constructs[0].name, "program");
    assert_eq!(result.constructs[0].children[0].name, "print");
    assert_eq!(result.diagnostics[0].code, "LM-PARSE-001");
}

#[test]
fn root_construct_allowlist_rejects_unknown_and_duplicate_names() {
    let grammar = LanguageSpec::new(
        "ScopedRoot",
        [ConstructSpec::new(
            "program",
            [PatternPart::literal("program")],
        )],
    )
    .unwrap();

    assert!(matches!(
        grammar.clone().with_root_constructs(&["missing"]),
        Err(langmeld_language::SpecError::UnknownRootConstruct(name)) if name == "missing"
    ));
    assert!(matches!(
        grammar.with_root_constructs(&["program", "program"]),
        Err(langmeld_language::SpecError::DuplicateRootConstruct(name)) if name == "program"
    ));
}

#[test]
fn rejects_literal_patterns_that_do_not_match_one_lexer_token() {
    let construct = ConstructSpec::new("invalid", [PatternPart::literal("let name")]);

    assert!(matches!(
        LanguageSpec::new("Example", [construct]),
        Err(langmeld_language::SpecError::InvalidLiteral { .. })
    ));
}

struct InvalidKeywordLexer;

impl Lexer for InvalidKeywordLexer {
    fn lex(&self, _source: &SourceFile) -> LexOutput {
        LexOutput {
            tokens: vec![LexedToken {
                text: "while".into(),
                kind: TokenKind::Invalid,
                span: Span::new(0, 5),
            }],
            diagnostics: Vec::new(),
        }
    }
}

#[test]
fn does_not_accept_invalid_tokens_as_literal_syntax() {
    let grammar = LanguageSpec::new(
        "InvalidToken",
        [ConstructSpec::new("while", [PatternPart::literal("while")])],
    )
    .unwrap();
    let source = SourceFile::new("main", "while");

    let result = parse_with_lexer(&source, &grammar, &InvalidKeywordLexer);

    assert!(result.constructs.is_empty());
    assert_eq!(result.diagnostics[0].code, "LM-PARSE-001");
}

#[test]
fn rejects_ambiguous_patterns_instead_of_using_declaration_order() {
    let short = ConstructSpec::new("short", [PatternPart::literal("print")]);
    let long = ConstructSpec::new(
        "long",
        [
            PatternPart::literal("print"),
            PatternPart::identifier("name"),
        ],
    );

    assert!(matches!(
        LanguageSpec::new("Example", [short, long]),
        Err(langmeld_language::SpecError::AmbiguousConstructs(_, _))
    ));
}

#[test]
fn parse_result_and_diagnostics_keep_source_revision() {
    let mut sources = SourceDatabase::default();
    let id = sources.insert("main", "? print value;").unwrap();
    let source = sources.get(id).unwrap();
    let result = parse(source, &language());
    let expected = SourceIdentity { id, revision: 1 };

    assert_eq!(result.source, Some(expected));
    assert_eq!(
        result.diagnostics[0].location.as_ref().unwrap().identity,
        Some(expected)
    );
}

#[test]
fn parses_nested_blocks_and_restricts_children_to_the_declared_scope() {
    let grammar = LanguageSpec::new(
        "Nested",
        [
            ConstructSpec::new("program", [PatternPart::literal("program")]).block(
                "{",
                "}",
                &["when", "print"],
            ),
            ConstructSpec::new(
                "when",
                [
                    PatternPart::literal("when"),
                    PatternPart::identifier("condition"),
                ],
            )
            .block("{", "}", &["print"]),
            ConstructSpec::new(
                "print",
                [
                    PatternPart::literal("print"),
                    PatternPart::identifier("name"),
                    PatternPart::literal(";"),
                ],
            ),
        ],
    )
    .unwrap();
    let source = SourceFile::new("main", "program { when ready { print item; } }");

    let result = parse(&source, &grammar);

    assert!(result.diagnostics.is_empty());
    let root = &result.constructs[0];
    assert_eq!(root.name, "program");
    assert_eq!(root.children[0].name, "when");
    assert_eq!(root.children[0].children[0].name, "print");
    assert_eq!(root.children[0].children[0].fields[0].name, "name");
}

#[test]
fn reports_disallowed_child_constructs_but_recovers_within_the_block() {
    let grammar = LanguageSpec::new(
        "Scoped",
        [
            ConstructSpec::new("program", [PatternPart::literal("program")]).block(
                "{",
                "}",
                &["print"],
            ),
            ConstructSpec::new(
                "let",
                [PatternPart::literal("let"), PatternPart::identifier("name")],
            ),
            ConstructSpec::new(
                "print",
                [
                    PatternPart::literal("print"),
                    PatternPart::identifier("name"),
                    PatternPart::literal(";"),
                ],
            ),
        ],
    )
    .unwrap();
    let source = SourceFile::new("main", "program { let item print value; }");

    let result = parse(&source, &grammar);

    assert!(!result.diagnostics.is_empty());
    assert_eq!(result.constructs[0].children.len(), 1);
    assert_eq!(result.constructs[0].children[0].name, "print");
}

#[test]
fn reports_missing_block_closer_at_end_of_source() {
    let grammar = LanguageSpec::new(
        "Block",
        [ConstructSpec::new("program", [PatternPart::literal("program")]).block("{", "}", &[])],
    )
    .unwrap();
    let source = SourceFile::new("main", "program {");

    let result = parse(&source, &grammar);

    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].code, "LM-PARSE-004");
    assert_eq!(
        result.diagnostics[0].location.as_ref().unwrap().span.start,
        source.text.len()
    );
}

#[test]
fn caps_recursive_block_depth_and_recovers_without_overflowing_the_stack() {
    let grammar = LanguageSpec::new(
        "Recursive",
        [ConstructSpec::new("nest", [PatternPart::literal("nest")]).block("{", "}", &["nest"])],
    )
    .unwrap();
    let depth = langmeld_language::MAX_NESTING_DEPTH + 4;
    let mut text = "nest { ".repeat(depth);
    text.push_str(&"} ".repeat(depth));
    let source = SourceFile::new("nested", text);

    let result = parse(&source, &grammar);

    assert!(result.diagnostics.iter().any(|d| d.code == "LM-PARSE-003"));
    assert_eq!(result.constructs.len(), 1);
}

#[test]
fn block_child_names_must_resolve_to_language_constructs() {
    let grammar = ConstructSpec::new("program", [PatternPart::literal("program")]).block(
        "{",
        "}",
        &["missing"],
    );

    assert!(matches!(
        LanguageSpec::new("Example", [grammar]),
        Err(langmeld_language::SpecError::UnknownChildConstruct { .. })
    ));
}

#[test]
fn parses_function_signatures_with_optional_and_repeated_parameters() {
    use PatternPart as P;

    let function = ConstructSpec::new(
        "function",
        [
            P::literal("fn"),
            P::identifier("name"),
            P::literal("("),
            P::optional([
                P::identifier("first_parameter"),
                P::literal(":"),
                P::identifier("first_type"),
                P::zero_or_more([
                    P::literal(","),
                    P::identifier("parameter"),
                    P::literal(":"),
                    P::identifier("parameter_type"),
                ]),
            ]),
            P::literal(")"),
            P::optional([P::literal("->"), P::identifier("return_type")]),
        ],
    );
    let grammar = LanguageSpec::new("Functions", [function]).unwrap();

    let empty = parse(&SourceFile::new("empty", "fn empty()"), &grammar);
    assert!(empty.diagnostics.is_empty());
    assert_eq!(empty.constructs[0].fields.len(), 1);
    assert_eq!(empty.constructs[0].fields[0].name, "name");

    let multiple = parse(
        &SourceFile::new(
            "multiple",
            "fn combine(left: Number, right: Text) -> Result",
        ),
        &grammar,
    );
    assert!(
        multiple.diagnostics.is_empty(),
        "{:?}",
        multiple.diagnostics
    );
    assert_eq!(multiple.constructs[0].fields.len(), 6);
    assert_eq!(multiple.constructs[0].fields[3].name, "parameter");
    assert_eq!(multiple.constructs[0].fields[5].name, "return_type");
}

#[test]
fn parses_alternative_construct_syntax_and_reports_its_variant() {
    let grammar = LanguageSpec::new(
        "Functions",
        [ConstructSpec::new(
            "function",
            [PatternPart::literal("fn"), PatternPart::identifier("name")],
        )
        .variant([
            PatternPart::literal("function"),
            PatternPart::identifier("name"),
            PatternPart::literal("("),
            PatternPart::literal(")"),
        ])],
    )
    .unwrap();

    let parsed = parse(&SourceFile::new("source", "function render()"), &grammar);

    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(parsed.constructs[0].name, "function");
    assert_eq!(parsed.constructs[0].variant, 1);
}

#[test]
fn rejects_overlapping_variants_of_one_construct() {
    let construct = ConstructSpec::new(
        "function",
        [PatternPart::literal("fn"), PatternPart::identifier("name")],
    )
    .variant([
        PatternPart::literal("fn"),
        PatternPart::identifier("function_name"),
    ]);

    assert!(matches!(
        LanguageSpec::new("Functions", [construct]),
        Err(langmeld_language::SpecError::AmbiguousVariants(_))
    ));
}

#[test]
fn custom_lexer_can_supply_host_specific_string_tokens() {
    struct RawStringLexer;

    impl Lexer for RawStringLexer {
        fn lex(&self, source: &SourceFile) -> LexOutput {
            let string_start = source.text.find("r#\"").unwrap();
            LexOutput {
                tokens: vec![
                    LexedToken {
                        text: "emit".into(),
                        kind: TokenKind::Identifier,
                        span: Span::new(0, 4),
                    },
                    LexedToken {
                        text: source.text[string_start..].into(),
                        kind: TokenKind::String,
                        span: Span::new(string_start, source.text.len()),
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

    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(
        parsed.constructs[0].fields[0].value,
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

    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(
        parsed.constructs[0].fields[0].value,
        CapturedValue::Token {
            text: "@tag".into(),
            kind: TokenKind::Punctuation,
        }
    );
}

#[test]
fn generic_token_patterns_are_checked_for_ambiguity() {
    let wildcard = ConstructSpec::new("wildcard", [PatternPart::token("value")]);
    let literal = ConstructSpec::new("keyword", [PatternPart::literal("while")]);

    assert!(matches!(
        LanguageSpec::new("Ambiguous", [wildcard, literal]),
        Err(langmeld_language::SpecError::AmbiguousConstructs(_, _))
    ));
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

    assert_eq!(parsed.diagnostics[0].code, "LM-LEX-002");
    assert!(parsed.constructs.is_empty());
}

#[test]
fn rejects_custom_token_ranges_inside_utf8_codepoints_without_panicking() {
    struct InvalidBoundaryLexer;

    impl Lexer for InvalidBoundaryLexer {
        fn lex(&self, _source: &SourceFile) -> LexOutput {
            LexOutput {
                tokens: vec![LexedToken {
                    text: "x".into(),
                    kind: TokenKind::Identifier,
                    span: Span::new(1, 2),
                }],
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

    let parsed = parse_with_lexer(&source, &grammar, &InvalidBoundaryLexer);

    assert_eq!(parsed.diagnostics[0].code, "LM-LEX-002");
    assert!(parsed.constructs.is_empty());
}

#[test]
fn rejects_optional_or_repeated_groups_with_ambiguous_boundaries() {
    let construct = ConstructSpec::new(
        "ambiguous",
        [
            PatternPart::literal("item"),
            PatternPart::optional([PatternPart::literal("name")]),
            PatternPart::identifier("next"),
        ],
    );

    assert!(matches!(
        LanguageSpec::new("Ambiguous", [construct]),
        Err(langmeld_language::SpecError::AmbiguousPatternBoundary(_))
    ));
}

#[test]
fn rejects_optional_groups_that_can_match_without_tokens() {
    let construct = ConstructSpec::new(
        "empty_optional",
        [
            PatternPart::literal("item"),
            PatternPart::optional([PatternPart::optional([PatternPart::literal("name")])]),
        ],
    );

    assert!(matches!(
        LanguageSpec::new("Invalid", [construct]),
        Err(langmeld_language::SpecError::EmptyOptional(_))
    ));
}

#[test]
fn reports_ambiguity_between_complete_patterns_at_parse_time() {
    let optional = ConstructSpec::new(
        "optional",
        [
            PatternPart::literal("item"),
            PatternPart::optional([PatternPart::literal("name")]),
        ],
    );
    let explicit = ConstructSpec::new(
        "explicit",
        [PatternPart::literal("item"), PatternPart::literal("name")],
    );
    let grammar = LanguageSpec::new("Ambiguous", [optional, explicit]).unwrap();

    let result = parse(&SourceFile::new("input", "item name"), &grammar);

    assert!(result.constructs.is_empty());
    assert_eq!(result.diagnostics[0].code, "LM-PARSE-006");
}

#[test]
fn one_or_more_requires_an_item_and_captures_each_match() {
    let grammar = LanguageSpec::new(
        "Pair",
        [ConstructSpec::new(
            "pair",
            [
                PatternPart::literal("pair"),
                PatternPart::integer("first"),
                PatternPart::one_or_more([PatternPart::literal(","), PatternPart::integer("next")]),
                PatternPart::literal(";"),
            ],
        )],
    )
    .unwrap();

    let valid = parse(&SourceFile::new("valid", "pair 1,2,3;"), &grammar);
    assert!(valid.diagnostics.is_empty());
    assert_eq!(valid.constructs[0].fields.len(), 3);

    let invalid = parse(&SourceFile::new("invalid", "pair 1;"), &grammar);
    assert_eq!(invalid.diagnostics[0].code, "LM-PARSE-002");
}
