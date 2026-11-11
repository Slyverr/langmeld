use std::collections::BTreeMap;

use langmeld_core::{Diagnostic, SourceFile, Span};
use langmeld_expression::{
    Associativity, Expression, ExpressionGrammar, ExpressionKind, ExpressionParser, OperatorSpec,
};
use langmeld_language::{LanguageParser, ParsedDocument};

const PROGRAM: &str = "let width = 20 + 4 * 2;\nlet height = (10 + 5) * 2;\n";

#[derive(Debug)]
struct Declaration {
    name: String,
    value: Expression,
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
        let expression_grammar = ExpressionGrammar::new([
            OperatorSpec::infix("+", 1, Associativity::Left),
            OperatorSpec::infix("*", 2, Associativity::Left),
        ])
        .expect("the example expression grammar is valid");

        let (tokens, mut diagnostics) = lex(source);
        let mut parser = Parser {
            source,
            tokens,
            cursor: 0,
            expression_parser: ExpressionParser::new(&expression_grammar),
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

struct Parser<'a, 'grammar> {
    source: &'a SourceFile,
    tokens: Vec<Token>,
    cursor: usize,
    expression_parser: ExpressionParser<'grammar>,
    diagnostics: &'a mut Vec<Diagnostic>,
}

impl Parser<'_, '_> {
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

        let expression_start = self
            .current()
            .map_or(self.source.text.len(), |token| token.span.start);
        let semicolon_index = self.find_statement_end();
        let expression_end = self
            .tokens
            .get(semicolon_index)
            .map_or(self.source.text.len(), |token| token.span.start);
        let parsed = self.expression_parser.parse_until(
            self.source,
            Span::new(expression_start, expression_end),
            &[";"],
        );
        self.diagnostics.extend(parsed.diagnostics);
        self.cursor = semicolon_index;

        let value = parsed.expression?;
        if !self.consume(";") {
            self.error_here("CALC-PARSE-004", "expected `;` after expression");
            return None;
        }
        Some(Declaration {
            name: self.token_text(name).to_owned(),
            value,
        })
    }

    fn find_statement_end(&self) -> usize {
        let mut nesting = 0usize;
        for (index, token) in self.tokens.iter().enumerate().skip(self.cursor) {
            match self.token_text(*token) {
                "(" | "[" => nesting += 1,
                ")" | "]" => nesting = nesting.saturating_sub(1),
                ";" if nesting == 0 => return index,
                _ => {}
            }
        }
        self.tokens.len()
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
        } else if "+-*=();".contains(character) {
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
    match &expression.kind {
        ExpressionKind::Integer(value) => value
            .parse()
            .map_err(|_| "integer literal is out of range".to_owned()),
        ExpressionKind::Name(name) => variables
            .get(name)
            .copied()
            .ok_or_else(|| format!("unknown variable `{name}`")),
        ExpressionKind::Group(expression) => evaluate(expression, variables),
        ExpressionKind::Prefix { operator, operand } if operator == "-" => {
            evaluate(operand, variables)?
                .checked_neg()
                .ok_or_else(|| "integer negation overflowed".to_owned())
        }
        ExpressionKind::Infix {
            operator,
            left,
            right,
        } if operator == "+" => evaluate(left, variables)?
            .checked_add(evaluate(right, variables)?)
            .ok_or_else(|| "integer addition overflowed".to_owned()),
        ExpressionKind::Infix {
            operator,
            left,
            right,
        } if operator == "*" => evaluate(left, variables)?
            .checked_mul(evaluate(right, variables)?)
            .ok_or_else(|| "integer multiplication overflowed".to_owned()),
        _ => Err("the example runtime does not support this expression".to_owned()),
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
