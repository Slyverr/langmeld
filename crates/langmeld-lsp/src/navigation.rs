//! Generic navigation over declarations and references in open documents.

use langmeld_core::{FieldKind, ScopeLookup, ScopeSpec, Span, ValidatedLanguageSpec};
use langmeld_language::{AstNode, CapturedValue, Program, Value, WorkspaceAnalysis};

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

pub(super) fn occurrence_at(
    spec: &ValidatedLanguageSpec,
    program: &Program,
    offset: usize,
) -> Option<SymbolOccurrence> {
    let mut occurrences = Vec::new();
    collect_occurrences(spec, &program.nodes, &mut occurrences);
    occurrences
        .into_iter()
        .find(|occurrence| occurrence.span.start <= offset && offset < occurrence.span.end)
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
            && let Value::Identifier(name) = &value.value
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
                && let Value::Identifier(name) = &value.value
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
                    Value::Identifier(name) => Some((scope.construct.clone(), name.clone())),
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

fn collect_occurrences(
    spec: &ValidatedLanguageSpec,
    nodes: &[AstNode],
    output: &mut Vec<SymbolOccurrence>,
) {
    for node in nodes {
        if let Some(construct) = spec.construct(&node.construct) {
            if let Some(symbol) = &construct.symbol
                && let Some(value) = node.fields.get(&symbol.name_field)
                && matches!(&value.value, Value::Identifier(_))
            {
                output.push(SymbolOccurrence {
                    span: value.span,
                    is_declaration: true,
                });
            }
            for field in &construct.fields {
                if let Some(value) = node.fields.get(&field.name) {
                    collect_field_occurrences(&field.kind, value, output);
                }
            }
        }
        collect_occurrences(spec, &node.children, output);
    }
}

fn collect_field_occurrences(
    kind: &FieldKind,
    captured: &CapturedValue,
    output: &mut Vec<SymbolOccurrence>,
) {
    match (kind, &captured.value) {
        (FieldKind::Reference { .. }, Value::Reference(_)) => {
            output.push(SymbolOccurrence {
                span: captured.span,
                is_declaration: false,
            });
        }
        (FieldKind::Properties { fields, .. }, Value::Properties(values)) => {
            for field in fields {
                if let Some(value) = values.get(&field.name) {
                    collect_field_occurrences(&field.kind, value, output);
                }
            }
        }
        (FieldKind::List { item }, Value::List(values)) => {
            for value in values {
                collect_field_occurrences(item, value, output);
            }
        }
        (FieldKind::Map { value, .. }, Value::Map(entries)) => {
            for entry in entries {
                collect_field_occurrences(value, &entry.value, output);
            }
        }
        _ => {}
    }
}
