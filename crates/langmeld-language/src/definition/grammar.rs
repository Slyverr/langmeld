//! Built-in grammar used to parse `.langmeld` language definitions.

use std::sync::OnceLock;

use crate::{CompletionKind, ConstructSpec, LanguageSpec, LexicalRules, PatternPart};

pub(super) fn definition_grammar() -> &'static LanguageSpec {
    static GRAMMAR: OnceLock<LanguageSpec> = OnceLock::new();
    GRAMMAR.get_or_init(|| {
        const PATTERN_ITEMS: &[&str] = &[
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
        ];
        LanguageSpec::new(
            "Langmeld definition",
            [
                ConstructSpec::new(
                    "language",
                    [
                        PatternPart::literal("language"),
                        PatternPart::string("name"),
                    ],
                )
                .block("{", "}", &["root", "construct", "import", "lexical"]),
                ConstructSpec::new(
                    "root",
                    [
                        PatternPart::literal("root"),
                        PatternPart::string("name"),
                        PatternPart::literal(";"),
                    ],
                ),
                ConstructSpec::new(
                    "import",
                    [
                        PatternPart::literal("import"),
                        PatternPart::string("path"),
                        PatternPart::literal(";"),
                    ],
                ),
                ConstructSpec::new("lexical", [PatternPart::literal("lexical")]).block(
                    "{",
                    "}",
                    &["line_comment", "block_comment"],
                ),
                ConstructSpec::new(
                    "line_comment",
                    [
                        PatternPart::literal("line_comment"),
                        PatternPart::string("marker"),
                        PatternPart::literal(";"),
                    ],
                ),
                ConstructSpec::new(
                    "block_comment",
                    [
                        PatternPart::literal("block_comment"),
                        PatternPart::string("open"),
                        PatternPart::string("close"),
                        PatternPart::literal(";"),
                    ],
                ),
                ConstructSpec::new(
                    "construct",
                    [
                        PatternPart::literal("construct"),
                        PatternPart::string("name"),
                    ],
                )
                .block(
                    "{",
                    "}",
                    &[
                        "syntax",
                        "block",
                        "optional_block",
                        "implements",
                        "completion",
                    ],
                ),
                ConstructSpec::new(
                    "completion",
                    [
                        PatternPart::literal("completion"),
                        PatternPart::choice(
                            "kind",
                            CompletionKind::ALL.map(CompletionKind::as_str),
                        ),
                        PatternPart::literal(";"),
                    ],
                )
                .variant([
                    PatternPart::literal("completion"),
                    PatternPart::string("kind"),
                    PatternPart::literal(";"),
                ]),
                ConstructSpec::new(
                    "syntax",
                    [
                        PatternPart::literal("syntax"),
                        PatternPart::optional([PatternPart::string("name")]),
                    ],
                )
                .block("{", "}", PATTERN_ITEMS),
                ConstructSpec::new(
                    "block",
                    [
                        PatternPart::literal("block"),
                        PatternPart::string("open"),
                        PatternPart::string("close"),
                    ],
                )
                .block("{", "}", &["allow", "allow_category"]),
                ConstructSpec::new(
                    "optional_block",
                    [
                        PatternPart::literal("optional_block"),
                        PatternPart::string("open"),
                        PatternPart::string("close"),
                    ],
                )
                .block("{", "}", &["allow", "allow_category"]),
                ConstructSpec::new(
                    "optional",
                    [
                        PatternPart::literal("optional"),
                        PatternPart::optional([PatternPart::string("name")]),
                    ],
                )
                .block("{", "}", PATTERN_ITEMS),
                ConstructSpec::new(
                    "zero_or_more",
                    [
                        PatternPart::literal("zero_or_more"),
                        PatternPart::optional([PatternPart::string("name")]),
                    ],
                )
                .block("{", "}", PATTERN_ITEMS),
                ConstructSpec::new(
                    "one_or_more",
                    [
                        PatternPart::literal("one_or_more"),
                        PatternPart::optional([PatternPart::string("name")]),
                    ],
                )
                .block("{", "}", PATTERN_ITEMS),
                ConstructSpec::new(
                    "choice",
                    [PatternPart::literal("choice"), PatternPart::string("name")],
                )
                .block("{", "}", &["completion", "choice_value"]),
                ConstructSpec::new(
                    "literal",
                    [
                        PatternPart::literal("literal"),
                        PatternPart::string("value"),
                        PatternPart::literal(";"),
                    ],
                ),
                ConstructSpec::new(
                    "token",
                    [
                        PatternPart::literal("token"),
                        PatternPart::string("value"),
                        PatternPart::literal(";"),
                    ],
                ),
                ConstructSpec::new(
                    "identifier",
                    [
                        PatternPart::literal("identifier"),
                        PatternPart::string("value"),
                        PatternPart::literal(";"),
                    ],
                ),
                ConstructSpec::new(
                    "integer",
                    [
                        PatternPart::literal("integer"),
                        PatternPart::string("value"),
                        PatternPart::literal(";"),
                    ],
                ),
                ConstructSpec::new(
                    "string",
                    [
                        PatternPart::literal("string"),
                        PatternPart::string("value"),
                        PatternPart::literal(";"),
                    ],
                ),
                ConstructSpec::new(
                    "choice_value",
                    [
                        PatternPart::literal("value"),
                        PatternPart::string("value"),
                        PatternPart::literal(";"),
                    ],
                ),
                ConstructSpec::new(
                    "expression",
                    [
                        PatternPart::literal("expression"),
                        PatternPart::string("name"),
                        PatternPart::literal("until"),
                        PatternPart::string("terminator"),
                        PatternPart::literal(";"),
                    ],
                ),
                ConstructSpec::new(
                    "allow",
                    [
                        PatternPart::literal("allow"),
                        PatternPart::string("name"),
                        PatternPart::literal(";"),
                    ],
                ),
                ConstructSpec::new(
                    "allow_category",
                    [
                        PatternPart::literal("allow_category"),
                        PatternPart::string("category"),
                        PatternPart::literal(";"),
                    ],
                ),
                ConstructSpec::new(
                    "implements",
                    [
                        PatternPart::literal("implements"),
                        PatternPart::string("category"),
                        PatternPart::literal(";"),
                    ],
                ),
            ],
        )
        .and_then(|language| language.with_root_constructs(&["language", "import", "construct"]))
        .and_then(|language| {
            language.with_lexical_rules(LexicalRules::default().line_comment("//"))
        })
        .expect("the built-in language-definition grammar must remain valid")
    })
}
