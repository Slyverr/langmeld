//! Recovering parser for token-pattern constructs and delimited blocks.

use langmeld_core::{Diagnostic, SourceFile, SourceIdentity, has_errors};

use crate::LanguageSpec;
use crate::lexer::{ConfiguredLexer, Lexer};

mod engine;
pub(crate) mod model;
mod pattern;
use model::ParseContext;
pub use model::{
    CaptureGroup, CaptureGroupKind, CaptureGroupOccurrence, CapturedField, CapturedValue,
    ParsedConstruct,
};

/// Parsed constructs and recoverable source diagnostics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseResult {
    /// Identity and revision of the parsed source snapshot.
    source: SourceIdentity,
    /// Successfully recognized constructs in source order.
    constructs: Vec<ParsedConstruct>,
    /// Lossless source structure for editor integrations and other tools.
    syntax_tree: crate::SyntaxTree,
    /// Lexical and syntax errors. Parsing continues after recoverable errors
    /// unless a resource limit stops it.
    diagnostics: Vec<Diagnostic>,
    /// Whether a configured parser resource limit truncated this result.
    resource_limited: bool,
}

/// Owned components of a consumed [`ParseResult`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseResultParts {
    /// Identity and revision of the parsed source snapshot.
    pub source_identity: SourceIdentity,
    /// Successfully recognized constructs in source order.
    pub constructs: Vec<ParsedConstruct>,
    /// Lossless source structure for editor integrations and other tools.
    pub syntax_tree: crate::SyntaxTree,
    /// Lexical and syntax errors, including recoverable findings.
    pub diagnostics: Vec<Diagnostic>,
    /// Whether a configured parser resource limit truncated this result.
    pub resource_limited: bool,
}

impl ParseResult {
    /// Returns the identity and revision of the parsed source snapshot.
    #[must_use]
    pub const fn source_identity(&self) -> SourceIdentity {
        self.source
    }

    /// Returns the successfully recognized constructs in source order.
    #[must_use]
    pub fn constructs(&self) -> &[ParsedConstruct] {
        &self.constructs
    }

    /// Returns whether `construct` is borrowed from this parse result.
    #[must_use]
    pub fn contains_construct(&self, target: &ParsedConstruct) -> bool {
        contains_construct(&self.constructs, target)
    }

    /// Returns the lossless syntax tree for editor integrations and other tools.
    #[must_use]
    pub const fn syntax_tree(&self) -> &crate::SyntaxTree {
        &self.syntax_tree
    }

    /// Returns lexical and syntax errors, including recoverable findings.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Returns whether a configured parser resource limit truncated this result.
    #[must_use]
    pub const fn resource_limited(&self) -> bool {
        self.resource_limited
    }

    /// Consumes the result and returns its source identity and owned components.
    #[must_use]
    pub fn into_parts(self) -> ParseResultParts {
        ParseResultParts {
            source_identity: self.source,
            constructs: self.constructs,
            syntax_tree: self.syntax_tree,
            diagnostics: self.diagnostics,
            resource_limited: self.resource_limited,
        }
    }

    /// Returns whether parsing completed without errors or resource limits.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        !self.resource_limited && !has_errors(&self.diagnostics)
    }
}

fn contains_construct(constructs: &[ParsedConstruct], target: &ParsedConstruct) -> bool {
    constructs.iter().any(|construct| {
        std::ptr::eq(construct, target) || contains_construct(&construct.children, target)
    })
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
/// Lexer diagnostics with missing, foreign, or invalid source locations are
/// suppressed and reported as `LM-LEX-004`.
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
