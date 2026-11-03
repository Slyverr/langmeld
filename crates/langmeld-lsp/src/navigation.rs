//! Generic navigation over declarations and references in open documents.

use langmeld_core::{FieldKind, Span, ValidatedLanguageSpec};
use langmeld_language::{AstNode, CapturedValue, Program, Value, WorkspaceAnalysis};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SymbolOccurrence {
    pub(super) span: Span,
    pub(super) is_declaration: bool,
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
