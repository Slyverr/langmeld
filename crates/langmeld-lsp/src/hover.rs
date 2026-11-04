//! Schema-backed symbol and value hover information.

use langmeld_core::{Span, ValidatedLanguageSpec};
use langmeld_language::{AstNode, TypePathSegment};
use serde_json::{Value, json};

use super::offset_at_position;

impl super::Server {
    pub(super) fn supports_hover(&self) -> bool {
        self.language
            .spec()
            .constructs
            .iter()
            .any(|construct| !construct.fields.is_empty() || construct.symbol.is_some())
    }

    pub(super) fn hover_result(&self, params: &Value) -> Value {
        let Some(uri) = params.pointer("/textDocument/uri").and_then(Value::as_str) else {
            return Value::Null;
        };
        let Some(line) = params.pointer("/position/line").and_then(Value::as_u64) else {
            return Value::Null;
        };
        let Some(character) = params
            .pointer("/position/character")
            .and_then(Value::as_u64)
        else {
            return Value::Null;
        };
        let (Ok(line), Ok(character)) = (u32::try_from(line), u32::try_from(character)) else {
            return Value::Null;
        };
        let Some(document) = self.documents.get(uri) else {
            return Value::Null;
        };
        let Some(offset) = offset_at_position(&document.source.text, line, character) else {
            return Value::Null;
        };
        if let Some(occurrence) =
            super::navigation::occurrence_at(&self.language, &document.parsed.program, offset)
        {
            let symbol_kind = declaration_symbol_kind(
                &self.language,
                &document.parsed.program.nodes,
                occurrence.span,
            );
            let (uris, files) = self.workspace_snapshot();
            let Some(source_index) = uris.iter().position(|open_uri| open_uri == uri) else {
                return Value::Null;
            };
            let analysis = self.analyze_files(&files);
            let resolved = analysis.resolved_references.iter().find(|reference| {
                reference.source_index == source_index
                    && reference.reference_span == occurrence.span
            });
            let name = document
                .source
                .text
                .get(occurrence.span.start..occurrence.span.end)
                .unwrap_or_default();
            if let Some(resolved) = resolved {
                return hover_markdown(format!(
                    "`{} {}`\n\nResolved to `{}`.",
                    resolved.kind, name, resolved.name
                ));
            }
            if let Some(kind) = symbol_kind {
                return hover_markdown(format!("`{kind} {name}`\n\nDeclared symbol."));
            }
            if !occurrence.is_declaration {
                return hover_markdown(format!("`{name}`\n\nUnresolved reference."));
            }
        }
        let files = [(&document.source, &document.parsed.program)];
        let analysis = self.analyze_files(&files);
        let Some(fact) = analysis
            .type_facts
            .iter()
            .filter(|fact| {
                fact.source_index == 0 && fact.span.start <= offset && offset < fact.span.end
            })
            .min_by_key(|fact| fact.span.end - fact.span.start)
        else {
            return Value::Null;
        };
        let path = fact
            .path
            .iter()
            .map(|segment| match segment {
                TypePathSegment::Field(name) => name.clone(),
                TypePathSegment::ListItem(index) | TypePathSegment::MapItem(index) => {
                    format!("[{index}]")
                }
                TypePathSegment::MapKey(key) => format!("key `{key}`"),
                TypePathSegment::MapValue(key) => format!("value `{key}`"),
            })
            .collect::<Vec<_>>()
            .join(" → ");
        let spelling = document
            .source
            .text
            .get(fact.span.start..fact.span.end)
            .unwrap_or_default();
        hover_markdown(format!(
            "`{}.{}`\n\nDeclared: `{}`  \nParsed as: `{}`  \nValue: `{}`",
            fact.construct,
            path,
            describe_field_kind(&fact.expected),
            describe_value_type(fact.actual),
            spelling
        ))
    }
}

fn hover_markdown(value: String) -> Value {
    json!({
        "contents": { "kind": "markdown", "value": value }
    })
}

fn describe_field_kind(kind: &langmeld_core::FieldKind) -> String {
    use langmeld_core::FieldKind;
    match kind {
        FieldKind::Identifier => "identifier".to_owned(),
        FieldKind::Number => "number".to_owned(),
        FieldKind::Text => "text".to_owned(),
        FieldKind::Enum { values } => format!("enum {{{}}}", values.join(", ")),
        FieldKind::NamedEnum { name, .. } => format!("enum {name}"),
        FieldKind::Reference { target } => format!("reference to {target}"),
        FieldKind::Properties { name, fields } => name.clone().unwrap_or_else(|| {
            format!(
                "record {{{}}}",
                fields
                    .iter()
                    .map(|field| format!("{}: {}", field.name, describe_field_kind(&field.kind)))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }),
        FieldKind::List { item } => format!("list of {}", describe_field_kind(item)),
        FieldKind::Map { key, value } => format!(
            "map of {} keys to {}",
            match key {
                langmeld_core::MapKeyKind::Identifier => "identifier",
                langmeld_core::MapKeyKind::Text => "text",
            },
            describe_field_kind(value)
        ),
        FieldKind::Expression {
            grammar,
            terminator,
        } => {
            format!("expression using {grammar} through {terminator}")
        }
    }
}

fn describe_value_type(kind: langmeld_language::ValueType) -> &'static str {
    use langmeld_language::ValueType;
    match kind {
        ValueType::Identifier => "identifier",
        ValueType::Number => "number",
        ValueType::Text => "text",
        ValueType::Enum => "enum member",
        ValueType::Reference => "reference",
        ValueType::Properties => "property record",
        ValueType::List => "list",
        ValueType::Map => "map",
        ValueType::Expression => "expression",
    }
}

fn declaration_symbol_kind<'language>(
    language: &'language ValidatedLanguageSpec,
    nodes: &[AstNode],
    span: Span,
) -> Option<&'language str> {
    for node in nodes {
        if let Some(symbol) = language
            .construct(&node.construct)
            .and_then(|construct| construct.symbol.as_ref())
            && node
                .fields
                .get(&symbol.name_field)
                .is_some_and(|field| field.span == span)
        {
            return Some(&symbol.kind);
        }
        if let Some(kind) = declaration_symbol_kind(language, &node.children, span) {
            return Some(kind);
        }
    }
    None
}
