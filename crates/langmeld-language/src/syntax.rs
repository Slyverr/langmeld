//! Lossless syntax trees and editor-facing token classification.

use std::{
    collections::{HashMap, HashSet},
    ops::Range,
};

use langmeld_core::{ConstructSpec, FieldKind, SourceFile, Span, ValidatedLanguageSpec};

use crate::{
    AstNode, LanguageToken, LanguageTokenKind, ParseResult, PreparedLanguage, Program, Value,
    lexer::lex_source,
};

/// A source-file or language construct node in the parent-aware syntax tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxNode {
    /// Structural role represented by this node.
    pub role: SyntaxNodeRole,
    /// Construct identifier for construct nodes; absent on the source root.
    pub construct: Option<String>,
    /// Selected syntax variant for construct nodes.
    pub syntax_variant: Option<String>,
    /// Declared field or property name for value nodes.
    pub field: Option<String>,
    /// Zero-based list item or map entry position.
    pub index: Option<usize>,
    /// Byte range covered by this node.
    pub span: Span,
    /// Child construct/value nodes and indices into the parse result token table.
    pub children: Vec<SyntaxElement>,
}

/// The semantic shape represented by a syntax node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyntaxNodeRole {
    /// Root containing all source tokens and top-level constructs.
    SourceFile,
    /// Construct declared by the language definition.
    Construct,
    /// Captured field value.
    Field,
    /// Named property in a property record.
    Property,
    /// Identifier naming a property in a property record.
    PropertyName,
    /// Indexed item in a list-valued field.
    ListItem,
    /// Indexed key/value pair in a map-valued field.
    MapEntry,
    /// Key token of a map entry.
    MapKey,
    /// Value of a map entry.
    MapValue,
}

/// A nested syntax node or an indexed lossless token leaf.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyntaxElement {
    /// Nested source, construct, or value node.
    Node(Box<SyntaxNode>),
    /// Index into [`ParseResult::syntax_tokens`].
    Token(usize),
}

/// A token or trivia segment in exact source order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyntaxToken {
    /// Lexical role of the source segment.
    pub kind: SyntaxTokenKind,
    /// UTF-8 byte range in the source snapshot.
    pub span: Span,
}

/// Token, trivia, and synthetic-token categories retained by the parser.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyntaxTokenKind {
    /// Whitespace omitted by the lexical stream, excluding line breaks.
    Whitespace,
    /// Identifier token.
    Identifier,
    /// Numeric token.
    Number,
    /// Quoted string token.
    String,
    /// Punctuation token.
    Punctuation,
    /// Arrow token.
    Arrow,
    /// Comment token provided by the lexer.
    Comment,
    /// Line break, preserving CR, LF, or CRLF in its span.
    Newline,
    /// A host-parsed source region with no generic lexical classification.
    Opaque,
    /// Unclassified source characters or input left after lexing stopped.
    Unknown,
    /// Synthetic zero-width right brace inserted at end of incomplete input.
    MissingRightBrace,
    /// Synthetic zero-width right bracket inserted at end of incomplete input.
    MissingRightBracket,
}

/// A source span and shared editor highlighting category.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Classification {
    /// UTF-8 byte range in the source snapshot.
    pub span: Span,
    /// Stable category name suitable for editor adapters.
    pub kind: &'static str,
}

/// Shared classification legend for editor adapters.
pub const CLASSIFICATION_TYPES: [&str; 10] = [
    "keyword",
    "operator",
    "string",
    "number",
    "comment",
    "identifier",
    "declaration",
    "reference",
    "enumMember",
    "property",
];

pub(super) fn lossless_syntax_tokens(
    source: &SourceFile,
    tokens: &[LanguageToken],
) -> Vec<SyntaxToken> {
    let mut syntax = Vec::new();
    let mut cursor = 0;
    for token in tokens {
        if matches!(token.kind, LanguageTokenKind::End) {
            continue;
        }
        append_gap(source, Span::new(cursor, token.span.start), &mut syntax);
        syntax.push(SyntaxToken {
            kind: syntax_token_kind(&token.kind),
            span: token.span,
        });
        cursor = token.span.end;
    }
    append_gap(source, Span::new(cursor, source.text.len()), &mut syntax);
    syntax
}

fn append_gap(source: &SourceFile, gap: Span, output: &mut Vec<SyntaxToken>) {
    let Some(text) = source.text.get(gap.start..gap.end) else {
        return;
    };
    let mut run_start = gap.start;
    let mut run_kind = None;
    let mut chars = text.char_indices().peekable();
    while let Some((relative, character)) = chars.next() {
        let start = gap.start + relative;
        let kind = if character == '\r' {
            if chars.peek().is_some_and(|(_, next)| *next == '\n') {
                chars.next();
            }
            SyntaxTokenKind::Newline
        } else if character == '\n' {
            SyntaxTokenKind::Newline
        } else if character.is_whitespace() {
            SyntaxTokenKind::Whitespace
        } else {
            SyntaxTokenKind::Unknown
        };
        let end = if let Some((next, _)) = chars.peek() {
            gap.start + next
        } else {
            gap.end
        };
        if run_kind != Some(kind) {
            if let Some(previous) = run_kind {
                output.push(SyntaxToken {
                    kind: previous,
                    span: Span::new(run_start, start),
                });
            }
            run_start = start;
            run_kind = Some(kind);
        }
        if end == gap.end
            && let Some(current) = run_kind.take()
        {
            output.push(SyntaxToken {
                kind: current,
                span: Span::new(run_start, gap.end),
            });
        }
    }
}

fn syntax_token_kind(kind: &LanguageTokenKind) -> SyntaxTokenKind {
    match kind {
        LanguageTokenKind::Identifier(_) => SyntaxTokenKind::Identifier,
        LanguageTokenKind::Number(_) => SyntaxTokenKind::Number,
        LanguageTokenKind::Text(_) => SyntaxTokenKind::String,
        LanguageTokenKind::Punctuation(_) => SyntaxTokenKind::Punctuation,
        LanguageTokenKind::Arrow => SyntaxTokenKind::Arrow,
        LanguageTokenKind::Unknown(_) => SyntaxTokenKind::Unknown,
        LanguageTokenKind::Opaque => SyntaxTokenKind::Opaque,
        LanguageTokenKind::Comment => SyntaxTokenKind::Comment,
        LanguageTokenKind::Newline => SyntaxTokenKind::Newline,
        LanguageTokenKind::End => SyntaxTokenKind::Unknown,
    }
}

pub(super) fn append_missing_tokens(
    source: &SourceFile,
    diagnostics: &[langmeld_core::Diagnostic],
    tokens: &mut Vec<SyntaxToken>,
) {
    let eof = Span::new(source.text.len(), source.text.len());
    for diagnostic in diagnostics {
        let kind = match diagnostic.code.as_str() {
            "LM1204" | "LM1217" | "LM1223" => SyntaxTokenKind::MissingRightBrace,
            "LM1219" => SyntaxTokenKind::MissingRightBracket,
            _ => continue,
        };
        tokens.push(SyntaxToken { kind, span: eof });
    }
}

pub(super) fn build_syntax_tree(
    program: &Program,
    tokens: &[SyntaxToken],
    source: &SourceFile,
) -> SyntaxNode {
    let context = ValueSpecContext { source, tokens };
    let root = NodeSpec {
        role: SyntaxNodeRole::SourceFile,
        construct: None,
        syntax_variant: None,
        field: None,
        index: None,
        span: Span::new(0, source.text.len()),
        children: program
            .nodes
            .iter()
            .map(|node| construct_spec(node, &context))
            .collect(),
    };
    build_node(&root, tokens, 0..tokens.len())
}

struct NodeSpec {
    role: SyntaxNodeRole,
    construct: Option<String>,
    syntax_variant: Option<String>,
    field: Option<String>,
    index: Option<usize>,
    span: Span,
    children: Vec<NodeSpec>,
}

fn construct_spec(ast: &AstNode, context: &ValueSpecContext<'_>) -> NodeSpec {
    let mut children = ast
        .children
        .iter()
        .map(|node| construct_spec(node, context))
        .collect::<Vec<_>>();
    children.extend(ast.fields.iter().map(|(name, captured)| {
        context.value_spec(
            SyntaxNodeRole::Field,
            Some(name.clone()),
            None,
            captured.span,
            &captured.value,
            0,
        )
    }));
    sort_specs(&mut children);
    NodeSpec {
        role: SyntaxNodeRole::Construct,
        construct: Some(ast.construct.clone()),
        syntax_variant: Some(ast.syntax_variant.clone()),
        field: None,
        index: None,
        span: ast.span,
        children,
    }
}

struct ValueSpecContext<'a> {
    source: &'a SourceFile,
    tokens: &'a [SyntaxToken],
}

impl ValueSpecContext<'_> {
    fn value_spec(
        &self,
        role: SyntaxNodeRole,
        field: Option<String>,
        index: Option<usize>,
        mut span: Span,
        value: &Value,
        depth: usize,
    ) -> NodeSpec {
        let mut children = if depth >= crate::MAX_NESTING_DEPTH {
            Vec::new()
        } else {
            match value {
                Value::Properties(properties) => properties
                    .iter()
                    .map(|(name, captured)| {
                        self.value_spec(
                            SyntaxNodeRole::Property,
                            Some(name.clone()),
                            None,
                            captured.span,
                            &captured.value,
                            depth + 1,
                        )
                    })
                    .collect(),
                Value::List(items) => items
                    .iter()
                    .enumerate()
                    .map(|(item_index, captured)| {
                        self.value_spec(
                            SyntaxNodeRole::ListItem,
                            field.clone(),
                            Some(item_index),
                            captured.span,
                            &captured.value,
                            depth + 1,
                        )
                    })
                    .collect(),
                Value::Map(entries) => entries
                    .iter()
                    .enumerate()
                    .map(|(entry_index, entry)| {
                        map_entry_spec(self, field.clone(), entry_index, entry, depth + 1)
                    })
                    .collect(),
                Value::Identifier(_)
                | Value::Number(_)
                | Value::Text(_)
                | Value::Enum(_)
                | Value::Reference(_) => Vec::new(),
            }
        };
        if role == SyntaxNodeRole::Property
            && let Some(name) = field.as_deref()
            && let Some(name_span) = property_name_span(name, span, self.source, self.tokens)
        {
            span.start = name_span.start;
            children.push(NodeSpec {
                role: SyntaxNodeRole::PropertyName,
                construct: None,
                syntax_variant: None,
                field: field.clone(),
                index: None,
                span: name_span,
                children: Vec::new(),
            });
        }
        sort_specs(&mut children);
        NodeSpec {
            role,
            construct: None,
            syntax_variant: None,
            field,
            index,
            span,
            children,
        }
    }
}

fn map_entry_spec(
    context: &ValueSpecContext<'_>,
    field: Option<String>,
    index: usize,
    entry: &crate::MapEntry,
    depth: usize,
) -> NodeSpec {
    let key = NodeSpec {
        role: SyntaxNodeRole::MapKey,
        construct: None,
        syntax_variant: None,
        field: field.clone(),
        index: Some(index),
        span: entry.key_span,
        children: Vec::new(),
    };
    let value = context.value_spec(
        SyntaxNodeRole::MapValue,
        field.clone(),
        Some(index),
        entry.value.span,
        &entry.value.value,
        depth,
    );
    NodeSpec {
        role: SyntaxNodeRole::MapEntry,
        construct: None,
        syntax_variant: None,
        field,
        index: Some(index),
        span: Span::new(entry.key_span.start, entry.value.span.end),
        children: vec![key, value],
    }
}

fn property_name_span(
    name: &str,
    value_span: Span,
    source: &SourceFile,
    tokens: &[SyntaxToken],
) -> Option<Span> {
    let first_candidate = tokens.partition_point(|token| token.span.end <= value_span.start);
    let value_index = tokens[first_candidate..]
        .iter()
        .position(|token| token.span.start < value_span.end)?
        + first_candidate;
    let mut preceding = tokens[..value_index].iter().rev().filter(|token| {
        !matches!(
            token.kind,
            SyntaxTokenKind::Whitespace | SyntaxTokenKind::Newline | SyntaxTokenKind::Comment
        )
    });
    let colon = preceding.next()?;
    let name_token = preceding.next()?;
    if source.text.get(colon.span.start..colon.span.end)? != ":"
        || source
            .text
            .get(name_token.span.start..name_token.span.end)?
            != name
    {
        return None;
    }
    Some(name_token.span)
}

fn sort_specs(specs: &mut [NodeSpec]) {
    specs.sort_by_key(|spec| (spec.span.start, spec.span.end));
}

fn build_node(spec: &NodeSpec, tokens: &[SyntaxToken], range: Range<usize>) -> SyntaxNode {
    let mut child_ranges = vec![0..0; spec.children.len()];
    let mut cursor = range.start;
    for (child_index, child) in spec.children.iter().enumerate() {
        while cursor < range.end {
            let token = tokens[cursor];
            if token.span.start >= token.span.end || token.span.start >= child.span.end {
                break;
            }
            if token.span.end <= child.span.start || token.span.start < child.span.start {
                cursor += 1;
                continue;
            }
            break;
        }
        let start = cursor;
        while cursor < range.end
            && tokens[cursor].span.start < tokens[cursor].span.end
            && tokens[cursor].span.start >= child.span.start
            && tokens[cursor].span.end <= child.span.end
        {
            cursor += 1;
        }
        child_ranges[child_index] = start..cursor;
    }

    let mut assigned = vec![false; range.len()];
    let mut elements = Vec::new();
    for (index, child) in spec.children.iter().enumerate() {
        let child_range = child_ranges[index].clone();
        for token_index in child_range.clone() {
            assigned[token_index - range.start] = true;
        }
        elements.push((
            child.span.start,
            child.span.end,
            1_u8,
            SyntaxElement::Node(Box::new(build_node(child, tokens, child_range))),
        ));
    }
    for token_index in range.clone() {
        if !assigned[token_index - range.start] {
            let token = tokens[token_index];
            elements.push((
                token.span.start,
                token.span.end,
                0_u8,
                SyntaxElement::Token(token_index),
            ));
        }
    }
    elements.sort_by_key(|element| (element.0, element.1, element.2));
    SyntaxNode {
        role: spec.role,
        construct: spec.construct.clone(),
        syntax_variant: spec.syntax_variant.clone(),
        field: spec.field.clone(),
        index: spec.index,
        span: spec.span,
        children: elements.into_iter().map(|element| element.3).collect(),
    }
}

/// Classifies significant source tokens and declaration/reference roles.
///
/// This convenience API lexes with Langmeld's built-in lexer. Use
/// [`classify_parse_result_prepared`] to classify the exact token stream used by
/// a parse, including a host-provided lexer.
#[must_use]
pub fn classify_prepared(
    language: &PreparedLanguage,
    source: &SourceFile,
    program: &Program,
) -> Vec<Classification> {
    let lexed = lex_source(source);
    classify_tokens(language, source, program, &lexed.tokens)
}

/// Classifies the exact retained token stream and semantic tree from a parse.
///
/// `source` must be the same snapshot supplied to the parser that produced
/// `parsed`; spans are byte offsets into that source.
#[must_use]
pub fn classify_parse_result_prepared(
    language: &PreparedLanguage,
    source: &SourceFile,
    parsed: &ParseResult,
) -> Vec<Classification> {
    let roles = semantic_roles(
        language.spec().spec().constructs.iter(),
        &parsed.program.nodes,
    );
    let keywords = language.keywords().collect::<HashSet<_>>();
    let mut property_spans = Vec::new();
    collect_property_spans(&parsed.syntax_tree, source, &mut property_spans);
    property_spans.sort_unstable();
    property_spans.dedup();
    let mut result = Vec::new();
    for token in &parsed.syntax_tokens {
        let Some(kind) = classification_kind(token.kind) else {
            continue;
        };
        let text = source
            .text
            .get(token.span.start..token.span.end)
            .unwrap_or_default();
        let kind = if property_spans
            .binary_search(&(token.span.start, token.span.end))
            .is_ok()
        {
            "property"
        } else if let Some(semantic) = roles.get(&(token.span.start, token.span.end)) {
            *semantic
        } else if token.kind == SyntaxTokenKind::Identifier && keywords.contains(text) {
            "keyword"
        } else if token.kind == SyntaxTokenKind::Identifier {
            "identifier"
        } else {
            kind
        };
        result.push(Classification {
            span: token.span,
            kind,
        });
    }
    result.sort_by_key(|item| (item.span.start, item.span.end));
    result
}

/// Classifies source with a prepared language and a generic program.
#[must_use]
pub fn classify(
    spec: &ValidatedLanguageSpec,
    source: &SourceFile,
    program: &Program,
) -> Vec<Classification> {
    PreparedLanguage::new(spec.clone()).map_or_else(
        |_| Vec::new(),
        |language| classify_prepared(&language, source, program),
    )
}

fn classify_tokens(
    language: &PreparedLanguage,
    source: &SourceFile,
    program: &Program,
    tokens: &[LanguageToken],
) -> Vec<Classification> {
    let roles = semantic_roles(language.spec().spec().constructs.iter(), &program.nodes);
    let keywords = language.keywords().collect::<HashSet<_>>();
    let mut classifications = tokens
        .iter()
        .filter_map(|token| {
            let span = token.span;
            let kind = match &token.kind {
                LanguageTokenKind::Identifier(word) => roles
                    .get(&(span.start, span.end))
                    .copied()
                    .unwrap_or_else(|| {
                        if keywords.contains(word.as_str()) {
                            "keyword"
                        } else {
                            "identifier"
                        }
                    }),
                LanguageTokenKind::Number(_) => "number",
                LanguageTokenKind::Text(_) => "string",
                LanguageTokenKind::Comment => "comment",
                LanguageTokenKind::Arrow | LanguageTokenKind::Punctuation(_) => "operator",
                LanguageTokenKind::Unknown(_) => "operator",
                LanguageTokenKind::Newline | LanguageTokenKind::Opaque | LanguageTokenKind::End => {
                    return None;
                }
            };
            Some(Classification { span, kind })
        })
        .collect::<Vec<_>>();
    classifications.sort_by_key(|item| (item.span.start, item.span.end));
    if source.text.is_empty() {
        classifications.clear();
    }
    classifications
}

fn classification_kind(kind: SyntaxTokenKind) -> Option<&'static str> {
    match kind {
        SyntaxTokenKind::Identifier => Some("identifier"),
        SyntaxTokenKind::Number => Some("number"),
        SyntaxTokenKind::String => Some("string"),
        SyntaxTokenKind::Punctuation | SyntaxTokenKind::Arrow | SyntaxTokenKind::Unknown => {
            Some("operator")
        }
        SyntaxTokenKind::Comment => Some("comment"),
        SyntaxTokenKind::Whitespace
        | SyntaxTokenKind::Newline
        | SyntaxTokenKind::Opaque
        | SyntaxTokenKind::MissingRightBrace
        | SyntaxTokenKind::MissingRightBracket => None,
    }
}

fn semantic_roles<'a>(
    constructs: impl Iterator<Item = &'a ConstructSpec>,
    nodes: &[AstNode],
) -> HashMap<(usize, usize), &'static str> {
    let constructs = constructs
        .map(|construct| (construct.id.as_str(), construct))
        .collect::<HashMap<_, _>>();
    let mut roles = HashMap::new();
    collect_roles(&constructs, nodes, &mut roles);
    roles
}

fn collect_roles(
    constructs: &HashMap<&str, &ConstructSpec>,
    nodes: &[AstNode],
    roles: &mut HashMap<(usize, usize), &'static str>,
) {
    for node in nodes {
        let Some(construct) = constructs.get(node.construct.as_str()) else {
            continue;
        };
        if let Some(symbol) = &construct.symbol
            && let Some(value) = node.fields.get(&symbol.name_field)
            && value.span.start < value.span.end
        {
            roles.insert((value.span.start, value.span.end), "declaration");
        }
        for field in &construct.fields {
            if let Some(value) = node.fields.get(&field.name)
                && value.span.start < value.span.end
            {
                collect_value_roles(&field.kind, value, roles, 0);
            }
        }
        collect_roles(constructs, &node.children, roles);
    }
}

fn collect_value_roles(
    kind: &FieldKind,
    captured: &crate::CapturedValue,
    roles: &mut HashMap<(usize, usize), &'static str>,
    depth: usize,
) {
    if depth >= crate::MAX_NESTING_DEPTH {
        return;
    }
    match (kind, &captured.value) {
        (FieldKind::Reference { .. }, _) => {
            roles.insert((captured.span.start, captured.span.end), "reference");
        }
        (FieldKind::Enum { .. } | FieldKind::NamedEnum { .. }, _) => {
            roles.insert((captured.span.start, captured.span.end), "enumMember");
        }
        (FieldKind::List { item: item_kind }, Value::List(items)) => {
            for item in items {
                collect_value_roles(item_kind, item, roles, depth + 1);
            }
        }
        (FieldKind::Map { value, .. }, Value::Map(entries)) => {
            for entry in entries {
                collect_value_roles(value, &entry.value, roles, depth + 1);
            }
        }
        (FieldKind::Properties { fields, .. }, Value::Properties(properties)) => {
            for field in fields {
                if let Some(value) = properties.get(&field.name) {
                    collect_value_roles(&field.kind, value, roles, depth + 1);
                }
            }
        }
        _ => {}
    }
}

fn collect_property_spans(node: &SyntaxNode, source: &SourceFile, spans: &mut Vec<(usize, usize)>) {
    match node.role {
        SyntaxNodeRole::PropertyName => spans.push((node.span.start, node.span.end)),
        SyntaxNodeRole::MapKey
            if source
                .text
                .get(node.span.start..node.span.end)
                .is_some_and(|text| !text.starts_with('"')) =>
        {
            spans.push((node.span.start, node.span.end));
        }
        _ => {}
    }
    for element in &node.children {
        if let SyntaxElement::Node(child) = element {
            collect_property_spans(child, source, spans);
        }
    }
}
