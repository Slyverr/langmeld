use langmeld_core::{Diagnostic, SourceFile, Span};

use crate::lexer::{LanguageToken as Token, LanguageTokenKind as TokenKind};

pub(super) fn lex(source: &SourceFile, max_tokens: usize) -> (Vec<Token>, Vec<Diagnostic>, bool) {
    let mut chars = source.text.char_indices().peekable();
    let mut tokens = Vec::new();
    let mut diagnostics = Vec::new();
    let mut resource_limited = false;

    while let Some((start, character)) = chars.next() {
        if tokens.len() >= max_tokens.saturating_sub(1) {
            diagnostics.push(Diagnostic::error(
                "LM1301",
                format!("token count exceeds the limit of {max_tokens}"),
                Span::new(start, start + character.len_utf8()),
            ));
            resource_limited = true;
            break;
        }
        if character == '\n' {
            tokens.push(Token {
                kind: TokenKind::Newline,
                span: Span::new(start, start + character.len_utf8()),
            });
            continue;
        }
        if character == '\r' {
            let mut end = start + character.len_utf8();
            if let Some((newline_start, newline)) = chars.next_if(|(_, next)| *next == '\n') {
                end = newline_start + newline.len_utf8();
            }
            tokens.push(Token {
                kind: TokenKind::Newline,
                span: Span::new(start, end),
            });
            continue;
        }
        if character.is_whitespace() {
            continue;
        }
        if character == '#' {
            let mut end = start + character.len_utf8();
            while let Some((offset, next)) = chars.peek().copied() {
                if next == '\n' || next == '\r' {
                    break;
                }
                chars.next();
                end = offset + next.len_utf8();
            }
            tokens.push(Token {
                kind: TokenKind::Comment,
                span: Span::new(start, end),
            });
            continue;
        }
        if character == '"' {
            let mut value = String::new();
            let mut end = start + 1;
            let mut terminated = false;
            let mut line_ended = false;
            let mut newline_span = None;
            while let Some((offset, next)) = chars.next() {
                end = offset + next.len_utf8();
                match next {
                    '"' => {
                        terminated = true;
                        break;
                    }
                    '\\' => {
                        if let Some((escaped_offset, escaped)) = chars.next() {
                            end = escaped_offset + escaped.len_utf8();
                            match escaped {
                                'n' => value.push('\n'),
                                'r' => value.push('\r'),
                                't' => value.push('\t'),
                                '"' => value.push('"'),
                                '\\' => value.push('\\'),
                                other => {
                                    diagnostics.push(Diagnostic::error(
                                        "LM1102",
                                        format!("unsupported string escape `\\{other}`"),
                                        Span::new(offset, end),
                                    ));
                                    value.push(other);
                                }
                            }
                        }
                    }
                    '\n' | '\r' => {
                        line_ended = true;
                        let mut newline_end = offset + next.len_utf8();
                        diagnostics.push(Diagnostic::error(
                            "LM1101",
                            "string literal is not closed before the line ends",
                            Span::new(start, offset),
                        ));
                        let line_feed = if next == '\r' {
                            chars.next_if(|(_, after)| *after == '\n')
                        } else {
                            None
                        };
                        if let Some((newline_offset, newline)) = line_feed {
                            newline_end = newline_offset + newline.len_utf8();
                        }
                        end = offset;
                        newline_span = Some(Span::new(offset, newline_end));
                        break;
                    }
                    other => value.push(other),
                }
            }
            if !terminated && !line_ended {
                diagnostics.push(Diagnostic::error(
                    "LM1101",
                    "unterminated string literal",
                    Span::new(start, end),
                ));
            }
            tokens.push(Token {
                kind: TokenKind::Text(value),
                span: Span::new(start, end),
            });
            if let Some(span) = newline_span {
                if tokens.len() >= max_tokens.saturating_sub(1) {
                    diagnostics.push(Diagnostic::error(
                        "LM1301",
                        format!("token count exceeds the limit of {max_tokens}"),
                        span,
                    ));
                    resource_limited = true;
                    break;
                }
                tokens.push(Token {
                    kind: TokenKind::Newline,
                    span,
                });
            }
            continue;
        }
        if character == '-' && chars.peek().is_some_and(|(_, next)| *next == '>') {
            if let Some((arrow_start, arrow)) = chars.next() {
                tokens.push(Token {
                    kind: TokenKind::Arrow,
                    span: Span::new(start, arrow_start + arrow.len_utf8()),
                });
            }
            continue;
        }
        if character.is_ascii_digit()
            || (character == '-' && chars.peek().is_some_and(|(_, next)| next.is_ascii_digit()))
        {
            let mut end = start + character.len_utf8();
            let mut decimal = false;
            while let Some((offset, next)) = chars.peek().copied() {
                if next.is_ascii_digit() {
                    chars.next();
                    end = offset + next.len_utf8();
                } else if next == '.' && !decimal {
                    chars.next();
                    end = offset + 1;
                    decimal = true;
                } else {
                    break;
                }
            }
            tokens.push(Token {
                kind: TokenKind::Number(source.text[start..end].to_owned()),
                span: Span::new(start, end),
            });
            continue;
        }
        if character == '_' || character.is_alphabetic() {
            let mut end = start + character.len_utf8();
            while let Some((offset, next)) = chars.peek().copied() {
                if next == '_' || next == '-' || next.is_alphanumeric() {
                    chars.next();
                    end = offset + next.len_utf8();
                } else {
                    break;
                }
            }
            tokens.push(Token {
                kind: TokenKind::Identifier(source.text[start..end].to_owned()),
                span: Span::new(start, end),
            });
            continue;
        }
        if character.is_ascii_punctuation() {
            tokens.push(Token {
                kind: TokenKind::Punctuation(character),
                span: Span::new(start, start + character.len_utf8()),
            });
            continue;
        }
        diagnostics.push(Diagnostic::error(
            "LM1100",
            format!("unexpected character `{character}`"),
            Span::new(start, start + character.len_utf8()),
        ));
        tokens.push(Token {
            kind: TokenKind::Unknown(character),
            span: Span::new(start, start + character.len_utf8()),
        });
    }
    tokens.push(Token {
        kind: TokenKind::End,
        span: Span::new(source.text.len(), source.text.len()),
    });
    (tokens, diagnostics, resource_limited)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_budget_includes_end_marker_and_zero_never_underflows() {
        let source = SourceFile::new("limited.source", "one two");
        let (tokens, diagnostics, limited) = lex(&source, 2);

        assert!(limited);
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].kind, TokenKind::Identifier("one".into()));
        assert_eq!(tokens[1].kind, TokenKind::End);
        assert_eq!(diagnostics[0].code, "LM1301");

        let (zero_tokens, zero_diagnostics, zero_limited) = lex(&source, 0);
        assert!(zero_limited);
        assert_eq!(zero_tokens.len(), 1);
        assert_eq!(zero_tokens[0].kind, TokenKind::End);
        assert_eq!(zero_diagnostics[0].code, "LM1301");
    }
}
