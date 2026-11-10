//! Source-spanned Pratt parser for common expression forms.

use langmeld_core::{Diagnostic, SourceFile, Span};

use crate::{Associativity, ExpressionGrammar, OperatorFixity};

/// Maximum recursive expression nesting accepted by the parser.
pub const MAX_EXPRESSION_NESTING_DEPTH: usize = 128;

/// A parsed expression and its source range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expression {
    /// Expression form and its children.
    pub kind: ExpressionKind,
    /// Complete source range of this expression.
    pub span: Span,
}

/// Syntax forms emitted by the generic expression parser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExpressionKind {
    /// Decimal integer text, preserved as written.
    Integer(String),
    /// Identifier-like name. Resolution is host-owned.
    Name(String),
    /// Quoted string text, including quotes.
    String(String),
    /// Parenthesized expression.
    Group(Box<Expression>),
    /// Prefix operation. Its meaning is host-owned.
    Prefix {
        /// Operator token.
        operator: String,
        /// Operand expression.
        operand: Box<Expression>,
    },
    /// Infix operation. Its meaning is host-owned.
    Infix {
        /// Operator token.
        operator: String,
        /// Left operand.
        left: Box<Expression>,
        /// Right operand.
        right: Box<Expression>,
    },
    /// Function or value call.
    Call {
        /// Called expression.
        callee: Box<Expression>,
        /// Arguments in source order.
        arguments: Vec<Expression>,
    },
    /// Member access through `.`.
    Member {
        /// Expression before the dot.
        target: Box<Expression>,
        /// Member name.
        name: String,
    },
    /// Indexing through square brackets.
    Index {
        /// Indexed expression.
        target: Box<Expression>,
        /// Index expression.
        index: Box<Expression>,
    },
}

/// An expression parse result with its consumed range and diagnostics.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExpressionParseResult {
    /// Parsed expression when a valid root was recognized.
    pub expression: Option<Expression>,
    /// Range consumed by the root expression, empty when parsing failed.
    pub consumed: Span,
    /// Lexical and syntax diagnostics.
    pub diagnostics: Vec<Diagnostic>,
}

/// Parses source ranges with one validated operator table.
pub struct ExpressionParser<'a> {
    grammar: &'a ExpressionGrammar,
}

impl<'a> ExpressionParser<'a> {
    /// Creates a parser backed by an expression grammar.
    #[must_use]
    pub const fn new(grammar: &'a ExpressionGrammar) -> Self {
        Self { grammar }
    }

    /// Parses one expression and requires the selected range to end there.
    #[must_use]
    pub fn parse(&self, source: &SourceFile, range: Span) -> ExpressionParseResult {
        self.parse_bounded(source, range, &[])
    }

    /// Parses one expression, stopping before a listed terminator.
    ///
    /// Terminators remain unconsumed so a host statement parser can handle
    /// punctuation such as `;`, `,`, or a closing delimiter itself.
    #[must_use]
    pub fn parse_until(
        &self,
        source: &SourceFile,
        range: Span,
        terminators: &[&str],
    ) -> ExpressionParseResult {
        self.parse_bounded(source, range, terminators)
    }

    fn parse_bounded(
        &self,
        source: &SourceFile,
        range: Span,
        terminators: &[&str],
    ) -> ExpressionParseResult {
        if range.start > range.end
            || range.end > source.text.len()
            || !source.text.is_char_boundary(range.start)
            || !source.text.is_char_boundary(range.end)
        {
            return ExpressionParseResult {
                expression: None,
                consumed: Span::new(range.start, range.start),
                diagnostics: vec![Diagnostic::spec_error(
                    "LM-EXPR-001",
                    "expression range is outside the source or not on UTF-8 boundaries",
                )],
            };
        }

        let (tokens, mut diagnostics) = lex(source, range, self.grammar);
        let mut parser = Parser {
            source,
            tokens,
            cursor: 0,
            range_end: range.end,
            diagnostics: &mut diagnostics,
            grammar: self.grammar,
        };
        let expression = parser.parse_expression(0, terminators, 0);
        if expression.is_none() && parser.diagnostics.is_empty() {
            parser.error_at("LM-EXPR-002", "expected an expression", range.start);
        }
        if expression.is_some()
            && let Some(token) = parser.current()
            && !terminators.contains(&parser.token_text(token))
        {
            parser.error(
                "LM-EXPR-003",
                "unexpected token after expression",
                token.span,
            );
        }
        let consumed = expression
            .as_ref()
            .map_or(Span::new(range.start, range.start), |root| root.span);
        ExpressionParseResult {
            expression,
            consumed,
            diagnostics,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TokenKind {
    Identifier,
    Integer,
    String,
    Operator,
    Punctuation,
    Invalid,
}

#[derive(Clone, Copy, Debug)]
struct Token {
    kind: TokenKind,
    span: Span,
}

struct Parser<'a> {
    source: &'a SourceFile,
    tokens: Vec<Token>,
    cursor: usize,
    range_end: usize,
    diagnostics: &'a mut Vec<Diagnostic>,
    grammar: &'a ExpressionGrammar,
}

impl Parser<'_> {
    fn parse_expression(
        &mut self,
        minimum_binding_power: u64,
        terminators: &[&str],
        depth: usize,
    ) -> Option<Expression> {
        if depth >= MAX_EXPRESSION_NESTING_DEPTH {
            self.error_at(
                "LM-EXPR-004",
                "maximum expression nesting depth exceeded",
                self.current()
                    .map_or(self.range_end, |token| token.span.start),
            );
            return None;
        }

        let Some(token) = self.advance() else {
            self.error_at("LM-EXPR-002", "expected an expression", self.range_end);
            return None;
        };
        let mut left = if token.kind == TokenKind::Operator
            && let Some(operator) = self
                .grammar
                .operator(self.token_text(token), OperatorFixity::Prefix)
        {
            let expression =
                self.parse_expression(u64::from(operator.precedence), terminators, depth + 1)?;
            Expression {
                span: Span::new(token.span.start, expression.span.end),
                kind: ExpressionKind::Prefix {
                    operator: operator.token.clone(),
                    operand: Box::new(expression),
                },
            }
        } else {
            match token.kind {
                TokenKind::Identifier => Expression {
                    kind: ExpressionKind::Name(self.token_text(token).to_owned()),
                    span: token.span,
                },
                TokenKind::Integer => Expression {
                    kind: ExpressionKind::Integer(self.token_text(token).to_owned()),
                    span: token.span,
                },
                TokenKind::String => Expression {
                    kind: ExpressionKind::String(self.token_text(token).to_owned()),
                    span: token.span,
                },
                TokenKind::Punctuation if self.token_text(token) == "(" => {
                    let inner = self.parse_expression(0, &[")"], depth + 1)?;
                    let Some(close) = self
                        .current()
                        .filter(|token| self.token_text(*token) == ")")
                    else {
                        self.error_at("LM-EXPR-005", "expected `)`", inner.span.end);
                        return None;
                    };
                    self.cursor += 1;
                    Expression {
                        span: Span::new(token.span.start, close.span.end),
                        kind: ExpressionKind::Group(Box::new(inner)),
                    }
                }
                _ => {
                    self.error("LM-EXPR-002", "expected an expression", token.span);
                    return None;
                }
            }
        };

        while let Some(next) = self.current() {
            let next_text = self.token_text(next);
            if terminators.contains(&next_text) {
                break;
            }
            match next_text {
                "(" => {
                    left = self.parse_call(left, depth + 1)?;
                    continue;
                }
                "." => {
                    self.cursor += 1;
                    let Some(name) = self
                        .current()
                        .filter(|token| token.kind == TokenKind::Identifier)
                    else {
                        self.error_at(
                            "LM-EXPR-006",
                            "expected a member name after `.`",
                            next.span.end,
                        );
                        return None;
                    };
                    self.cursor += 1;
                    left = Expression {
                        span: Span::new(left.span.start, name.span.end),
                        kind: ExpressionKind::Member {
                            target: Box::new(left),
                            name: self.token_text(name).to_owned(),
                        },
                    };
                    continue;
                }
                "[" => {
                    self.cursor += 1;
                    let index = self.parse_expression(0, &["]"], depth + 1)?;
                    let Some(close) = self
                        .current()
                        .filter(|token| self.token_text(*token) == "]")
                    else {
                        self.error_at("LM-EXPR-007", "expected `]` after index", index.span.end);
                        return None;
                    };
                    self.cursor += 1;
                    left = Expression {
                        span: Span::new(left.span.start, close.span.end),
                        kind: ExpressionKind::Index {
                            target: Box::new(left),
                            index: Box::new(index),
                        },
                    };
                    continue;
                }
                _ => {}
            }

            let Some(operator) = self.grammar.operators.iter().find(|operator| {
                operator.token == next_text && matches!(operator.fixity, OperatorFixity::Infix(_))
            }) else {
                break;
            };
            if u64::from(operator.precedence) < minimum_binding_power {
                break;
            }
            let operator = operator.clone();
            let operator_span = next.span;
            self.cursor += 1;
            let OperatorFixity::Infix(associativity) = operator.fixity else {
                self.error("LM-EXPR-011", "operator is not infix", operator_span);
                return None;
            };
            let right_binding_power = match associativity {
                Associativity::Left => u64::from(operator.precedence) + 1,
                Associativity::Right => u64::from(operator.precedence),
            };
            let right = self.parse_expression(right_binding_power, terminators, depth + 1)?;
            left = Expression {
                span: Span::new(left.span.start, right.span.end),
                kind: ExpressionKind::Infix {
                    operator: operator.token,
                    left: Box::new(left),
                    right: Box::new(right),
                },
            };
        }
        Some(left)
    }

    fn parse_call(&mut self, callee: Expression, depth: usize) -> Option<Expression> {
        let open = self.advance()?;
        let mut arguments = Vec::new();
        if self
            .current()
            .is_some_and(|token| self.token_text(token) != ")")
        {
            loop {
                let argument = self.parse_expression(0, &[",", ")"], depth + 1)?;
                arguments.push(argument);
                if self
                    .current()
                    .is_some_and(|token| self.token_text(token) == ",")
                {
                    self.cursor += 1;
                    if self
                        .current()
                        .is_some_and(|token| self.token_text(token) == ")")
                    {
                        break;
                    }
                } else {
                    break;
                }
            }
        }
        let Some(close) = self
            .current()
            .filter(|token| self.token_text(*token) == ")")
        else {
            self.error_at("LM-EXPR-008", "expected `)` after arguments", open.span.end);
            return None;
        };
        self.cursor += 1;
        Some(Expression {
            span: Span::new(callee.span.start, close.span.end),
            kind: ExpressionKind::Call {
                callee: Box::new(callee),
                arguments,
            },
        })
    }

    fn current(&self) -> Option<Token> {
        self.tokens.get(self.cursor).copied()
    }

    fn advance(&mut self) -> Option<Token> {
        let token = self.current()?;
        self.cursor += 1;
        Some(token)
    }

    fn token_text(&self, token: Token) -> &str {
        &self.source.text[token.span.start..token.span.end]
    }

    fn error(&mut self, code: &str, message: &str, span: Span) {
        self.diagnostics
            .push(Diagnostic::error(code, message, self.source, span));
    }

    fn error_at(&mut self, code: &str, message: &str, offset: usize) {
        self.error(code, message, Span::new(offset, offset));
    }
}

fn lex(
    source: &SourceFile,
    range: Span,
    grammar: &ExpressionGrammar,
) -> (Vec<Token>, Vec<Diagnostic>) {
    let mut tokens = Vec::new();
    let mut diagnostics = Vec::new();
    let mut cursor = range.start;
    while cursor < range.end {
        let character = source.text[cursor..range.end]
            .chars()
            .next()
            .expect("cursor is within a validated source range");
        if character.is_whitespace() {
            cursor += character.len_utf8();
            continue;
        }
        let start = cursor;
        let (kind, end) = if character == '"' {
            let mut escaped = false;
            let mut end = cursor + character.len_utf8();
            let mut closed = false;
            while end < range.end {
                let next = source.text[end..range.end]
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
                let next = source.text[end..range.end]
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
            while end < range.end && source.text.as_bytes()[end].is_ascii_digit() {
                end += 1;
            }
            (TokenKind::Integer, end)
        } else if let Some(operator) = grammar
            .operator_tokens
            .iter()
            .find(|operator| source.text[start..range.end].starts_with(operator.as_str()))
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
