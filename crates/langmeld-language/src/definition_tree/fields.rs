use langmeld_core::{Diagnostic, FieldConstraints, FieldKind, FieldSpec, MapKeyKind, SourceFile};
use std::collections::BTreeMap;

use super::helpers::*;
use crate::{AstNode, Value};

fn convert_field(
    node: &AstNode,
    enums: &BTreeMap<String, Vec<String>>,
    named_types: &BTreeMap<String, Vec<FieldSpec>>,
    properties: &BTreeMap<String, Vec<FieldSpec>>,
    source: &SourceFile,
) -> Result<FieldSpec, Vec<Diagnostic>> {
    let name = identifier(node, "name")?;
    let kind = match node.syntax_variant.as_str() {
        "inline_enum" | "inline_enum_default" => FieldKind::Enum {
            values: identifiers(node, "enum_values")?,
        },
        "reference" | "reference_keyword" => FieldKind::Reference {
            target: identifier(node, "item_kind")?,
        },
        "list_reference" | "list_reference_keyword" => FieldKind::List {
            item: Box::new(FieldKind::Reference {
                target: identifier(node, "item_kind")?,
            }),
        },
        "list" => FieldKind::List {
            item: Box::new(convert_scalar_kind(
                node,
                &identifier(node, "item_kind")?,
                enums,
                named_types,
                properties,
            )?),
        },
        "list_inline_enum" => FieldKind::List {
            item: Box::new(FieldKind::Enum {
                values: identifiers(node, "enum_values")?,
            }),
        },
        "list_properties" | "list_named_properties" => {
            let item_kind = identifier(node, "item_kind")?;
            let item = if let Some(fields) = named_types.get(&item_kind) {
                FieldKind::Properties {
                    name: Some(item_kind),
                    fields: fields.clone(),
                }
            } else if let Some(fields) = properties.get(&item_kind) {
                FieldKind::Properties {
                    name: None,
                    fields: fields.clone(),
                }
            } else {
                return Err(vec![error(
                    node,
                    format!("unknown or forward property type `{item_kind}`"),
                )]);
            };
            FieldKind::List {
                item: Box::new(item),
            }
        }
        "named_properties" => {
            let item_kind = identifier(node, "item_kind")?;
            if let Some(fields) = named_types.get(&item_kind) {
                FieldKind::Properties {
                    name: Some(item_kind),
                    fields: fields.clone(),
                }
            } else if let Some(fields) = properties.get(&item_kind) {
                FieldKind::Properties {
                    name: None,
                    fields: fields.clone(),
                }
            } else {
                return Err(vec![error(
                    node,
                    format!("unknown or forward property type `{item_kind}`"),
                )]);
            }
        }
        "nested_list" => {
            let item_kind = identifier(node, "item_kind")?;
            FieldKind::List {
                item: Box::new(FieldKind::List {
                    item: Box::new(convert_scalar_kind(
                        node,
                        &item_kind,
                        enums,
                        named_types,
                        properties,
                    )?),
                }),
            }
        }
        "map" | "map_reference" | "map_list" | "list_map" => {
            let key = convert_map_key(node, &identifier(node, "key_kind")?)?;
            let item_kind = identifier(node, "item_kind")?;
            let scalar = if node.syntax_variant == "map_reference" {
                FieldKind::Reference { target: item_kind }
            } else {
                convert_scalar_kind(node, &item_kind, enums, named_types, properties)?
            };
            match node.syntax_variant.as_str() {
                "map" => FieldKind::Map {
                    key,
                    value: Box::new(scalar),
                },
                "map_reference" => FieldKind::Map {
                    key,
                    value: Box::new(scalar),
                },
                "map_list" => FieldKind::Map {
                    key,
                    value: Box::new(FieldKind::List {
                        item: Box::new(scalar),
                    }),
                },
                "list_map" => FieldKind::List {
                    item: Box::new(FieldKind::Map {
                        key,
                        value: Box::new(scalar),
                    }),
                },
                _ => unreachable!(),
            }
        }
        _ => match optional_identifier(node, "kind")? {
            None => {
                let Some(bundle_name) = optional_identifier(node, "item_kind")? else {
                    return Err(vec![error(
                        node,
                        "property fields must declare a reusable bundle",
                    )]);
                };
                let Some(fields) = named_types
                    .get(bundle_name)
                    .or_else(|| properties.get(bundle_name))
                else {
                    return Err(vec![error(
                        node,
                        format!("unknown or forward property bundle `{bundle_name}`"),
                    )]);
                };
                FieldKind::Properties {
                    name: named_types
                        .contains_key(bundle_name)
                        .then(|| bundle_name.to_owned()),
                    fields: fields.clone(),
                }
            }
            Some("identifier") => FieldKind::Identifier,
            Some("number") => FieldKind::Number,
            Some("string" | "text") => FieldKind::Text,
            Some("list") => {
                let Some(item_kind) = optional_identifier(node, "item_kind")? else {
                    return Err(vec![error(
                        node,
                        "bootstrap list fields must declare an item kind",
                    )]);
                };
                let item = convert_scalar_kind(node, item_kind, enums, named_types, properties)?;
                FieldKind::List {
                    item: Box::new(item),
                }
            }
            Some(other) => convert_scalar_kind(node, other, enums, named_types, properties)?,
        },
    };
    let requirement = optional_identifier_or_enum(node, "requirement")?.unwrap_or("required");
    let required = match requirement {
        "required" => true,
        "optional" => false,
        other => {
            return Err(vec![error(
                node,
                format!("unsupported field requirement `{other}`"),
            )]);
        }
    };
    let constraints = convert_field_constraints(node, source)?;
    let default = convert_default(node, source)?;
    Ok(build_field(name, kind, required, default, constraints))
}

fn convert_map_key(node: &AstNode, name: &str) -> Result<MapKeyKind, Vec<Diagnostic>> {
    match name {
        "identifier" => Ok(MapKeyKind::Identifier),
        "string" | "text" => Ok(MapKeyKind::Text),
        other => Err(vec![error(
            node,
            format!("unsupported bootstrap map key kind `{other}`"),
        )]),
    }
}

fn convert_field_constraints(
    node: &AstNode,
    source: &SourceFile,
) -> Result<FieldConstraints, Vec<Diagnostic>> {
    Ok(FieldConstraints {
        min: optional_integer(node, "min", source)?,
        max: optional_integer(node, "max", source)?,
        min_length: optional_usize(node, "min_length", source)?,
        max_length: optional_usize(node, "max_length", source)?,
        min_items: optional_usize(node, "min_items", source)?,
        max_items: optional_usize(node, "max_items", source)?,
    })
}

fn convert_default(node: &AstNode, source: &SourceFile) -> Result<Option<String>, Vec<Diagnostic>> {
    let mut defaults = Vec::new();
    for field in ["default_text", "default_number", "default_identifier"] {
        let Some(captured) = node.fields.get(field) else {
            continue;
        };
        let value = match &captured.value {
            Value::Text(value) | Value::Identifier(value) => value.clone(),
            Value::Number(_) if field == "default_number" => source
                .text
                .get(captured.span.start..captured.span.end)
                .ok_or_else(|| vec![error(node, "number default span is outside the source")])?
                .to_owned(),
            Value::Number(value) => value.to_string(),
            other => {
                return Err(vec![error(
                    node,
                    format!("field default has unsupported value {other:?}"),
                )]);
            }
        };
        defaults.push(value);
    }
    match defaults.as_slice() {
        [] => Ok(None),
        [value] => Ok(Some(value.clone())),
        _ => Err(vec![error(
            node,
            "a field cannot declare more than one default value",
        )]),
    }
}

fn convert_scalar_kind(
    node: &AstNode,
    name: &str,
    enums: &BTreeMap<String, Vec<String>>,
    named_types: &BTreeMap<String, Vec<FieldSpec>>,
    properties: &BTreeMap<String, Vec<FieldSpec>>,
) -> Result<FieldKind, Vec<Diagnostic>> {
    match name {
        "identifier" => Ok(FieldKind::Identifier),
        "number" => Ok(FieldKind::Number),
        "string" | "text" => Ok(FieldKind::Text),
        named_enum if enums.contains_key(named_enum) => Ok(FieldKind::named_enumeration(
            named_enum,
            enums[named_enum].clone(),
        )),
        named_type if named_types.contains_key(named_type) => Ok(FieldKind::Properties {
            name: Some(named_type.to_owned()),
            fields: named_types[named_type].clone(),
        }),
        property_bundle if properties.contains_key(property_bundle) => Ok(FieldKind::Properties {
            name: None,
            fields: properties[property_bundle].clone(),
        }),
        unsupported => Err(vec![Diagnostic::error(
            "LMDEF018",
            format!("unknown field type `{unsupported}`"),
            node.span,
        )]),
    }
}

pub(super) fn optional_integer(
    node: &AstNode,
    field: &str,
    source: &SourceFile,
) -> Result<Option<i64>, Vec<Diagnostic>> {
    optional_number(node, field, source, |value| value.parse().ok())
}

fn optional_usize(
    node: &AstNode,
    field: &str,
    source: &SourceFile,
) -> Result<Option<usize>, Vec<Diagnostic>> {
    optional_number(node, field, source, |value| value.parse().ok())
}

fn optional_number<T>(
    node: &AstNode,
    field: &str,
    source: &SourceFile,
    parse: impl FnOnce(&str) -> Option<T>,
) -> Result<Option<T>, Vec<Diagnostic>> {
    // Generic AST numbers are f64; integer constraints must use the exact source token.
    match node.fields.get(field) {
        Some(captured) => source
            .text
            .get(captured.span.start..captured.span.end)
            .and_then(parse)
            .map(Some)
            .ok_or_else(|| {
                vec![error(
                    node,
                    format!("constraint `{field}` must be an in-range integer"),
                )]
            }),
        None => Ok(None),
    }
}

pub(super) fn convert_field_declarations(
    node: &AstNode,
    enums: &BTreeMap<String, Vec<String>>,
    named_types: &BTreeMap<String, Vec<FieldSpec>>,
    properties: &BTreeMap<String, Vec<FieldSpec>>,
    source: &SourceFile,
) -> Result<Vec<FieldSpec>, Vec<Diagnostic>> {
    let mut fields = Vec::new();
    for child in &node.children {
        match child.construct.as_str() {
            "field" => fields.push(convert_field(
                child,
                enums,
                named_types,
                properties,
                source,
            )?),
            "typed_field" => fields.push(convert_typed_field(
                child,
                enums,
                named_types,
                properties,
                source,
            )?),
            "property_use" => {
                let id = identifier(child, "id")?;
                let Some(reused) = properties.get(&id) else {
                    return Err(vec![error(
                        child,
                        format!("unknown or forward property bundle `{id}`"),
                    )]);
                };
                fields.extend(reused.iter().cloned());
            }
            unsupported => {
                return Err(vec![error(
                    child,
                    format!("unsupported field declaration `{unsupported}`"),
                )]);
            }
        }
    }
    Ok(fields)
}

fn convert_typed_field(
    node: &AstNode,
    enums: &BTreeMap<String, Vec<String>>,
    named_types: &BTreeMap<String, Vec<FieldSpec>>,
    properties: &BTreeMap<String, Vec<FieldSpec>>,
    source: &SourceFile,
) -> Result<FieldSpec, Vec<Diagnostic>> {
    let kind_node = only_child_matching(node, is_field_type_construct)?;
    let kind = convert_recursive_field_type(kind_node, enums, named_types, properties)?;
    let requirement = optional_identifier_or_enum(node, "requirement")?.ok_or_else(|| {
        vec![error(
            node,
            "typed field is missing its required or optional declaration",
        )]
    })?;
    let required = match requirement {
        "required" => true,
        "optional" => false,
        other => {
            return Err(vec![error(
                node,
                format!("unsupported field requirement `{other}`"),
            )]);
        }
    };
    let constraints = convert_field_constraints(node, source)?;
    let mut defaults = node.children.iter().filter(|child| {
        matches!(
            child.construct.as_str(),
            "field_default_text" | "field_default_number" | "field_default_identifier"
        )
    });
    let default = defaults
        .next()
        .map(|declaration| convert_typed_default(declaration, source))
        .transpose()?;
    if let Some(duplicate) = defaults.next() {
        return Err(vec![error(
            duplicate,
            "a field cannot declare more than one default value",
        )]);
    }
    Ok(build_field(
        identifier(node, "name")?,
        kind,
        required,
        default,
        constraints,
    ))
}

fn build_field(
    name: String,
    kind: FieldKind,
    required: bool,
    default: Option<String>,
    constraints: FieldConstraints,
) -> FieldSpec {
    let field = if required {
        FieldSpec::required(name, kind)
    } else {
        FieldSpec::optional(name, kind)
    };
    let field = match default {
        Some(value) => field.default_value(value),
        None => field,
    };
    field.constraints(constraints)
}

fn is_field_type_construct(construct: &str) -> bool {
    matches!(
        construct,
        "field_type_scalar"
            | "field_type_enum"
            | "field_type_reference"
            | "field_type_list"
            | "field_type_map"
    )
}

fn only_child_matching(
    node: &AstNode,
    predicate: impl Fn(&str) -> bool,
) -> Result<&AstNode, Vec<Diagnostic>> {
    let mut matches = node
        .children
        .iter()
        .filter(|child| predicate(&child.construct));
    let Some(child) = matches.next() else {
        return Err(vec![error(node, "typed field is missing its field type")]);
    };
    if let Some(duplicate) = matches.next() {
        return Err(vec![error(
            duplicate,
            "field type declarations must contain exactly one type",
        )]);
    }
    Ok(child)
}

fn convert_recursive_field_type(
    node: &AstNode,
    enums: &BTreeMap<String, Vec<String>>,
    named_types: &BTreeMap<String, Vec<FieldSpec>>,
    properties: &BTreeMap<String, Vec<FieldSpec>>,
) -> Result<FieldKind, Vec<Diagnostic>> {
    match node.construct.as_str() {
        "field_type_scalar" => convert_scalar_kind(
            node,
            &identifier(node, "kind")?,
            enums,
            named_types,
            properties,
        ),
        "field_type_enum" => Ok(FieldKind::Enum {
            values: identifiers(node, "values")?,
        }),
        "field_type_reference" => Ok(FieldKind::Reference {
            target: identifier(node, "item_kind")?,
        }),
        "field_type_list" => Ok(FieldKind::List {
            item: Box::new(convert_recursive_field_type(
                only_child_matching(node, is_field_type_construct)?,
                enums,
                named_types,
                properties,
            )?),
        }),
        "field_type_map" => {
            let key_kind = optional_identifier_or_enum(node, "key_kind")?
                .ok_or_else(|| vec![error(node, "map field type is missing its key kind")])?;
            let key = convert_map_key(node, key_kind)?;
            let value = convert_recursive_field_type(
                only_child_matching(node, is_field_type_construct)?,
                enums,
                named_types,
                properties,
            )?;
            Ok(FieldKind::Map {
                key,
                value: Box::new(value),
            })
        }
        unsupported => Err(vec![error(
            node,
            format!("unsupported recursive field type `{unsupported}`"),
        )]),
    }
}

fn convert_typed_default(node: &AstNode, source: &SourceFile) -> Result<String, Vec<Diagnostic>> {
    let captured = captured(node, "value")?;
    match node.construct.as_str() {
        "field_default_text" => Ok(text(node, "value")?.to_owned()),
        "field_default_identifier" => Ok(identifier(node, "value")?.to_owned()),
        "field_default_number" => source
            .text
            .get(captured.span.start..captured.span.end)
            .map(str::to_owned)
            .ok_or_else(|| vec![error(node, "number default span is outside the source")]),
        _ => Err(vec![error(node, "unsupported typed field default")]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CapturedValue;
    use langmeld_core::Span;

    fn captured(value: Value) -> CapturedValue {
        CapturedValue::new(value, Span::new(0, 1))
    }

    fn scalar_type() -> AstNode {
        AstNode::new("field_type_scalar", "scalar", Span::new(0, 1))
            .with_field("kind", captured(Value::Identifier("number".to_owned())))
    }

    #[test]
    fn typed_field_without_requirement_returns_a_diagnostic() {
        let field = AstNode::new("typed_field", "recursive", Span::new(0, 1))
            .with_field("name", captured(Value::Identifier("count".to_owned())))
            .with_children([scalar_type()]);
        let source = SourceFile::new("language.langmeld", "count: number");

        let diagnostics = convert_typed_field(
            &field,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &source,
        )
        .expect_err("missing requirement is reported");

        assert_eq!(diagnostics[0].code, "LMBOOT001");
        assert!(
            diagnostics[0]
                .message
                .contains("missing its required or optional")
        );
        assert_eq!(diagnostics[0].span, Some(field.span));
    }

    #[test]
    fn map_type_without_key_kind_returns_a_diagnostic() {
        let map =
            AstNode::new("field_type_map", "map", Span::new(0, 1)).with_children([scalar_type()]);

        let diagnostic = convert_recursive_field_type(
            &map,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .expect_err("missing key kind is reported");

        assert_eq!(diagnostic[0].code, "LMBOOT001");
        assert!(diagnostic[0].message.contains("missing its key kind"));
        assert_eq!(diagnostic[0].span, Some(map.span));
    }
}
