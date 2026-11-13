use langmeld_core::{Diagnostic, SourceFile, Span};

use crate::{
    Associativity, Expression, ExpressionGrammar, ExpressionKind, ExpressionLimits,
    ExpressionParseResult, MAX_EXPRESSION_NESTING_DEPTH, OperatorFixity,
};

use super::lexer::{Token, TokenKind, lex};

pub(super) fn parse(
    source: &SourceFile,
    range: Span,
    grammar: &ExpressionGrammar,
    limits: ExpressionLimits,
    terminators: &[&str],
) -> ExpressionParseResult {
    if !source.is_valid_span(range) {
        let empty_offset = source_boundary_at_or_before(source, range.start);

        return ExpressionParseResult {
            source: source.identity(),
            expression: None,
            consumed: Span::new(empty_offset, empty_offset),
            diagnostics: vec![Diagnostic::spec_error(
                "LM-EXPR-001",
                "expression range is outside the source or not on UTF-8 boundaries",
            )],
            resource_limited: false,
        };
    }

    if range.end - range.start > limits.max_source_bytes {
        let offset = source_limit_offset(source, range.start, limits.max_source_bytes, range.end);
        return ExpressionParseResult {
            source: source.identity(),
            expression: None,
            consumed: Span::new(range.start, range.start),
            diagnostics: vec![Diagnostic::error(
                "LM-EXPR-012",
                format!(
                    "expression source range exceeds the configured limit of {} bytes",
                    limits.max_source_bytes
                ),
                source,
                Span::new(offset, offset),
            )],
            resource_limited: true,
        };
    }

    let (tokens, mut diagnostics) = lex(source, range, grammar, limits.max_tokens);
    let resource_limited = diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "LM-EXPR-013");
    let mut parser = Parser {
        source,
        tokens,
        cursor: 0,
        range_end: range.end,
        diagnostics: &mut diagnostics,
        grammar,
        max_nesting_depth: limits.max_nesting_depth.min(MAX_EXPRESSION_NESTING_DEPTH),
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
        source: source.identity(),
        expression: parsed.map(|parsed| parsed.expression),
        consumed,
        diagnostics,
        resource_limited,
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
        &self.source.text()[token.span.start..token.span.end]
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
    source_boundary_at_or_before(source, start + limit.min(end - start))
}

fn source_boundary_at_or_before(source: &SourceFile, offset: usize) -> usize {
    let mut offset = offset.min(source.text().len());
    while !source.text().is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}
