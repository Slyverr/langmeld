use std::sync::OnceLock;

use langmeld_core::{Diagnostic, SourceFile, Span};
use langmeld_expression::{
    Associativity, Expression, ExpressionGrammar, ExpressionParser, OperatorSpec,
};
use langmeld_language::{LanguageParser, ParsedDocument};

use crate::{Function, Parameter, Program, Statement, ValueType};

const MAX_BLOCK_NESTING_DEPTH: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TokenKind {
    Identifier,
    Integer,
    String,
    Punctuation,
    Invalid,
}

#[derive(Clone, Copy, Debug)]
struct Token {
    kind: TokenKind,
    span: Span,
}

/// Host parser for Aster declarations, statements, and expression boundaries.
#[derive(Clone, Copy, Debug, Default)]
pub struct AsterParser;

impl LanguageParser for AsterParser {
    type Tree = Program;

    fn parse(&self, source: &SourceFile) -> ParsedDocument<Self::Tree> {
        let (tokens, diagnostics) = lex(source);
        let mut parser = Parser {
            source,
            tokens,
            cursor: 0,
            diagnostics,
            expressions: ExpressionParser::new(expression_grammar()),
        };
        let mut functions = Vec::new();

        while parser.cursor < parser.tokens.len() {
            let start = parser.cursor;
            if parser.at("fn") {
                if let Some(function) = parser.parse_function() {
                    functions.push(function);
                }
            } else {
                parser.error_here("ASTER-PARSE-001", "expected a function declaration");
                parser.recover_to_function();
            }
            if parser.cursor == start {
                parser.cursor += 1;
            }
        }

        ParsedDocument::new(source, Program { functions }, parser.diagnostics)
    }
}

/// Parses Aster source and returns its tree with source-located diagnostics.
#[must_use]
pub fn parse_program(source: &SourceFile) -> ParsedDocument<Program> {
    AsterParser.parse(source)
}

struct Parser<'a> {
    source: &'a SourceFile,
    tokens: Vec<Token>,
    cursor: usize,
    diagnostics: Vec<Diagnostic>,
    expressions: ExpressionParser<'a>,
}

impl Parser<'_> {
    fn parse_function(&mut self) -> Option<Function> {
        let start = self.current()?.span.start;
        self.cursor += 1;
        let (name, name_span) =
            self.expect_identifier("ASTER-PARSE-002", "expected function name")?;
        self.expect("(", "ASTER-PARSE-003", "expected `(` after function name");

        let mut parameters = Vec::new();
        while self.cursor < self.tokens.len() && !self.at(")") && !self.at("{") {
            let before = self.cursor;
            if let Some(parameter) = self.parse_parameter() {
                parameters.push(parameter);
            }
            if !self.consume(",") && !self.at(")") {
                self.error_here("ASTER-PARSE-004", "expected `,` or `)` after parameter");
                self.recover_to(&[",", ")", "{"]);
                self.consume(",");
            }
            if self.cursor == before {
                self.cursor += 1;
            }
        }
        self.expect(")", "ASTER-PARSE-005", "expected `)` after parameters");
        self.expect("->", "ASTER-PARSE-006", "expected `->` before return type");
        let return_type = self.parse_type();
        if !self.consume("{") {
            self.error_here("ASTER-PARSE-007", "expected `{` before function body");
            return Some(Function {
                name,
                name_span,
                parameters,
                return_type,
                body: Vec::new(),
                span: Span::new(start, self.previous_end(start)),
            });
        }
        let body = self.parse_statements(0);
        Some(Function {
            name,
            name_span,
            parameters,
            return_type,
            body,
            span: Span::new(start, self.previous_end(start)),
        })
    }

    fn parse_parameter(&mut self) -> Option<Parameter> {
        let (name, name_span) =
            self.expect_identifier("ASTER-PARSE-008", "expected parameter name")?;
        self.expect(":", "ASTER-PARSE-009", "expected `:` after parameter name");
        let value_type = self.parse_type();
        Some(Parameter {
            name,
            value_type,
            name_span,
        })
    }

    fn parse_type(&mut self) -> ValueType {
        let Some(token) = self.current() else {
            self.error_here("ASTER-PARSE-010", "expected a type");
            return ValueType::Error;
        };
        if token.kind != TokenKind::Identifier {
            self.error_here("ASTER-PARSE-010", "expected a type name");
            self.cursor += 1;
            return ValueType::Error;
        }
        let value_type = match self.text(token) {
            "i32" => ValueType::I32,
            "bool" => ValueType::Bool,
            "string" => ValueType::String,
            name => {
                self.error(
                    "ASTER-PARSE-011",
                    format!("unknown type `{name}`"),
                    token.span,
                );
                ValueType::Error
            }
        };
        self.cursor += 1;
        value_type
    }

    fn parse_statements(&mut self, depth: usize) -> Vec<Statement> {
        let mut statements = Vec::new();
        while self.cursor < self.tokens.len() && !self.at("}") {
            let before = self.cursor;
            if let Some(statement) = self.parse_statement(depth) {
                statements.push(statement);
            }
            if self.cursor == before {
                self.cursor += 1;
            }
        }
        if !self.consume("}") {
            self.error_here("ASTER-PARSE-012", "expected `}` to close function or block");
        }
        statements
    }

    fn parse_statement(&mut self, depth: usize) -> Option<Statement> {
        let token = self.current()?;
        match self.text(token) {
            "let" => self.parse_let(),
            "return" => self.parse_return(),
            "{" => self.parse_nested_block(depth),
            _ => {
                self.error(
                    "ASTER-PARSE-013",
                    "expected `let`, `return`, or a nested block",
                    token.span,
                );
                self.recover_statement();
                None
            }
        }
    }

    fn parse_let(&mut self) -> Option<Statement> {
        let start = self.current()?.span.start;
        self.cursor += 1;
        let (name, name_span) = self.expect_identifier("ASTER-PARSE-014", "expected local name")?;
        self.expect("=", "ASTER-PARSE-015", "expected `=` after local name");
        let value = self.parse_expression_statement()?;
        let end = self.previous_end(value.span.end);
        Some(Statement::Let {
            name,
            name_span,
            value,
            span: Span::new(start, end),
        })
    }

    fn parse_return(&mut self) -> Option<Statement> {
        let start = self.current()?.span.start;
        self.cursor += 1;
        let value = self.parse_expression_statement()?;
        let end = self.previous_end(value.span.end);
        Some(Statement::Return {
            value,
            span: Span::new(start, end),
        })
    }

    fn parse_nested_block(&mut self, depth: usize) -> Option<Statement> {
        let open = self.current()?.span;
        if depth >= MAX_BLOCK_NESTING_DEPTH {
            self.error(
                "ASTER-PARSE-014",
                format!("maximum block nesting depth ({MAX_BLOCK_NESTING_DEPTH}) exceeded"),
                open,
            );
            self.skip_balanced_block();
            return None;
        }
        self.cursor += 1;
        let statements = self.parse_statements(depth + 1);
        Some(Statement::Block {
            statements,
            span: Span::new(open.start, self.previous_end(open.end)),
        })
    }

    fn parse_expression_statement(&mut self) -> Option<Expression> {
        let start = self
            .current()
            .map_or(self.source.text().len(), |token| token.span.start);
        let (end, semicolon) = self.find_statement_end();
        let parsed = self.expressions.parse(self.source, Span::new(start, end));
        self.diagnostics.extend(parsed.diagnostics);
        self.cursor =
            semicolon.unwrap_or_else(|| self.tokens.partition_point(|token| token.span.end <= end));
        if semicolon.is_some() {
            self.cursor += 1;
        } else {
            self.error_here("ASTER-PARSE-016", "expected `;` after expression");
        }
        parsed.expression
    }

    fn find_statement_end(&self) -> (usize, Option<usize>) {
        let mut nesting = 0usize;
        for (index, token) in self.tokens.iter().enumerate().skip(self.cursor) {
            match self.text(*token) {
                "(" | "[" => nesting += 1,
                ")" | "]" => nesting = nesting.saturating_sub(1),
                ";" if nesting == 0 => return (token.span.start, Some(index)),
                "}" if nesting == 0 => return (token.span.start, None),
                _ => {}
            }
        }
        (self.source.text().len(), None)
    }

    fn recover_statement(&mut self) {
        while let Some(token) = self.current() {
            if self.text(token) == "}" {
                break;
            }
            self.cursor += 1;
            if self.text(token) == ";" {
                break;
            }
        }
    }

    fn recover_to(&mut self, delimiters: &[&str]) {
        while let Some(token) = self.current() {
            if delimiters.contains(&self.text(token)) {
                break;
            }
            self.cursor += 1;
        }
    }

    fn recover_to_function(&mut self) {
        while let Some(token) = self.current() {
            if self.text(token) == "fn" {
                break;
            }
            self.cursor += 1;
        }
    }

    fn skip_balanced_block(&mut self) {
        let mut depth = 0usize;
        while let Some(token) = self.current() {
            match self.text(token) {
                "{" => depth += 1,
                "}" => {
                    depth = depth.saturating_sub(1);
                    self.cursor += 1;
                    if depth == 0 {
                        return;
                    }
                    continue;
                }
                _ => {}
            }
            self.cursor += 1;
        }
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
        self.cursor += 1;
        Some((self.text(token).to_owned(), token.span))
    }

    fn expect(&mut self, expected: &str, code: &str, message: &str) -> bool {
        if self.consume(expected) {
            true
        } else {
            self.error_here(code, message);
            false
        }
    }

    fn consume(&mut self, expected: &str) -> bool {
        if self.at(expected) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    fn at(&self, expected: &str) -> bool {
        self.current()
            .is_some_and(|token| self.text(token) == expected)
    }

    fn current(&self) -> Option<Token> {
        self.tokens.get(self.cursor).copied()
    }

    fn text(&self, token: Token) -> &str {
        &self.source.text()[token.span.start..token.span.end]
    }

    fn previous_end(&self, fallback: usize) -> usize {
        self.cursor
            .checked_sub(1)
            .and_then(|index| self.tokens.get(index))
            .map_or(fallback, |token| token.span.end)
    }

    fn error_here(&mut self, code: &str, message: &str) {
        let offset = self
            .current()
            .map_or(self.source.text().len(), |token| token.span.start);
        self.error(code, message, Span::new(offset, offset));
    }

    fn error(&mut self, code: &str, message: impl Into<String>, span: Span) {
        self.diagnostics
            .push(Diagnostic::error(code, message, self.source, span));
    }
}

fn expression_grammar() -> &'static ExpressionGrammar {
    static GRAMMAR: OnceLock<ExpressionGrammar> = OnceLock::new();
    GRAMMAR.get_or_init(|| {
        ExpressionGrammar::new([
            OperatorSpec::infix("==", 1, Associativity::Left),
            OperatorSpec::infix("<", 1, Associativity::Left),
            OperatorSpec::infix("+", 2, Associativity::Left),
            OperatorSpec::infix("-", 2, Associativity::Left),
            OperatorSpec::infix("*", 3, Associativity::Left),
            OperatorSpec::infix("/", 3, Associativity::Left),
            OperatorSpec::prefix("-", 4),
        ])
        .expect("Aster's built-in expression grammar must remain valid")
    })
}

fn lex(source: &SourceFile) -> (Vec<Token>, Vec<Diagnostic>) {
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
