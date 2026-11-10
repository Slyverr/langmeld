//! Recovering parser for token-pattern constructs and delimited blocks.

use langmeld_core::{Diagnostic, SourceFile, SourceIdentity, Span};

use crate::{FieldKind, LanguageSpec, PatternPart, RepeatMinimum};

#[derive(Clone, Debug)]
struct Token {
    text: String,
    kind: TokenKind,
    span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TokenKind {
    Identifier,
    Integer,
    String,
    Punctuation,
    Invalid,
}

/// A field value captured by a syntax pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapturedValue {
    /// Identifier text without transformation.
    Identifier(String),
    /// Decimal integer text preserved as written.
    Integer(String),
    /// Quoted string text preserved as written.
    String(String),
}

/// A named source value captured by a construct.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedField {
    /// Field name from the language specification.
    pub name: String,
    /// Captured lexical value.
    pub value: CapturedValue,
    /// Source range of the captured token.
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
    pub source: Option<SourceIdentity>,
    /// Successfully recognized constructs in source order.
    pub constructs: Vec<ParsedConstruct>,
    /// Lexical and syntax errors. Parsing continues after recoverable errors.
    pub diagnostics: Vec<Diagnostic>,
}

/// Maximum nesting level accepted by the generic block parser.
pub const MAX_NESTING_DEPTH: usize = 128;

/// Parses source using a validated token-pattern language specification.
///
/// The generic parser supports token patterns, primitive lexical captures,
/// and nested blocks whose child constructs are explicitly listed in the
/// grammar. Expressions, custom lexers, and custom parser hooks remain host or
/// future API concerns.
#[must_use]
pub fn parse(source: &SourceFile, language: &LanguageSpec) -> ParseResult {
    let (tokens, mut diagnostics) = lex(source);
    let allowed = language.constructs.iter().collect::<Vec<_>>();
    let constructs = Parser {
        source,
        language,
        tokens,
        cursor: 0,
        diagnostics: &mut diagnostics,
    }
    .parse_children(&allowed, None, 0);
    ParseResult {
        source: source.identity(),
        constructs,
        diagnostics,
    }
}

struct Parser<'a> {
    source: &'a SourceFile,
    language: &'a LanguageSpec,
    tokens: Vec<Token>,
    cursor: usize,
    diagnostics: &'a mut Vec<Diagnostic>,
}

impl Parser<'_> {
    fn parse_children(
        &mut self,
        allowed: &[&crate::ConstructSpec],
        close: Option<&str>,
        depth: usize,
    ) -> Vec<ParsedConstruct> {
        let mut children = Vec::new();
        while let Some(token) = self.tokens.get(self.cursor) {
            if close.is_some_and(|close| token.text == close) {
                break;
            }
            if let Some(child) = self.parse_one(allowed, depth) {
                children.push(child);
            }
        }
        children
    }

    fn parse_one(
        &mut self,
        allowed: &[&crate::ConstructSpec],
        depth: usize,
    ) -> Option<ParsedConstruct> {
        let start_index = self.cursor;
        let mut best_partial: Option<(&crate::ConstructSpec, usize, MatchAttempt<'_>)> = None;
        let mut matched = None;
        let mut ambiguous = false;
        for spec in allowed {
            for (variant, pattern) in spec.variants().iter().enumerate() {
                let attempt = match_sequence(pattern, &self.tokens, self.cursor);
                if attempt.complete {
                    if matched.is_some() {
                        ambiguous = true;
                        break;
                    }
                    matched = Some((*spec, variant, attempt));
                    continue;
                }
                if attempt.end > self.cursor
                    && best_partial
                        .as_ref()
                        .is_none_or(|(_, _, best)| attempt.end > best.end)
                {
                    best_partial = Some((*spec, variant, attempt));
                }
            }
            if ambiguous {
                break;
            }
        }

        if ambiguous {
            let token = &self.tokens[self.cursor];
            self.diagnostics.push(Diagnostic::error(
                "LM-PARSE-006",
                "source matches more than one construct at this position",
                self.source,
                token.span,
            ));
            self.cursor += 1;
            return None;
        }

        let Some((spec, variant, attempt)) = matched else {
            if let Some((spec, _, attempt)) = best_partial {
                let span = self.tokens.get(attempt.end).map_or_else(
                    || Span::new(self.source.text.len(), self.source.text.len()),
                    |token| token.span,
                );
                let expected = attempt
                    .expected
                    .map_or("end of construct", describe_pattern);
                self.diagnostics.push(Diagnostic::error(
                    "LM-PARSE-002",
                    format!("expected {expected} in construct `{}`", spec.name),
                    self.source,
                    span,
                ));
                self.cursor = (attempt.end + usize::from(attempt.end < self.tokens.len()))
                    .max(self.cursor + 1)
                    .min(self.tokens.len());
            } else if let Some(token) = self.tokens.get(self.cursor) {
                self.diagnostics.push(Diagnostic::error(
                    "LM-PARSE-001",
                    format!("unexpected token `{}`", token.text),
                    self.source,
                    token.span,
                ));
                self.cursor += 1;
            }
            return None;
        };

        self.cursor = attempt.end;
        let start = self.tokens[start_index].span.start;
        let mut end = self.tokens[self.cursor - 1].span.end;
        let mut children = Vec::new();
        if let Some(block) = &spec.block {
            let Some(opening) = self.tokens.get(self.cursor) else {
                self.missing_block_open(spec, block.open.as_str());
                return Some(ParsedConstruct {
                    name: spec.name.clone(),
                    variant,
                    span: Span::new(start, end),
                    fields: attempt.fields,
                    children,
                });
            };
            if opening.text != block.open {
                self.missing_block_open(spec, block.open.as_str());
                return Some(ParsedConstruct {
                    name: spec.name.clone(),
                    variant,
                    span: Span::new(start, end),
                    fields: attempt.fields,
                    children,
                });
            }
            if depth >= MAX_NESTING_DEPTH {
                self.diagnostics.push(Diagnostic::error(
                    "LM-PARSE-003",
                    format!("maximum block nesting depth ({MAX_NESTING_DEPTH}) exceeded"),
                    self.source,
                    opening.span,
                ));
                self.skip_block(&block.open, &block.close);
                end = self
                    .tokens
                    .get(self.cursor.saturating_sub(1))
                    .map_or(self.source.text.len(), |token| token.span.end);
            } else {
                self.cursor += 1;
                let allowed = block
                    .constructs
                    .iter()
                    .filter_map(|name| {
                        self.language
                            .constructs
                            .iter()
                            .find(|item| item.name == *name)
                    })
                    .collect::<Vec<_>>();
                children = self.parse_children(&allowed, Some(&block.close), depth + 1);
                if let Some(closing) = self.tokens.get(self.cursor) {
                    end = closing.span.end;
                    self.cursor += 1;
                } else {
                    end = self.source.text.len();
                    self.diagnostics.push(Diagnostic::error(
                        "LM-PARSE-004",
                        format!("expected closing token `{}`", block.close),
                        self.source,
                        Span::new(end, end),
                    ));
                }
            }
        }
        Some(ParsedConstruct {
            name: spec.name.clone(),
            variant,
            span: Span::new(start, end),
            fields: attempt.fields,
            children,
        })
    }

    fn missing_block_open(&mut self, spec: &crate::ConstructSpec, open: &str) {
        let span = self.tokens.get(self.cursor).map_or_else(
            || Span::new(self.source.text.len(), self.source.text.len()),
            |token| token.span,
        );
        self.diagnostics.push(Diagnostic::error(
            "LM-PARSE-005",
            format!(
                "expected opening token `{open}` after construct `{}`",
                spec.name
            ),
            self.source,
            span,
        ));
    }

    fn skip_block(&mut self, open: &str, close: &str) {
        let mut nesting = 0usize;
        while let Some(token) = self.tokens.get(self.cursor) {
            if token.text == open {
                nesting += 1;
            } else if token.text == close {
                nesting -= 1;
                self.cursor += 1;
                if nesting == 0 {
                    return;
                }
                continue;
            }
            self.cursor += 1;
        }
    }
}

struct MatchAttempt<'a> {
    fields: Vec<CapturedField>,
    end: usize,
    complete: bool,
    expected: Option<&'a PatternPart>,
}

fn match_sequence<'a>(
    pattern: &'a [PatternPart],
    tokens: &[Token],
    start: usize,
) -> MatchAttempt<'a> {
    let mut fields = Vec::new();
    let mut cursor = start;
    for part in pattern {
        match part {
            PatternPart::Literal(literal) => {
                if tokens
                    .get(cursor)
                    .is_some_and(|token| token.text == *literal)
                {
                    cursor += 1;
                } else {
                    return MatchAttempt {
                        fields,
                        end: cursor,
                        complete: false,
                        expected: Some(part),
                    };
                }
            }
            PatternPart::Field { name, kind } => {
                let Some(token) = tokens
                    .get(cursor)
                    .filter(|token| accepts(*kind, token.kind))
                else {
                    return MatchAttempt {
                        fields,
                        end: cursor,
                        complete: false,
                        expected: Some(part),
                    };
                };
                fields.push(CapturedField {
                    name: name.clone(),
                    value: match kind {
                        FieldKind::Identifier => CapturedValue::Identifier(token.text.clone()),
                        FieldKind::Integer => CapturedValue::Integer(token.text.clone()),
                        FieldKind::String => CapturedValue::String(token.text.clone()),
                    },
                    span: token.span,
                });
                cursor += 1;
            }
            PatternPart::Optional(parts) => {
                let attempt = match_sequence(parts, tokens, cursor);
                if attempt.complete {
                    fields.extend(attempt.fields);
                    cursor = attempt.end;
                } else if attempt.end > cursor {
                    return MatchAttempt {
                        fields,
                        end: attempt.end,
                        complete: false,
                        expected: attempt.expected,
                    };
                }
            }
            PatternPart::Repeat { parts, minimum } => {
                let mut repetitions = 0;
                loop {
                    let attempt = match_sequence(parts, tokens, cursor);
                    if attempt.complete && attempt.end > cursor {
                        fields.extend(attempt.fields);
                        cursor = attempt.end;
                        repetitions += 1;
                    } else if attempt.end > cursor {
                        return MatchAttempt {
                            fields,
                            end: attempt.end,
                            complete: false,
                            expected: attempt.expected,
                        };
                    } else {
                        if *minimum == RepeatMinimum::One && repetitions == 0 {
                            return MatchAttempt {
                                fields,
                                end: cursor,
                                complete: false,
                                expected: parts.first(),
                            };
                        }
                        break;
                    }
                }
            }
        }
    }
    MatchAttempt {
        fields,
        end: cursor,
        complete: true,
        expected: None,
    }
}

fn describe_pattern(part: &PatternPart) -> &'static str {
    match part {
        PatternPart::Literal(_) => "the expected token",
        PatternPart::Field {
            kind: FieldKind::Identifier,
            ..
        } => "an identifier",
        PatternPart::Field {
            kind: FieldKind::Integer,
            ..
        } => "an integer",
        PatternPart::Field {
            kind: FieldKind::String,
            ..
        } => "a quoted string",
        PatternPart::Optional(_) | PatternPart::Repeat { .. } => "the expected syntax sequence",
    }
}

fn accepts(expected: FieldKind, actual: TokenKind) -> bool {
    matches!(
        (expected, actual),
        (FieldKind::Identifier, TokenKind::Identifier)
            | (FieldKind::Integer, TokenKind::Integer)
            | (FieldKind::String, TokenKind::String)
    )
}

fn lex(source: &SourceFile) -> (Vec<Token>, Vec<Diagnostic>) {
    let mut tokens = Vec::new();
    let mut diagnostics = Vec::new();
    let mut chars = source.text.char_indices().peekable();
    while let Some((start, character)) = chars.next() {
        if character.is_whitespace() {
            continue;
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
            tokens.push(Token {
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
            tokens.push(Token {
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
            tokens.push(Token {
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
            tokens.push(Token {
                text: source.text[start..end].to_owned(),
                kind: TokenKind::Punctuation,
                span: Span::new(start, end),
            });
            continue;
        }
        tokens.push(Token {
            text: character.to_string(),
            kind: TokenKind::Punctuation,
            span: Span::new(start, start + character.len_utf8()),
        });
    }
    (tokens, diagnostics)
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
