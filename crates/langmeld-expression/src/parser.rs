//! Source-spanned Pratt parser for common expression forms.

use langmeld_core::{Diagnostic, SourceFile, Span};

use crate::{Associativity, ExpressionGrammar, OperatorFixity};

mod lexer;

use lexer::{Token, TokenKind, lex};

/// Maximum nested expression-tree depth accepted by the parser.
pub const MAX_EXPRESSION_NESTING_DEPTH: usize = 128;

/// Input and expression-tree bounds for the expression parser.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExpressionLimits {
    /// Maximum byte length of the selected source range.
    pub max_source_bytes: usize,
    /// Maximum number of tokens emitted from the selected range.
    pub max_tokens: usize,
    /// Maximum nested expression-tree depth. Values above
    /// [`MAX_EXPRESSION_NESTING_DEPTH`] are capped.
    pub max_nesting_depth: usize,
}

impl ExpressionLimits {
    /// Default bounds used by [`ExpressionParser::new`].
    pub const DEFAULT: Self = Self {
        max_source_bytes: 4 * 1024 * 1024,
        max_tokens: 250_000,
        max_nesting_depth: MAX_EXPRESSION_NESTING_DEPTH,
    };
}

impl Default for ExpressionLimits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

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
    /// Whether a parser resource limit stopped or truncated the result.
    pub resource_limited: bool,
}

/// Parses source ranges with one validated operator table.
pub struct ExpressionParser<'a> {
    grammar: &'a ExpressionGrammar,
    limits: ExpressionLimits,
}

impl<'a> ExpressionParser<'a> {
    /// Creates a parser backed by an expression grammar.
    #[must_use]
    pub const fn new(grammar: &'a ExpressionGrammar) -> Self {
        Self {
            grammar,
            limits: ExpressionLimits::DEFAULT,
        }
    }

    /// Creates a parser with caller-selected input and expression-tree limits.
    #[must_use]
    pub const fn with_limits(grammar: &'a ExpressionGrammar, limits: ExpressionLimits) -> Self {
        Self { grammar, limits }
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
                resource_limited: false,
            };
        }

        if range.end - range.start > self.limits.max_source_bytes {
            let offset =
                source_limit_offset(source, range.start, self.limits.max_source_bytes, range.end);
            return ExpressionParseResult {
                expression: None,
                consumed: Span::new(range.start, range.start),
                diagnostics: vec![Diagnostic::error(
                    "LM-EXPR-012",
                    format!(
                        "expression source range exceeds the configured limit of {} bytes",
                        self.limits.max_source_bytes
                    ),
                    source,
                    Span::new(offset, offset),
                )],
                resource_limited: true,
            };
        }

        let (tokens, mut diagnostics) = lex(source, range, self.grammar, self.limits.max_tokens);
        let resource_limited = diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "LM-EXPR-013");
        let mut parser = Parser {
            source,
            tokens,
            cursor: 0,
            range_end: range.end,
            diagnostics: &mut diagnostics,
            grammar: self.grammar,
            max_nesting_depth: self
                .limits
                .max_nesting_depth
                .min(MAX_EXPRESSION_NESTING_DEPTH),
            resource_limited,
        };
        let parsed = parser.parse_expression(0, terminators, 0);
        if parsed.is_none() && parser.diagnostics.is_empty() {
            parser.error_at("LM-EXPR-002", "expected an expression", range.start);
        }
        if parsed.is_some()
            && let Some(token) = parser.current()
            && !terminators.contains(&parser.token_text(token))
        {
            parser.error(
                "LM-EXPR-003",
                "unexpected token after expression",
                token.span,
            );
        }
        let consumed = parsed
            .as_ref()
            .map_or(Span::new(range.start, range.start), |root| {
                root.expression.span
            });
        let resource_limited = parser.resource_limited;
        ExpressionParseResult {
            expression: parsed.map(|parsed| parsed.expression),
            consumed,
            diagnostics,
            resource_limited,
        }
    }
}

struct ParsedExpression {
    expression: Expression,
    depth: usize,
}

fn parsed_leaf(expression: Expression) -> ParsedExpression {
    ParsedExpression {
        expression,
        depth: 1,
    }
}

struct Parser<'a> {
    source: &'a SourceFile,
    tokens: Vec<Token>,
    cursor: usize,
    range_end: usize,
    diagnostics: &'a mut Vec<Diagnostic>,
    grammar: &'a ExpressionGrammar,
    max_nesting_depth: usize,
    resource_limited: bool,
}

impl Parser<'_> {
    fn parse_expression(
        &mut self,
        minimum_binding_power: u64,
        terminators: &[&str],
        depth: usize,
    ) -> Option<ParsedExpression> {
        if depth >= self.max_nesting_depth {
            self.report_depth_limit(
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
            let node_depth = expression.depth + 1;
            self.ensure_depth(node_depth, token.span)?;
            ParsedExpression {
                depth: node_depth,
                expression: Expression {
                    span: Span::new(token.span.start, expression.expression.span.end),
                    kind: ExpressionKind::Prefix {
                        operator: operator.token.clone(),
                        operand: Box::new(expression.expression),
                    },
                },
            }
        } else {
            match token.kind {
                TokenKind::Identifier => parsed_leaf(Expression {
                    kind: ExpressionKind::Name(self.token_text(token).to_owned()),
                    span: token.span,
                }),
                TokenKind::Integer => parsed_leaf(Expression {
                    kind: ExpressionKind::Integer(self.token_text(token).to_owned()),
                    span: token.span,
                }),
                TokenKind::String => parsed_leaf(Expression {
                    kind: ExpressionKind::String(self.token_text(token).to_owned()),
                    span: token.span,
                }),
                TokenKind::Punctuation if self.token_text(token) == "(" => {
                    let inner = self.parse_expression(0, &[")"], depth + 1)?;
                    let Some(close) = self
                        .current()
                        .filter(|token| self.token_text(*token) == ")")
                    else {
                        self.error_at("LM-EXPR-005", "expected `)`", inner.expression.span.end);
                        return None;
                    };
                    self.cursor += 1;
                    let node_depth = inner.depth + 1;
                    self.ensure_depth(node_depth, token.span)?;
                    ParsedExpression {
                        depth: node_depth,
                        expression: Expression {
                            span: Span::new(token.span.start, close.span.end),
                            kind: ExpressionKind::Group(Box::new(inner.expression)),
                        },
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
                    let node_depth = left.depth + 1;
                    self.ensure_depth(node_depth, next.span)?;
                    left = ParsedExpression {
                        depth: node_depth,
                        expression: Expression {
                            span: Span::new(left.expression.span.start, name.span.end),
                            kind: ExpressionKind::Member {
                                target: Box::new(left.expression),
                                name: self.token_text(name).to_owned(),
                            },
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
                        self.error_at(
                            "LM-EXPR-007",
                            "expected `]` after index",
                            index.expression.span.end,
                        );
                        return None;
                    };
                    self.cursor += 1;
                    let node_depth = left.depth.max(index.depth) + 1;
                    self.ensure_depth(node_depth, next.span)?;
                    left = ParsedExpression {
                        depth: node_depth,
                        expression: Expression {
                            span: Span::new(left.expression.span.start, close.span.end),
                            kind: ExpressionKind::Index {
                                target: Box::new(left.expression),
                                index: Box::new(index.expression),
                            },
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
            let node_depth = left.depth.max(right.depth) + 1;
            self.ensure_depth(node_depth, operator_span)?;
            left = ParsedExpression {
                depth: node_depth,
                expression: Expression {
                    span: Span::new(left.expression.span.start, right.expression.span.end),
                    kind: ExpressionKind::Infix {
                        operator: operator.token,
                        left: Box::new(left.expression),
                        right: Box::new(right.expression),
                    },
                },
            };
        }
        Some(left)
    }

    fn parse_call(&mut self, callee: ParsedExpression, depth: usize) -> Option<ParsedExpression> {
        let open = self.advance()?;
        let mut arguments = Vec::new();
        let mut child_depth = callee.depth;
        if self
            .current()
            .is_some_and(|token| self.token_text(token) != ")")
        {
            loop {
                let argument = self.parse_expression(0, &[",", ")"], depth + 1)?;
                child_depth = child_depth.max(argument.depth);
                arguments.push(argument.expression);
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
        let node_depth = child_depth + 1;
        self.ensure_depth(node_depth, open.span)?;
        Some(ParsedExpression {
            depth: node_depth,
            expression: Expression {
                span: Span::new(callee.expression.span.start, close.span.end),
                kind: ExpressionKind::Call {
                    callee: Box::new(callee.expression),
                    arguments,
                },
            },
        })
    }

    fn ensure_depth(&mut self, depth: usize, span: Span) -> Option<()> {
        if depth <= self.max_nesting_depth {
            Some(())
        } else {
            self.report_depth_limit(span.start);
            None
        }
    }

    fn report_depth_limit(&mut self, offset: usize) {
        self.resource_limited = true;
        self.error_at(
            "LM-EXPR-004",
            "maximum expression nesting depth exceeded",
            offset,
        );
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

fn source_limit_offset(source: &SourceFile, start: usize, limit: usize, end: usize) -> usize {
    let mut offset = start + limit.min(end - start);
    while !source.text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}
