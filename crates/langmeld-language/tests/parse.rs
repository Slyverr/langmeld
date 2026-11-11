use langmeld_core::{Severity, SourceDatabase, SourceFile, Span};
use langmeld_language::{
    CapturedValue, ConstructSpec, LanguageSpec, LexOutput, LexedToken, Lexer, LexicalRules,
    ParseLimits, PatternPart, TokenKind, parse, parse_with_lexer, parse_with_lexer_and_limits,
    parse_with_limits,
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

    assert!(result.is_valid());
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
fn resumes_after_a_missing_terminator_before_a_valid_construct() {
    let statements = language();
    let constructs = std::iter::once(
        ConstructSpec::new("program", [PatternPart::literal("program")]).block(
            "{",
            "}",
            &["let", "print"],
        ),
    )
    .chain(statements.constructs().iter().cloned());
    let grammar = LanguageSpec::new("Statements", constructs)
        .unwrap()
        .with_root_constructs(&["program"])
        .unwrap();
    let source = SourceFile::new(
        "main",
        "program { let first = 1 print second; let third = 3; }",
    );

    let result = parse(&source, &grammar);

    assert!(!result.is_valid());
    assert_eq!(
        result.constructs[0]
            .children
            .iter()
            .map(|construct| construct.name.as_str())
            .collect::<Vec<_>>(),
        ["print", "let"]
    );
    let missing_terminator = result
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "LM-PARSE-002")
        .expect("missing semicolon diagnostic");
    let next_construct = source.text.find("print").unwrap();
    assert_eq!(
        missing_terminator.location.as_ref().unwrap().span,
        Span::new(next_construct, next_construct + "print".len())
    );
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
}

#[test]
fn captures_expression_span_and_consumes_its_top_level_terminator() {
    let grammar = LanguageSpec::new(
        "Expressions",
        [ConstructSpec::new(
            "print",
            [
                PatternPart::literal("print"),
                PatternPart::expression("value", ";"),
            ],
        )],
    )
    .unwrap();
    let source = SourceFile::new("main", "print call(a + nested(b)) + { let x = 3; x };");

    let result = parse(&source, &grammar);

    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let construct = &result.constructs[0];
    let field = &construct.fields[0];
    assert_eq!(field.value, CapturedValue::Expression);
    assert_eq!(
        &source.text[field.span.start..field.span.end],
        "call(a + nested(b)) + { let x = 3; x }"
    );
    assert_eq!(
        &source.text[construct.span.start..construct.span.end],
        "print call(a + nested(b)) + { let x = 3; x };"
    );
}

#[test]
fn expression_capture_rejects_empty_or_unterminated_expressions() {
    let grammar = LanguageSpec::new(
        "Expressions",
        [ConstructSpec::new(
            "print",
            [
                PatternPart::literal("print"),
                PatternPart::expression("value", ";"),
            ],
        )],
    )
    .unwrap();

    for input in ["print ;", "print call(1)"] {
        let result = parse(&SourceFile::new("main", input), &grammar);
        assert!(result.constructs.is_empty(), "accepted {input:?}");
        assert!(
            !result.diagnostics.is_empty(),
            "no diagnostic for {input:?}"
        );
    }
}

#[test]
fn rejects_expression_terminators_that_are_not_single_tokens() {
    let construct = ConstructSpec::new("print", [PatternPart::expression("value", "; ;")]);

    assert!(matches!(
        LanguageSpec::new("Expressions", [construct]),
        Err(langmeld_language::SpecError::InvalidExpressionTerminator { .. })
    ));
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

    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(parsed.constructs.len(), 2);
    assert_eq!(
        parsed.constructs[0].fields[0].value,
        CapturedValue::String("\"/* string */\"".into())
    );
    assert_eq!(
        parsed.constructs[1].fields[0].value,
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
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "LM-LEX-003")
        .expect("unterminated comment diagnostic");
    assert_eq!(
        diagnostic.location.as_ref().unwrap().span,
        Span::new(6, source.text.len())
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

    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(parsed.constructs.len(), 1);
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
fn parses_multiple_explicit_roots_with_and_without_blocks() {
    let grammar = LanguageSpec::new(
        "MixedRoots",
        [
            ConstructSpec::new(
                "visual",
                [
                    PatternPart::literal("visual"),
                    PatternPart::identifier("name"),
                ],
            )
            .block("{", "}", &["node"]),
            ConstructSpec::new(
                "node",
                [
                    PatternPart::literal("node"),
                    PatternPart::identifier("name"),
                ],
            ),
            ConstructSpec::new(
                "scenario",
                [
                    PatternPart::literal("scenario"),
                    PatternPart::identifier("name"),
                ],
            ),
        ],
    )
    .unwrap()
    .with_root_constructs(&["visual", "scenario"])
    .unwrap();
    let source = SourceFile::new("main", "visual board { node title } scenario startup");

    let result = parse(&source, &grammar);

    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        result
            .constructs
            .iter()
            .map(|construct| construct.name.as_str())
            .collect::<Vec<_>>(),
        ["visual", "scenario"]
    );
    assert_eq!(result.constructs[0].children[0].name, "node");
    assert_eq!(result.constructs[1].children.len(), 0);
    assert_eq!(
        &source.text[result.constructs[1].span.start..result.constructs[1].span.end],
        "scenario startup"
    );
}

#[test]
fn recovers_at_a_later_root_through_unclosed_nested_blocks() {
    let grammar = LanguageSpec::new(
        "RootRecovery",
        [
            ConstructSpec::new(
                "visual",
                [
                    PatternPart::literal("visual"),
                    PatternPart::identifier("name"),
                ],
            )
            .block("{", "}", &["group"]),
            ConstructSpec::new(
                "group",
                [
                    PatternPart::literal("group"),
                    PatternPart::identifier("name"),
                ],
            )
            .block("{", "}", &["node"]),
            ConstructSpec::new(
                "node",
                [
                    PatternPart::literal("node"),
                    PatternPart::identifier("name"),
                ],
            ),
            ConstructSpec::new(
                "scenario",
                [
                    PatternPart::literal("scenario"),
                    PatternPart::identifier("name"),
                ],
            ),
        ],
    )
    .unwrap()
    .with_root_constructs(&["visual", "scenario"])
    .unwrap();
    let source = SourceFile::new(
        "main",
        "visual board { group panel { node title scenario setup",
    );

    let result = parse(&source, &grammar);

    assert_eq!(
        result
            .constructs
            .iter()
            .map(|construct| construct.name.as_str())
            .collect::<Vec<_>>(),
        ["visual", "scenario"]
    );
    assert_eq!(result.constructs[0].children[0].name, "group");
    assert_eq!(result.constructs[0].children[0].children[0].name, "node");
    let missing_closers = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "LM-PARSE-004")
        .collect::<Vec<_>>();
    let scenario_start = source.text.find("scenario").unwrap();
    assert!(result.constructs[0].span.end <= scenario_start);
    assert!(result.constructs[1].span.start >= scenario_start);
    assert_eq!(missing_closers.len(), 2, "{:?}", result.diagnostics);
    assert!(missing_closers.iter().all(|diagnostic| {
        diagnostic.location.as_ref().unwrap().span == Span::new(scenario_start, scenario_start)
    }));
    assert_eq!(result.diagnostics.len(), 2, "{:?}", result.diagnostics);
}

#[test]
fn root_recovery_keeps_a_root_construct_allowed_as_a_block_child() {
    let grammar = LanguageSpec::new(
        "RootChildRecovery",
        [
            ConstructSpec::new(
                "visual",
                [
                    PatternPart::literal("visual"),
                    PatternPart::identifier("name"),
                ],
            )
            .block("{", "}", &["scenario"]),
            ConstructSpec::new(
                "scenario",
                [
                    PatternPart::literal("scenario"),
                    PatternPart::identifier("name"),
                ],
            ),
        ],
    )
    .unwrap()
    .with_root_constructs(&["visual", "scenario"])
    .unwrap();
    let source = SourceFile::new("main", "visual board { scenario nested");

    let result = parse(&source, &grammar);

    assert_eq!(result.constructs.len(), 1);
    assert_eq!(result.constructs[0].children.len(), 1);
    assert_eq!(result.constructs[0].children[0].name, "scenario");
    assert_eq!(result.diagnostics[0].code, "LM-PARSE-004");
}

#[test]
fn resumes_at_a_root_after_a_malformed_block_child() {
    let grammar = LanguageSpec::new(
        "RootRecovery",
        [
            ConstructSpec::new(
                "visual",
                [
                    PatternPart::literal("visual"),
                    PatternPart::identifier("name"),
                ],
            )
            .block("{", "}", &["node"]),
            ConstructSpec::new(
                "node",
                [
                    PatternPart::literal("node"),
                    PatternPart::identifier("name"),
                    PatternPart::literal(";"),
                ],
            ),
            ConstructSpec::new(
                "scenario",
                [
                    PatternPart::literal("scenario"),
                    PatternPart::identifier("name"),
                ],
            ),
        ],
    )
    .unwrap()
    .with_root_constructs(&["visual", "scenario"])
    .unwrap();
    let source = SourceFile::new("main", "visual board { node broken scenario setup");

    let result = parse(&source, &grammar);

    assert_eq!(
        result
            .constructs
            .iter()
            .map(|construct| construct.name.as_str())
            .collect::<Vec<_>>(),
        ["visual", "scenario"]
    );
    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "LM-PARSE-002" && diagnostic.message.contains("construct `node`")
    }));
    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "LM-PARSE-004" && diagnostic.message.contains("closing token `}`")
    }));
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
    let expected = source.identity();

    assert_eq!(result.source, expected);
    assert_eq!(
        result.diagnostics[0].location.as_ref().unwrap().identity,
        expected
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

    let result = parse_with_limits(
        &source,
        &grammar,
        ParseLimits {
            max_nesting_depth: usize::MAX,
            ..ParseLimits::default()
        },
    );

    assert!(result.diagnostics.iter().any(|d| d.code == "LM-PARSE-003"));
    assert!(result.resource_limited);
    assert_eq!(result.constructs.len(), 1);
}

#[test]
fn rejects_oversized_source_before_calling_a_host_lexer() {
    struct UnexpectedLexer;

    impl Lexer for UnexpectedLexer {
        fn lex(&self, _: &SourceFile) -> LexOutput {
            panic!("source-limit rejection must happen before lexing");
        }
    }

    let source = SourceFile::new("large", "é");
    let result = parse_with_lexer_and_limits(
        &source,
        &language(),
        &UnexpectedLexer,
        ParseLimits {
            max_source_bytes: 1,
            ..ParseLimits::default()
        },
    );

    assert!(!result.is_valid());
    assert!(result.resource_limited);
    assert!(result.constructs.is_empty());
    assert_eq!(result.diagnostics[0].code, "LM-PARSE-007");
    assert_eq!(
        result.diagnostics[0].location.as_ref().unwrap().span,
        Span::new(0, 0)
    );
}

#[test]
fn token_limit_keeps_the_completed_default_lexer_prefix() {
    let source = SourceFile::new("tokens", "print first; print second;");
    let result = parse_with_limits(
        &source,
        &language(),
        ParseLimits {
            max_tokens: 3,
            ..ParseLimits::default()
        },
    );

    assert!(result.resource_limited);
    assert_eq!(result.constructs.len(), 1);
    assert_eq!(result.constructs[0].name, "print");
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code == "LM-PARSE-008" })
    );
}

#[test]
fn custom_lexer_token_overflow_is_rejected_before_parsing() {
    struct ManyTokens;

    impl Lexer for ManyTokens {
        fn lex(&self, _source: &SourceFile) -> LexOutput {
            LexOutput {
                tokens: ["print", "value", ";"]
                    .into_iter()
                    .scan(0, |start, text| {
                        let token = LexedToken {
                            text: text.to_owned(),
                            kind: if text == ";" {
                                TokenKind::Punctuation
                            } else {
                                TokenKind::Identifier
                            },
                            span: Span::new(*start, *start + text.len()),
                        };
                        *start += text.len() + 1;
                        Some(token)
                    })
                    .collect(),
                diagnostics: Vec::new(),
            }
        }
    }

    let source = SourceFile::new("tokens", "print value ;");
    let result = parse_with_lexer_and_limits(
        &source,
        &language(),
        &ManyTokens,
        ParseLimits {
            max_tokens: 2,
            ..ParseLimits::default()
        },
    );

    assert!(result.resource_limited);
    assert!(result.constructs.is_empty());
    assert_eq!(result.diagnostics[0].code, "LM-PARSE-008");
}

#[test]
fn construct_limit_keeps_completed_nodes_and_stops_at_the_next_match() {
    let source = SourceFile::new("nodes", "print first; print second;");
    let result = parse_with_limits(
        &source,
        &language(),
        ParseLimits {
            max_constructs: 1,
            ..ParseLimits::default()
        },
    );

    assert!(result.resource_limited);
    assert_eq!(result.constructs.len(), 1);
    assert_eq!(result.constructs[0].name, "print");
    assert_eq!(result.diagnostics.last().unwrap().code, "LM-PARSE-009");
}

#[test]
fn pattern_step_limit_stops_candidate_matching_with_a_diagnostic() {
    let source = SourceFile::new("work", "let answer = 42;");
    let result = parse_with_limits(
        &source,
        &language(),
        ParseLimits {
            max_pattern_steps: 2,
            ..ParseLimits::default()
        },
    );

    assert!(result.resource_limited);
    assert!(result.constructs.is_empty());
    assert_eq!(result.diagnostics[0].code, "LM-PARSE-010");
}

#[test]
fn configurable_nesting_limit_is_capped_and_reported_as_resource_limited() {
    let grammar = LanguageSpec::new(
        "Recursive",
        [ConstructSpec::new("nest", [PatternPart::literal("nest")]).block("{", "}", &["nest"])],
    )
    .unwrap();
    let source = SourceFile::new("nested", "nest { nest { } }");
    let result = parse_with_limits(
        &source,
        &grammar,
        ParseLimits {
            max_nesting_depth: 1,
            ..ParseLimits::default()
        },
    );

    assert!(result.resource_limited);
    assert_eq!(result.diagnostics[0].code, "LM-PARSE-003");
    assert_eq!(result.constructs[0].children.len(), 1);
    assert!(result.constructs[0].children[0].children.is_empty());
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
fn category_members_expand_into_block_children_in_construct_order() {
    let grammar =
        LanguageSpec::new(
            "Grouped",
            [
                ConstructSpec::new("program", [PatternPart::literal("program")])
                    .block_with_categories("{", "}", &["standalone"], &["statement"]),
                ConstructSpec::new("let", [PatternPart::literal("let")]).implements(&["statement"]),
                ConstructSpec::new("standalone", [PatternPart::literal("standalone")]),
                ConstructSpec::new("print", [PatternPart::literal("print")])
                    .implements(&["statement"]),
            ],
        )
        .unwrap();

    let block = grammar.constructs()[0].block_spec().unwrap();
    assert_eq!(block.categories(), ["statement"]);
    assert_eq!(block.constructs(), ["standalone", "let", "print"]);
    assert_eq!(grammar.constructs()[1].categories(), ["statement"]);

    let parsed = parse(
        &SourceFile::new("main", "program { standalone let print }"),
        &grammar,
    );
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(
        parsed.constructs[0]
            .children
            .iter()
            .map(|child| child.name.as_str())
            .collect::<Vec<_>>(),
        ["standalone", "let", "print"]
    );
}

#[test]
fn rejects_block_categories_without_members() {
    let grammar = ConstructSpec::new("program", [PatternPart::literal("program")])
        .block_with_categories("{", "}", &[], &["statement"]);

    assert!(matches!(
        LanguageSpec::new("Grouped", [grammar]),
        Err(langmeld_language::SpecError::UnknownBlockCategory {
            category,
            ..
        }) if category == "statement"
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
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(
        parsed.constructs[0].fields[0].value,
        CapturedValue::Token {
            text: "left".into(),
            kind: TokenKind::Identifier,
        }
    );

    let rejected = parse(&SourceFile::new("program", "move up;"), &grammar);
    let diagnostic = rejected
        .diagnostics
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
