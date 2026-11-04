//! Source-size checks and trusted-boundary validation for host lexers.

use langmeld_core::{Diagnostic, SourceFile, Span};

use crate::{
    LanguageToken, LanguageTokenKind, LexedSource, ParseLimits, ParseResult, Program, SyntaxToken,
    SyntaxTokenKind,
    syntax::{append_missing_tokens, build_syntax_tree, lossless_syntax_tokens},
};

use super::prepared::ConfiguredLanguageParser;

pub(super) fn parse_configured(
    parser: &ConfiguredLanguageParser,
    source: &SourceFile,
    limits: ParseLimits,
) -> ParseResult {
    if source.text.len() > limits.max_source_bytes {
        return failure(
            source,
            Diagnostic::error(
                "LM1300",
                format!(
                    "source is {} bytes; parsing limit is {} bytes",
                    source.text.len(),
                    limits.max_source_bytes
                ),
                Span::new(0, source.text.len()),
            ),
            true,
        );
    }
    if limits.max_tokens == 0 {
        return failure(
            source,
            Diagnostic::error(
                "LM1301",
                "token count exceeds the limit of 0",
                Span::new(0, 0),
            ),
            true,
        );
    }

    let (tokens, mut diagnostics, resource_limited) = if let Some(lexer) = &parser.lexer {
        let output = (lexer.lex)(source);
        match validate_custom_tokens(source, output, limits.max_tokens) {
            Ok((tokens, diagnostics)) => (tokens, diagnostics, false),
            Err((diagnostic, limited)) => return failure(source, diagnostic, limited),
        }
    } else {
        crate::lexing::lex(source, limits.max_tokens)
    };
    let mut syntax_tokens = lossless_syntax_tokens(source, &tokens);
    let super::engine::ParsedTokens {
        program,
        diagnostics: mut parse_diagnostics,
        custom_parser_spans,
        block_delimiters,
        completion_frontiers,
        completion_blocks,
        resource_limited: parse_limited,
    } = super::engine::parse_tokens(parser, source, tokens, limits);
    diagnostics.append(&mut parse_diagnostics);
    append_missing_tokens(source, &diagnostics, &mut syntax_tokens);
    let syntax_tree = build_syntax_tree(&program, &syntax_tokens, source);
    ParseResult {
        program,
        syntax_tree,
        syntax_tokens,
        diagnostics,
        custom_parser_spans,
        block_delimiters,
        completion_frontiers,
        completion_blocks,
        resource_limited: resource_limited || parse_limited,
    }
}

fn validate_custom_tokens(
    source: &SourceFile,
    output: LexedSource,
    max_tokens: usize,
) -> Result<(Vec<LanguageToken>, Vec<Diagnostic>), (Diagnostic, bool)> {
    if output.tokens.len() >= max_tokens {
        return Err((
            Diagnostic::error(
                "LM1301",
                format!("token count exceeds the limit of {max_tokens}"),
                Span::new(0, source.text.len()),
            ),
            true,
        ));
    }
    let mut previous_end = 0;
    for token in &output.tokens {
        let span = token.span;
        if matches!(token.kind, LanguageTokenKind::End)
            || span.start >= span.end
            || span.start < previous_end
            || span.end > source.text.len()
            || !source.text.is_char_boundary(span.start)
            || !source.text.is_char_boundary(span.end)
        {
            return Err((
                Diagnostic::error(
                    "LM1303",
                    "custom lexer returned an invalid, unordered, or overlapping token span",
                    Span::new(0, source.text.len()),
                ),
                false,
            ));
        }
        previous_end = span.end;
    }
    for diagnostic in &output.diagnostics {
        if diagnostic
            .span
            .is_some_and(|span| !valid_span(source, span))
        {
            return Err((
                Diagnostic::error(
                    "LM1303",
                    "custom lexer returned a diagnostic with an invalid source span",
                    Span::new(0, source.text.len()),
                ),
                false,
            ));
        }
    }
    let mut tokens = output.tokens;
    tokens.push(LanguageToken::new(
        LanguageTokenKind::End,
        Span::new(source.text.len(), source.text.len()),
    ));
    Ok((tokens, output.diagnostics))
}

fn valid_span(source: &SourceFile, span: Span) -> bool {
    span.start <= span.end
        && span.end <= source.text.len()
        && source.text.is_char_boundary(span.start)
        && source.text.is_char_boundary(span.end)
}

fn failure(source: &SourceFile, diagnostic: Diagnostic, resource_limited: bool) -> ParseResult {
    let syntax_tokens = if source.text.is_empty() {
        Vec::new()
    } else {
        vec![SyntaxToken {
            kind: SyntaxTokenKind::Unknown,
            span: Span::new(0, source.text.len()),
        }]
    };
    let program = Program { nodes: Vec::new() };
    let syntax_tree = build_syntax_tree(&program, &syntax_tokens, source);
    ParseResult {
        program,
        syntax_tree,
        syntax_tokens,
        diagnostics: vec![diagnostic],
        custom_parser_spans: Vec::new(),
        block_delimiters: Vec::new(),
        completion_frontiers: Vec::new(),
        completion_blocks: Vec::new(),
        resource_limited,
    }
}

#[cfg(test)]
mod tests {
    use langmeld_core::Span;

    use super::*;
    use crate::parser::PreparedLanguage;
    use crate::{CustomLexer, LanguageTokenKind as Kind};
    use langmeld_core::{ConstructBuilder, LanguageBuilder};

    fn parser() -> crate::parser::prepared::ConfiguredLanguageParser {
        let spec = LanguageBuilder::new("Input")
            .version("0.1.0")
            .root("root")
            .construct(
                ConstructBuilder::new("root")
                    .syntax("root {")
                    .block(std::iter::empty::<&str>()),
            )
            .build()
            .expect("valid language");
        PreparedLanguage::new(spec)
            .expect("patterns compile")
            .with_custom_parsers([])
            .expect("there are no custom parsers")
    }

    #[test]
    fn rejects_source_over_limit_before_calling_host_lexer() {
        let called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let observed = called.clone();
        let parser = parser().with_custom_lexer(CustomLexer::new(move |_| {
            observed.store(true, std::sync::atomic::Ordering::Relaxed);
            LexedSource::default()
        }));
        let source = SourceFile::new("large.source", "root");
        let result = parser.parse_with_limits(
            &source,
            ParseLimits {
                max_source_bytes: 3,
                ..ParseLimits::default()
            },
        );
        assert!(!called.load(std::sync::atomic::Ordering::Relaxed));
        assert_eq!(result.diagnostics[0].code, "LM1300");
        assert!(result.resource_limited);
    }

    #[test]
    fn rejects_host_tokens_with_overlapping_or_invalid_utf8_spans() {
        for invalid_span in [Span::new(0, 1), Span::new(1, 2)] {
            let parser = parser().with_custom_lexer(CustomLexer::new(move |_| {
                LexedSource::new(
                    [LanguageToken::new(
                        Kind::Identifier("é".to_owned()),
                        invalid_span,
                    )],
                    [],
                )
            }));
            let source = SourceFile::new("unicode.source", "é");
            let result = parser.parse(&source);
            assert_eq!(result.diagnostics[0].code, "LM1303");
        }
    }

    #[test]
    fn host_eof_token_and_bad_diagnostic_spans_are_rejected() {
        for output in [
            LexedSource::new([LanguageToken::new(Kind::End, Span::new(0, 0))], []),
            LexedSource::new([], [Diagnostic::error("HOST", "bad span", Span::new(1, 8))]),
        ] {
            let parser = parser().with_custom_lexer(CustomLexer::new(move |_| output.clone()));
            let result = parser.parse(&SourceFile::new("test.source", "root"));
            assert_eq!(result.diagnostics[0].code, "LM1303");
        }
    }

    #[test]
    fn rejects_overlapping_tokens_and_host_token_overflow() {
        let overlapping = parser().with_custom_lexer(CustomLexer::new(|_| {
            LexedSource::new(
                [
                    LanguageToken::new(Kind::Identifier("a".to_owned()), Span::new(0, 1)),
                    LanguageToken::new(Kind::Identifier("b".to_owned()), Span::new(0, 1)),
                ],
                [],
            )
        }));
        assert_eq!(
            overlapping
                .parse(&SourceFile::new("overlap.source", "ab"))
                .diagnostics[0]
                .code,
            "LM1303"
        );

        let limited = parser().with_custom_lexer(CustomLexer::new(|_| {
            LexedSource::new(
                [LanguageToken::new(
                    Kind::Identifier("root".to_owned()),
                    Span::new(0, 4),
                )],
                [],
            )
        }));
        let result = limited.parse_with_limits(
            &SourceFile::new("tokens.source", "root"),
            ParseLimits {
                max_tokens: 1,
                ..ParseLimits::default()
            },
        );
        assert_eq!(result.diagnostics[0].code, "LM1301");
        assert!(result.resource_limited);
    }
}
