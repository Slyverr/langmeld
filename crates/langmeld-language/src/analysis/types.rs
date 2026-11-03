use super::*;
use std::collections::HashMap;

pub(super) fn collect_program_type_facts(
    constructs_by_id: &HashMap<&str, &ConstructSpec>,
    nodes: &[AstNode],
    source_index: usize,
    output: &mut Vec<TypeFact>,
    diagnostics: &mut Vec<WorkspaceDiagnostic>,
) {
    for node in nodes {
        if let Some(construct) = constructs_by_id.get(node.construct.as_str()) {
            for field in &construct.fields {
                if let Some(captured) = node.fields.get(&field.name) {
                    collect_value_type_facts(
                        source_index,
                        &node.construct,
                        field.kind.clone(),
                        captured,
                        vec![TypePathSegment::Field(field.name.clone())],
                        output,
                        diagnostics,
                    );
                }
            }
        }
        collect_program_type_facts(
            constructs_by_id,
            &node.children,
            source_index,
            output,
            diagnostics,
        );
    }
}

fn collect_value_type_facts(
    source_index: usize,
    construct: &str,
    expected: FieldKind,
    captured: &CapturedValue,
    path: Vec<TypePathSegment>,
    output: &mut Vec<TypeFact>,
    diagnostics: &mut Vec<WorkspaceDiagnostic>,
) {
    let actual = value_type(&captured.value);
    output.push(TypeFact {
        source_index,
        source_id: None,
        source_revision: None,
        construct: construct.to_owned(),
        path: path.clone(),
        expected: expected.clone(),
        actual,
        span: captured.span,
    });
    if !value_matches_kind(&expected, &captured.value) {
        diagnostics.push(type_mismatch_diagnostic(
            source_index,
            construct,
            &path,
            value_type_name(actual),
            field_kind_name(&expected),
            captured.span,
        ));
    }

    match (&expected, &captured.value) {
        (FieldKind::Properties { fields, .. }, Value::Properties(values)) => {
            for field in fields {
                if let Some(value) = values.get(&field.name) {
                    let mut property_path = path.clone();
                    property_path.push(TypePathSegment::Field(field.name.clone()));
                    collect_value_type_facts(
                        source_index,
                        construct,
                        field.kind.clone(),
                        value,
                        property_path,
                        output,
                        diagnostics,
                    );
                }
            }
        }
        (FieldKind::List { item }, Value::List(values)) => {
            for (index, value) in values.iter().enumerate() {
                let mut item_path = path.clone();
                item_path.push(TypePathSegment::ListItem(index));
                collect_value_type_facts(
                    source_index,
                    construct,
                    item.as_ref().clone(),
                    value,
                    item_path,
                    output,
                    diagnostics,
                );
            }
        }
        (FieldKind::Map { key, value }, Value::Map(entries)) => {
            let key_kind = match key {
                MapKeyKind::Identifier => FieldKind::Identifier,
                MapKeyKind::Text => FieldKind::Text,
            };
            for (index, entry) in entries.iter().enumerate() {
                let mut key_path = path.clone();
                key_path.push(TypePathSegment::MapItem(index));
                key_path.push(TypePathSegment::MapKey(entry.key.clone()));
                output.push(TypeFact {
                    source_index,
                    source_id: None,
                    source_revision: None,
                    construct: construct.to_owned(),
                    path: key_path.clone(),
                    expected: key_kind.clone(),
                    actual: entry.key_type,
                    span: entry.key_span,
                });
                let expected_key_type = match key {
                    MapKeyKind::Identifier => ValueType::Identifier,
                    MapKeyKind::Text => ValueType::Text,
                };
                if entry.key_type != expected_key_type {
                    diagnostics.push(type_mismatch_diagnostic(
                        source_index,
                        construct,
                        &key_path,
                        value_type_name(entry.key_type),
                        field_kind_name(&key_kind),
                        entry.key_span,
                    ));
                }

                let mut value_path = path.clone();
                value_path.push(TypePathSegment::MapItem(index));
                value_path.push(TypePathSegment::MapValue(entry.key.clone()));
                collect_value_type_facts(
                    source_index,
                    construct,
                    value.as_ref().clone(),
                    &entry.value,
                    value_path,
                    output,
                    diagnostics,
                );
            }
        }
        _ => {}
    }
}

fn value_type(value: &Value) -> ValueType {
    match value {
        Value::Identifier(_) => ValueType::Identifier,
        Value::Number(_) => ValueType::Number,
        Value::Text(_) => ValueType::Text,
        Value::Enum(_) => ValueType::Enum,
        Value::Reference(_) => ValueType::Reference,
        Value::Properties(_) => ValueType::Properties,
        Value::List(_) => ValueType::List,
        Value::Map(_) => ValueType::Map,
    }
}

fn value_matches_kind(kind: &FieldKind, value: &Value) -> bool {
    match (kind, value) {
        (FieldKind::Identifier, Value::Identifier(_))
        | (FieldKind::Text, Value::Text(_))
        | (FieldKind::Reference { .. }, Value::Reference(_))
        | (FieldKind::Properties { .. }, Value::Properties(_))
        | (FieldKind::List { .. }, Value::List(_))
        | (FieldKind::Map { .. }, Value::Map(_)) => true,
        (FieldKind::Number, Value::Number(value)) => value.is_finite(),
        (FieldKind::Enum { values } | FieldKind::NamedEnum { values, .. }, Value::Enum(value)) => {
            values.contains(value)
        }
        _ => false,
    }
}

fn field_kind_name(kind: &FieldKind) -> &'static str {
    match kind {
        FieldKind::Identifier => "identifier",
        FieldKind::Number => "number",
        FieldKind::Text => "text",
        FieldKind::Enum { .. } | FieldKind::NamedEnum { .. } => "enum",
        FieldKind::Reference { .. } => "reference",
        FieldKind::Properties { .. } => "properties",
        FieldKind::List { .. } => "list",
        FieldKind::Map { .. } => "map",
    }
}

fn value_type_name(kind: ValueType) -> &'static str {
    match kind {
        ValueType::Identifier => "identifier",
        ValueType::Number => "number",
        ValueType::Text => "text",
        ValueType::Enum => "enum",
        ValueType::Reference => "reference",
        ValueType::Properties => "properties",
        ValueType::List => "list",
        ValueType::Map => "map",
    }
}

fn type_path_display(path: &[TypePathSegment]) -> String {
    let mut display = String::new();
    for segment in path {
        match segment {
            TypePathSegment::Field(name) => {
                if !display.is_empty() {
                    display.push('.');
                }
                display.push_str(name);
            }
            TypePathSegment::ListItem(index) => display.push_str(&format!("[{index}]")),
            TypePathSegment::MapItem(index) => display.push_str(&format!(".entry[{index}]")),
            TypePathSegment::MapKey(key) => display.push_str(&format!(".key({key:?})")),
            TypePathSegment::MapValue(key) => display.push_str(&format!(".value({key:?})")),
        }
    }
    display
}

fn type_mismatch_diagnostic(
    source_index: usize,
    construct: &str,
    path: &[TypePathSegment],
    actual: &str,
    expected: &str,
    span: Span,
) -> WorkspaceDiagnostic {
    WorkspaceDiagnostic {
        source_index,
        source_id: None,
        source_revision: None,
        diagnostic: Diagnostic::error(
            "LM2010",
            format!(
                "value at `{construct}.{}` has type `{actual}`, expected `{expected}`",
                type_path_display(path)
            ),
            span,
        ),
    }
}
