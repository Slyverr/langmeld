//! Tokenization contracts and the default Langmeld lexer.

use langmeld_core::{Diagnostic, SourceFile, Span};

use crate::LexicalRules;

pub(crate) const DEFAULT_MAX_TOKENS: usize = 250_000;

/// A token emitted by a language lexer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LexedToken {
    /// Exact token text from the source snapshot.
    pub text: String,
    /// Lexical category used by declarative field patterns.
    pub kind: TokenKind,
    /// Byte range of `text` in the source snapshot.
    pub span: Span,
}

/// Lexical category assigned to a token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    /// Identifier-like token.
    Identifier,
    /// Decimal integer token.
    Integer,
    /// Quoted or otherwise host-defined string token.
    String,
    /// Operator or punctuation token.
    Punctuation,
    /// Invalid token retained for recovery and never accepted as a field.
    Invalid,
}

/// Tokens and lexical diagnostics produced for one source snapshot.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LexOutput {
    /// Non-overlapping tokens in source order. Whitespace and comments may be omitted.
    pub tokens: Vec<LexedToken>,
    /// Source-located lexical diagnostics.
    pub diagnostics: Vec<Diagnostic>,
}

/// Host-provided lexical behavior for the generic Langmeld parser.
pub trait Lexer {
    /// Lexes the supplied immutable source snapshot.
    fn lex(&self, source: &SourceFile) -> LexOutput;

    /// Lexes with a token limit. The default applies the limit after [`Self::lex`]
    /// returns, so it rejects oversized output but cannot bound the lexer's own
    /// work or allocations. Implementations may override this method to stop
    /// early and retain a valid prefix. An override that truncates must report
    /// `LM-PARSE-008` so the parse result records that it is resource-limited.
    fn lex_with_token_limit(&self, source: &SourceFile, max_tokens: usize) -> LexOutput {
        limit_lex_output(source, self.lex(source), max_tokens)
    }
}

/// Lexes the initial built-in token subset used by [`crate::parse`].
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultLexer;

impl Lexer for DefaultLexer {
    fn lex(&self, source: &SourceFile) -> LexOutput {
        lex_default(source, DEFAULT_MAX_TOKENS, &LexicalRules::default())
    }

    fn lex_with_token_limit(&self, source: &SourceFile, max_tokens: usize) -> LexOutput {
        lex_default(source, max_tokens, &LexicalRules::default())
    }
}

pub(crate) struct ConfiguredLexer<'a> {
    pub(crate) rules: &'a LexicalRules,
}

impl Lexer for ConfiguredLexer<'_> {
    fn lex(&self, source: &SourceFile) -> LexOutput {
        lex_default(source, DEFAULT_MAX_TOKENS, self.rules)
    }

    fn lex_with_token_limit(&self, source: &SourceFile, max_tokens: usize) -> LexOutput {
        lex_default(source, max_tokens, self.rules)
    }
}

fn lex_default(source: &SourceFile, max_tokens: usize, rules: &LexicalRules) -> LexOutput {
    let mut tokens = Vec::new();
    let mut diagnostics = Vec::new();
    let mut chars = source.text.char_indices().peekable();
    while let Some((start, character)) = chars.next() {
        if character.is_whitespace() {
            continue;
        }
        if let Some((opening, closing)) = comment_start(&source.text[start..], rules) {
            let content_start = start + opening.len();
            if let Some(closing) = closing {
                if let Some(relative_end) = source.text[content_start..].find(closing) {
                    let end = content_start + relative_end + closing.len();
                    while chars.peek().is_some_and(|(byte, _)| *byte < end) {
                        chars.next();
                    }
                } else {
                    let span = Span::new(start, source.text.len());
                    diagnostics.push(Diagnostic::error(
                        "LM-LEX-003",
                        format!("unterminated block comment starting with `{opening}`"),
                        source,
                        span,
                    ));
                    break;
                }
            } else {
                while chars
                    .peek()
                    .is_some_and(|(_, next)| *next != '\n' && *next != '\r')
                {
                    chars.next();
                }
            }
            continue;
        }
        if tokens.len() == max_tokens {
            diagnostics.push(Diagnostic::error(
                "LM-PARSE-008",
                format!("token count exceeds the configured limit of {max_tokens}"),
                source,
                Span::new(start, start + character.len_utf8()),
            ));
            break;
        }
        if character == '"' {
            let mut escaped = false;
            let mut end = source.text.len();
            let mut closed = false;
            for (byte, next) in chars.by_ref() {
                end = byte + next.len_utf8();
                if escaped {
                    escaped = false;
                } else if next == '\\' {
                    escaped = true;
                } else if next == '"' {
                    closed = true;
                    break;
                }
            }
            let span = Span::new(start, end);
            tokens.push(LexedToken {
                text: source.text[start..end].to_owned(),
                kind: if closed {
                    TokenKind::String
                } else {
                    TokenKind::Invalid
                },
                span,
            });
            if !closed {
                diagnostics.push(Diagnostic::error(
                    "LM-LEX-001",
                    "unterminated string literal",
                    source,
                    span,
                ));
            }
            continue;
        }
        if character.is_alphabetic() || character == '_' {
            let mut end = start + character.len_utf8();
            while let Some(&(byte, next)) = chars.peek() {
                if next.is_alphanumeric() || next == '_' {
                    chars.next();
                    end = byte + next.len_utf8();
                } else {
                    break;
                }
            }
            tokens.push(LexedToken {
                text: source.text[start..end].to_owned(),
                kind: TokenKind::Identifier,
                span: Span::new(start, end),
            });
            continue;
        }
        if character.is_ascii_digit() {
            let mut end = start + character.len_utf8();
            while let Some(&(byte, next)) = chars.peek() {
                if next.is_ascii_digit() {
                    chars.next();
                    end = byte + next.len_utf8();
                } else {
                    break;
                }
            }
            tokens.push(LexedToken {
                text: source.text[start..end].to_owned(),
                kind: TokenKind::Integer,
                span: Span::new(start, end),
            });
            continue;
        }
        if is_grouped_punctuation(character) {
            let mut end = start + character.len_utf8();
            while let Some(&(byte, next)) = chars.peek() {
                if is_grouped_punctuation(next) {
                    chars.next();
                    end = byte + next.len_utf8();
                } else {
                    break;
                }
            }
            tokens.push(LexedToken {
                text: source.text[start..end].to_owned(),
                kind: TokenKind::Punctuation,
                span: Span::new(start, end),
            });
            continue;
        }
        tokens.push(LexedToken {
            text: character.to_string(),
            kind: TokenKind::Punctuation,
            span: Span::new(start, start + character.len_utf8()),
        });
    }
    LexOutput {
        tokens,
        diagnostics,
    }
}

#[derive(Clone, Copy)]
enum CommentStart<'a> {
    Line(&'a str),
    Block(&'a str, &'a str),
}

impl<'a> CommentStart<'a> {
    fn opening(self) -> &'a str {
        match self {
            Self::Line(opening) | Self::Block(opening, _) => opening,
        }
    }

    fn closing(self) -> Option<&'a str> {
        match self {
            Self::Line(_) => None,
            Self::Block(_, closing) => Some(closing),
        }
    }
}

fn comment_start<'a>(
    remaining: &str,
    rules: &'a LexicalRules,
) -> Option<(&'a str, Option<&'a str>)> {
    let line = rules
        .line_comments()
        .iter()
        .filter(|opening| remaining.starts_with(opening.as_str()))
        .map(|opening| CommentStart::Line(opening.as_str()));
    let block = rules
        .block_comments()
        .iter()
        .filter(|(opening, _)| remaining.starts_with(opening.as_str()))
        .map(|(opening, closing)| CommentStart::Block(opening.as_str(), closing.as_str()));
    line.chain(block)
        .max_by_key(|comment| comment.opening().len())
        .map(|comment| (comment.opening(), comment.closing()))
}

fn limit_lex_output(source: &SourceFile, mut output: LexOutput, max_tokens: usize) -> LexOutput {
    if output.tokens.len() > max_tokens {
        output.tokens = Vec::new();
        if !output
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "LM-PARSE-008")
        {
            output.diagnostics.push(Diagnostic::error(
                "LM-PARSE-008",
                format!("token count exceeds the configured limit of {max_tokens}"),
                source,
                Span::new(source.text.len(), source.text.len()),
            ));
        }
    }
    output
}

pub(crate) fn lex_bounded(source: &SourceFile, lexer: &impl Lexer, max_tokens: usize) -> LexOutput {
    limit_lex_output(
        source,
        lexer.lex_with_token_limit(source, max_tokens),
        max_tokens,
    )
}

pub(crate) fn validate_tokens(
    source: &SourceFile,
    tokens: Vec<LexedToken>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<LexedToken> {
    let mut valid = Vec::with_capacity(tokens.len());
    let mut previous_end = 0;
    for token in tokens {
        let span = token.span;
        let valid_range = span.start < span.end
            && span.end <= source.text.len()
            && source.text.is_char_boundary(span.start)
            && source.text.is_char_boundary(span.end);
        if !valid_range {
            diagnostics.push(Diagnostic::spec_error(
                "LM-LEX-002",
                "lexer emitted an empty or invalid source range",
            ));
            continue;
        }
        if span.start < previous_end {
            diagnostics.push(Diagnostic::error(
                "LM-LEX-002",
                "lexer emitted overlapping or out-of-order tokens",
                source,
                span,
            ));
            continue;
        }
        if source.text.get(span.start..span.end) != Some(token.text.as_str()) {
            diagnostics.push(Diagnostic::error(
                "LM-LEX-002",
                "lexer token text does not match its source range",
                source,
                span,
            ));
            continue;
        }
        previous_end = span.end;
        valid.push(token);
    }
    valid
}

fn is_grouped_punctuation(character: char) -> bool {
    matches!(
        character,
        '+' | '-'
            | '*'
            | '/'
            | '='
            | '!'
            | '<'
            | '>'
            | '%'
            | '&'
            | '|'
            | '^'
            | '~'
            | '?'
            | ':'
            | '.'
    )
}
