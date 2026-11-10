use langmeld_core::{SourceFile, Span};

use crate::parser::{ConfiguredLexer, Lexer, lex_bounded, parse_with_contexts};
use crate::{
    AnalysisDocument, LanguageCompleter, LanguageParser, LanguageSpec, ParseLimits, ParsedDocument,
    PatternPart, RepeatMinimum,
};

/// Semantic category for a completion suggestion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionKind {
    /// Plain text or an unspecified item.
    Text,
    /// A method.
    Method,
    /// A function.
    Function,
    /// A constructor.
    Constructor,
    /// A field.
    Field,
    /// A variable.
    Variable,
    /// A class.
    Class,
    /// An interface.
    Interface,
    /// A module.
    Module,
    /// A property.
    Property,
    /// A unit.
    Unit,
    /// A value.
    Value,
    /// An enumeration.
    Enum,
    /// A keyword.
    Keyword,
    /// A snippet.
    Snippet,
    /// A color.
    Color,
    /// A file.
    File,
    /// A reference.
    Reference,
    /// A folder.
    Folder,
    /// An enumeration member.
    EnumMember,
    /// A constant.
    Constant,
    /// A structure or record type.
    Struct,
    /// An event.
    Event,
    /// An operator.
    Operator,
    /// A type parameter.
    TypeParameter,
}

impl CompletionKind {
    /// All completion kinds and their canonical `.langmeld` names.
    pub const ALL: [Self; 25] = [
        Self::Text,
        Self::Method,
        Self::Function,
        Self::Constructor,
        Self::Field,
        Self::Variable,
        Self::Class,
        Self::Interface,
        Self::Module,
        Self::Property,
        Self::Unit,
        Self::Value,
        Self::Enum,
        Self::Keyword,
        Self::Snippet,
        Self::Color,
        Self::File,
        Self::Reference,
        Self::Folder,
        Self::EnumMember,
        Self::Constant,
        Self::Struct,
        Self::Event,
        Self::Operator,
        Self::TypeParameter,
    ];

    /// Returns the canonical `.langmeld` spelling for this kind.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Method => "method",
            Self::Function => "function",
            Self::Constructor => "constructor",
            Self::Field => "field",
            Self::Variable => "variable",
            Self::Class => "class",
            Self::Interface => "interface",
            Self::Module => "module",
            Self::Property => "property",
            Self::Unit => "unit",
            Self::Value => "value",
            Self::Enum => "enum",
            Self::Keyword => "keyword",
            Self::Snippet => "snippet",
            Self::Color => "color",
            Self::File => "file",
            Self::Reference => "reference",
            Self::Folder => "folder",
            Self::EnumMember => "enum_member",
            Self::Constant => "constant",
            Self::Struct => "struct",
            Self::Event => "event",
            Self::Operator => "operator",
            Self::TypeParameter => "type_parameter",
        }
    }

    /// Parses a canonical `.langmeld` completion-kind name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == name)
    }
}

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
    pub(crate) values: Vec<String>,
    pub(crate) value_kind: CompletionKind,
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
    let lexer = ConfiguredLexer {
        rules: language.lexical_rules(),
    };
    complete_at_with_lexer(source, language, offset, &lexer)
}

/// Suggests construct-start tokens accepted at the source offset using a host lexer.
#[must_use]
pub fn complete_at_with_lexer(
    source: &SourceFile,
    language: &LanguageSpec,
    offset: usize,
    lexer: &impl Lexer,
) -> Vec<CompletionCandidate> {
    let limits = ParseLimits::default();
    if offset > source.text.len()
        || !source.text.is_char_boundary(offset)
        || source.text.len() > limits.max_source_bytes
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
    let is_choice_value_position = contexts.iter().any(|context| {
        context.span == Span::new(prefix_end, prefix_end) && !context.values.is_empty()
    });
    if parsed.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .location
            .as_ref()
            .is_some_and(|location| location.span.end >= prefix_end)
            && diagnostic.code != "LM-PARSE-004"
            && !(is_choice_value_position && diagnostic.code == "LM-PARSE-002")
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
    for text in &context.values {
        push_candidate(
            &mut candidates,
            text,
            &prefix,
            replacement,
            Some(context.value_kind),
        );
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
                    &mut candidates,
                    &text,
                    &prefix,
                    replacement,
                    construct.completion_kind(),
                );
            }
        }
    }
    if let Some(close) = &context.close
        && close.starts_with(&prefix)
        && close != &prefix
        && !candidates.iter().any(|candidate| candidate.text == *close)
    {
        candidates.push(CompletionCandidate::new(close.clone(), replacement));
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
            PatternPart::Choice { values, .. } => {
                output.extend(values.iter().cloned());
                return;
            }
            PatternPart::Field { .. } | PatternPart::Expression { .. } => return,
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
