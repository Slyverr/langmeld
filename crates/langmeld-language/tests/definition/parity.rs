use langmeld_core::SourceFile;
use langmeld_language::{
    CompletionKind, ConstructSpec, LanguageSpec, LexicalRules, PatternPart,
    parse_language_definition,
};

const DEFINITION: &str = r##"
language "Parity" {
    root "program";
    lexical {
        line_comment "#";
        block_comment "/*" "*/";
    }
    construct "program" {
        completion keyword;
        syntax "braced" {
            literal "program";
        }
        block "{" "}" {
            allow "invoke";
            allow_category "statement";
        }
    }
    construct "invoke" {
        syntax "modern" {
            literal "invoke";
            identifier "target";
            optional "arguments" {
                literal "(";
                identifier "first";
                zero_or_more "additional" {
                    literal ",";
                    choice "mode" {
                        completion keyword;
                        value "fast";
                        value "safe";
                    }
                }
                literal ")";
            }
            literal ";";
        }
        syntax "legacy" {
            literal "call";
            identifier "target";
            one_or_more "suffixes" {
                literal ".";
                identifier "suffix";
            }
        }
        completion function;
        implements "statement";
    }
    construct "return" {
        syntax {
            literal "return";
            expression "value" until ";";
            literal ";";
        }
        implements "statement";
    }
    construct "declare" {
        syntax {
            literal "let";
            identifier "name";
            literal "=";
            integer "count";
            string "label";
            token "terminator";
        }
        implements "statement";
    }
}
"##;

#[test]
fn declarative_and_rust_frontends_build_the_same_language_specification() {
    let source = SourceFile::new("language/main.langmeld", DEFINITION);
    let definition = parse_language_definition(&source);
    assert!(
        definition.diagnostics.is_empty(),
        "{:?}",
        definition.diagnostics
    );
    let declarative = definition
        .language
        .expect("valid definition should produce a language specification");

    let rust = LanguageSpec::new(
        "Parity",
        [
            ConstructSpec::new("program", [PatternPart::literal("program")])
                .with_first_variant_name("braced")
                .with_completion_kind(CompletionKind::Keyword)
                .block_with_categories("{", "}", &["invoke"], &["statement"]),
            ConstructSpec::new(
                "invoke",
                [
                    PatternPart::literal("invoke"),
                    PatternPart::identifier("target"),
                    PatternPart::optional_named(
                        "arguments",
                        [
                            PatternPart::literal("("),
                            PatternPart::identifier("first"),
                            PatternPart::zero_or_more_named(
                                "additional",
                                [
                                    PatternPart::literal(","),
                                    PatternPart::choice_with_completion_kind(
                                        "mode",
                                        ["fast", "safe"],
                                        CompletionKind::Keyword,
                                    ),
                                ],
                            ),
                            PatternPart::literal(")"),
                        ],
                    ),
                    PatternPart::literal(";"),
                ],
            )
            .with_first_variant_name("modern")
            .variant_named(
                "legacy",
                [
                    PatternPart::literal("call"),
                    PatternPart::identifier("target"),
                    PatternPart::one_or_more_named(
                        "suffixes",
                        [PatternPart::literal("."), PatternPart::identifier("suffix")],
                    ),
                ],
            )
            .with_completion_kind(CompletionKind::Function)
            .implements(&["statement"]),
            ConstructSpec::new(
                "return",
                [
                    PatternPart::literal("return"),
                    PatternPart::expression("value", ";"),
                    PatternPart::literal(";"),
                ],
            )
            .implements(&["statement"]),
            ConstructSpec::new(
                "declare",
                [
                    PatternPart::literal("let"),
                    PatternPart::identifier("name"),
                    PatternPart::literal("="),
                    PatternPart::integer("count"),
                    PatternPart::string("label"),
                    PatternPart::token("terminator"),
                ],
            )
            .implements(&["statement"]),
        ],
    )
    .expect("the equivalent Rust grammar should be valid")
    .with_lexical_rules(
        LexicalRules::default()
            .line_comment("#")
            .block_comment("/*", "*/"),
    )
    .expect("the equivalent Rust lexer configuration should be valid")
    .with_root_constructs(&["program"])
    .expect("the equivalent Rust root should be valid");

    assert_eq!(declarative, rust);
}
