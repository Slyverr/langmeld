use langmeld_core::{Severity, SourceDatabase, SourceFile, Span};
use langmeld_language::{
    CaptureGroupKind, CapturedValue, ConstructSpec, LanguageSpec, LexOutput, LexedToken, Lexer,
    LexicalRules, ParseLimits, PatternPart, TokenKind, parse, parse_with_lexer,
    parse_with_lexer_and_limits, parse_with_limits,
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

#[path = "parse/blocks.rs"]
mod blocks;
#[path = "parse/core.rs"]
mod core;
#[path = "parse/lexing.rs"]
mod lexing;
#[path = "parse/limits.rs"]
mod limits;
#[path = "parse/patterns.rs"]
mod patterns;
#[path = "parse/roots.rs"]
mod roots;
