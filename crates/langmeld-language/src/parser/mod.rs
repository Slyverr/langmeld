//! Recovering parser for token-pattern constructs and delimited blocks.

use langmeld_core::{Diagnostic, SourceFile, SourceIdentity, Span, has_errors};

use crate::LanguageSpec;
use crate::completion::ParseContext;
use crate::lexer::{ConfiguredLexer, Lexer, TokenKind};

mod engine;
mod pattern;

/// A field value captured by a syntax pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapturedValue {
    /// Text and lexical category captured without narrowing its token type.
    Token {
        /// Exact source text of the token.
        text: String,
        /// Lexical category provided by the lexer.
        kind: TokenKind,
    },
    /// Identifier text without transformation.
    Identifier(String),
    /// Decimal integer text preserved as written.
    Integer(String),
    /// Quoted string text preserved as written.
    String(String),
    /// Expression source captured before a top-level pattern terminator.
    ///
    /// The corresponding [`CapturedField::span`] identifies the exact source
    /// range. Use that range with an expression parser chosen by the host.
    Expression,
}

/// A named source value captured by a construct.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedField {
    /// Field name from the language specification.
    pub name: String,
    /// Captured lexical value.
    pub value: CapturedValue,
    /// Source range of the captured value, excluding surrounding syntax.
    pub span: Span,
}

/// One construct recognized in the source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedConstruct {
    /// Name of the matching construct specification.
    pub name: String,
    /// Zero-based index of the matching syntax alternative.
    pub variant: usize,
    /// Complete source range consumed by the construct.
    pub span: Span,
    /// Values captured by named fields.
    pub fields: Vec<CapturedField>,
    /// Child constructs parsed from this construct's delimited block.
    pub children: Vec<ParsedConstruct>,
}

/// Parsed constructs and recoverable source diagnostics.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParseResult {
    /// Identity and revision of the parsed snapshot, if database-managed.
    source: Option<SourceIdentity>,
    /// Successfully recognized constructs in source order.
    pub constructs: Vec<ParsedConstruct>,
    /// Lossless source structure for editor integrations and other tools.
    pub syntax_tree: crate::SyntaxTree,
    /// Lexical and syntax errors. Parsing continues after recoverable errors
    /// unless a resource limit stops it.
    pub diagnostics: Vec<Diagnostic>,
    /// Whether a configured parser resource limit truncated this result.
    pub resource_limited: bool,
}

impl ParseResult {
    /// Returns the identity and revision of the parsed source snapshot.
    #[must_use]
    pub const fn source_identity(&self) -> Option<SourceIdentity> {
        self.source
    }

    /// Returns whether parsing completed without errors or resource limits.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        !self.resource_limited && !has_errors(&self.diagnostics)
    }
}

/// Input and work bounds for the generic token-pattern parser.
///
/// These limits do not constrain custom [`LanguageParser`](crate::LanguageParser)
/// implementations; host parsers own their parsing and resource policies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseLimits {
    /// Maximum UTF-8 source size in bytes.
    pub max_source_bytes: usize,
    /// Maximum number of tokens emitted by the lexer.
    pub max_tokens: usize,
    /// Maximum number of parsed construct nodes.
    pub max_constructs: usize,
    /// Maximum nested block depth. Values above [`MAX_NESTING_DEPTH`] are capped.
    pub max_nesting_depth: usize,
    /// Maximum pattern parts evaluated across all candidates and groups per parse.
    pub max_pattern_steps: usize,
}

impl Default for ParseLimits {
    fn default() -> Self {
        Self {
            max_source_bytes: 4 * 1024 * 1024,
            max_tokens: crate::lexer::DEFAULT_MAX_TOKENS,
            max_constructs: 250_000,
            max_nesting_depth: MAX_NESTING_DEPTH,
            max_pattern_steps: 1_000_000,
        }
    }
}

/// Maximum nesting level accepted by the generic block parser.
pub const MAX_NESTING_DEPTH: usize = 128;

/// Parses source using a validated token-pattern language specification.
///
/// The generic parser supports token patterns, typed and generic captures,
/// and nested blocks whose child constructs are explicitly listed in the
/// grammar. Use a custom lexer for different tokenization, or implement
/// [`LanguageParser`](crate::LanguageParser) to own the syntax and tree shape.
#[must_use]
pub fn parse(source: &SourceFile, language: &LanguageSpec) -> ParseResult {
    parse_with_limits(source, language, ParseLimits::default())
}

/// Parses source using the built-in lexer and caller-selected resource limits.
#[must_use]
pub fn parse_with_limits(
    source: &SourceFile,
    language: &LanguageSpec,
    limits: ParseLimits,
) -> ParseResult {
    let lexer = ConfiguredLexer {
        rules: language.lexical_rules(),
    };
    parse_with_lexer_and_limits(source, language, &lexer, limits)
}

/// Parses a source snapshot using a host-provided lexer and Langmeld grammar.
///
/// Tokens with invalid, overlapping, out-of-order, or mismatched source spans
/// are discarded and reported as `LM-LEX-002` diagnostics before parsing.
#[must_use]
pub fn parse_with_lexer(
    source: &SourceFile,
    language: &LanguageSpec,
    lexer: &impl Lexer,
) -> ParseResult {
    parse_with_lexer_and_limits(source, language, lexer, ParseLimits::default())
}

/// Parses with a host lexer and caller-selected generic-parser resource limits.
///
/// Source bytes are checked before invoking the lexer. A custom lexer may
/// override [`Lexer::lex_with_token_limit`] to stop early; otherwise an
/// oversized token list is discarded before generic validation or parsing.
#[must_use]
pub fn parse_with_lexer_and_limits(
    source: &SourceFile,
    language: &LanguageSpec,
    lexer: &impl Lexer,
    limits: ParseLimits,
) -> ParseResult {
    engine::parse_with_lexer_internal(source, language, lexer, limits, false).0
}

pub(crate) fn parse_with_contexts(
    source: &SourceFile,
    language: &LanguageSpec,
    lexer: &impl Lexer,
) -> (ParseResult, Vec<ParseContext>) {
    engine::parse_with_lexer_internal(source, language, lexer, ParseLimits::default(), true)
}
