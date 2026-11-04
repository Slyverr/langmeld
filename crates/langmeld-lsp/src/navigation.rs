//! Generic navigation over declarations and references in open documents.

use std::collections::BTreeMap;

use langmeld_core::{FieldKind, ScopeLookup, ScopeSpec, SourceFile, Span, ValidatedLanguageSpec};
use langmeld_language::{
    AstNode, CapturedValue, Program, Value as LanguageValue, WorkspaceAnalysis,
};
use serde_json::{Value, json};

use super::{MAX_DOCUMENT_SYMBOLS, MAX_RENAME_EDITS, MAX_RENAME_NAME_BYTES, offset_at_position};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SymbolOccurrence {
    pub(super) span: Span,
    pub(super) is_declaration: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SymbolIdentity {
    pub(super) name: String,
    kind: String,
    scope: Option<Vec<(String, String)>>,
    scope_lookup: ScopeLookup,
}

struct NavigationRequest<'a> {
    uris: Vec<String>,
    files: Vec<(&'a SourceFile, &'a Program)>,
    source_index: usize,
    occurrence: SymbolOccurrence,
    analysis: WorkspaceAnalysis,
}

impl super::Server {
    pub(super) fn supports_navigation(&self) -> bool {
        self.language
            .spec()
            .constructs
            .iter()
            .any(|construct| construct.symbol.is_some())
    }

    pub(super) fn supports_rename(&self) -> bool {
        self.supports_navigation() && self.symbol_resolver_hooks.is_empty()
    }

    pub(super) fn document_symbols(&self, params: &Value) -> Vec<Value> {
        let Some(uri) = params.pointer("/textDocument/uri").and_then(Value::as_str) else {
            return Vec::new();
        };
        let Some(document) = self.documents.get(uri) else {
            return Vec::new();
        };
        let mut remaining = MAX_DOCUMENT_SYMBOLS;
        document_symbol_items(
            &self.language,
            &document.source.text,
            &document.parsed.program.nodes,
            &mut remaining,
        )
    }

    pub(super) fn definition_result(&self, params: &Value) -> Vec<Value> {
        let Some(request) = self.navigation_request(params) else {
            return Vec::new();
        };
        if request.analysis.resource_limited {
            return Vec::new();
        }
        let Some((target_index, span)) =
            resolved_target(request.occurrence, request.source_index, &request.analysis)
        else {
            return Vec::new();
        };
        let (Some(uri), Some((source, _))) = (
            request.uris.get(target_index),
            request.files.get(target_index),
        ) else {
            return Vec::new();
        };
        vec![json!({ "uri": uri, "range": super::lsp_range(&source.text, span) })]
    }

    pub(super) fn rename_result(&self, params: &Value) -> Option<Value> {
        if !self.supports_rename() {
            return None;
        }
        let new_name = params.get("newName")?.as_str()?;
        if new_name.len() > MAX_RENAME_NAME_BYTES || !valid_symbol_name(new_name) {
            return None;
        }
        let request = self.navigation_request(params)?;
        if request.analysis.resource_limited {
            return None;
        }
        let (target_index, target_span) =
            resolved_target(request.occurrence, request.source_index, &request.analysis)?;
        let target = validate_rename_target(
            &self.language,
            &request.files,
            target_index,
            target_span,
            new_name,
        )?;
        if new_name == target.name {
            return Some(json!({ "changes": {} }));
        }

        let mut occurrences = vec![(target_index, target_span)];
        for reference in request
            .analysis
            .resolved_references
            .iter()
            .filter(|reference| {
                reference.declaration_source_index == target_index
                    && reference.declaration_span == target_span
            })
        {
            if reference.name != target.name {
                return None;
            }
            occurrences.push((reference.source_index, reference.reference_span));
        }
        if occurrences.len() > MAX_RENAME_EDITS {
            return None;
        }
        occurrences.sort_by_key(|(source_index, span)| (*source_index, span.start, span.end));

        let mut changes = BTreeMap::<String, Vec<Value>>::new();
        let mut last_by_source = None::<(usize, Span)>;
        for (source_index, span) in occurrences {
            if let Some((last_source_index, last_span)) = last_by_source
                && last_source_index == source_index
                && last_span.end > span.start
            {
                return None;
            }
            let uri = request.uris.get(source_index)?;
            let (source, _) = request.files.get(source_index)?;
            if span.start >= span.end
                || source.text.get(span.start..span.end) != Some(target.name.as_str())
            {
                return None;
            }
            changes.entry(uri.clone()).or_default().push(json!({
                "range": super::lsp_range(&source.text, span),
                "newText": new_name
            }));
            last_by_source = Some((source_index, span));
        }
        Some(json!({ "changes": changes }))
    }

    pub(super) fn reference_results(&self, params: &Value) -> Vec<Value> {
        let Some(request) = self.navigation_request(params) else {
            return Vec::new();
        };
        if request.analysis.resource_limited {
            return Vec::new();
        }
        let Some((target_index, target_span)) =
            resolved_target(request.occurrence, request.source_index, &request.analysis)
        else {
            return Vec::new();
        };
        let include_declaration = params
            .pointer("/context/includeDeclaration")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mut locations = Vec::new();
        if include_declaration
            && let (Some(uri), Some((source, _))) = (
                request.uris.get(target_index),
                request.files.get(target_index),
            )
        {
            locations.push(json!({
                "uri": uri,
                "range": super::lsp_range(&source.text, target_span)
            }));
        }
        for reference in request
            .analysis
            .resolved_references
            .iter()
            .filter(|reference| {
                reference.declaration_source_index == target_index
                    && reference.declaration_span == target_span
            })
        {
            let (Some(uri), Some((source, _))) = (
                request.uris.get(reference.source_index),
                request.files.get(reference.source_index),
            ) else {
                continue;
            };
            locations.push(json!({
                "uri": uri,
                "range": super::lsp_range(&source.text, reference.reference_span)
            }));
        }
        locations
    }

    fn navigation_request(&self, params: &Value) -> Option<NavigationRequest<'_>> {
        if self
            .project
            .as_ref()
            .is_some_and(|project| project.navigation_index_limited)
        {
            return None;
        }
        let uri = params.pointer("/textDocument/uri")?.as_str()?;
        let document = self.documents.get(uri)?;
        let line = u32::try_from(params.pointer("/position/line")?.as_u64()?).ok()?;
        let character = u32::try_from(params.pointer("/position/character")?.as_u64()?).ok()?;
        let offset = offset_at_position(&document.source.text, line, character)?;
        let occurrence = occurrence_at(&self.language, &document.parsed.program, offset)?;
        let (uris, files) = self.workspace_snapshot();
        let source_index = uris.iter().position(|candidate| candidate == uri)?;
        let analysis = self.analyze_files(&files);
        Some(NavigationRequest {
            uris,
            files,
            source_index,
            occurrence,
            analysis,
        })
    }
}

fn document_symbol_items(
    spec: &ValidatedLanguageSpec,
    source: &str,
    nodes: &[AstNode],
    remaining: &mut usize,
) -> Vec<Value> {
    let mut symbols = Vec::new();
    for node in nodes {
        if *remaining == 0 {
            break;
        }
        *remaining -= 1;
        let construct = spec.construct(&node.construct);
        let declaration = construct
            .and_then(|construct| construct.symbol.as_ref())
            .and_then(|symbol| node.fields.get(&symbol.name_field));
        let selection_span = declaration.map_or(node.span, |value| value.span);
        let name = declaration
            .and_then(|value| source.get(value.span.start..value.span.end))
            .unwrap_or(&node.construct);
        let children = document_symbol_items(spec, source, &node.children, remaining);
        symbols.push(json!({
            "name": name,
            "detail": node.construct,
            "kind": 13,
            "range": super::lsp_range(source, node.span),
            "selectionRange": super::lsp_range(source, selection_span),
            "children": children
        }));
    }
    symbols
}

fn valid_symbol_name(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|character| character == '_' || character.is_alphabetic())
        && characters
            .all(|character| character == '_' || character == '-' || character.is_alphanumeric())
}

pub(super) fn occurrence_at(
    spec: &ValidatedLanguageSpec,
    program: &Program,
    offset: usize,
) -> Option<SymbolOccurrence> {
    occurrence_at_nodes(spec, &program.nodes, offset)
}

fn occurrence_at_nodes(
    spec: &ValidatedLanguageSpec,
    nodes: &[AstNode],
    offset: usize,
) -> Option<SymbolOccurrence> {
    for node in nodes {
        if let Some(construct) = spec.construct(&node.construct) {
            if let Some(symbol) = &construct.symbol
                && let Some(value) = node.fields.get(&symbol.name_field)
                && matches!(&value.value, LanguageValue::Identifier(_))
                && value.span.start <= offset
                && offset < value.span.end
            {
                return Some(SymbolOccurrence {
                    span: value.span,
                    is_declaration: true,
                });
            }
            for field in &construct.fields {
                if let Some(value) = node.fields.get(&field.name)
                    && let Some(span) = field_occurrence_at(&field.kind, value, offset)
                {
                    return Some(SymbolOccurrence {
                        span,
                        is_declaration: false,
                    });
                }
            }
        }
        if let Some(occurrence) = occurrence_at_nodes(spec, &node.children, offset) {
            return Some(occurrence);
        }
    }
    None
}

fn field_occurrence_at(kind: &FieldKind, captured: &CapturedValue, offset: usize) -> Option<Span> {
    match (kind, &captured.value) {
        (FieldKind::Reference { .. }, LanguageValue::Reference(_))
            if captured.span.start <= offset && offset < captured.span.end =>
        {
            Some(captured.span)
        }
        (FieldKind::Properties { fields, .. }, LanguageValue::Properties(values)) => fields
            .iter()
            .find_map(|field| field_occurrence_at(&field.kind, values.get(&field.name)?, offset)),
        (FieldKind::List { item }, LanguageValue::List(values)) => values
            .iter()
            .find_map(|value| field_occurrence_at(item, value, offset)),
        (FieldKind::Map { value, .. }, LanguageValue::Map(entries)) => entries
            .iter()
            .find_map(|entry| field_occurrence_at(value, &entry.value, offset)),
        _ => None,
    }
}

pub(super) fn resolved_target(
    occurrence: SymbolOccurrence,
    source_index: usize,
    analysis: &WorkspaceAnalysis,
) -> Option<(usize, Span)> {
    if occurrence.is_declaration {
        return Some((source_index, occurrence.span));
    }
    analysis
        .resolved_references
        .iter()
        .find(|reference| {
            reference.source_index == source_index && reference.reference_span == occurrence.span
        })
        .map(|reference| {
            (
                reference.declaration_source_index,
                reference.declaration_span,
            )
        })
}

pub(super) fn validate_rename_target(
    spec: &ValidatedLanguageSpec,
    files: &[(&langmeld_core::SourceFile, &Program)],
    source_index: usize,
    declaration_span: Span,
    new_name: &str,
) -> Option<SymbolIdentity> {
    let target_program = files.get(source_index)?.1;
    let target = find_symbol_identity(
        spec,
        &target_program.nodes,
        &mut Vec::new(),
        declaration_span,
    )?;
    let mut target_declarations = 0_usize;
    let mut name_conflict = false;
    for (current_source_index, (_, program)) in files.iter().enumerate() {
        inspect_rename_symbols(
            spec,
            &program.nodes,
            &mut Vec::new(),
            &target,
            new_name,
            source_index,
            declaration_span,
            current_source_index,
            &mut target_declarations,
            &mut name_conflict,
        );
    }
    (target_declarations == 1 && !name_conflict).then_some(target)
}

fn find_symbol_identity<'a>(
    spec: &ValidatedLanguageSpec,
    nodes: &'a [AstNode],
    ancestors: &mut Vec<&'a AstNode>,
    span: Span,
) -> Option<SymbolIdentity> {
    for node in nodes {
        ancestors.push(node);
        if let Some(construct) = spec.construct(&node.construct)
            && let Some(symbol) = &construct.symbol
            && let Some(value) = node.fields.get(&symbol.name_field)
            && value.span == span
            && let LanguageValue::Identifier(name) = &value.value
        {
            let Some(scope) = scope_identity(symbol.scope.as_ref(), ancestors) else {
                ancestors.pop();
                return None;
            };
            let identity = SymbolIdentity {
                name: name.clone(),
                kind: symbol.kind.clone(),
                scope,
                scope_lookup: symbol
                    .scope
                    .as_ref()
                    .map_or(ScopeLookup::Exact, |scope| scope.lookup),
            };
            ancestors.pop();
            return Some(identity);
        }
        if let Some(identity) = find_symbol_identity(spec, &node.children, ancestors, span) {
            ancestors.pop();
            return Some(identity);
        }
        ancestors.pop();
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn inspect_rename_symbols<'a>(
    spec: &ValidatedLanguageSpec,
    nodes: &'a [AstNode],
    ancestors: &mut Vec<&'a AstNode>,
    target: &SymbolIdentity,
    new_name: &str,
    target_source_index: usize,
    target_span: Span,
    current_source_index: usize,
    target_declarations: &mut usize,
    name_conflict: &mut bool,
) {
    for node in nodes {
        ancestors.push(node);
        if let Some(construct) = spec.construct(&node.construct)
            && let Some(symbol) = &construct.symbol
            && symbol.kind == target.kind
        {
            let declaration_scope = scope_identity(symbol.scope.as_ref(), ancestors);
            let same_scope = declaration_scope.as_ref() == Some(&target.scope);
            let visible_from_target = match (&target.scope, &declaration_scope) {
                (Some(target), Some(Some(declaration))) => {
                    declaration.starts_with(target.as_slice())
                }
                (None, Some(None)) => true,
                _ => false,
            };
            let check_declaration = match target.scope_lookup {
                ScopeLookup::Exact => same_scope,
                ScopeLookup::LexicalParents => visible_from_target,
            };
            if let Some(value) = node.fields.get(&symbol.name_field)
                && let LanguageValue::Identifier(name) = &value.value
            {
                let is_target =
                    current_source_index == target_source_index && value.span == target_span;
                if is_target {
                    *target_declarations = target_declarations.saturating_add(1);
                } else {
                    if same_scope && name == target.name.as_str() {
                        *target_declarations = target_declarations.saturating_add(1);
                    }
                    let unclassified_scope = declaration_scope.is_none();
                    if name == new_name
                        && new_name != target.name
                        && (check_declaration
                            || (unclassified_scope
                                && target.scope_lookup == ScopeLookup::LexicalParents))
                    {
                        *name_conflict = true;
                    }
                }
            }
        }
        inspect_rename_symbols(
            spec,
            &node.children,
            ancestors,
            target,
            new_name,
            target_source_index,
            target_span,
            current_source_index,
            target_declarations,
            name_conflict,
        );
        ancestors.pop();
    }
}

fn scope_identity(
    scope: Option<&ScopeSpec>,
    ancestors: &[&AstNode],
) -> Option<Option<Vec<(String, String)>>> {
    let Some(scope) = scope else {
        return Some(None);
    };
    let matching_scopes = ancestors
        .iter()
        .filter(|ancestor| ancestor.construct == scope.construct)
        .filter_map(|ancestor| {
            ancestor
                .fields
                .get(&scope.name_field)
                .and_then(|field| match &field.value {
                    LanguageValue::Identifier(name) => {
                        Some((scope.construct.clone(), name.clone()))
                    }
                    _ => None,
                })
        })
        .collect::<Vec<_>>();
    if matching_scopes.is_empty() {
        return None;
    }
    let identity = match scope.lookup {
        ScopeLookup::Exact => vec![matching_scopes.last()?.clone()],
        ScopeLookup::LexicalParents => matching_scopes,
    };
    Some(Some(identity))
}
