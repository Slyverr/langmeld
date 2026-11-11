use langmeld_core::{Diagnostic, SourceFile, Span};

use crate::ExpressionGrammar;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TokenKind {
    Identifier,
    Integer,
    String,
    Operator,
    Punctuation,
    Invalid,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Token {
    pub(super) kind: TokenKind,
    pub(super) span: Span,
}

pub(super) fn lex(
    source: &SourceFile,
    range: Span,
    grammar: &ExpressionGrammar,
    max_tokens: usize,
) -> (Vec<Token>, Vec<Diagnostic>) {
    let mut tokens = Vec::new();
    let mut diagnostics = Vec::new();
    let mut cursor = range.start;

    while cursor < range.end {
        let character = source.text()[cursor..range.end]
            .chars()
            .next()
            .expect("cursor is within a validated source range");
        if character.is_whitespace() {
            cursor += character.len_utf8();
            continue;
        }

        let start = cursor;
        if tokens.len() == max_tokens {
            diagnostics.push(Diagnostic::error(
                "LM-EXPR-013",
                format!("expression token count exceeds the configured limit of {max_tokens}"),
                source,
                Span::new(start, start + character.len_utf8()),
            ));
            break;
        }

        let (kind, end) = if character == '"' {
            let mut escaped = false;
            let mut end = cursor + character.len_utf8();
            let mut closed = false;

            while end < range.end {
                let next = source.text()[end..range.end]
                    .chars()
                    .next()
                    .expect("end is within the validated source range");
                end += next.len_utf8();

                if escaped {
                    escaped = false;
                } else if next == '\\' {
                    escaped = true;
                } else if next == '"' {
                    closed = true;
                    break;
                }
            }

            if !closed {
                diagnostics.push(Diagnostic::error(
                    "LM-EXPR-009",
                    "unterminated string literal",
                    source,
                    Span::new(start, end),
                ));
            }

            (
                if closed {
                    TokenKind::String
                } else {
                    TokenKind::Invalid
                },
                end,
            )
        } else if character.is_alphabetic() || character == '_' {
            let mut end = cursor + character.len_utf8();

            while end < range.end {
                let next = source.text()[end..range.end]
                    .chars()
                    .next()
                    .expect("end is within the validated source range");
                if next.is_alphanumeric() || next == '_' {
                    end += next.len_utf8();
                } else {
                    break;
                }
            }

            (TokenKind::Identifier, end)
        } else if character.is_ascii_digit() {
            let mut end = cursor + character.len_utf8();
            while end < range.end && source.text().as_bytes()[end].is_ascii_digit() {
                end += 1;
            }

            (TokenKind::Integer, end)
        } else if let Some(operator) = grammar
            .operator_tokens
            .iter()
            .find(|operator| source.text()[start..range.end].starts_with(operator.as_str()))
        {
            (TokenKind::Operator, start + operator.len())
        } else if "()[],.;".contains(character) {
            (TokenKind::Punctuation, start + character.len_utf8())
        } else {
            let end = start + character.len_utf8();
            diagnostics.push(Diagnostic::error(
                "LM-EXPR-010",
                format!("unexpected character `{character}`"),
                source,
                Span::new(start, end),
            ));

            (TokenKind::Invalid, end)
        };

        tokens.push(Token {
            kind,
            span: Span::new(start, end),
        });
        cursor = end;
    }

    (tokens, diagnostics)
}
