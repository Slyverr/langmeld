use super::*;

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

    assert!(
        result
            .diagnostics()
            .iter()
            .any(|d| d.code == "LM-PARSE-003")
    );
    assert!(result.resource_limited());
    assert_eq!(result.constructs().len(), 1);
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
    assert!(result.resource_limited());
    assert!(result.constructs().is_empty());
    assert_eq!(result.diagnostics()[0].code, "LM-PARSE-007");
    assert_eq!(
        result.diagnostics()[0].location.as_ref().unwrap().span(),
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

    assert!(result.resource_limited());
    assert_eq!(result.constructs().len(), 1);
    assert_eq!(result.constructs()[0].name, "print");
    assert!(
        result
            .diagnostics()
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

    assert!(result.resource_limited());
    assert!(result.constructs().is_empty());
    assert_eq!(result.diagnostics()[0].code, "LM-PARSE-008");
}

#[test]
fn custom_diagnostics_cannot_hide_a_token_limit_error() {
    struct InvalidLimitDiagnostic;

    impl Lexer for InvalidLimitDiagnostic {
        fn lex(&self, source: &SourceFile) -> LexOutput {
            let foreign_source = SourceFile::new(source.name(), source.text());
            LexOutput {
                tokens: vec![
                    LexedToken {
                        text: "print".into(),
                        kind: TokenKind::Identifier,
                        span: Span::new(0, 5),
                    },
                    LexedToken {
                        text: "value".into(),
                        kind: TokenKind::Identifier,
                        span: Span::new(6, 11),
                    },
                ],
                diagnostics: vec![
                    Diagnostic::error(
                        "LM-PARSE-008",
                        "foreign token limit finding",
                        &foreign_source,
                        Span::new(0, 5),
                    ),
                    Diagnostic::error(
                        "LM-PARSE-008",
                        "host reused a framework diagnostic code",
                        source,
                        Span::new(0, 5),
                    ),
                ],
            }
        }
    }

    let source = SourceFile::new("tokens", "print value");
    let result = parse_with_lexer_and_limits(
        &source,
        &language(),
        &InvalidLimitDiagnostic,
        ParseLimits {
            max_tokens: 1,
            ..ParseLimits::default()
        },
    );

    assert!(result.resource_limited());
    assert!(
        result
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code == "LM-LEX-004")
    );
    let token_limit = result
        .diagnostics()
        .iter()
        .find(|diagnostic| {
            diagnostic.code == "LM-PARSE-008"
                && diagnostic.message == "token count exceeds the configured limit of 1"
        })
        .expect("token overflow must report its own source-bound diagnostic");
    assert_eq!(
        token_limit
            .location
            .as_ref()
            .expect("token limit diagnostics are source located")
            .source_identity(),
        source.identity()
    );
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

    assert!(result.resource_limited());
    assert_eq!(result.constructs().len(), 1);
    assert_eq!(result.constructs()[0].name, "print");
    assert_eq!(result.diagnostics().last().unwrap().code, "LM-PARSE-009");
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

    assert!(result.resource_limited());
    assert!(result.constructs().is_empty());
    assert_eq!(result.diagnostics()[0].code, "LM-PARSE-010");
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

    assert!(result.resource_limited());
    assert_eq!(result.diagnostics()[0].code, "LM-PARSE-003");
    assert_eq!(result.constructs()[0].children.len(), 1);
    assert!(result.constructs()[0].children[0].children.is_empty());
}
