use langmeld_core::{SourceFile, Span};

use crate::parser::{DefaultLexer, Lexer, parse_with_contexts};
use crate::{
    AnalysisDocument, LanguageCompleter, LanguageParser, LanguageSpec, ParsedDocument, PatternPart,
    RepeatMinimum,
};

/// A grammar-derived token suggestion at a source location.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionCandidate {
    /// Token text to insert.
    pub text: String,
    /// Source range to replace, empty when completing at a token boundary.
    pub replace: Span,
}

/// Requests host-defined completions for a parsed source snapshot.
///
/// Parse or analysis results from a different database-managed revision are
/// rejected. Detached snapshots have no revision identity, so callers must
/// pair those values correctly. Candidates with replacement spans outside the
/// source or inside a UTF-8 code point are discarded.
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
    C: LanguageCompleter<P>,
{
    if offset > source.text.len()
        || !source.text.is_char_boundary(offset)
        || parsed.source != source.identity()
        || analysis.is_some_and(|analysis| analysis.source != source.identity())
    {
        return Vec::new();
    }

    completer
        .complete(source, parsed, analysis, offset)
        .into_iter()
        .filter(|candidate| {
            candidate.replace.start <= candidate.replace.end
                && candidate.replace.end <= source.text.len()
                && source.text.is_char_boundary(candidate.replace.start)
                && source.text.is_char_boundary(candidate.replace.end)
        })
        .collect()
}

/// Parser scope accepted at a source range, used to derive completions.
pub(crate) struct ParseContext {
    pub(crate) span: Span,
    pub(crate) constructs: Vec<String>,
    pub(crate) close: Option<String>,
    pub(crate) blocked: bool,
}

/// Suggests construct-start tokens accepted at the source offset.
///
/// This uses Langmeld's default lexer. Use [`complete_at_with_lexer`] when the
/// language uses host-defined tokenization.
#[must_use]
pub fn complete_at(
    source: &SourceFile,
    language: &LanguageSpec,
    offset: usize,
) -> Vec<CompletionCandidate> {
    complete_at_with_lexer(source, language, offset, &DefaultLexer)
}

/// Suggests construct-start tokens accepted at the source offset using a host lexer.
#[must_use]
pub fn complete_at_with_lexer(
    source: &SourceFile,
    language: &LanguageSpec,
    offset: usize,
    lexer: &impl Lexer,
) -> Vec<CompletionCandidate> {
    if offset > source.text.len() || !source.text.is_char_boundary(offset) {
        return Vec::new();
    }

    let prefix_token = lexer.lex(source).tokens.into_iter().find(|token| {
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
            && token.span.end <= source.text.len()
            && source.text.is_char_boundary(token.span.start)
            && source.text.is_char_boundary(token.span.end)
            && source.text.get(token.span.start..token.span.end) == Some(token.text.as_str())
    });
    let (prefix_end, replacement, prefix) = if let Some(token) = prefix_token {
        (
            token.span.start,
            token.span,
            source.text[token.span.start..offset].to_owned(),
        )
    } else {
        (offset, Span::new(offset, offset), String::new())
    };

    let prefix_source = SourceFile::new(source.name.clone(), source.text[..prefix_end].to_owned());
    let (parsed, contexts) = parse_with_contexts(&prefix_source, language, lexer);
    if parsed.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .location
            .as_ref()
            .is_some_and(|location| location.span.end >= prefix_end)
            && diagnostic.code != "LM-PARSE-004"
    }) {
        return Vec::new();
    }

    let Some(context) = contexts
        .iter()
        .filter(|context| context.span.start <= prefix_end && prefix_end <= context.span.end)
        .min_by_key(|context| context.span.len())
    else {
        return Vec::new();
    };
    if context.blocked {
        return Vec::new();
    }

    let mut candidates = Vec::new();
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
                if !text.starts_with(&prefix)
                    || text == prefix
                    || candidates
                        .iter()
                        .any(|candidate: &CompletionCandidate| candidate.text == text)
                {
                    continue;
                }
                candidates.push(CompletionCandidate {
                    text,
                    replace: replacement,
                });
            }
        }
    }
    if let Some(close) = &context.close
        && close.starts_with(&prefix)
        && close != &prefix
        && !candidates.iter().any(|candidate| candidate.text == *close)
    {
        candidates.push(CompletionCandidate {
            text: close.clone(),
            replace: replacement,
        });
    }
    candidates
}

fn collect_first_literals(parts: &[PatternPart], output: &mut Vec<String>) {
    for part in parts {
        match part {
            PatternPart::Literal(text) => {
                output.push(text.clone());
                return;
            }
            PatternPart::Field { .. } => return,
            PatternPart::Optional(parts) => collect_first_literals(parts, output),
            PatternPart::Repeat { parts, minimum } => {
                collect_first_literals(parts, output);
                if *minimum == RepeatMinimum::One {
                    return;
                }
            }
        }
    }
}
