use langmeld_core::{SourceFile, Span};
use langmeld_language::{
    CompletionKind, ConstructSpec, DefaultLexer, LanguageSpec, LexOutput, Lexer, LexicalRules,
    ParseLimits, PatternPart, TokenKind, complete_at, complete_at_with_lexer,
    language_definition_grammar,
};

fn language() -> LanguageSpec {
    LanguageSpec::new(
        "EditorExample",
        [
            ConstructSpec::new("program", [PatternPart::literal("program")]).block(
                "{",
                "}",
                &["when", "repeat", "print", "say"],
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
                "repeat",
                [
                    PatternPart::literal("repeat"),
                    PatternPart::identifier("item"),
                    PatternPart::literal(";"),
                ],
            ),
            ConstructSpec::new(
                "print",
                [
                    PatternPart::literal("print"),
                    PatternPart::identifier("item"),
                    PatternPart::literal(";"),
                ],
            ),
            ConstructSpec::new(
                "say",
                [
                    PatternPart::literal("say"),
                    PatternPart::string("text"),
                    PatternPart::literal(";"),
                ],
            ),
            ConstructSpec::new("function", [PatternPart::literal("fn")]),
        ],
    )
    .expect("valid example grammar")
}

fn suggestions(source: &str, offset: usize) -> Vec<String> {
    complete_at(
        &SourceFile::new("main.langmeld", source),
        &language(),
        offset,
    )
    .into_iter()
    .map(|candidate| candidate.text)
    .collect()
}

#[test]
fn suggests_only_constructs_allowed_in_the_current_nested_block() {
    let source = "program { when ready { ";

    assert_eq!(suggestions(source, source.len()), ["print", "}"]);
}

#[test]
fn category_members_are_suggested_only_in_blocks_that_allow_the_category() {
    let language =
        LanguageSpec::new(
            "Categorized",
            [
                ConstructSpec::new("program", [PatternPart::literal("program")])
                    .block_with_categories("{", "}", &[], &["statement"]),
                ConstructSpec::new("print", [PatternPart::literal("print")])
                    .implements(&["statement"]),
                ConstructSpec::new("function", [PatternPart::literal("fn")]),
            ],
        )
        .unwrap();
    let source = "program { ";

    let candidates = complete_at(&SourceFile::new("main", source), &language, source.len());

    assert_eq!(
        candidates
            .iter()
            .map(|candidate| candidate.text.as_str())
            .collect::<Vec<_>>(),
        ["print", "}"]
    );
}

#[test]
fn shared_start_tokens_drop_conflicting_completion_kinds() {
    let language = LanguageSpec::new(
        "SharedStarts",
        [
            ConstructSpec::new("program", [PatternPart::literal("program")]).block(
                "{",
                "}",
                &["assign", "configure"],
            ),
            ConstructSpec::new(
                "assign",
                [PatternPart::literal("set"), PatternPart::identifier("name")],
            )
            .with_completion_kind(CompletionKind::Keyword),
            ConstructSpec::new(
                "configure",
                [PatternPart::literal("set"), PatternPart::string("value")],
            )
            .with_completion_kind(CompletionKind::Function),
        ],
    )
    .unwrap();
    let source = "program { ";

    let candidates = complete_at(&SourceFile::new("main", source), &language, source.len());

    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0].text, "set");
    assert_eq!(candidates[0].kind, None);
    assert_eq!(candidates[1].text, "}");
}

#[test]
fn completion_ignores_configured_comments_before_the_cursor() {
    let language = LanguageSpec::new(
        "Comments",
        [
            ConstructSpec::new("program", [PatternPart::literal("program")]).block(
                "{",
                "}",
                &["print"],
            ),
            ConstructSpec::new("print", [PatternPart::literal("print")]),
        ],
    )
    .unwrap()
    .with_lexical_rules(LexicalRules::default().line_comment("//"))
    .unwrap();
    let source = "program { // ignore print here\npri";
    let candidates = complete_at(&SourceFile::new("main", source), &language, source.len());

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].text, "print");
}

#[test]
fn suggests_children_immediately_after_an_opening_delimiter() {
    let source = "program {";

    assert_eq!(
        suggestions(source, source.len()),
        ["when", "repeat", "print", "say", "}"]
    );
}

#[test]
fn filters_by_partial_token_and_replaces_the_whole_token() {
    let source = "program { pri";
    let candidates = complete_at(
        &SourceFile::new("main.langmeld", source),
        &language(),
        source.len(),
    );

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].text, "print");
    assert_eq!(candidates[0].replace, Span::new(10, 13));
}

#[test]
fn finite_choice_completion_suggests_only_allowed_values_in_field_context() {
    let language = LanguageSpec::new(
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
    let source = "move r";

    let candidates = complete_at(&SourceFile::new("main", source), &language, source.len());

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].text, "right");
    assert_eq!(candidates[0].kind, Some(CompletionKind::EnumMember));
    assert_eq!(candidates[0].replace, Span::new(5, 6));
}

#[test]
fn finite_choice_completion_kind_can_be_overridden() {
    let language = LanguageSpec::new(
        "Movement",
        [ConstructSpec::new(
            "move",
            [
                PatternPart::literal("move"),
                PatternPart::choice_with_completion_kind(
                    "direction",
                    ["left", "right"],
                    CompletionKind::Keyword,
                ),
                PatternPart::literal(";"),
            ],
        )],
    )
    .unwrap();
    let source = "move r";

    let candidates = complete_at(&SourceFile::new("main", source), &language, source.len());

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].text, "right");
    assert_eq!(candidates[0].kind, Some(CompletionKind::Keyword));
}

#[test]
fn does_not_offer_sibling_constructs_inside_an_incomplete_construct() {
    let source = "program { repeat ";

    assert!(suggestions(source, source.len()).is_empty());
}

#[test]
fn does_not_offer_siblings_after_a_mismatched_token_in_an_incomplete_construct() {
    let source = "program { repeat ? ";

    assert!(suggestions(source, source.len()).is_empty());
}

#[test]
fn resumes_scoped_suggestions_after_parser_recovers_to_a_valid_construct() {
    let source = "program { repeat ? print item; ";

    assert_eq!(
        suggestions(source, source.len()),
        ["when", "repeat", "print", "say", "}"]
    );
}

#[test]
fn does_not_offer_siblings_while_an_unterminated_string_is_being_typed() {
    let source = "program { say \"unfinished";

    assert!(suggestions(source, source.len()).is_empty());
}

#[test]
fn offers_root_constructs_when_the_source_is_empty() {
    assert_eq!(
        suggestions("", 0),
        ["program", "when", "repeat", "print", "say", "fn"]
    );
}

#[test]
fn definition_grammar_completions_follow_root_and_nested_scopes() {
    let grammar = language_definition_grammar();
    let root = complete_at(&SourceFile::new("language.langmeld", ""), grammar, 0);
    assert_eq!(
        root.iter()
            .map(|candidate| candidate.text.as_str())
            .collect::<Vec<_>>(),
        ["language", "import", "construct"]
    );

    let source = "language \"Toy\" { ";
    let language_body = complete_at(
        &SourceFile::new("language.langmeld", source),
        grammar,
        source.len(),
    );
    assert_eq!(
        language_body
            .iter()
            .map(|candidate| candidate.text.as_str())
            .collect::<Vec<_>>(),
        ["root", "construct", "import", "lexical", "}"]
    );

    let source = "language \"Toy\" { construct \"item\" { syntax { ";
    let nested = complete_at(
        &SourceFile::new("language.langmeld", source),
        grammar,
        source.len(),
    );
    assert_eq!(
        nested
            .iter()
            .map(|candidate| candidate.text.as_str())
            .collect::<Vec<_>>(),
        [
            "literal",
            "token",
            "identifier",
            "integer",
            "string",
            "choice",
            "expression",
            "optional",
            "zero_or_more",
            "one_or_more",
            "}"
        ]
    );

    let source = "language \"Toy\" { construct \"item\" { syntax { choice \"mode\" { ";
    let values = complete_at(
        &SourceFile::new("language.langmeld", source),
        grammar,
        source.len(),
    );
    assert_eq!(
        values
            .iter()
            .map(|candidate| candidate.text.as_str())
            .collect::<Vec<_>>(),
        ["completion", "value", "}"]
    );

    let source = "language \"Toy\" { construct \"item\" { completion key";
    let kinds = complete_at(
        &SourceFile::new("language.langmeld", source),
        grammar,
        source.len(),
    );
    assert_eq!(
        kinds
            .iter()
            .map(|candidate| candidate.text.as_str())
            .collect::<Vec<_>>(),
        ["keyword"]
    );
    assert_eq!(kinds[0].kind, Some(CompletionKind::EnumMember));

    let source = "language \"Toy\" { construct \"item\" { ";
    let nested = complete_at(
        &SourceFile::new("language.langmeld", source),
        grammar,
        source.len(),
    );
    assert_eq!(
        nested
            .iter()
            .map(|candidate| candidate.text.as_str())
            .collect::<Vec<_>>(),
        ["syntax", "block", "implements", "completion", "}"]
    );

    let source =
        "language \"Toy\" { construct \"item\" { syntax { literal \"x\"; } block \"{\" \"}\" { ";
    let nested = complete_at(
        &SourceFile::new("language.langmeld", source),
        grammar,
        source.len(),
    );
    assert_eq!(
        nested
            .iter()
            .map(|candidate| candidate.text.as_str())
            .collect::<Vec<_>>(),
        ["allow", "allow_category", "}"]
    );
}

#[test]
fn rejects_offsets_outside_the_source_or_inside_a_utf8_codepoint() {
    let source = SourceFile::new("main.langmeld", "é");

    assert!(complete_at(&source, &language(), source.text.len() + 1).is_empty());
    assert!(complete_at(&source, &language(), 1).is_empty());
}

#[test]
fn rejects_oversized_sources_before_calling_the_completion_lexer() {
    struct UnexpectedLexer;

    impl Lexer for UnexpectedLexer {
        fn lex(&self, _: &SourceFile) -> LexOutput {
            panic!("oversized completion input must be rejected before lexing");
        }
    }

    let source = SourceFile::new(
        "large",
        "x".repeat(ParseLimits::default().max_source_bytes + 1),
    );
    let result = complete_at_with_lexer(&source, &language(), 0, &UnexpectedLexer);

    assert!(result.is_empty());
}

struct KeywordAsPunctuation;

impl Lexer for KeywordAsPunctuation {
    fn lex(&self, source: &SourceFile) -> LexOutput {
        let mut output = DefaultLexer.lex(source);
        for token in &mut output.tokens {
            if token.kind == TokenKind::Identifier {
                token.kind = TokenKind::Punctuation;
            }
        }
        output
    }
}

#[test]
fn custom_lexer_can_filter_partial_textual_tokens() {
    let source = SourceFile::new("main.langmeld", "program { pri");
    let candidates = complete_at_with_lexer(
        &source,
        &language(),
        source.text.len(),
        &KeywordAsPunctuation,
    );

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].text, "print");
}
