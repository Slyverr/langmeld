//! Configurable source-spanned Pratt parsing and host-defined expression typing.
//!
//! This crate supplies expression structure and precedence handling. Literal,
//! variable, operator, type, and runtime meanings belong to the language host
//! through [`ExpressionTypeRules`] and [`ExpressionRuntime`]. Hosts can extend
//! atom parsing or own a broader expression grammar when this shared subset
//! does not fit their language.

use langmeld_core::{Diagnostic, SourceFile, Span};

mod runtime;
mod typing;

pub use runtime::{ExpressionRuntime, evaluate_expression};
pub use typing::{ExpressionTypeRules, infer_expression_type};

/// Maximum expression tokens accepted by the shared lexer, excluding the end marker.
pub const MAX_EXPRESSION_TOKENS: usize = 250_000;
/// Maximum recursive parse, type, and runtime expression depth.
pub const MAX_EXPRESSION_NESTING: usize = 64;

/// Associativity for an infix operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Associativity {
    /// Operators of the same precedence group from left to right.
    Left,
    /// Operators of the same precedence group from right to left.
    Right,
}

/// One operator's token and precedence declarations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperatorSpec {
    /// Source spelling, such as `+`, `!`, or `**`.
    pub token: String,
    /// Prefix binding power, if the operator can appear before an expression.
    pub prefix_precedence: Option<u8>,
    /// Infix binding power and associativity, if the operator is binary.
    pub infix: Option<(u8, Associativity)>,
}

impl OperatorSpec {
    /// Declares a prefix operator.
    #[must_use]
    pub fn prefix(token: impl Into<String>, precedence: u8) -> Self {
        Self {
            token: token.into(),
            prefix_precedence: Some(precedence),
            infix: None,
        }
    }

    /// Declares a left-associative infix operator.
    #[must_use]
    pub fn infix(token: impl Into<String>, precedence: u8) -> Self {
        Self::infix_with(token, precedence, Associativity::Left)
    }

    /// Declares an infix operator with explicit associativity.
    #[must_use]
    pub fn infix_with(
        token: impl Into<String>,
        precedence: u8,
        associativity: Associativity,
    ) -> Self {
        Self {
            token: token.into(),
            prefix_precedence: None,
            infix: Some((precedence, associativity)),
        }
    }

    /// Declares one token as both prefix and infix.
    #[must_use]
    pub fn prefix_infix(
        token: impl Into<String>,
        prefix_precedence: u8,
        infix_precedence: u8,
        associativity: Associativity,
    ) -> Self {
        Self {
            token: token.into(),
            prefix_precedence: Some(prefix_precedence),
            infix: Some((infix_precedence, associativity)),
        }
    }
}

/// Validated operator declarations used by the parser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpressionGrammar {
    operators: Vec<OperatorSpec>,
}

impl ExpressionGrammar {
    /// Builds an operator table and rejects duplicate or invalid spellings.
    pub fn new(operators: impl IntoIterator<Item = OperatorSpec>) -> Result<Self, String> {
        let mut operators = operators.into_iter().collect::<Vec<_>>();
        for operator in &operators {
            if operator.token.is_empty()
                || !operator.token.chars().all(|character| {
                    character.is_ascii_punctuation()
                        && !matches!(character, '(' | ')' | ',' | '[' | ']' | '.')
                })
                || (operator.prefix_precedence.is_none() && operator.infix.is_none())
            {
                return Err(format!(
                    "operator `{}` must have a punctuation spelling and at least one fixity",
                    operator.token
                ));
            }
        }
        operators.sort_by(|left, right| {
            right
                .token
                .len()
                .cmp(&left.token.len())
                .then_with(|| left.token.cmp(&right.token))
        });
        if operators
            .windows(2)
            .any(|pair| pair[0].token == pair[1].token)
        {
            return Err("expression grammar repeats an operator spelling".to_owned());
        }
        Ok(Self { operators })
    }

    fn operator(&self, token: &str) -> Option<&OperatorSpec> {
        self.operators
            .iter()
            .find(|operator| operator.token == token)
    }
}

/// A literal or identifier whose value is supplied by the host.
#[derive(Clone, Debug, PartialEq)]
pub enum Atom {
    /// Decimal source spelling. The host chooses its numeric representation.
    Number(String),
    /// `true` or `false`.
    Boolean(bool),
    /// Language identifier.
    Name(String),
    /// Host-defined literal or token value.
    Custom {
        /// Host namespace used to interpret the value.
        kind: String,
        /// Exact authored token text.
        text: String,
    },
}

/// A custom atom token returned by a host parser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomAtomToken {
    /// Host namespace used to interpret the value.
    pub kind: String,
    /// Exact authored token text.
    pub text: String,
    /// Absolute byte range consumed in the source file.
    pub span: Span,
}

/// Extension point for literal/token forms outside the built-in atom grammar.
pub trait ExpressionAtomParser {
    /// Parses a custom atom beginning at `offset`, returning `None` when it does not match.
    fn parse_atom(
        &mut self,
        source: &SourceFile,
        range: Span,
        offset: usize,
    ) -> Result<Option<CustomAtomToken>, Diagnostic>;
}

impl<F> ExpressionAtomParser for F
where
    F: FnMut(&SourceFile, Span, usize) -> Result<Option<CustomAtomToken>, Diagnostic>,
{
    fn parse_atom(
        &mut self,
        source: &SourceFile,
        range: Span,
        offset: usize,
    ) -> Result<Option<CustomAtomToken>, Diagnostic> {
        self(source, range, offset)
    }
}

/// Parsed expression nodes with byte ranges into the original source snapshot.
#[derive(Clone, Debug, PartialEq)]
pub enum Expression {
    /// Literal or variable name.
    Atom(Atom, Span),
    /// Prefix operator application.
    Prefix {
        /// Operator spelling.
        operator: String,
        /// Operator token range.
        operator_span: Span,
        /// Operand expression.
        operand: Box<Expression>,
        /// Full expression range.
        span: Span,
    },
    /// Infix operator application.
    Infix {
        /// Left operand.
        left: Box<Expression>,
        /// Operator spelling.
        operator: String,
        /// Operator token range.
        operator_span: Span,
        /// Right operand.
        right: Box<Expression>,
        /// Full expression range.
        span: Span,
    },
    /// Parenthesized expression.
    Group {
        /// Inner expression.
        expression: Box<Expression>,
        /// Parenthesis-inclusive range.
        span: Span,
    },
    /// Function or callable expression with positional arguments.
    Call {
        /// Expression that produces the callable value.
        callee: Box<Expression>,
        /// Arguments in authored evaluation order.
        arguments: Vec<Expression>,
        /// Full range, including the callee and parentheses.
        span: Span,
    },
    /// Named member access.
    Member {
        /// Expression that produces the object or namespace.
        receiver: Box<Expression>,
        /// Authored member name.
        name: String,
        /// Source range of the member name.
        name_span: Span,
        /// Full range, including the receiver and member name.
        span: Span,
    },
    /// Indexed access into a host-defined collection or value.
    Index {
        /// Expression that produces the indexed value.
        receiver: Box<Expression>,
        /// Expression that produces the index or key.
        index: Box<Expression>,
        /// Full range, including the receiver and brackets.
        span: Span,
    },
}

impl Expression {
    /// Returns the source range occupied by this expression.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::Atom(_, span)
            | Self::Group { span, .. }
            | Self::Prefix { span, .. }
            | Self::Infix { span, .. }
            | Self::Call { span, .. }
            | Self::Member { span, .. }
            | Self::Index { span, .. } => *span,
        }
    }
}

/// Parses one expression fragment using a host-provided operator table.
pub fn parse_expression(
    source: &SourceFile,
    range: Span,
    grammar: &ExpressionGrammar,
) -> Result<Expression, Vec<Diagnostic>> {
    parse_expression_with_atoms(source, range, grammar, &mut NoCustomAtoms)
}

/// Parses an expression with additional host-defined atom forms.
pub fn parse_expression_with_atoms<P: ExpressionAtomParser>(
    source: &SourceFile,
    range: Span,
    grammar: &ExpressionGrammar,
    atom_parser: &mut P,
) -> Result<Expression, Vec<Diagnostic>> {
    if range.start > range.end
        || range.end > source.text.len()
        || !source.text.is_char_boundary(range.start)
        || !source.text.is_char_boundary(range.end)
    {
        let offset = range.start.min(source.text.len());
        return Err(vec![Diagnostic::error(
            "LE1001",
            "expression range is not a valid source range",
            Span::new(offset, offset),
        )]);
    }
    let tokens = lex(source, range, grammar, atom_parser)?;
    let mut parser = Parser {
        tokens,
        position: 0,
        grammar,
        diagnostics: Vec::new(),
    };
    let expression = parser.parse_bp(0, 0);
    if !matches!(parser.current().kind, TokenKind::End) {
        parser.diagnostics.push(Diagnostic::error(
            "LE1002",
            "unexpected token after expression",
            parser.current().span,
        ));
    }
    if !parser.diagnostics.is_empty() {
        let diagnostics = parser.diagnostics;
        return Err(diagnostics);
    }
    expression.ok_or_else(|| vec![Diagnostic::error("LE1003", "expected an expression", range)])
}

struct NoCustomAtoms;

impl ExpressionAtomParser for NoCustomAtoms {
    fn parse_atom(
        &mut self,
        _source: &SourceFile,
        _range: Span,
        _offset: usize,
    ) -> Result<Option<CustomAtomToken>, Diagnostic> {
        Ok(None)
    }
}

#[derive(Clone, Debug, PartialEq)]
enum TokenKind {
    Atom(Atom),
    Number(String),
    Boolean(bool),
    Identifier(String),
    Operator(String),
    LeftParen,
    RightParen,
    Comma,
    Dot,
    LeftBracket,
    RightBracket,
    End,
}

#[derive(Clone, Debug)]
struct Token {
    kind: TokenKind,
    span: Span,
}

fn lex<P: ExpressionAtomParser>(
    source: &SourceFile,
    range: Span,
    grammar: &ExpressionGrammar,
    atom_parser: &mut P,
) -> Result<Vec<Token>, Vec<Diagnostic>> {
    let fragment = &source.text[range.start..range.end];
    let mut tokens = Vec::new();
    let mut diagnostics = Vec::new();
    let mut offset = 0;
    while offset < fragment.len() {
        let rest = &fragment[offset..];
        let character = rest.chars().next().expect("offset is a character boundary");
        if character.is_whitespace() {
            offset += character.len_utf8();
            continue;
        }
        let start = offset;
        let absolute_start = range.start + start;
        if let Some(custom_atom) = atom_parser
            .parse_atom(source, range, absolute_start)
            .map_err(|diagnostic| vec![diagnostic])?
        {
            if custom_atom.span.start != absolute_start
                || custom_atom.span.end <= absolute_start
                || custom_atom.span.end > range.end
                || !source.text.is_char_boundary(custom_atom.span.end)
                || custom_atom.kind.is_empty()
                || source
                    .text
                    .get(custom_atom.span.start..custom_atom.span.end)
                    != Some(custom_atom.text.as_str())
            {
                return Err(vec![Diagnostic::error(
                    "LE1007",
                    "custom atom parser returned an invalid token or source range",
                    Span::new(absolute_start, absolute_start),
                )]);
            }
            offset = custom_atom.span.end - range.start;
            tokens.push(Token {
                kind: TokenKind::Atom(Atom::Custom {
                    kind: custom_atom.kind,
                    text: custom_atom.text,
                }),
                span: custom_atom.span,
            });
        } else if character.is_ascii_digit() {
            let mut decimal = false;
            offset += character.len_utf8();
            while let Some(next) = fragment[offset..].chars().next() {
                if next.is_ascii_digit()
                    || (next == '.'
                        && !decimal
                        && fragment[offset + 1..]
                            .chars()
                            .next()
                            .is_none_or(|following| {
                                following.is_ascii_digit()
                                    || (!following.is_alphanumeric() && following != '_')
                            }))
                {
                    decimal |= next == '.';
                    offset += next.len_utf8();
                } else {
                    break;
                }
            }
            tokens.push(Token {
                kind: TokenKind::Number(fragment[start..offset].to_owned()),
                span: Span::new(range.start + start, range.start + offset),
            });
        } else if character == '_' || character.is_alphabetic() {
            offset += character.len_utf8();
            while let Some(next) = fragment[offset..].chars().next() {
                if next == '_' || next.is_alphanumeric() {
                    offset += next.len_utf8();
                } else {
                    break;
                }
            }
            let name = &fragment[start..offset];
            let kind = match name {
                "true" => TokenKind::Boolean(true),
                "false" => TokenKind::Boolean(false),
                _ => TokenKind::Identifier(name.to_owned()),
            };
            tokens.push(Token {
                kind,
                span: Span::new(range.start + start, range.start + offset),
            });
        } else if matches!(character, '(' | ')' | ',' | '.' | '[' | ']') {
            offset += character.len_utf8();
            tokens.push(Token {
                kind: match character {
                    '(' => TokenKind::LeftParen,
                    ')' => TokenKind::RightParen,
                    ',' => TokenKind::Comma,
                    '.' => TokenKind::Dot,
                    '[' => TokenKind::LeftBracket,
                    _ => TokenKind::RightBracket,
                },
                span: Span::new(range.start + start, range.start + offset),
            });
        } else if let Some(operator) = grammar
            .operators
            .iter()
            .find(|operator| rest.starts_with(&operator.token))
        {
            offset += operator.token.len();
            tokens.push(Token {
                kind: TokenKind::Operator(operator.token.clone()),
                span: Span::new(range.start + start, range.start + offset),
            });
        } else {
            offset += character.len_utf8();
            diagnostics.push(Diagnostic::error(
                "LE1004",
                format!("unexpected character `{character}` in expression"),
                Span::new(range.start + start, range.start + offset),
            ));
        }
        if tokens.len() > MAX_EXPRESSION_TOKENS {
            return Err(vec![Diagnostic::error(
                "LE1009",
                format!("expression exceeds the {MAX_EXPRESSION_TOKENS}-token limit"),
                Span::new(range.start + start, range.start + offset),
            )]);
        }
    }
    tokens.push(Token {
        kind: TokenKind::End,
        span: Span::new(range.end, range.end),
    });
    if diagnostics.is_empty() {
        Ok(tokens)
    } else {
        Err(diagnostics)
    }
}

struct Parser<'a> {
    tokens: Vec<Token>,
    position: usize,
    grammar: &'a ExpressionGrammar,
    diagnostics: Vec<Diagnostic>,
}

impl Parser<'_> {
    fn current(&self) -> &Token {
        &self.tokens[self.position.min(self.tokens.len() - 1)]
    }

    fn advance(&mut self) -> Token {
        let token = self.current().clone();
        self.position = (self.position + 1).min(self.tokens.len() - 1);
        token
    }

    fn parse_bp(&mut self, minimum: u8, depth: usize) -> Option<Expression> {
        if depth > MAX_EXPRESSION_NESTING {
            self.diagnostics.push(Diagnostic::error(
                "LE1010",
                format!("expression exceeds the {MAX_EXPRESSION_NESTING}-level nesting limit"),
                self.current().span,
            ));
            return None;
        }
        let mut left = match self.current().kind.clone() {
            TokenKind::Atom(atom) => {
                let token = self.advance();
                Expression::Atom(atom, token.span)
            }
            TokenKind::Number(number) => {
                let token = self.advance();
                Expression::Atom(Atom::Number(number), token.span)
            }
            TokenKind::Boolean(value) => {
                let token = self.advance();
                Expression::Atom(Atom::Boolean(value), token.span)
            }
            TokenKind::Identifier(name) => {
                let token = self.advance();
                Expression::Atom(Atom::Name(name), token.span)
            }
            TokenKind::Operator(operator) => {
                let spec = self.grammar.operator(&operator)?;
                let precedence = spec.prefix_precedence?;
                if precedence < minimum {
                    return None;
                }
                let token = self.advance();
                let operand = self.parse_bp(precedence, depth + 1)?;
                let span = Span::new(token.span.start, operand.span().end);
                Expression::Prefix {
                    operator,
                    operator_span: token.span,
                    operand: Box::new(operand),
                    span,
                }
            }
            TokenKind::LeftParen => {
                let opening = self.advance();
                let expression = self.parse_bp(0, depth + 1)?;
                if !matches!(self.current().kind, TokenKind::RightParen) {
                    self.diagnostics.push(Diagnostic::error(
                        "LE1005",
                        "expected `)` to close this expression",
                        self.current().span,
                    ));
                    return None;
                }
                let closing = self.advance();
                Expression::Group {
                    expression: Box::new(expression),
                    span: Span::new(opening.span.start, closing.span.end),
                }
            }
            _ => {
                self.diagnostics.push(Diagnostic::error(
                    "LE1003",
                    "expected a literal, identifier, prefix operator, or parenthesized expression",
                    self.current().span,
                ));
                return None;
            }
        };
        let mut chain_depth = 0;

        loop {
            if matches!(self.current().kind, TokenKind::LeftParen) {
                if !self.reserve_chain_depth(depth, &mut chain_depth) {
                    return None;
                }
                self.advance();
                let mut arguments = Vec::new();
                if !matches!(self.current().kind, TokenKind::RightParen) {
                    loop {
                        let argument = self.parse_bp(0, depth + chain_depth + 1)?;
                        arguments.push(argument);
                        if matches!(self.current().kind, TokenKind::RightParen) {
                            break;
                        }
                        if !matches!(self.current().kind, TokenKind::Comma) {
                            self.diagnostics.push(Diagnostic::error(
                                "LE1008",
                                "expected `,` or `)` after call argument",
                                self.current().span,
                            ));
                            return None;
                        }
                        self.advance();
                    }
                }
                let closing = self.advance();
                let span = Span::new(left.span().start, closing.span.end);
                left = Expression::Call {
                    callee: Box::new(left),
                    arguments,
                    span,
                };
                continue;
            }
            if matches!(self.current().kind, TokenKind::Dot) {
                if !self.reserve_chain_depth(depth, &mut chain_depth) {
                    return None;
                }
                self.advance();
                let token = self.current().clone();
                let TokenKind::Identifier(name) = token.kind else {
                    self.diagnostics.push(Diagnostic::error(
                        "LE1011",
                        "expected a member name after `.`",
                        token.span,
                    ));
                    return None;
                };
                self.advance();
                let span = Span::new(left.span().start, token.span.end);
                left = Expression::Member {
                    receiver: Box::new(left),
                    name,
                    name_span: token.span,
                    span,
                };
                continue;
            }
            if matches!(self.current().kind, TokenKind::LeftBracket) {
                if !self.reserve_chain_depth(depth, &mut chain_depth) {
                    return None;
                }
                self.advance();
                let index = self.parse_bp(0, depth + chain_depth + 1)?;
                if !matches!(self.current().kind, TokenKind::RightBracket) {
                    self.diagnostics.push(Diagnostic::error(
                        "LE1012",
                        "expected `]` to close indexed access",
                        self.current().span,
                    ));
                    return None;
                }
                let closing = self.advance();
                let span = Span::new(left.span().start, closing.span.end);
                left = Expression::Index {
                    receiver: Box::new(left),
                    index: Box::new(index),
                    span,
                };
                continue;
            }
            let TokenKind::Operator(operator) = self.current().kind.clone() else {
                break;
            };
            let Some((precedence, associativity)) =
                self.grammar.operator(&operator).and_then(|spec| spec.infix)
            else {
                break;
            };
            if precedence < minimum {
                break;
            }
            if !self.reserve_chain_depth(depth, &mut chain_depth) {
                return None;
            }
            let operator_token = self.advance();
            let right_minimum = match associativity {
                Associativity::Left => precedence.saturating_add(1),
                Associativity::Right => precedence,
            };
            let Some(right) = self.parse_bp(right_minimum, depth + chain_depth + 1) else {
                self.diagnostics.push(Diagnostic::error(
                    "LE1006",
                    format!("expected an expression after `{operator}`"),
                    self.current().span,
                ));
                return None;
            };
            let span = Span::new(left.span().start, right.span().end);
            left = Expression::Infix {
                left: Box::new(left),
                operator,
                operator_span: operator_token.span,
                right: Box::new(right),
                span,
            };
        }
        Some(left)
    }

    fn reserve_chain_depth(&mut self, depth: usize, chain_depth: &mut usize) -> bool {
        *chain_depth += 1;
        if depth + *chain_depth <= MAX_EXPRESSION_NESTING {
            return true;
        }
        self.diagnostics.push(Diagnostic::error(
            "LE1010",
            format!("expression exceeds the {MAX_EXPRESSION_NESTING}-level nesting limit"),
            self.current().span,
        ));
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Type {
        Number,
        Boolean,
    }

    struct Rules;

    impl ExpressionTypeRules for Rules {
        type Type = Type;

        fn atom(&mut self, atom: &Atom, span: Span) -> Result<Self::Type, String> {
            match atom {
                Atom::Number(value) if value.parse::<f64>().is_ok_and(f64::is_finite) => {
                    Ok(Type::Number)
                }
                Atom::Number(_) => Err("numeric literal is not finite".to_owned()),
                Atom::Boolean(_) => Ok(Type::Boolean),
                Atom::Name(name) => Err(format!("unknown variable `{name}` at {span:?}")),
                Atom::Custom { kind, .. } => Err(format!("unknown custom atom `{kind}`")),
            }
        }

        fn prefix(
            &mut self,
            operator: &str,
            _operator_span: Span,
            operand: &Self::Type,
        ) -> Result<Self::Type, String> {
            match (operator, operand) {
                ("-", Type::Number) => Ok(Type::Number),
                ("!", Type::Boolean) => Ok(Type::Boolean),
                _ => Err(format!("operator `{operator}` does not accept {operand:?}")),
            }
        }

        fn infix(
            &mut self,
            operator: &str,
            _operator_span: Span,
            left: &Self::Type,
            right: &Self::Type,
        ) -> Result<Self::Type, String> {
            match (operator, left, right) {
                ("+" | "-" | "*", Type::Number, Type::Number) => Ok(Type::Number),
                ("**", Type::Number, Type::Number) => Ok(Type::Number),
                ("==", left, right) if left == right => Ok(Type::Boolean),
                ("&&", Type::Boolean, Type::Boolean) => Ok(Type::Boolean),
                _ => Err(format!(
                    "operator `{operator}` does not accept {left:?} and {right:?}"
                )),
            }
        }
    }

    fn grammar() -> ExpressionGrammar {
        ExpressionGrammar::new([
            OperatorSpec::prefix_infix("-", 30, 10, Associativity::Left),
            OperatorSpec::prefix("!", 30),
            OperatorSpec::infix("+", 10),
            OperatorSpec::infix("*", 20),
            OperatorSpec::infix_with("**", 25, Associativity::Right),
            OperatorSpec::infix("==", 5),
            OperatorSpec::infix("&&", 3),
        ])
        .expect("test operator table is valid")
    }

    #[test]
    fn longest_operator_match_and_right_associativity_are_configurable() {
        let text = "2 ** 3 ** 2 + 1";
        let source = SourceFile::new("power.expr", text);
        let expression = parse_expression(&source, Span::new(0, text.len()), &grammar())
            .expect("custom operators parse");
        let Expression::Infix { left, operator, .. } = &expression else {
            panic!("addition is the lowest-precedence root");
        };
        assert_eq!(operator, "+");
        let Expression::Infix {
            right, operator, ..
        } = left.as_ref()
        else {
            panic!("power expression is nested on the left");
        };
        assert_eq!(operator, "**");
        assert!(matches!(right.as_ref(), Expression::Infix { operator, .. } if operator == "**"));
    }

    #[test]
    fn host_rules_define_types_and_operator_meanings() {
        let text = "!(enabled) && 2 ** 3 == 8";
        let source = SourceFile::new("policy.expr", text);
        let expression = parse_expression(&source, Span::new(0, text.len()), &grammar())
            .expect("expression parses independently of its semantics");
        let mut rules = Rules;
        let error = infer_expression_type(&expression, &mut rules)
            .expect_err("host variable environment rejects an undeclared name");
        assert_eq!(error[0].code, "LE2001");
        assert!(error[0].span.is_some());
    }

    #[test]
    fn operator_definition_errors_are_reported_before_source_parsing() {
        let error =
            ExpressionGrammar::new([OperatorSpec::infix("==", 1), OperatorSpec::prefix("==", 2)])
                .expect_err("duplicate token spellings are invalid");
        assert!(error.contains("repeats"));
    }

    #[test]
    fn empty_operator_table_still_parses_atoms() {
        let grammar = ExpressionGrammar::new([]).expect("atom-only grammars are useful");
        let source = SourceFile::new("atom.expr", "answer");
        let expression = parse_expression(&source, Span::new(0, 6), &grammar)
            .expect("an identifier does not require operator declarations");
        assert!(matches!(
            expression,
            Expression::Atom(Atom::Name(name), _) if name == "answer"
        ));
    }

    #[test]
    fn custom_atom_parser_ranges_and_text_must_match_the_source() {
        let grammar = ExpressionGrammar::new([]).expect("atom-only grammar is valid");
        let source = SourceFile::new("custom.expr", "@value");
        let mut malformed_atom = |_: &SourceFile, _: Span, offset: usize| {
            Ok(Some(CustomAtomToken {
                kind: "identifier".to_owned(),
                text: "different".to_owned(),
                span: Span::new(offset, offset + 1),
            }))
        };
        let diagnostics = parse_expression_with_atoms(
            &source,
            Span::new(0, source.text.len()),
            &grammar,
            &mut malformed_atom,
        )
        .expect_err("host tokens cannot invent source text or ranges");
        assert_eq!(diagnostics[0].code, "LE1007");
    }

    #[test]
    fn nested_calls_delegate_static_types_and_execution_to_the_host() {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum HostType {
            Callable,
            Number,
        }

        struct HostRules;

        impl ExpressionTypeRules for HostRules {
            type Type = HostType;

            fn atom(&mut self, atom: &Atom, _span: Span) -> Result<Self::Type, String> {
                match atom {
                    Atom::Number(_) => Ok(HostType::Number),
                    Atom::Name(name) if name == "double" => Ok(HostType::Callable),
                    _ => Err("unknown name or literal".to_owned()),
                }
            }

            fn prefix(
                &mut self,
                _operator: &str,
                _operator_span: Span,
                _operand: &Self::Type,
            ) -> Result<Self::Type, String> {
                Err("prefix operators are not supported".to_owned())
            }

            fn infix(
                &mut self,
                operator: &str,
                _operator_span: Span,
                left: &Self::Type,
                right: &Self::Type,
            ) -> Result<Self::Type, String> {
                match (operator, left, right) {
                    ("+", HostType::Number, HostType::Number) => Ok(HostType::Number),
                    _ => Err("expected numeric operands".to_owned()),
                }
            }

            fn call(
                &mut self,
                callee: &Self::Type,
                arguments: &[Self::Type],
                _call_span: Span,
            ) -> Result<Self::Type, String> {
                match (callee, arguments) {
                    (HostType::Callable, [HostType::Number]) => Ok(HostType::Number),
                    _ => Err("double expects one numeric argument".to_owned()),
                }
            }
        }

        #[derive(Clone, Debug, PartialEq)]
        enum HostValue {
            Double,
            Number(f64),
        }

        struct HostRuntime;

        impl ExpressionRuntime for HostRuntime {
            type Value = HostValue;

            fn atom(&mut self, atom: &Atom, _span: Span) -> Result<Self::Value, String> {
                match atom {
                    Atom::Number(value) => value
                        .parse()
                        .map(HostValue::Number)
                        .map_err(|_| "invalid number".to_owned()),
                    Atom::Name(name) if name == "double" => Ok(HostValue::Double),
                    _ => Err("unknown name or literal".to_owned()),
                }
            }

            fn prefix(
                &mut self,
                _operator: &str,
                _operator_span: Span,
                _operand: &Self::Value,
            ) -> Result<Self::Value, String> {
                Err("prefix operators are not supported".to_owned())
            }

            fn infix(
                &mut self,
                operator: &str,
                _operator_span: Span,
                left: &Self::Value,
                right: &Self::Value,
            ) -> Result<Self::Value, String> {
                match (operator, left, right) {
                    ("+", HostValue::Number(left), HostValue::Number(right)) => {
                        Ok(HostValue::Number(left + right))
                    }
                    _ => Err("expected numeric operands".to_owned()),
                }
            }

            fn call(
                &mut self,
                callee: &Self::Value,
                arguments: &[Self::Value],
                _call_span: Span,
            ) -> Result<Self::Value, String> {
                match (callee, arguments) {
                    (HostValue::Double, [HostValue::Number(value)]) => {
                        Ok(HostValue::Number(value * 2.0))
                    }
                    _ => Err("double expects one numeric argument".to_owned()),
                }
            }
        }

        let grammar =
            ExpressionGrammar::new([OperatorSpec::infix("+", 10)]).expect("valid grammar");
        let text = "double(2 + double(1))";
        let source = SourceFile::new("nested-calls.expr", text);
        let expression = parse_expression(&source, Span::new(0, text.len()), &grammar)
            .expect("nested calls parse");
        assert_eq!(
            infer_expression_type(&expression, &mut HostRules).expect("host types calls"),
            HostType::Number
        );
        assert_eq!(
            evaluate_expression(&expression, &mut HostRuntime).expect("host runs calls"),
            HostValue::Number(8.0)
        );

        let text = "double(1 2)";
        let source = SourceFile::new("invalid-call.expr", text);
        let diagnostics = parse_expression(&source, Span::new(0, text.len()), &grammar)
            .expect_err("arguments must be separated");
        assert_eq!(diagnostics[0].code, "LE1008");
    }

    #[test]
    fn expression_token_and_nesting_budgets_return_diagnostics() {
        let grammar = ExpressionGrammar::new([]).expect("atom-only grammar is valid");

        let text = format!(
            "{}1{}",
            "(".repeat(MAX_EXPRESSION_NESTING + 1),
            ")".repeat(MAX_EXPRESSION_NESTING + 1)
        );
        let source = SourceFile::new("deep.expr", text);
        let diagnostic = parse_expression(&source, Span::new(0, source.text.len()), &grammar)
            .expect_err("deep input must stop at the explicit nesting limit");
        assert!(diagnostic.iter().any(|item| item.code == "LE1010"));

        let grammar =
            ExpressionGrammar::new([OperatorSpec::infix("+", 1)]).expect("plus grammar is valid");
        let text = "1+".repeat(MAX_EXPRESSION_TOKENS / 2 + 1);
        let source = SourceFile::new("large.expr", text);
        let diagnostic = parse_expression(&source, Span::new(0, source.text.len()), &grammar)
            .expect_err("large input must stop at the explicit token limit");
        assert!(diagnostic.iter().any(|item| item.code == "LE1009"));

        let grammar = ExpressionGrammar::new([]).expect("postfix-only grammar is valid");
        let text = format!("x{}", ".field".repeat(MAX_EXPRESSION_NESTING + 1));
        let source = SourceFile::new("deep-postfix.expr", text);
        let diagnostic = parse_expression(&source, Span::new(0, source.text.len()), &grammar)
            .expect_err("a long postfix chain must fit the same AST depth budget");
        assert!(diagnostic.iter().any(|item| item.code == "LE1010"));

        let mut expression = Expression::Atom(Atom::Number("1".to_owned()), Span::new(0, 1));
        for _ in 0..=MAX_EXPRESSION_NESTING {
            expression = Expression::Group {
                expression: Box::new(expression),
                span: Span::new(0, 1),
            };
        }
        let diagnostic = infer_expression_type(&expression, &mut Rules)
            .expect_err("host-built ASTs obey the same recursion budget");
        assert_eq!(diagnostic[0].code, "LE2001");
    }

    #[test]
    fn member_and_index_operations_use_host_types_and_runtime_values() {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum HostType {
            User,
            Places,
            Place,
            Text,
            Number,
        }

        struct HostRules;

        impl ExpressionTypeRules for HostRules {
            type Type = HostType;

            fn atom(&mut self, atom: &Atom, _span: Span) -> Result<Self::Type, String> {
                match atom {
                    Atom::Name(name) if name == "user" => Ok(HostType::User),
                    Atom::Number(_) => Ok(HostType::Number),
                    _ => Err("unknown value".to_owned()),
                }
            }

            fn prefix(
                &mut self,
                _operator: &str,
                _operator_span: Span,
                _operand: &Self::Type,
            ) -> Result<Self::Type, String> {
                Err("prefix operators are not defined".to_owned())
            }

            fn infix(
                &mut self,
                _operator: &str,
                _operator_span: Span,
                _left: &Self::Type,
                _right: &Self::Type,
            ) -> Result<Self::Type, String> {
                Err("infix operators are not defined".to_owned())
            }

            fn member(
                &mut self,
                receiver: &Self::Type,
                name: &str,
                _member_span: Span,
            ) -> Result<Self::Type, String> {
                match (receiver, name) {
                    (HostType::User, "places") => Ok(HostType::Places),
                    (HostType::Place, "city") => Ok(HostType::Text),
                    _ => Err(format!("{receiver:?} has no member `{name}`")),
                }
            }

            fn index(
                &mut self,
                receiver: &Self::Type,
                index: &Self::Type,
                _index_span: Span,
            ) -> Result<Self::Type, String> {
                match (receiver, index) {
                    (HostType::Places, HostType::Number) => Ok(HostType::Place),
                    _ => Err("this value cannot be indexed that way".to_owned()),
                }
            }
        }

        #[derive(Clone, Debug, PartialEq)]
        enum HostValue {
            User,
            Places,
            Place,
            Text(&'static str),
            Number(f64),
        }

        struct HostRuntime;

        impl ExpressionRuntime for HostRuntime {
            type Value = HostValue;

            fn atom(&mut self, atom: &Atom, _span: Span) -> Result<Self::Value, String> {
                match atom {
                    Atom::Name(name) if name == "user" => Ok(HostValue::User),
                    Atom::Number(value) => value
                        .parse::<f64>()
                        .map(HostValue::Number)
                        .map_err(|_| "invalid number".to_owned()),
                    _ => Err("unknown value".to_owned()),
                }
            }

            fn prefix(
                &mut self,
                _operator: &str,
                _operator_span: Span,
                _operand: &Self::Value,
            ) -> Result<Self::Value, String> {
                Err("prefix operators are not defined".to_owned())
            }

            fn infix(
                &mut self,
                _operator: &str,
                _operator_span: Span,
                _left: &Self::Value,
                _right: &Self::Value,
            ) -> Result<Self::Value, String> {
                Err("infix operators are not defined".to_owned())
            }

            fn member(
                &mut self,
                receiver: &Self::Value,
                name: &str,
                _member_span: Span,
            ) -> Result<Self::Value, String> {
                match (receiver, name) {
                    (HostValue::User, "places") => Ok(HostValue::Places),
                    (HostValue::Place, "city") => Ok(HostValue::Text("Rabat")),
                    _ => Err(format!("{receiver:?} has no member `{name}`")),
                }
            }

            fn index(
                &mut self,
                receiver: &Self::Value,
                index: &Self::Value,
                _index_span: Span,
            ) -> Result<Self::Value, String> {
                match (receiver, index) {
                    (HostValue::Places, HostValue::Number(0.0)) => Ok(HostValue::Place),
                    _ => Err("this value cannot be indexed that way".to_owned()),
                }
            }
        }

        let grammar = ExpressionGrammar::new([]).expect("postfix-only grammar is valid");
        let text = "user.places[0].city";
        let source = SourceFile::new("member-index.expr", text);
        let expression = parse_expression(&source, Span::new(0, text.len()), &grammar)
            .expect("member and indexed access parse");
        assert!(matches!(
            &expression,
            Expression::Member { name, name_span, .. }
                if name == "city" && source.text.get(name_span.start..name_span.end) == Some("city")
        ));
        assert_eq!(
            infer_expression_type(&expression, &mut HostRules).expect("host types access"),
            HostType::Text
        );
        assert_eq!(
            evaluate_expression(&expression, &mut HostRuntime).expect("host evaluates access"),
            HostValue::Text("Rabat")
        );

        for (text, expected_code) in [("user.", "LE1011"), ("user.places[0", "LE1012")] {
            let source = SourceFile::new("incomplete.expr", text);
            let diagnostics = parse_expression(&source, Span::new(0, text.len()), &grammar)
                .expect_err("incomplete postfix syntax must be diagnosed");
            assert_eq!(diagnostics[0].code, expected_code);
        }
    }
}
