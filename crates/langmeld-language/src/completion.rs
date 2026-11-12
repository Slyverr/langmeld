use langmeld_core::{SourceFile, Span};

use crate::completion_kind::CompletionKind;
use crate::lexer::{ConfiguredLexer, Lexer, lex_bounded};
use crate::parser::{model::ParseContext, parse_with_contexts};
use crate::{
    AnalysisDocument, LanguageCompleter, LanguageParser, LanguageSpec, ParseLimits, ParseResult,
    ParsedDocument, PatternPart, RepeatMinimum,
};

/// A completion suggestion and the source range it replaces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionCandidate {
    /// Token text to insert.
    pub text: String,
    /// Optional display label; defaults to [`Self::text`].
    pub label: Option<String>,
    /// Optional matching text; defaults to [`Self::text`].
    pub filter_text: Option<String>,
    /// Optional semantic category for editor presentation.
    pub kind: Option<CompletionKind>,
    /// Source range to replace, empty when completing at a token boundary.
    pub replace: Span,
    /// Optional short explanation shown beside the candidate in editors.
    pub detail: Option<String>,
}

impl CompletionCandidate {
    /// Creates a candidate whose label and filter text default to its
    /// inserted text.
    #[must_use]
    pub fn new(text: impl Into<String>, replace: Span) -> Self {
        Self {
            text: text.into(),
            label: None,
            filter_text: None,
            kind: None,
            replace,
            detail: None,
        }
    }

    /// Sets a display label that can differ from the inserted text.
    #[must_use]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Sets text that editors use to match this candidate while filtering.
    #[must_use]
    pub fn with_filter_text(mut self, filter_text: impl Into<String>) -> Self {
        self.filter_text = Some(filter_text.into());
        self
    }

    /// Sets the candidate's semantic category for editor presentation.
    #[must_use]
    pub fn with_kind(mut self, kind: CompletionKind) -> Self {
        self.kind = Some(kind);
        self
    }

    /// Adds a short editor detail, such as a symbol kind or value category.
    #[must_use]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

/// Requests host-defined completions for a parsed source snapshot.
///
/// Parse or analysis results from a different source revision are rejected for
/// database-managed and standalone snapshots alike. Candidates with
/// replacement spans outside the source or inside a UTF-8 code point are
/// discarded.
#[must_use]
pub fn complete_with<P, C>(
    source: &SourceFile,
    parsed: &ParsedDocument<P::Tree>,
    analysis: Option<&AnalysisDocument<C::Analysis>>,
    offset: usize,
    completer: &C,
) -> Vec<CompletionCandidate>
where
    P: LanguageParser,
    C: LanguageCompleter<P> + ?Sized,
{
    if offset > source.text().len()
        || !source.text().is_char_boundary(offset)
        || parsed.source_identity() != source.identity()
        || analysis.is_some_and(|analysis| analysis.source_identity() != source.identity())
    {
        return Vec::new();
    }

    completer
        .complete(source, parsed, analysis, offset)
        .into_iter()
        .filter(|candidate| {
            candidate.replace.start <= candidate.replace.end
                && candidate.replace.end <= source.text().len()
                && source.text().is_char_boundary(candidate.replace.start)
                && source.text().is_char_boundary(candidate.replace.end)
        })
        .collect()
}

/// Suggests grammar tokens accepted at the source offset.
///
/// Suggestions include construct starts, finite literal or choice values, and
/// block closing tokens. Identifier and expression values remain host-defined.
/// This uses Langmeld's default lexer; use [`complete_at_with_lexer`] when the
/// language uses host-defined tokenization.
#[must_use]
pub fn complete_at(
    source: &SourceFile,
    language: &LanguageSpec,
    offset: usize,
) -> Vec<CompletionCandidate> {
    let lexer = ConfiguredLexer {
        rules: language.lexical_rules(),
    };
    complete_at_with_lexer(source, language, offset, &lexer)
}

/// Suggests grammar tokens accepted at the source offset using a host lexer.
///
/// Provides the same construct-start, finite-value, and block-close
/// suggestions as [`complete_at`], but tokenizes with `lexer`.
#[must_use]
pub fn complete_at_with_lexer(
    source: &SourceFile,
    language: &LanguageSpec,
    offset: usize,
    lexer: &impl Lexer,
) -> Vec<CompletionCandidate> {
    let limits = ParseLimits::default();
    if offset > source.text().len()
        || !source.text().is_char_boundary(offset)
        || source.text().len() > limits.max_source_bytes
    {
        return Vec::new();
    }

    let prefix_token = lex_bounded(source, lexer, limits.max_tokens)
        .tokens
        .into_iter()
        .find(|token| {
            token.kind != crate::TokenKind::Invalid
                && token.span.start < offset
                && offset <= token.span.end
                && (offset < token.span.end
                    || token
                        .text
                        .chars()
                        .last()
                        .is_some_and(|character| character == '_' || character.is_alphanumeric()))
                && token.span.start < token.span.end
                && token.span.end <= source.text().len()
                && source.text().is_char_boundary(token.span.start)
                && source.text().is_char_boundary(token.span.end)
                && source.text().get(token.span.start..token.span.end) == Some(token.text.as_str())
        });
    let token_is_complete = prefix_token
        .as_ref()
        .is_some_and(|token| offset == token.span.end);
    let (prefix_end, replacement, prefix) = if let Some(token) = prefix_token.as_ref() {
        (
            token.span.start,
            token.span,
            source.text()[token.span.start..offset].to_owned(),
        )
    } else {
        (offset, Span::new(offset, offset), String::new())
    };

    let prefix_source = SourceFile::new(
        source.name().to_owned(),
        source.text()[..prefix_end].to_owned(),
    );
    let (parsed, contexts) = parse_with_contexts(&prefix_source, language, lexer);
    if has_blocking_completion_diagnostics(&parsed, &contexts, prefix_end, token_is_complete) {
        return Vec::new();
    }

    let Some(context) = context_at(&contexts, prefix_end) else {
        return Vec::new();
    };
    let mut candidates = Vec::new();
    if !context.blocked {
        push_context_candidates(&mut candidates, context, language, &prefix, replacement);
    }

    if token_is_complete {
        let completed_source =
            SourceFile::new(source.name().to_owned(), source.text()[..offset].to_owned());
        let (completed_parse, completed_contexts) =
            parse_with_contexts(&completed_source, language, lexer);

        if !has_blocking_completion_diagnostics(
            &completed_parse,
            &completed_contexts,
            offset,
            false,
        ) && let Some(next_context) = context_at(&completed_contexts, offset)
            && !next_context.blocked
        {
            push_context_candidates(
                &mut candidates,
                next_context,
                language,
                "",
                Span::new(offset, offset),
            );
        }
    }

    candidates
}

fn has_blocking_completion_diagnostics(
    parsed: &ParseResult,
    contexts: &[ParseContext],
    offset: usize,
    allow_cursor_field_completion: bool,
) -> bool {
    let has_finite_values = contexts
        .iter()
        .any(|context| context.span == Span::new(offset, offset) && !context.values.is_empty());

    parsed.diagnostics().iter().any(|diagnostic| {
        diagnostic
            .location
            .as_ref()
            .is_some_and(|location| location.span().end >= offset)
            && diagnostic.code != "LM-PARSE-004"
            && !(has_finite_values && diagnostic.code == "LM-PARSE-002")
            && !(allow_cursor_field_completion && diagnostic.code == "LM-PARSE-002")
    })
}

fn context_at(contexts: &[ParseContext], offset: usize) -> Option<&ParseContext> {
    contexts
        .iter()
        .filter(|context| context.span.start <= offset && offset <= context.span.end)
        .min_by_key(|context| context.span.len())
}

fn push_context_candidates(
    candidates: &mut Vec<CompletionCandidate>,
    context: &ParseContext,
    language: &LanguageSpec,
    prefix: &str,
    replacement: Span,
) {
    for text in &context.values {
        push_candidate(candidates, text, prefix, replacement, context.value_kind);
    }
    for name in &context.constructs {
        let Some(construct) = language
            .constructs()
            .iter()
            .find(|construct| construct.name() == name)
        else {
            continue;
        };
        for pattern in construct.variants() {
            let mut literals = Vec::new();
            collect_first_literals(pattern, &mut literals);
            for text in literals {
                push_candidate(
                    candidates,
                    &text,
                    prefix,
                    replacement,
                    construct.completion_kind(),
                );
            }
        }
    }
    if let Some(close) = &context.close
        && close.starts_with(prefix)
        && close != prefix
    {
        push_candidate(candidates, close, prefix, replacement, None);
    }
}

fn collect_first_literals(parts: &[PatternPart], output: &mut Vec<String>) {
    for part in parts {
        match part {
            PatternPart::Literal(text) => {
                output.push(text.clone());
                return;
            }
            PatternPart::Choice { values, .. } => {
                output.extend(values.iter().cloned());
                return;
            }
            PatternPart::Field { .. } | PatternPart::Expression { .. } => return,
            PatternPart::Optional { parts, .. } => collect_first_literals(parts, output),
            PatternPart::Repeat { parts, minimum, .. } => {
                collect_first_literals(parts, output);
                if *minimum == RepeatMinimum::One {
                    return;
                }
            }
        }
    }
}

fn push_candidate(
    candidates: &mut Vec<CompletionCandidate>,
    text: &str,
    prefix: &str,
    replacement: Span,
    kind: Option<CompletionKind>,
) {
    if !text.starts_with(prefix) || text == prefix {
        return;
    }
    if let Some(existing) = candidates
        .iter_mut()
        .find(|candidate| candidate.text == text)
    {
        // A shared token has a meaningful kind only when all matching
        // constructs agree on its category.
        if existing.kind != kind {
            existing.kind = None;
        }
        return;
    }
    let candidate = CompletionCandidate::new(text, replacement);
    candidates.push(if let Some(kind) = kind {
        candidate.with_kind(kind)
    } else {
        candidate
    });
}
