//! Public contracts for host-provided lexical analysis.

use std::sync::Arc;

use langmeld_core::{Diagnostic, SourceFile, Span};

/// Lexical category understood by Langmeld's generic parser and editor model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LanguageTokenKind {
    /// A name or keyword. Keyword status can be declared separately in the language spec.
    Identifier(String),
    /// A numeric token in the host lexer's chosen representation.
    Number(String),
    /// A decoded string or other text value.
    Text(String),
    /// One punctuation character.
    Punctuation(char),
    /// A host-recognized arrow token.
    Arrow,
    /// A source character the host lexer does not classify.
    Unknown(char),
    /// A comment token.
    Comment,
    /// A line break token.
    Newline,
    /// Reserved for Langmeld's end-of-input sentinel; custom lexers must omit it.
    End,
}

/// One source-spanned token emitted by a host lexer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguageToken {
    /// The token's shared Langmeld category and value.
    pub kind: LanguageTokenKind,
    /// UTF-8 byte range in the original source.
    pub span: Span,
}

impl LanguageToken {
    /// Creates a token from its category and source range.
    #[must_use]
    pub const fn new(kind: LanguageTokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}

/// The lexical output returned by a custom lexer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LexedSource {
    /// Significant tokens in source order. Whitespace may be omitted; Langmeld reconstructs it.
    pub tokens: Vec<LanguageToken>,
    /// Recoverable lexical diagnostics with spans in the original source.
    pub diagnostics: Vec<Diagnostic>,
}

impl LexedSource {
    /// Creates lexical output from tokens and diagnostics.
    #[must_use]
    pub fn new(
        tokens: impl IntoIterator<Item = LanguageToken>,
        diagnostics: impl IntoIterator<Item = Diagnostic>,
    ) -> Self {
        Self {
            tokens: tokens.into_iter().collect(),
            diagnostics: diagnostics.into_iter().collect(),
        }
    }
}

/// Tokenizes source with Langmeld's built-in lexer and default token limit.
///
/// Custom lexers can use this as a starting point, then rewrite token kinds or
/// spans for language-specific lexical rules. The returned stream omits the
/// end-of-input sentinel because the parser adds it after validating host tokens.
#[must_use]
pub fn lex_source(source: &SourceFile) -> LexedSource {
    let (mut tokens, diagnostics, _) =
        crate::lexing::lex(source, crate::ParseLimits::default().max_tokens);
    if matches!(
        tokens.last().map(|token| &token.kind),
        Some(LanguageTokenKind::End)
    ) {
        tokens.pop();
    }
    LexedSource::new(tokens, diagnostics)
}

/// Host lexer callback. It sees the untouched source and returns source-spanned tokens.
pub type CustomLexerFn = dyn Fn(&SourceFile) -> LexedSource + Send + Sync;

/// Replaces Langmeld's built-in lexer for one configured language parser.
#[derive(Clone)]
pub struct CustomLexer {
    /// Host implementation for tokenizing a source snapshot.
    pub lex: Arc<CustomLexerFn>,
}

impl CustomLexer {
    /// Creates a custom lexer from a function or configured closure.
    #[must_use]
    pub fn new(lex: impl Fn(&SourceFile) -> LexedSource + Send + Sync + 'static) -> Self {
        Self { lex: Arc::new(lex) }
    }
}

impl std::fmt::Debug for CustomLexer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CustomLexer")
            .finish_non_exhaustive()
    }
}
