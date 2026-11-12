use super::*;

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

    assert!(result.constructs().is_empty());
    assert_eq!(result.diagnostics()[0].code, "LM-PARSE-001");
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
    assert!(empty.diagnostics().is_empty());
    assert_eq!(empty.constructs()[0].fields.len(), 1);
    assert_eq!(empty.constructs()[0].fields[0].name, "name");

    let multiple = parse(
        &SourceFile::new(
            "multiple",
            "fn combine(left: Number, right: Text) -> Result",
        ),
        &grammar,
    );
    assert!(
        multiple.diagnostics().is_empty(),
        "{:?}",
        multiple.diagnostics()
    );
    assert_eq!(multiple.constructs()[0].fields.len(), 6);
    assert_eq!(multiple.constructs()[0].fields[3].name, "parameter");
    assert_eq!(multiple.constructs()[0].fields[5].name, "return_type");
}

#[test]
fn capture_groups_preserve_optional_and_repeated_field_boundaries() {
    use PatternPart as P;

    let grammar = LanguageSpec::new(
        "Functions",
        [ConstructSpec::new(
            "function",
            [
                P::literal("fn"),
                P::identifier("name"),
                P::literal("("),
                P::optional([
                    P::identifier("first_name"),
                    P::literal(":"),
                    P::identifier("first_type"),
                    P::zero_or_more([
                        P::literal(","),
                        P::identifier("additional_name"),
                        P::literal(":"),
                        P::identifier("additional_type"),
                    ]),
                ]),
                P::literal(")"),
            ],
        )],
    )
    .unwrap();
    let source = SourceFile::new(
        "function",
        "fn combine(left: Number, right: Text, final: Bool)",
    );
    let parsed = parse(&source, &grammar);
    let construct = &parsed.constructs()[0];

    assert!(parsed.diagnostics().is_empty());
    assert_eq!(construct.capture_groups.len(), 1);

    let parameters = &construct.capture_groups[0];
    assert_eq!(parameters.kind, CaptureGroupKind::Optional);
    assert_eq!(parameters.occurrences.len(), 1);

    let parameter_list = &parameters.occurrences[0];
    assert_eq!(
        parameter_list
            .captures(construct)
            .unwrap()
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        [
            "first_name",
            "first_type",
            "additional_name",
            "additional_type",
            "additional_name",
            "additional_type",
        ]
    );
    assert_eq!(parameter_list.groups.len(), 1);
    assert_eq!(
        parameter_list.span.start,
        source.text().find("left").unwrap()
    );
    assert_eq!(parameter_list.span.end, source.text().len() - 1);

    let additional_parameters = &parameter_list.groups[0];
    assert_eq!(additional_parameters.kind, CaptureGroupKind::Repeat);
    assert_eq!(additional_parameters.occurrences.len(), 2);
    assert_eq!(
        additional_parameters.occurrences[0]
            .captures(construct)
            .unwrap()
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        ["additional_name", "additional_type"]
    );
    assert_eq!(
        additional_parameters.occurrences[1]
            .captures(construct)
            .unwrap()
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        ["additional_name", "additional_type"]
    );

    let empty = parse(&SourceFile::new("empty", "fn empty()"), &grammar);
    assert_eq!(empty.constructs()[0].capture_groups.len(), 1);
    assert!(
        empty.constructs()[0].capture_groups[0]
            .occurrences
            .is_empty()
    );
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

    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert_eq!(parsed.constructs()[0].name, "function");
    assert_eq!(parsed.constructs()[0].variant, 1);
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
fn generic_token_patterns_are_checked_for_ambiguity() {
    let wildcard = ConstructSpec::new("wildcard", [PatternPart::token("value")]);
    let literal = ConstructSpec::new("keyword", [PatternPart::literal("while")]);

    assert!(matches!(
        LanguageSpec::new("Ambiguous", [wildcard, literal]),
        Err(langmeld_language::SpecError::AmbiguousConstructs(_, _))
    ));
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

    assert!(result.constructs().is_empty());
    assert_eq!(result.diagnostics()[0].code, "LM-PARSE-006");
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
    assert!(valid.diagnostics().is_empty());
    assert_eq!(valid.constructs()[0].fields.len(), 3);

    let invalid = parse(&SourceFile::new("invalid", "pair 1;"), &grammar);
    assert_eq!(invalid.diagnostics()[0].code, "LM-PARSE-002");
}
