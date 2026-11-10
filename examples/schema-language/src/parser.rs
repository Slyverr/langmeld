use langmeld_core::{Diagnostic, SourceFile, Span};
use langmeld_language::{LanguageParser, ParsedDocument};

use crate::{Field, Record, Schema, TypeName};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TokenKind {
    Identifier,
    LeftBrace,
    RightBrace,
    Colon,
    Separator,
}

#[derive(Clone, Copy, Debug)]
struct Token {
    kind: TokenKind,
    span: Span,
}

/// Parser for `record Name { field: Type, ... }` schema declarations.
#[derive(Clone, Copy, Debug, Default)]
pub struct SchemaParser;

impl LanguageParser for SchemaParser {
    type Tree = Schema;

    fn parse(&self, source: &SourceFile) -> ParsedDocument<Self::Tree> {
        let (tokens, diagnostics) = lex(source);
        let mut parser = Parser {
            source,
            tokens,
            cursor: 0,
            diagnostics,
        };
        let mut records = Vec::new();

        while parser.cursor < parser.tokens.len() {
            let before = parser.cursor;
            if parser.at_word("record") {
                if let Some(record) = parser.parse_record() {
                    records.push(record);
                }
            } else {
                parser.error_here("SCHEMA-PARSE-001", "expected a `record` declaration");
                parser.recover_to_record();
            }
            if parser.cursor == before {
                parser.cursor += 1;
            }
        }

        ParsedDocument::new(source, Schema { records }, parser.diagnostics)
    }
}

/// Parses schema source and returns its tree with source-located diagnostics.
#[must_use]
pub fn parse_schema(source: &SourceFile) -> ParsedDocument<Schema> {
    SchemaParser.parse(source)
}

struct Parser<'a> {
    source: &'a SourceFile,
    tokens: Vec<Token>,
    cursor: usize,
    diagnostics: Vec<Diagnostic>,
}

impl Parser<'_> {
    fn parse_record(&mut self) -> Option<Record> {
        let start = self.current()?.span.start;
        self.cursor += 1;
        let (name, name_span) =
            self.expect_identifier("SCHEMA-PARSE-002", "expected record name")?;
        if !self.consume(TokenKind::LeftBrace) {
            self.error_here("SCHEMA-PARSE-003", "expected `{` after record name");
            return Some(Record {
                name,
                name_span,
                fields: Vec::new(),
                span: Span::new(start, name_span.end),
            });
        }

        let mut fields = Vec::new();
        while self.cursor < self.tokens.len() && !self.at(TokenKind::RightBrace) {
            let before = self.cursor;
            if let Some(field) = self.parse_field() {
                fields.push(field);
            }
            if self.consume(TokenKind::Separator) || self.at(TokenKind::RightBrace) {
                continue;
            }
            self.error_here("SCHEMA-PARSE-004", "expected `,` or `}` after field");
            self.recover_field();
            if self.cursor == before {
                self.cursor += 1;
            }
        }

        let end = if self.consume(TokenKind::RightBrace) {
            self.previous_end()
        } else {
            self.error_here("SCHEMA-PARSE-005", "expected `}` to close record");
            self.previous_end().max(name_span.end)
        };
        Some(Record {
            name,
            name_span,
            fields,
            span: Span::new(start, end),
        })
    }

    fn parse_field(&mut self) -> Option<Field> {
        let start = self.current()?.span.start;
        let Some((name, name_span)) =
            self.expect_identifier("SCHEMA-PARSE-006", "expected field name")
        else {
            self.recover_field();
            return None;
        };
        if !self.consume(TokenKind::Colon) {
            self.error_here("SCHEMA-PARSE-007", "expected `:` after field name");
            self.recover_field();
            return None;
        }
        let Some((type_name, type_span)) =
            self.expect_identifier("SCHEMA-PARSE-008", "expected field type")
        else {
            self.recover_field();
            return None;
        };
        Some(Field {
            name,
            name_span,
            ty: TypeName {
                name: type_name,
                span: type_span,
            },
            span: Span::new(start, type_span.end),
        })
    }

    fn expect_identifier(&mut self, code: &str, message: &str) -> Option<(String, Span)> {
        let Some(token) = self.current() else {
            self.error_here(code, message);
            return None;
        };
        if token.kind != TokenKind::Identifier {
            self.error(code, message, token.span);
            return None;
        }
        let word = self.text(token).to_owned();
        self.cursor += 1;
        Some((word, token.span))
    }

    fn at_word(&self, word: &str) -> bool {
        self.current()
            .is_some_and(|token| token.kind == TokenKind::Identifier && self.text(token) == word)
    }

    fn at(&self, kind: TokenKind) -> bool {
        self.current().is_some_and(|token| token.kind == kind)
    }

    fn consume(&mut self, kind: TokenKind) -> bool {
        if self.at(kind) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    fn current(&self) -> Option<Token> {
        self.tokens.get(self.cursor).copied()
    }

    fn text(&self, token: Token) -> &str {
        &self.source.text[token.span.start..token.span.end]
    }

    fn previous_end(&self) -> usize {
        self.tokens
            .get(self.cursor.saturating_sub(1))
            .map_or(0, |token| token.span.end)
    }

    fn error_here(&mut self, code: &str, message: &str) {
        let span = self.current().map_or(
            Span::new(self.source.text.len(), self.source.text.len()),
            |t| t.span,
        );
        self.error(code, message, span);
    }

    fn error(&mut self, code: &str, message: &str, span: Span) {
        self.diagnostics
            .push(Diagnostic::error(code, message, self.source, span));
    }

    fn recover_field(&mut self) {
        while self.cursor < self.tokens.len()
            && !self.at(TokenKind::Separator)
            && !self.at(TokenKind::RightBrace)
        {
            self.cursor += 1;
        }
        self.consume(TokenKind::Separator);
    }

    fn recover_to_record(&mut self) {
        self.cursor += 1;
        while self.cursor < self.tokens.len() && !self.at_word("record") {
            self.cursor += 1;
        }
    }
}

fn lex(source: &SourceFile) -> (Vec<Token>, Vec<Diagnostic>) {
    let mut tokens = Vec::new();
    let mut diagnostics = Vec::new();
    let mut chars = source.text.char_indices().peekable();

    while let Some((start, character)) = chars.next() {
        if character.is_whitespace() {
            continue;
        }
        let kind = match character {
            '{' => Some(TokenKind::LeftBrace),
            '}' => Some(TokenKind::RightBrace),
            ':' => Some(TokenKind::Colon),
            ',' | ';' => Some(TokenKind::Separator),
            c if c == '_' || c.is_ascii_alphabetic() => {
                let mut end = start + c.len_utf8();
                while let Some(&(next_start, next)) = chars.peek() {
                    if next == '_' || next.is_ascii_alphanumeric() {
                        chars.next();
                        end = next_start + next.len_utf8();
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
            _ => None,
        };
        if let Some(kind) = kind {
            tokens.push(Token {
                kind,
                span: Span::new(start, start + character.len_utf8()),
            });
        } else {
            diagnostics.push(Diagnostic::error(
                "SCHEMA-LEX-001",
                format!("unexpected character `{character}`"),
                source,
                Span::new(start, start + character.len_utf8()),
            ));
        }
    }

    (tokens, diagnostics)
}
