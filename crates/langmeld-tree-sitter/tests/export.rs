use langmeld_language::{ConstructSpec, LanguageSpec, LexicalRules, PatternPart};
use langmeld_tree_sitter::{TreeSitterOptions, generate};

fn language() -> LanguageSpec {
    LanguageSpec::new(
        "Widget Script",
        [
            ConstructSpec::new("program", [PatternPart::literal("program")]).block(
                "{",
                "}",
                &["say", "raw"],
            ),
            ConstructSpec::new(
                "say",
                [
                    PatternPart::literal("say"),
                    PatternPart::identifier("subject"),
                    PatternPart::optional([
                        PatternPart::literal("at"),
                        PatternPart::integer("time"),
                    ]),
                    PatternPart::zero_or_more([
                        PatternPart::literal("with"),
                        PatternPart::string("label"),
                    ]),
                ],
            ),
            ConstructSpec::new(
                "raw",
                [PatternPart::literal("raw"), PatternPart::token("value")],
            ),
        ],
    )
    .expect("valid test grammar")
    .with_root_constructs(&["program"])
    .expect("program is a root construct")
}

#[test]
fn exports_constructs_fields_groups_and_blocks_as_tree_sitter_rules() {
    let output = generate(&language(), &TreeSitterOptions::new("widget_script"))
        .expect("valid grammar name");

    assert!(output.contains("name: \"widget_script\""));
    assert!(output.contains("source_file: $ => repeat($.construct_0_program)"));
    assert!(output.contains("field(\"subject\", $.identifier)"));
    assert!(output.contains("optional(seq(\"at\", field(\"time\", $.integer)))"));
    assert!(output.contains("repeat(seq(\"with\", field(\"label\", $.string)))"));
    assert!(output.contains(
        "seq(\"program\", \"{\", repeat(choice($.construct_1_say, $.construct_2_raw)), \"}\")"
    ));
    assert!(output.contains("identifier: _ =>"));
    assert!(output.contains("integer: _ => /[0-9]+/"));
    assert!(output.contains("string: _ =>"));
    assert!(output.contains("field(\"value\", $.token)"));
    assert!(output.contains("punctuation: _ =>"));
    assert!(
        output.contains("token: $ => choice($.identifier, $.integer, $.string, $.punctuation)")
    );
}

#[test]
fn rejects_invalid_grammar_names_and_languages_without_root_constructs() {
    assert!(generate(&language(), &TreeSitterOptions::new("Widget Script")).is_err());
    let no_roots = language()
        .with_root_constructs(&[])
        .expect("an empty root list is valid for parsing");
    assert!(generate(&no_roots, &TreeSitterOptions::new("widget_script")).is_err());
}

#[test]
fn exports_configured_line_comments_as_tree_sitter_extras() {
    let language = language()
        .with_lexical_rules(LexicalRules::default().line_comment("//"))
        .unwrap();

    let output = generate(&language, &TreeSitterOptions::new("widget_script"))
        .expect("line comments are supported");

    assert!(output.contains("extras: $ => [/\\s/, $.line_comment_0]"));
    assert!(output.contains("line_comment_0: _ => token(seq(\"//\", /[^\\r\\n]*/))"));
}

#[test]
fn reports_configured_block_comments_as_unsupported() {
    let language = language()
        .with_lexical_rules(LexicalRules::default().block_comment("/*", "*/"))
        .unwrap();

    assert!(matches!(
        generate(&language, &TreeSitterOptions::new("widget_script")),
        Err(langmeld_tree_sitter::TreeSitterError::UnsupportedBlockComment {
            opening,
            closing,
        }) if opening == "/*" && closing == "*/"
    ));
}

#[test]
fn reports_host_parsed_expression_captures_as_unsupported() {
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
    .unwrap()
    .with_root_constructs(&["print"])
    .unwrap();

    let error = generate(&grammar, &TreeSitterOptions::new("expressions"))
        .expect_err("host-parsed expression captures need an explicit Tree-sitter grammar");

    assert!(matches!(
        error,
        langmeld_tree_sitter::TreeSitterError::UnsupportedExpressionCapture {
            construct,
            field
        } if construct == "print" && field == "value"
    ));
}

#[test]
fn exports_finite_choice_fields_as_named_token_alternatives() {
    let grammar = LanguageSpec::new(
        "Movement",
        [ConstructSpec::new(
            "move",
            [
                PatternPart::literal("move"),
                PatternPart::choice("direction", ["left", "right"]),
            ],
        )],
    )
    .unwrap()
    .with_root_constructs(&["move"])
    .unwrap();

    let output = generate(&grammar, &TreeSitterOptions::new("movement")).unwrap();

    assert!(output.contains("seq(\"move\", field(\"direction\", choice(\"left\", \"right\")))"));
}
