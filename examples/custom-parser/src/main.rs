use std::collections::BTreeMap;

use langmeld_core::{Diagnostic, SourceFile, Span};
use langmeld_language::{LanguageParser, ParsedDocument};

const PROGRAM: &str = "let width = 20 + 4 * 2;\nlet height = (10 + 5) * 2;\n";

#[derive(Debug)]
struct Declaration {
    name: String,
    value: Expression,
}

#[derive(Debug)]
enum Expression {
    Integer(i64),
    Name(String),
    Add(Box<Self>, Box<Self>),
    Multiply(Box<Self>, Box<Self>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TokenKind {
    Identifier,
    Integer,
    Punctuation,
    Invalid,
}

#[derive(Clone, Copy, Debug)]
struct Token {
    kind: TokenKind,
    span: Span,
}

struct CalculatorParser;

impl LanguageParser for CalculatorParser {
    type Tree = Vec<Declaration>;

    fn parse(&self, source: &SourceFile) -> ParsedDocument<Self::Tree> {
        let (tokens, mut diagnostics) = lex(source);
        let mut parser = Parser {
            source,
            tokens,
            cursor: 0,
            diagnostics: &mut diagnostics,
        };
        let mut declarations = Vec::new();

        while parser.cursor < parser.tokens.len() {
            if let Some(declaration) = parser.parse_declaration() {
                declarations.push(declaration);
            } else {
                parser.recover_statement();
            }
        }

        ParsedDocument::new(source, declarations, diagnostics)
    }
}

struct Parser<'a> {
    source: &'a SourceFile,
    tokens: Vec<Token>,
    cursor: usize,
    diagnostics: &'a mut Vec<Diagnostic>,
}

impl Parser<'_> {
    fn parse_declaration(&mut self) -> Option<Declaration> {
        let keyword = self.advance()?;
        if self.token_text(keyword) != "let" {
            self.error("CALC-PARSE-001", "expected `let`", keyword.span);
            return None;
        }
        let Some(name) = self.advance() else {
            self.error_here("CALC-PARSE-002", "expected a declaration name");
            return None;
        };
        if name.kind != TokenKind::Identifier || self.token_text(name) == "let" {
            self.error("CALC-PARSE-002", "expected a declaration name", name.span);
            return None;
        }
        if !self.consume("=") {
            self.error_here("CALC-PARSE-003", "expected `=`");
            return None;
        }
        let value = self.parse_expression(0)?;
        if !self.consume(";") {
            self.error_here("CALC-PARSE-004", "expected `;` after expression");
            return None;
        }
        Some(Declaration {
            name: self.token_text(name).to_owned(),
            value,
        })
    }

    fn parse_expression(&mut self, minimum_binding_power: u8) -> Option<Expression> {
        let Some(token) = self.advance() else {
            self.error_here("CALC-PARSE-007", "expected an expression");
            return None;
        };
        let mut left = match token.kind {
            TokenKind::Integer => match self.token_text(token).parse() {
                Ok(value) => Expression::Integer(value),
                Err(_) => {
                    self.error("CALC-PARSE-005", "integer is out of range", token.span);
                    return None;
                }
            },
            TokenKind::Identifier => Expression::Name(self.token_text(token).to_owned()),
            TokenKind::Punctuation if self.token_text(token) == "(" => {
                let expression = self.parse_expression(0)?;
                if !self.consume(")") {
                    self.error_here("CALC-PARSE-006", "expected `)`");
                    return None;
                }
                expression
            }
            _ => {
                self.error("CALC-PARSE-007", "expected an expression", token.span);
                return None;
            }
        };

        while let Some(operator) = self.current() {
            let (left_power, right_power) = match self.token_text(operator) {
                "+" => (1, 2),
                "*" => (3, 4),
                _ => break,
            };
            if left_power < minimum_binding_power {
                break;
            }
            let operator = self.advance()?;
            let right = self.parse_expression(right_power)?;
            left = match self.token_text(operator) {
                "+" => Expression::Add(Box::new(left), Box::new(right)),
                "*" => Expression::Multiply(Box::new(left), Box::new(right)),
                _ => return None,
            };
        }
        Some(left)
    }

    fn recover_statement(&mut self) {
        while let Some(token) = self.advance() {
            if self.token_text(token) == ";" {
                break;
            }
        }
    }

    fn current(&self) -> Option<Token> {
        self.tokens.get(self.cursor).copied()
    }

    fn advance(&mut self) -> Option<Token> {
        let token = self.current()?;
        self.cursor += 1;
        Some(token)
    }

    fn consume(&mut self, text: &str) -> bool {
        if self
            .current()
            .is_some_and(|token| self.token_text(token) == text)
        {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    fn token_text(&self, token: Token) -> &str {
        &self.source.text[token.span.start..token.span.end]
    }

    fn error_here(&mut self, code: &str, message: &str) {
        let offset = self
            .current()
            .map_or(self.source.text.len(), |token| token.span.start);
        self.error(code, message, Span::new(offset, offset));
    }

    fn error(&mut self, code: &str, message: &str, span: Span) {
        self.diagnostics
            .push(Diagnostic::error(code, message, self.source, span));
    }
}

fn lex(source: &SourceFile) -> (Vec<Token>, Vec<Diagnostic>) {
    let mut tokens = Vec::new();
    let mut diagnostics = Vec::new();
    let mut characters = source.text.char_indices().peekable();
    while let Some((start, character)) = characters.next() {
        if character.is_whitespace() {
            continue;
        }
        let kind = if character.is_ascii_alphabetic() || character == '_' {
            while characters
                .peek()
                .is_some_and(|(_, next)| next.is_ascii_alphanumeric() || *next == '_')
            {
                characters.next();
            }
            TokenKind::Identifier
        } else if character.is_ascii_digit() {
            while characters
                .peek()
                .is_some_and(|(_, next)| next.is_ascii_digit())
            {
                characters.next();
            }
            TokenKind::Integer
        } else if "+*=();".contains(character) {
            TokenKind::Punctuation
        } else {
            let span = Span::new(start, start + character.len_utf8());
            diagnostics.push(Diagnostic::error(
                "CALC-LEX-001",
                format!("unexpected character `{character}`"),
                source,
                span,
            ));
            TokenKind::Invalid
        };
        let end = characters
            .peek()
            .map_or(source.text.len(), |(next_start, _)| *next_start);
        let span = Span::new(start, end);
        tokens.push(Token { kind, span });
    }
    (tokens, diagnostics)
}

fn evaluate(expression: &Expression, variables: &BTreeMap<String, i64>) -> Result<i64, String> {
    match expression {
        Expression::Integer(value) => Ok(*value),
        Expression::Name(name) => variables
            .get(name)
            .copied()
            .ok_or_else(|| format!("unknown variable `{name}`")),
        Expression::Add(left, right) => evaluate(left, variables)?
            .checked_add(evaluate(right, variables)?)
            .ok_or_else(|| "integer addition overflowed".to_owned()),
        Expression::Multiply(left, right) => evaluate(left, variables)?
            .checked_mul(evaluate(right, variables)?)
            .ok_or_else(|| "integer multiplication overflowed".to_owned()),
    }
}

fn main() {
    let source = SourceFile::new("main.calc", PROGRAM);
    let parsed = CalculatorParser.parse(&source);
    if !parsed.is_valid() {
        for diagnostic in parsed.diagnostics {
            let (line, column) = diagnostic
                .location
                .map(|location| source.line_column(location.span.start))
                .unwrap_or((1, 1));
            eprintln!(
                "{}:{line}:{column}: {}",
                diagnostic.code, diagnostic.message
            );
        }
        std::process::exit(1);
    }

    let mut variables = BTreeMap::new();
    for declaration in &parsed.tree {
        match evaluate(&declaration.value, &variables) {
            Ok(value) => {
                variables.insert(declaration.name.clone(), value);
            }
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
    }
    for (name, value) in variables {
        println!("{name} = {value}");
    }
}
