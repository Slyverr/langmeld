use langmeld_core::{Diagnostic, SourceFile, Span};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TokenKind {
    Identifier,
    Integer,
    String,
    Punctuation,
    Invalid,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Token {
    pub(super) kind: TokenKind,
    pub(super) span: Span,
}

pub(super) fn lex(source: &SourceFile) -> (Vec<Token>, Vec<Diagnostic>) {
    let mut tokens = Vec::new();
    let mut diagnostics = Vec::new();
    let mut chars = source.text().char_indices().peekable();

    while let Some((start, character)) = chars.next() {
        if character.is_whitespace() {
            continue;
        }

        if character == '/' && chars.peek().is_some_and(|(_, next)| *next == '/') {
            chars.next();
            while chars.next().is_some_and(|(_, next)| next != '\n') {}
            continue;
        }

        if character == '"' {
            let mut escaped = false;
            let mut end = start + character.len_utf8();
            let mut closed = false;

            for (byte, next) in chars.by_ref() {
                end = byte + next.len_utf8();
                if escaped {
                    escaped = false;
                } else if next == '\\' {
                    escaped = true;
                } else if next == '"' {
                    closed = true;
                    break;
                }
            }

            let span = Span::new(start, end);
            tokens.push(Token {
                kind: if closed {
                    TokenKind::String
                } else {
                    TokenKind::Invalid
                },
                span,
            });

            if !closed {
                diagnostics.push(Diagnostic::error(
                    "ASTER-LEX-001",
                    "unterminated string literal",
                    source,
                    span,
                ));
            }

            continue;
        }

        if character.is_alphabetic() || character == '_' {
            let mut end = start + character.len_utf8();
            while let Some(&(byte, next)) = chars.peek() {
                if next.is_alphanumeric() || next == '_' {
                    chars.next();
                    end = byte + next.len_utf8();
                } else {
                    break;
                }
            }

            tokens.push(Token {
                kind: TokenKind::Identifier,
                span: Span::new(start, end),
            });

            continue;
        }

        if character.is_ascii_digit() {
            let mut end = start + character.len_utf8();
            while let Some(&(byte, next)) = chars.peek() {
                if next.is_ascii_digit() {
                    chars.next();
                    end = byte + next.len_utf8();
                } else {
                    break;
                }
            }

            tokens.push(Token {
                kind: TokenKind::Integer,
                span: Span::new(start, end),
            });

            continue;
        }

        if "+-*/=!<>".contains(character) {
            let mut end = start + character.len_utf8();
            while let Some(&(byte, next)) = chars.peek() {
                if "+-*/=!<>".contains(next) {
                    chars.next();
                    end = byte + next.len_utf8();
                } else {
                    break;
                }
            }

            tokens.push(Token {
                kind: TokenKind::Punctuation,
                span: Span::new(start, end),
            });

            continue;
        }

        if "(){}:;,".contains(character) {
            tokens.push(Token {
                kind: TokenKind::Punctuation,
                span: Span::new(start, start + character.len_utf8()),
            });

            continue;
        }

        let span = Span::new(start, start + character.len_utf8());
        tokens.push(Token {
            kind: TokenKind::Invalid,
            span,
        });
        diagnostics.push(Diagnostic::error(
            "ASTER-LEX-002",
            format!("unexpected character `{character}`"),
            source,
            span,
        ));
    }

    (tokens, diagnostics)
}
