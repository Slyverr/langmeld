//! Source-spanned Pratt parser for common expression forms.

use langmeld_core::{Diagnostic, SourceFile, Span};

use crate::ExpressionGrammar;

mod engine;
mod lexer;

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
        engine::parse(source, range, self.grammar, self.limits, terminators)
    }
}
