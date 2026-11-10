use langmeld_language::{ConstructSpec, LanguageSpec, PatternPart};
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
