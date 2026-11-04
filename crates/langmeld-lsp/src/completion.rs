//! Completion protocol support and host completion callbacks.

use std::collections::BTreeSet;

use langmeld_core::{SourceFile, Span, ValidatedLanguageSpec};
use langmeld_language::{AstNode, CompletionPart, ParseResult};
use serde_json::{Value, json};

use super::{
    MAX_COMPLETION_DETAIL_BYTES, MAX_COMPLETION_INSERT_BYTES, MAX_COMPLETION_ITEMS,
    MAX_COMPLETION_LABEL_BYTES, offset_at_position,
};

/// LSP completion item kind supported by the host completion API.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CompletionItemKind {
    /// Plain text suggestion.
    #[default]
    Text,
    /// Method suggestion.
    Method,
    /// Function suggestion.
    Function,
    /// Field suggestion.
    Field,
    /// Variable suggestion.
    Variable,
    /// Class or construct suggestion.
    Class,
    /// Enum suggestion.
    Enum,
    /// Enum member suggestion.
    EnumMember,
    /// Keyword suggestion.
    Keyword,
    /// Constant suggestion.
    Constant,
}

/// Format used for a completion candidate's inserted text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CompletionInsertTextFormat {
    /// Insert text literally.
    #[default]
    PlainText,
    /// Interpret inserted text as an LSP snippet.
    Snippet,
}

impl CompletionItemKind {
    const fn lsp_number(self) -> u8 {
        match self {
            Self::Text => 1,
            Self::Method => 2,
            Self::Function => 3,
            Self::Field => 5,
            Self::Variable => 6,
            Self::Class => 7,
            Self::Enum => 13,
            Self::EnumMember => 12,
            Self::Keyword => 14,
            Self::Constant => 21,
        }
    }
}

/// A host completion suggestion. Replacement spans use UTF-8 byte offsets in
/// the current source snapshot and must end at the request cursor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CompletionCandidate {
    /// Visible completion label.
    pub label: String,
    /// Optional short description shown by the client.
    pub detail: Option<String>,
    /// LSP completion category.
    pub kind: CompletionItemKind,
    /// Text inserted when the suggestion is accepted; defaults to `label`.
    pub insert_text: Option<String>,
    /// Whether inserted text is plain text or an LSP snippet.
    pub insert_text_format: CompletionInsertTextFormat,
    /// Optional UTF-8 byte range replaced by the suggestion.
    pub replacement: Option<Span>,
}

impl CompletionCandidate {
    /// Creates a text suggestion with the supplied label.
    #[must_use]
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            ..Self::default()
        }
    }
}

/// One open source document included in completion context.
pub struct CompletionDocument<'a> {
    /// Stable document URI.
    pub uri: &'a str,
    /// Current source snapshot.
    pub source: &'a SourceFile,
    /// Parse result corresponding to the source snapshot.
    pub parsed: &'a ParseResult,
}

/// Read-only request data supplied to an embedded completion provider.
pub struct CompletionContext<'a> {
    /// Validated language definition.
    pub language: &'a ValidatedLanguageSpec,
    /// Current document URI.
    pub uri: &'a str,
    /// Current source snapshot.
    pub source: &'a SourceFile,
    /// Parse result corresponding to the source snapshot.
    pub parsed: &'a ParseResult,
    /// Open documents in deterministic URI order.
    pub workspace: &'a [CompletionDocument<'a>],
    /// Current cursor as a UTF-8 byte offset.
    pub byte_offset: usize,
    /// Current cursor line, zero-based.
    pub line: u32,
    /// Current cursor UTF-16 character offset.
    pub character: u32,
    /// Identifier-like text immediately before the cursor.
    pub prefix: &'a str,
}

pub(super) type CompletionProvider = Box<
    dyn for<'context> Fn(&CompletionContext<'context>) -> Vec<CompletionCandidate> + Send + Sync,
>;

/// A registered host completion callback for an embedded language server.
pub struct CompletionProviderRegistration {
    pub(super) provider: CompletionProvider,
}

impl CompletionProviderRegistration {
    /// Registers a callback whose typed suggestions are validated and capped
    /// before the server writes the completion response.
    #[must_use]
    pub fn new(
        provider: impl for<'context> Fn(&CompletionContext<'context>) -> Vec<CompletionCandidate>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self {
            provider: Box::new(provider),
        }
    }
}

impl super::Server {
    pub(super) fn completion_items(&self, params: &Value) -> Vec<Value> {
        let (Some(uri), Some(line), Some(character)) = (
            params.pointer("/textDocument/uri").and_then(Value::as_str),
            params.pointer("/position/line").and_then(Value::as_u64),
            params
                .pointer("/position/character")
                .and_then(Value::as_u64),
        ) else {
            return Vec::new();
        };
        let (Ok(line), Ok(character)) = (u32::try_from(line), u32::try_from(character)) else {
            return Vec::new();
        };
        let Some(document) = self.documents.get(uri) else {
            return Vec::new();
        };
        let Some(offset) = offset_at_position(&document.source.text, line, character) else {
            return Vec::new();
        };
        let text = &document.source.text;
        let (word_start, prefix) = completion_prefix(text, offset);
        let mut items = Vec::new();
        let mut labels = BTreeSet::new();
        let has_contextual_fields = self
            .language
            .spec()
            .constructs
            .iter()
            .flat_map(|construct| &construct.fields)
            .any(|field| field_kind_has_enum(&field.kind) || field_kind_has_reference(&field.kind));
        let (uris, files) = self.workspace_snapshot();
        let Some(source_index) = uris.iter().position(|open_uri| open_uri == uri) else {
            return Vec::new();
        };
        if has_contextual_fields {
            let analysis = self.analyze_files(&files);
            if !analysis.resource_limited
                && let Some(fact) = analysis
                    .type_facts
                    .iter()
                    .filter(|fact| {
                        fact.source_index == source_index
                            && fact.span.start <= offset
                            && offset <= fact.span.end
                    })
                    .min_by_key(|fact| fact.span.end - fact.span.start)
            {
                if let langmeld_core::FieldKind::Enum { values }
                | langmeld_core::FieldKind::NamedEnum { values, .. } = &fact.expected
                {
                    for value in values
                        .iter()
                        .filter(|value| value.starts_with(prefix))
                        .take(MAX_COMPLETION_ITEMS)
                    {
                        if let Some(item) = completion_item(
                            value,
                            value,
                            CompletionItemKind::EnumMember,
                            None,
                            Span::new(word_start, offset),
                            text,
                        ) {
                            push_unique_completion(&mut items, &mut labels, item);
                        }
                    }
                } else if let langmeld_core::FieldKind::Reference { target } = &fact.expected
                    && let Some((symbol_scope, ancestors)) = find_reference_context(
                        &self.language,
                        &document.parsed.program.nodes,
                        fact.span,
                    )
                    && symbol_scope == *target
                {
                    let Some(scope) = self
                        .language
                        .spec()
                        .constructs
                        .iter()
                        .filter_map(|construct| construct.symbol.as_ref())
                        .find(|symbol| symbol.kind == *target)
                        .map(|symbol| symbol.scope.as_ref())
                    else {
                        return items;
                    };
                    let visible_scopes = visible_symbol_scopes(scope, &ancestors);
                    for (workspace_uri, (_, program)) in uris.iter().zip(&files) {
                        let mut ancestor_nodes = Vec::new();
                        collect_visible_declarations(
                            &self.language,
                            &program.nodes,
                            &mut ancestor_nodes,
                            target,
                            &visible_scopes,
                            workspace_uri,
                            prefix,
                            &mut items,
                            &mut labels,
                            text,
                            offset,
                            word_start,
                        );
                        if items.len() >= MAX_COMPLETION_ITEMS {
                            break;
                        }
                    }
                }
            }
        }
        let frontier = document
            .parsed
            .completion_frontiers
            .iter()
            .filter(|frontier| frontier.span.start <= offset && offset <= frontier.span.end)
            .min_by_key(|frontier| frontier.span.end - frontier.span.start);
        let has_frontier = frontier.is_some();
        if let Some(frontier) = frontier.filter(|frontier| !frontier.ambiguous) {
            for capture in &frontier.captures {
                if let langmeld_core::FieldKind::Enum { values }
                | langmeld_core::FieldKind::NamedEnum { values, .. } = &capture.kind
                {
                    for value in values
                        .iter()
                        .filter(|value| value.starts_with(prefix))
                        .take(MAX_COMPLETION_ITEMS.saturating_sub(items.len()))
                    {
                        if let Some(item) = completion_item(
                            value,
                            value,
                            CompletionItemKind::EnumMember,
                            Some(&capture.field),
                            Span::new(word_start, offset),
                            text,
                        ) {
                            push_unique_completion(&mut items, &mut labels, item);
                        }
                    }
                }
            }
        }
        if items.len() < MAX_COMPLETION_ITEMS
            && let Some(frontier) = frontier
            && let Some(parts) = &frontier.continuation
            && let Some(snippet) = completion_continuation_snippet(parts)
            && let Some(replacement) =
                continuation_replacement_range(frontier.span, offset, word_start)
            && let Some(mut item) = completion_item(
                "Continue syntax",
                &snippet,
                CompletionItemKind::Text,
                None,
                replacement,
                text,
            )
        {
            item["insertTextFormat"] = json!(2);
            push_unique_completion(&mut items, &mut labels, item);
        }
        if !has_frontier && items.len() < MAX_COMPLETION_ITEMS {
            let spec = self.language.spec();
            let block = document
                .parsed
                .completion_blocks
                .iter()
                .filter(|block| block.span.start <= offset && offset <= block.span.end)
                .min_by_key(|block| block.span.end - block.span.start);
            if let Some(block) = block {
                for construct in spec.constructs.iter().filter(|construct| {
                    block.allowed.iter().any(|allowed| {
                        allowed == &construct.id || construct.implements.contains(allowed)
                    }) && construct.id.starts_with(prefix)
                }) {
                    let Some(pattern) = construct.syntax.first() else {
                        continue;
                    };
                    let Some(snippet) = construct_pattern_snippet(pattern) else {
                        continue;
                    };
                    if let Some(mut item) = completion_item(
                        &construct.id,
                        &snippet,
                        CompletionItemKind::Class,
                        Some(&block.parent),
                        Span::new(word_start, offset),
                        text,
                    ) {
                        item["insertTextFormat"] = json!(2);
                        push_unique_completion(&mut items, &mut labels, item);
                    }
                    if items.len() >= MAX_COMPLETION_ITEMS {
                        break;
                    }
                }
            }
        }
        let workspace = uris
            .iter()
            .filter_map(|open_uri| {
                let open = self.documents.get(open_uri)?;
                Some(CompletionDocument {
                    uri: open_uri,
                    source: &open.source,
                    parsed: &open.parsed,
                })
            })
            .collect::<Vec<_>>();
        let context = CompletionContext {
            language: &self.language,
            uri,
            source: &document.source,
            parsed: &document.parsed,
            workspace: &workspace,
            byte_offset: offset,
            line,
            character,
            prefix,
        };
        for provider in &self.completion_providers {
            if items.len() >= MAX_COMPLETION_ITEMS {
                break;
            }
            for candidate in provider(&context) {
                if items.len() >= MAX_COMPLETION_ITEMS {
                    break;
                }
                if let Some(item) = completion_candidate_item(candidate, text, offset, word_start) {
                    push_unique_completion(&mut items, &mut labels, item);
                }
            }
            if items.len() >= MAX_COMPLETION_ITEMS {
                break;
            }
        }
        if items.len() < MAX_COMPLETION_ITEMS {
            for item in keyword_completion_items(text, offset, self.parser.language().keywords()) {
                if items.len() >= MAX_COMPLETION_ITEMS {
                    break;
                }
                push_unique_completion(&mut items, &mut labels, item);
            }
        }
        items
    }
}

pub(super) fn field_kind_has_enum(kind: &langmeld_core::FieldKind) -> bool {
    use langmeld_core::FieldKind;
    match kind {
        FieldKind::Enum { .. } | FieldKind::NamedEnum { .. } => true,
        FieldKind::Properties { fields, .. } => {
            fields.iter().any(|field| field_kind_has_enum(&field.kind))
        }
        FieldKind::List { item } => field_kind_has_enum(item),
        FieldKind::Map { value, .. } => field_kind_has_enum(value),
        FieldKind::Identifier
        | FieldKind::Number
        | FieldKind::Text
        | FieldKind::Reference { .. }
        | FieldKind::Expression { .. } => false,
    }
}

pub(super) fn field_kind_has_reference(kind: &langmeld_core::FieldKind) -> bool {
    use langmeld_core::FieldKind;
    match kind {
        FieldKind::Reference { .. } => true,
        FieldKind::Properties { fields, .. } => fields
            .iter()
            .any(|field| field_kind_has_reference(&field.kind)),
        FieldKind::List { item } => field_kind_has_reference(item),
        FieldKind::Map { value, .. } => field_kind_has_reference(value),
        FieldKind::Identifier
        | FieldKind::Number
        | FieldKind::Text
        | FieldKind::Enum { .. }
        | FieldKind::NamedEnum { .. }
        | FieldKind::Expression { .. } => false,
    }
}

fn find_reference_context<'a>(
    language: &'a ValidatedLanguageSpec,
    nodes: &'a [AstNode],
    span: Span,
) -> Option<(String, Vec<&'a AstNode>)> {
    fn visit<'a>(
        language: &'a ValidatedLanguageSpec,
        nodes: &'a [AstNode],
        span: Span,
        ancestors: &mut Vec<&'a AstNode>,
    ) -> Option<(String, Vec<&'a AstNode>)> {
        for node in nodes {
            ancestors.push(node);
            if let Some(construct) = language.construct(&node.construct) {
                for field in &construct.fields {
                    if let Some(captured) = node.fields.get(&field.name)
                        && let Some(target) = reference_target_at(&field.kind, captured, span)
                    {
                        return Some((target.to_owned(), ancestors.clone()));
                    }
                }
            }
            if let Some(found) = visit(language, &node.children, span, ancestors) {
                return Some(found);
            }
            ancestors.pop();
        }
        None
    }

    visit(language, nodes, span, &mut Vec::new())
}

fn reference_target_at<'a>(
    kind: &'a langmeld_core::FieldKind,
    captured: &'a langmeld_language::CapturedValue,
    span: Span,
) -> Option<&'a str> {
    use langmeld_core::FieldKind;
    use langmeld_language::Value;
    match (kind, &captured.value) {
        (FieldKind::Reference { target }, Value::Reference(_)) if captured.span == span => {
            Some(target)
        }
        (FieldKind::Properties { fields, .. }, Value::Properties(values)) => fields
            .iter()
            .find_map(|field| reference_target_at(&field.kind, values.get(&field.name)?, span)),
        (FieldKind::List { item }, Value::List(values)) => values
            .iter()
            .find_map(|value| reference_target_at(item, value, span)),
        (FieldKind::Map { value, .. }, Value::Map(entries)) => entries
            .iter()
            .find_map(|entry| reference_target_at(value, &entry.value, span)),
        _ => None,
    }
}

type SymbolScopeKey = Vec<(String, String)>;

fn visible_symbol_scopes(
    scope: Option<&langmeld_core::ScopeSpec>,
    ancestors: &[&AstNode],
) -> BTreeSet<SymbolScopeKey> {
    let Some(scope) = scope else {
        return [Vec::new()].into_iter().collect();
    };
    let matching = ancestors
        .iter()
        .filter(|ancestor| ancestor.construct == scope.construct)
        .filter_map(|ancestor| {
            let value = &ancestor.fields.get(&scope.name_field)?.value;
            let langmeld_language::Value::Identifier(name) = value else {
                return None;
            };
            Some((scope.construct.clone(), name.clone()))
        })
        .collect::<Vec<_>>();
    if matching.is_empty() {
        return BTreeSet::new();
    }
    match scope.lookup {
        langmeld_core::ScopeLookup::Exact => {
            [vec![matching.last().expect("nonempty scope").clone()]]
                .into_iter()
                .collect()
        }
        langmeld_core::ScopeLookup::LexicalParents => (1..=matching.len())
            .rev()
            .map(|length| matching[..length].to_vec())
            .collect(),
    }
}

fn declaration_scope_key(
    scope: Option<&langmeld_core::ScopeSpec>,
    ancestors: &[&AstNode],
) -> Option<SymbolScopeKey> {
    let Some(scope) = scope else {
        return Some(Vec::new());
    };
    let matching = ancestors
        .iter()
        .filter(|ancestor| ancestor.construct == scope.construct)
        .filter_map(|ancestor| {
            let value = &ancestor.fields.get(&scope.name_field)?.value;
            let langmeld_language::Value::Identifier(name) = value else {
                return None;
            };
            Some((scope.construct.clone(), name.clone()))
        })
        .collect::<Vec<_>>();
    if matching.is_empty() {
        None
    } else if scope.lookup == langmeld_core::ScopeLookup::Exact {
        Some(vec![matching.last()?.clone()])
    } else {
        Some(matching)
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_visible_declarations<'a>(
    language: &ValidatedLanguageSpec,
    nodes: &'a [AstNode],
    ancestors: &mut Vec<&'a AstNode>,
    target: &str,
    visible_scopes: &BTreeSet<SymbolScopeKey>,
    uri: &str,
    prefix: &str,
    items: &mut Vec<Value>,
    labels: &mut BTreeSet<String>,
    source: &str,
    offset: usize,
    word_start: usize,
) {
    for node in nodes {
        if items.len() >= MAX_COMPLETION_ITEMS {
            break;
        }
        ancestors.push(node);
        let declaration = language
            .construct(&node.construct)
            .and_then(|construct| construct.symbol.as_ref())
            .filter(|symbol| symbol.kind == target)
            .and_then(|symbol| {
                let langmeld_language::Value::Identifier(name) =
                    &node.fields.get(&symbol.name_field)?.value
                else {
                    return None;
                };
                Some((symbol.scope.as_ref(), name.as_str()))
            });
        if let Some((declaration_scope, name)) = declaration
            && name.starts_with(prefix)
            && declaration_scope_key(declaration_scope, ancestors)
                .is_some_and(|key| visible_scopes.contains(&key))
            && let Some(item) = completion_item(
                name,
                name,
                CompletionItemKind::Variable,
                Some(&format!("{target} in {uri}")),
                Span::new(word_start, offset),
                source,
            )
        {
            push_unique_completion(items, labels, item);
        }
        collect_visible_declarations(
            language,
            &node.children,
            ancestors,
            target,
            visible_scopes,
            uri,
            prefix,
            items,
            labels,
            source,
            offset,
            word_start,
        );
        ancestors.pop();
    }
}

pub(super) fn completion_prefix(text: &str, offset: usize) -> (usize, &str) {
    if offset > text.len() || !text.is_char_boundary(offset) {
        return (offset.min(text.len()), "");
    }
    let line_start = text[..offset]
        .rfind(['\r', '\n'])
        .map_or(0, |index| index + 1);
    let line_prefix = &text[line_start..offset];
    let word_start = line_prefix
        .char_indices()
        .rev()
        .find(|(_, value)| !value.is_alphanumeric() && *value != '_' && *value != '-')
        .map_or(line_start, |(index, value)| {
            line_start + index + value.len_utf8()
        });
    (word_start, &text[word_start..offset])
}

pub(super) fn construct_pattern_snippet(pattern: &str) -> Option<String> {
    let mut output = String::with_capacity(pattern.len());
    let mut characters = pattern.char_indices().peekable();
    let mut tab_stop = 1;
    while let Some((offset, character)) = characters.next() {
        match character {
            '\\' => {
                let (_, escaped) = characters.next()?;
                push_snippet_literal(&mut output, escaped);
            }
            '{' => {
                let remaining = &pattern[offset + character.len_utf8()..];
                let Some(end) = remaining.find('}') else {
                    if offset + character.len_utf8() == pattern.len() {
                        push_snippet_literal(&mut output, character);
                        continue;
                    }
                    return None;
                };
                let field = &remaining[..end];
                if field.is_empty()
                    || !field
                        .chars()
                        .all(|value| value.is_ascii_alphanumeric() || value == '_')
                {
                    return None;
                }
                for _ in 0..=field.chars().count() {
                    characters.next()?;
                }
                output.push_str(&format!("${{{tab_stop}:{field}}}"));
                tab_stop += 1;
            }
            '[' | ']' | '(' | ')' | '|' | '*' | '+' | '@' => return None,
            value => push_snippet_literal(&mut output, value),
        }
    }
    Some(output)
}

pub(super) fn completion_continuation_snippet(parts: &[CompletionPart]) -> Option<String> {
    let mut output = String::new();
    let mut tab_stop = 1;
    let mut previous = None::<String>;
    for part in parts {
        let current = match part {
            CompletionPart::Literal(value) => value.as_str(),
            CompletionPart::Capture(value) => value.as_str(),
        };
        let word_like = |value: &str| {
            value
                .chars()
                .next()
                .is_some_and(|character| character.is_alphanumeric() || character == '_')
        };
        if let Some(previous) = previous.as_deref()
            && (previous == "->"
                || current == "->"
                || (word_like(previous) && word_like(current))
                || matches!(previous, "," | ";"))
        {
            output.push(' ');
        }
        match part {
            CompletionPart::Literal(value) => {
                for character in value.chars() {
                    push_snippet_literal(&mut output, character);
                }
            }
            CompletionPart::Capture(field) => {
                output.push_str(&format!("${{{tab_stop}:"));
                for character in field.chars() {
                    push_snippet_literal(&mut output, character);
                }
                output.push('}');
                tab_stop += 1;
            }
        }
        if output.len() > MAX_COMPLETION_INSERT_BYTES {
            return None;
        }
        previous = Some(current.to_owned());
    }
    (!output.is_empty()).then_some(output)
}

fn continuation_replacement_range(
    frontier: Span,
    offset: usize,
    word_start: usize,
) -> Option<Span> {
    if frontier.start > frontier.end || offset > frontier.end {
        return None;
    }
    if frontier.start < offset {
        Some(Span::new(word_start.max(frontier.start), offset))
    } else {
        Some(Span::new(offset, offset))
    }
}

fn push_snippet_literal(output: &mut String, value: char) {
    if matches!(value, '$' | '}' | '\\') {
        output.push('\\');
    }
    output.push(value);
}

fn completion_item(
    label: &str,
    insert_text: &str,
    kind: CompletionItemKind,
    detail: Option<&str>,
    replacement: Span,
    source: &str,
) -> Option<Value> {
    if label.is_empty()
        || label.len() > MAX_COMPLETION_LABEL_BYTES
        || insert_text.len() > MAX_COMPLETION_INSERT_BYTES
        || detail.is_some_and(|value| value.len() > MAX_COMPLETION_DETAIL_BYTES)
        || replacement.start > replacement.end
        || replacement.end > source.len()
        || !source.is_char_boundary(replacement.start)
        || !source.is_char_boundary(replacement.end)
    {
        return None;
    }
    let mut item = json!({
        "label": label,
        "kind": kind.lsp_number(),
        "filterText": label,
        "textEdit": {
            "range": {
                "start": super::lsp_position(source, replacement.start),
                "end": super::lsp_position(source, replacement.end),
            },
            "newText": insert_text,
        }
    });
    if let Some(detail) = detail {
        item["detail"] = json!(detail);
    }
    Some(item)
}

fn completion_candidate_item(
    candidate: CompletionCandidate,
    source: &str,
    offset: usize,
    word_start: usize,
) -> Option<Value> {
    let replacement = candidate
        .replacement
        .unwrap_or_else(|| Span::new(word_start, offset));
    if replacement.end != offset {
        return None;
    }
    let insertion = candidate.insert_text.as_deref().unwrap_or(&candidate.label);
    let mut item = completion_item(
        &candidate.label,
        insertion,
        candidate.kind,
        candidate.detail.as_deref(),
        replacement,
        source,
    )?;
    if candidate.insert_text_format == CompletionInsertTextFormat::Snippet {
        item["insertTextFormat"] = json!(2);
    }
    Some(item)
}

fn push_unique_completion(items: &mut Vec<Value>, labels: &mut BTreeSet<String>, item: Value) {
    if items.len() >= MAX_COMPLETION_ITEMS {
        return;
    }
    if let Some(label) = item["label"].as_str()
        && labels.insert(label.to_owned())
    {
        items.push(item);
    }
}

pub(super) fn keyword_completion_items<'a>(
    text: &str,
    offset: usize,
    keywords: impl Iterator<Item = &'a str>,
) -> Vec<Value> {
    let (word_start, prefix) = completion_prefix(text, offset);
    if offset > text.len() || !text.is_char_boundary(offset) {
        return Vec::new();
    }
    keywords
        .filter(|keyword| keyword.starts_with(prefix))
        .take(MAX_COMPLETION_ITEMS)
        .filter_map(|keyword| {
            completion_item(
                keyword,
                keyword,
                CompletionItemKind::Keyword,
                None,
                Span::new(word_start, offset),
                text,
            )
        })
        .collect()
}
