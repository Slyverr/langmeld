use langmeld_core::{Diagnostic, SourceFile, Span};

use crate::completion_model::{CompletionKind, ParseContext};
use crate::lexer::{LexOutput, LexedToken, Lexer, lex_bounded, validate_tokens};
use crate::syntax::{build_syntax_tree, unparsed_syntax_tree};
use crate::{LanguageSpec, PatternPart};

mod recovery;

use super::pattern::{MatchAttempt, any_construct_matches, describe_pattern, match_sequence};
use super::{MAX_NESTING_DEPTH, ParseLimits, ParseResult, ParsedConstruct};

pub(super) fn parse_with_lexer_internal(
    source: &SourceFile,
    language: &LanguageSpec,
    lexer: &impl Lexer,
    limits: ParseLimits,
    capture_contexts: bool,
) -> (ParseResult, Vec<ParseContext>) {
    if source.text().len() > limits.max_source_bytes {
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
            .map_or(self.source.text().len(), |token| token.span.start);
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
                .map_or(self.source.text().len(), |token| token.span.start);
            self.completion_contexts.push(ParseContext {
                span: Span::new(start, end),
                constructs: allowed
                    .iter()
                    .map(|construct| construct.name().to_owned())
                    .collect(),
                close: close.map(str::to_owned),
                values: Vec::new(),
                value_kind: CompletionKind::EnumMember,
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
                        .map_or(self.source.text().len(), |token| token.span.start);
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
                    || Span::new(self.source.text().len(), self.source.text().len()),
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
                    .map_or(self.source.text().len(), |token| token.span.end);
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
                    end = boundary.map_or(self.source.text().len(), |_| {
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
}

fn source_limit_span(source: &SourceFile, limit: usize) -> Span {
    let mut offset = limit.min(source.text().len());
    while !source.text().is_char_boundary(offset) {
        offset -= 1;
    }
    Span::new(offset, offset)
}
