//! Recovering parser for token-pattern constructs and delimited blocks.

use langmeld_core::{Diagnostic, SourceFile, SourceIdentity, Span};

use crate::completion::ParseContext;
use crate::lexer::{
    ConfiguredLexer, LexOutput, LexedToken, Lexer, TokenKind, lex_bounded, validate_tokens,
};
use crate::syntax::{build_syntax_tree, unparsed_syntax_tree};
use crate::{FieldKind, LanguageSpec, PatternPart, RepeatMinimum};

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
    pub source: Option<SourceIdentity>,
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
    parse_with_lexer_internal(source, language, lexer, limits, false).0
}

pub(crate) fn parse_with_contexts(
    source: &SourceFile,
    language: &LanguageSpec,
    lexer: &impl Lexer,
) -> (ParseResult, Vec<ParseContext>) {
    parse_with_lexer_internal(source, language, lexer, ParseLimits::default(), true)
}

fn parse_with_lexer_internal(
    source: &SourceFile,
    language: &LanguageSpec,
    lexer: &impl Lexer,
    limits: ParseLimits,
    capture_contexts: bool,
) -> (ParseResult, Vec<ParseContext>) {
    if source.text.len() > limits.max_source_bytes {
        let span = source_limit_span(source, limits.max_source_bytes);
        return (
            ParseResult {
                source: source.identity(),
                constructs: Vec::new(),
                syntax_tree: unparsed_syntax_tree(source),
                diagnostics: vec![Diagnostic::error(
                    "LM-PARSE-007",
                    format!(
                        "source size exceeds the configured limit of {} bytes",
                        limits.max_source_bytes
                    ),
                    source,
                    span,
                )],
                resource_limited: true,
            },
            Vec::new(),
        );
    }

    let LexOutput {
        tokens,
        mut diagnostics,
    } = lex_bounded(source, lexer, limits.max_tokens);
    let resource_limited = diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "LM-PARSE-008");
    let tokens = validate_tokens(source, tokens, &mut diagnostics);
    let allowed = language.root_constructs().collect::<Vec<_>>();
    let mut parser = Parser {
        source,
        language,
        tokens,
        cursor: 0,
        diagnostics: &mut diagnostics,
        limits,
        pattern_steps_remaining: limits.max_pattern_steps,
        construct_count: 0,
        resource_limited,
        stopped: false,
        root_constructs: allowed.clone(),
        recover_at_root_boundaries: language.has_explicit_root_constructs(),
        capture_contexts,
        completion_contexts: Vec::new(),
        completion_blocked: false,
    };
    let constructs = parser.parse_children(&allowed, None, 0);
    let resource_limited = parser.resource_limited;
    let contexts = std::mem::take(&mut parser.completion_contexts);
    let syntax_tokens = std::mem::take(&mut parser.tokens);
    drop(parser);
    let syntax_tree = build_syntax_tree(
        source,
        &constructs,
        &syntax_tokens,
        language.lexical_rules(),
    );
    (
        ParseResult {
            source: source.identity(),
            constructs,
            syntax_tree,
            diagnostics,
            resource_limited,
        },
        contexts,
    )
}

struct Parser<'a> {
    source: &'a SourceFile,
    language: &'a LanguageSpec,
    tokens: Vec<LexedToken>,
    cursor: usize,
    diagnostics: &'a mut Vec<Diagnostic>,
    limits: ParseLimits,
    pattern_steps_remaining: usize,
    construct_count: usize,
    resource_limited: bool,
    stopped: bool,
    root_constructs: Vec<&'a crate::ConstructSpec>,
    recover_at_root_boundaries: bool,
    capture_contexts: bool,
    completion_contexts: Vec<ParseContext>,
    completion_blocked: bool,
}

impl Parser<'_> {
    fn parse_children(
        &mut self,
        allowed: &[&crate::ConstructSpec],
        close: Option<&str>,
        depth: usize,
    ) -> Vec<ParsedConstruct> {
        let start = self
            .tokens
            .get(self.cursor)
            .map_or(self.source.text.len(), |token| token.span.start);
        let mut children = Vec::new();
        while !self.stopped {
            let Some(token) = self.tokens.get(self.cursor) else {
                break;
            };
            if close.is_some_and(|close| token.text == close) {
                break;
            }
            if close.is_some() && self.should_resume_at_root(allowed) {
                break;
            }
            if let Some(child) = self.parse_one(allowed, depth) {
                children.push(child);
            }
        }
        if self.capture_contexts {
            let end = self
                .tokens
                .get(self.cursor)
                .map_or(self.source.text.len(), |token| token.span.start);
            self.completion_contexts.push(ParseContext {
                span: Span::new(start, end),
                constructs: allowed
                    .iter()
                    .map(|construct| construct.name().to_owned())
                    .collect(),
                close: close.map(str::to_owned),
                values: Vec::new(),
                value_kind: crate::CompletionKind::EnumMember,
                blocked: self.completion_blocked,
            });
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
                let attempt = match match_sequence(
                    pattern,
                    &self.tokens,
                    self.cursor,
                    &mut self.pattern_steps_remaining,
                ) {
                    Ok(attempt) => attempt,
                    Err(()) => {
                        self.stop_for_pattern_limit();
                        return None;
                    }
                };
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
            self.completion_blocked = false;
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
                let resume_at_construct = if attempt.end < self.tokens.len() {
                    let child_matches = match any_construct_matches(
                        allowed,
                        &self.tokens,
                        attempt.end,
                        &mut self.pattern_steps_remaining,
                    ) {
                        Ok(matches) => matches,
                        Err(()) => {
                            self.stop_for_pattern_limit_at(attempt.end);
                            return None;
                        }
                    };
                    child_matches || self.should_resume_at_root_at(allowed, attempt.end)
                } else {
                    false
                };
                self.completion_blocked = true;
                if self.capture_contexts
                    && let Some(PatternPart::Choice {
                        values,
                        completion_kind,
                        ..
                    }) = attempt.expected
                {
                    let offset = self
                        .tokens
                        .get(attempt.end)
                        .map_or(self.source.text.len(), |token| token.span.start);
                    self.completion_contexts.push(ParseContext {
                        span: Span::new(offset, offset),
                        constructs: Vec::new(),
                        close: None,
                        values: values.clone(),
                        value_kind: *completion_kind,
                        blocked: false,
                    });
                }
                let span = self.tokens.get(attempt.end).map_or_else(
                    || Span::new(self.source.text.len(), self.source.text.len()),
                    |token| token.span,
                );
                let expected = attempt
                    .expected
                    .map_or_else(|| "end of construct".to_owned(), describe_pattern);
                self.diagnostics.push(Diagnostic::error(
                    "LM-PARSE-002",
                    format!("expected {expected} in construct `{}`", spec.name),
                    self.source,
                    span,
                ));
                self.cursor = if resume_at_construct {
                    attempt.end
                } else {
                    (attempt.end + usize::from(attempt.end < self.tokens.len()))
                        .max(self.cursor + 1)
                        .min(self.tokens.len())
                };
            } else if let Some(token) = self.tokens.get(self.cursor) {
                self.completion_blocked = false;
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

        if self.construct_count >= self.limits.max_constructs {
            self.resource_limited = true;
            self.stopped = true;
            let token = &self.tokens[start_index];
            self.diagnostics.push(Diagnostic::error(
                "LM-PARSE-009",
                format!(
                    "parsed construct count exceeds the configured limit of {}",
                    self.limits.max_constructs
                ),
                self.source,
                token.span,
            ));
            return None;
        }
        self.construct_count += 1;

        self.completion_blocked = false;
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
            let opening_end = opening.span.end;
            let max_nesting_depth = self.limits.max_nesting_depth.min(MAX_NESTING_DEPTH);
            if depth >= max_nesting_depth {
                self.resource_limited = true;
                self.completion_blocked = true;
                self.diagnostics.push(Diagnostic::error(
                    "LM-PARSE-003",
                    format!("maximum block nesting depth ({max_nesting_depth}) exceeded"),
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
                if self.stopped {
                    end = self
                        .tokens
                        .get(self.cursor.saturating_sub(1))
                        .map_or(end, |token| token.span.end);
                } else if let Some(closing) = self
                    .tokens
                    .get(self.cursor)
                    .filter(|token| token.text == block.close)
                {
                    end = closing.span.end;
                    self.cursor += 1;
                    self.completion_blocked = false;
                } else {
                    let boundary = self.tokens.get(self.cursor);
                    end = boundary.map_or(self.source.text.len(), |_| {
                        children.last().map_or(opening_end, |child| child.span.end)
                    });
                    let diagnostic_span = boundary.map_or_else(
                        || Span::new(end, end),
                        |token| Span::new(token.span.start, token.span.start),
                    );
                    self.diagnostics.push(Diagnostic::error(
                        "LM-PARSE-004",
                        format!("expected closing token `{}`", block.close),
                        self.source,
                        diagnostic_span,
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

    fn should_resume_at_root(&mut self, allowed: &[&crate::ConstructSpec]) -> bool {
        self.should_resume_at_root_at(allowed, self.cursor)
    }

    fn should_resume_at_root_at(
        &mut self,
        allowed: &[&crate::ConstructSpec],
        position: usize,
    ) -> bool {
        if !self.recover_at_root_boundaries {
            return false;
        }

        let root_matches = match any_construct_matches(
            &self.root_constructs,
            &self.tokens,
            position,
            &mut self.pattern_steps_remaining,
        ) {
            Ok(matches) => matches,
            Err(()) => {
                self.stop_for_pattern_limit_at(position);
                return false;
            }
        };
        if !root_matches {
            return false;
        }

        let child_matches = match any_construct_matches(
            allowed,
            &self.tokens,
            position,
            &mut self.pattern_steps_remaining,
        ) {
            Ok(matches) => matches,
            Err(()) => {
                self.stop_for_pattern_limit_at(position);
                return false;
            }
        };
        !child_matches
    }

    fn missing_block_open(&mut self, spec: &crate::ConstructSpec, open: &str) {
        self.completion_blocked = true;
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

    fn stop_for_pattern_limit(&mut self) {
        self.stop_for_pattern_limit_at(self.cursor);
    }

    fn stop_for_pattern_limit_at(&mut self, position: usize) {
        self.resource_limited = true;
        self.stopped = true;
        let span = self.tokens.get(position).map_or_else(
            || Span::new(self.source.text.len(), self.source.text.len()),
            |token| token.span,
        );
        self.diagnostics.push(Diagnostic::error(
            "LM-PARSE-010",
            format!(
                "pattern matching exceeds the configured limit of {} steps",
                self.limits.max_pattern_steps
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

fn any_construct_matches(
    constructs: &[&crate::ConstructSpec],
    tokens: &[LexedToken],
    start: usize,
    remaining_steps: &mut usize,
) -> Result<bool, ()> {
    let Some(token) = tokens.get(start) else {
        return Ok(false);
    };
    for construct in constructs {
        for pattern in construct.variants() {
            if !pattern_may_start_with(pattern, token) {
                continue;
            }
            let attempt = match_sequence(pattern, tokens, start, remaining_steps)?;
            if attempt.complete {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn pattern_may_start_with(pattern: &[PatternPart], token: &LexedToken) -> bool {
    for part in pattern {
        match part {
            PatternPart::Literal(literal) => {
                return token.kind != TokenKind::Invalid && token.text == *literal;
            }
            PatternPart::Field { kind, .. } => return accepts(*kind, token.kind),
            PatternPart::Choice { values, .. } => {
                return token.kind != TokenKind::Invalid
                    && values.iter().any(|value| value == &token.text);
            }
            PatternPart::Expression { .. } => return token.kind != TokenKind::Invalid,
            PatternPart::Optional(parts) => {
                if pattern_may_start_with(parts, token) {
                    return true;
                }
            }
            PatternPart::Repeat { parts, minimum } => {
                if pattern_may_start_with(parts, token) {
                    return true;
                }
                if *minimum == RepeatMinimum::One {
                    return false;
                }
            }
        }
    }
    false
}

fn match_sequence<'a>(
    pattern: &'a [PatternPart],
    tokens: &[LexedToken],
    start: usize,
    remaining_steps: &mut usize,
) -> Result<MatchAttempt<'a>, ()> {
    let mut fields = Vec::new();
    let mut cursor = start;
    for part in pattern {
        if *remaining_steps == 0 {
            return Err(());
        }
        *remaining_steps -= 1;
        match part {
            PatternPart::Literal(literal) => {
                if tokens
                    .get(cursor)
                    .is_some_and(|token| token.kind != TokenKind::Invalid && token.text == *literal)
                {
                    cursor += 1;
                } else {
                    return Ok(MatchAttempt {
                        fields,
                        end: cursor,
                        complete: false,
                        expected: Some(part),
                    });
                }
            }
            PatternPart::Field { name, kind } => {
                let Some(token) = tokens
                    .get(cursor)
                    .filter(|token| accepts(*kind, token.kind))
                else {
                    return Ok(MatchAttempt {
                        fields,
                        end: cursor,
                        complete: false,
                        expected: Some(part),
                    });
                };
                fields.push(CapturedField {
                    name: name.clone(),
                    value: match kind {
                        FieldKind::Token => CapturedValue::Token {
                            text: token.text.clone(),
                            kind: token.kind,
                        },
                        FieldKind::Identifier => CapturedValue::Identifier(token.text.clone()),
                        FieldKind::Integer => CapturedValue::Integer(token.text.clone()),
                        FieldKind::String => CapturedValue::String(token.text.clone()),
                    },
                    span: token.span,
                });
                cursor += 1;
            }
            PatternPart::Choice { name, values, .. } => {
                let Some(token) = tokens.get(cursor).filter(|token| {
                    token.kind != TokenKind::Invalid
                        && values.iter().any(|value| value == &token.text)
                }) else {
                    return Ok(MatchAttempt {
                        fields,
                        end: cursor,
                        complete: false,
                        expected: Some(part),
                    });
                };
                fields.push(CapturedField {
                    name: name.clone(),
                    value: CapturedValue::Token {
                        text: token.text.clone(),
                        kind: token.kind,
                    },
                    span: token.span,
                });
                cursor += 1;
            }
            PatternPart::Expression { name, terminator } => {
                let Some(end) = expression_end(tokens, cursor, terminator, remaining_steps)? else {
                    return Ok(MatchAttempt {
                        fields,
                        end: tokens.len(),
                        complete: false,
                        expected: Some(part),
                    });
                };
                if end == cursor {
                    return Ok(MatchAttempt {
                        fields,
                        end: cursor,
                        complete: false,
                        expected: Some(part),
                    });
                }
                fields.push(CapturedField {
                    name: name.clone(),
                    value: CapturedValue::Expression,
                    span: Span::new(tokens[cursor].span.start, tokens[end - 1].span.end),
                });
                cursor = end + 1;
            }
            PatternPart::Optional(parts) => {
                let attempt = match_sequence(parts, tokens, cursor, remaining_steps)?;
                if attempt.complete {
                    fields.extend(attempt.fields);
                    cursor = attempt.end;
                } else if attempt.end > cursor {
                    return Ok(MatchAttempt {
                        fields,
                        end: attempt.end,
                        complete: false,
                        expected: attempt.expected,
                    });
                }
            }
            PatternPart::Repeat { parts, minimum } => {
                let mut repetitions = 0;
                loop {
                    let attempt = match_sequence(parts, tokens, cursor, remaining_steps)?;
                    if attempt.complete && attempt.end > cursor {
                        fields.extend(attempt.fields);
                        cursor = attempt.end;
                        repetitions += 1;
                    } else if attempt.end > cursor {
                        return Ok(MatchAttempt {
                            fields,
                            end: attempt.end,
                            complete: false,
                            expected: attempt.expected,
                        });
                    } else {
                        if *minimum == RepeatMinimum::One && repetitions == 0 {
                            return Ok(MatchAttempt {
                                fields,
                                end: cursor,
                                complete: false,
                                expected: parts.first(),
                            });
                        }
                        break;
                    }
                }
            }
        }
    }
    Ok(MatchAttempt {
        fields,
        end: cursor,
        complete: true,
        expected: None,
    })
}

fn describe_pattern(part: &PatternPart) -> String {
    match part {
        PatternPart::Literal(_) => "the expected token".to_owned(),
        PatternPart::Field {
            kind: FieldKind::Token,
            ..
        } => "a token".to_owned(),
        PatternPart::Field {
            kind: FieldKind::Identifier,
            ..
        } => "an identifier".to_owned(),
        PatternPart::Field {
            kind: FieldKind::Integer,
            ..
        } => "an integer".to_owned(),
        PatternPart::Field {
            kind: FieldKind::String,
            ..
        } => "a quoted string".to_owned(),
        PatternPart::Choice { values, .. } => format!(
            "one of {}",
            values
                .iter()
                .map(|value| format!("`{value}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        PatternPart::Expression { .. } => "an expression".to_owned(),
        PatternPart::Optional(_) | PatternPart::Repeat { .. } => {
            "the expected syntax sequence".to_owned()
        }
    }
}

fn expression_end(
    tokens: &[LexedToken],
    start: usize,
    terminator: &str,
    remaining_steps: &mut usize,
) -> Result<Option<usize>, ()> {
    let mut closing = Vec::new();
    let mut cursor = start;
    while let Some(token) = tokens.get(cursor) {
        if *remaining_steps == 0 {
            return Err(());
        }
        *remaining_steps -= 1;
        if closing.is_empty() && token.kind != TokenKind::Invalid && token.text == terminator {
            return Ok(Some(cursor));
        }
        match token.text.as_str() {
            "(" => closing.push(")"),
            "[" => closing.push("]"),
            "{" => closing.push("}"),
            ")" | "]" | "}" if closing.last() == Some(&token.text.as_str()) => {
                closing.pop();
            }
            _ => {}
        }
        cursor += 1;
    }
    Ok(None)
}

fn accepts(expected: FieldKind, actual: TokenKind) -> bool {
    if expected == FieldKind::Token {
        return actual != TokenKind::Invalid;
    }
    matches!(
        (expected, actual),
        (FieldKind::Identifier, TokenKind::Identifier)
            | (FieldKind::Integer, TokenKind::Integer)
            | (FieldKind::String, TokenKind::String)
    )
}

fn source_limit_span(source: &SourceFile, limit: usize) -> Span {
    let mut offset = limit.min(source.text.len());
    while !source.text.is_char_boundary(offset) {
        offset -= 1;
    }
    Span::new(offset, offset)
}
