use langmeld_language::{ConstructSpec, LanguageSpec, LexicalRules, PatternPart};
use langmeld_textmate::{TextMateOptions, generate};
use serde_json::Value;

fn language() -> LanguageSpec {
    LanguageSpec::new(
        "Widget Script",
        [
            ConstructSpec::new("program", [PatternPart::literal("program")]).block(
                "[",
                "]",
                &["conditional", "output"],
            ),
            ConstructSpec::new(
                "conditional",
                [
                    PatternPart::literal("if"),
                    PatternPart::identifier("condition"),
                    PatternPart::optional([PatternPart::literal("else")]),
                    PatternPart::literal("{"),
                    PatternPart::one_or_more([PatternPart::literal("print")]),
                    PatternPart::literal("}"),
                ],
            ),
            ConstructSpec::new(
                "output",
                [
                    PatternPart::literal("print"),
                    PatternPart::string("value"),
                    PatternPart::literal("|"),
                ],
            ),
        ],
    )
    .expect("valid test grammar")
}

#[test]
fn exports_deduplicated_literals_and_builtin_lexical_patterns() {
    let output = generate(
        &language(),
        &TextMateOptions::new("source.widget").file_types(["widget"]),
    )
    .unwrap();
    let grammar: Value = serde_json::from_str(&output).unwrap();

    assert_eq!(grammar["name"], "Widget Script");
    assert_eq!(grammar["scopeName"], "source.widget");
    assert_eq!(grammar["fileTypes"], serde_json::json!(["widget"]));
    let patterns = grammar["patterns"].as_array().unwrap();
    assert_eq!(patterns[0]["name"], "string.quoted.double.widget");
    assert_eq!(patterns[0]["patterns"][0]["match"], r"\\.");
    assert_eq!(patterns[1]["match"], r"\b(?:program|if|else|print)\b");
    assert_eq!(patterns[2]["match"], r"\b[0-9]+\b");
    assert_eq!(patterns[3]["match"], r"(?:\[|\]|\{|\}|\|)");
}

#[test]
fn rejects_invalid_scope_names_and_file_extensions() {
    assert!(generate(&language(), &TextMateOptions::new("widget")).is_err());
    assert!(
        generate(
            &language(),
            &TextMateOptions::new("source.widget").file_types([".widget"])
        )
        .is_err()
    );
}

#[test]
fn exports_configured_line_and_block_comments() {
    let language = language()
        .with_lexical_rules(
            LexicalRules::default()
                .line_comment("//")
                .block_comment("/*", "*/"),
        )
        .unwrap();

    let output = generate(&language, &TextMateOptions::new("source.widget")).unwrap();
    let grammar: Value = serde_json::from_str(&output).unwrap();
    let patterns = grammar["patterns"].as_array().unwrap();

    assert_eq!(patterns[1]["name"], "comment.line.widget");
    assert_eq!(patterns[1]["match"], "//[^\\r\\n]*");
    assert_eq!(patterns[2]["name"], "comment.block.widget");
    assert_eq!(patterns[2]["begin"], r"/\*");
    assert_eq!(patterns[2]["end"], r"\*/");
}

#[test]
fn includes_finite_choice_values_in_lexical_highlighting() {
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
    .unwrap();

    let output = generate(&grammar, &TextMateOptions::new("source.movement")).unwrap();
    let grammar: Value = serde_json::from_str(&output).unwrap();

    assert_eq!(grammar["patterns"][1]["match"], r"\b(?:move|left|right)\b");
}
