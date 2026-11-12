use std::collections::HashMap;

use langmeld_core::{Diagnostic, SourceFile};

use crate::{CapturedValue, CompletionKind, FieldKind, ParsedConstruct, PatternPart};

use super::{decode_quoted, string_field};

#[derive(Clone, Copy, PartialEq, Eq)]
enum PatternNameKind {
    Field,
    Group,
}

pub(super) fn validate_pattern_group_names(
    parts: &[ParsedConstruct],
    source: &SourceFile,
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    let mut names = HashMap::<String, PatternNameKind>::new();

    for part in parts {
        match part.name.as_str() {
            "optional" | "zero_or_more" | "one_or_more" => {
                if part.fields.iter().any(|field| field.name == "name") {
                    let Some(name) = string_field(part, "name").and_then(decode_quoted) else {
                        continue;
                    };
                    if name.trim().is_empty() {
                        diagnostics.push(Diagnostic::error(
                            "LM-DEF-036",
                            "syntax group name cannot be empty",
                            source,
                            part.span,
                        ));
                        return false;
                    }
                    if names.insert(name.clone(), PatternNameKind::Group).is_some() {
                        diagnostics.push(Diagnostic::error(
                            "LM-DEF-037",
                            format!(
                                "syntax field or group name `{name}` is repeated in this scope"
                            ),
                            source,
                            part.span,
                        ));
                        return false;
                    }
                }
                if !validate_pattern_group_names(&part.children, source, diagnostics) {
                    return false;
                }
            }
            "identifier" | "integer" | "string" => {
                if let Some(name) = string_field(part, "value").and_then(decode_quoted)
                    && !record_pattern_field_name(name, part, &mut names, source, diagnostics)
                {
                    return false;
                }
            }
            "choice" | "expression" => {
                if let Some(name) = string_field(part, "name").and_then(decode_quoted)
                    && !record_pattern_field_name(name, part, &mut names, source, diagnostics)
                {
                    return false;
                }
            }
            "literal" | "token" => {}
            _ => {}
        }
    }

    true
}

fn record_pattern_field_name(
    name: String,
    part: &ParsedConstruct,
    names: &mut HashMap<String, PatternNameKind>,
    source: &SourceFile,
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    if names.get(&name) == Some(&PatternNameKind::Group) {
        diagnostics.push(Diagnostic::error(
            "LM-DEF-037",
            format!("syntax field or group name `{name}` is repeated in this scope"),
            source,
            part.span,
        ));
        return false;
    }

    names.entry(name).or_insert(PatternNameKind::Field);
    true
}

pub(super) fn lower_pattern(
    node: &ParsedConstruct,
    source: &SourceFile,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<PatternPart> {
    match node.name.as_str() {
        "literal" | "token" | "identifier" | "integer" | "string" => {
            let Some(value) = string_field(node, "value").and_then(decode_quoted) else {
                diagnostics.push(Diagnostic::error(
                    "LM-DEF-006",
                    "syntax item requires a valid quoted value",
                    source,
                    node.span,
                ));
                return None;
            };
            Some(match node.name.as_str() {
                "literal" => PatternPart::literal(value),
                "token" => PatternPart::token(value),
                "identifier" => PatternPart::Field {
                    name: value,
                    kind: FieldKind::Identifier,
                },
                "integer" => PatternPart::Field {
                    name: value,
                    kind: FieldKind::Integer,
                },
                "string" => PatternPart::Field {
                    name: value,
                    kind: FieldKind::String,
                },
                _ => return None,
            })
        }
        "choice" => {
            let Some(name) = string_field(node, "name").and_then(decode_quoted) else {
                diagnostics.push(Diagnostic::error(
                    "LM-DEF-006",
                    "choice syntax requires a field name",
                    source,
                    node.span,
                ));
                return None;
            };
            let mut values = Vec::new();
            for value in node
                .children
                .iter()
                .filter(|child| child.name == "choice_value")
            {
                if let Some(value) = string_field(value, "value").and_then(decode_quoted) {
                    values.push(value);
                } else {
                    diagnostics.push(Diagnostic::error(
                        "LM-DEF-006",
                        "choice values must be valid quoted tokens",
                        source,
                        value.span,
                    ));
                }
            }
            if values.is_empty() {
                diagnostics.push(Diagnostic::error(
                    "LM-DEF-028",
                    "choice syntax requires at least one `value` declaration",
                    source,
                    node.span,
                ));
                return None;
            }
            let completion_nodes: Vec<_> = node
                .children
                .iter()
                .filter(|child| child.name == "completion")
                .collect();
            if completion_nodes.len() > 1 {
                diagnostics.push(Diagnostic::error(
                    "LM-DEF-032",
                    "a choice field may declare only one completion kind",
                    source,
                    completion_nodes[1].span,
                ));
                return None;
            }
            let completion_kind = completion_nodes.first().and_then(|completion| {
                lower_completion_kind(
                    completion,
                    source,
                    diagnostics,
                    "LM-DEF-033",
                    " for choice field",
                )
            });
            if !completion_nodes.is_empty() && completion_kind.is_none() {
                return None;
            }
            Some(if let Some(kind) = completion_kind {
                PatternPart::choice_with_completion_kind(name, values, kind)
            } else {
                PatternPart::choice(name, values)
            })
        }
        "expression" => {
            let (Some(name), Some(terminator)) = (
                string_field(node, "name").and_then(decode_quoted),
                string_field(node, "terminator").and_then(decode_quoted),
            ) else {
                diagnostics.push(Diagnostic::error(
                    "LM-DEF-006",
                    "expression syntax requires a field name and a valid quoted terminator",
                    source,
                    node.span,
                ));
                return None;
            };
            Some(PatternPart::expression(name, terminator))
        }
        "optional" | "zero_or_more" | "one_or_more" => {
            let has_name = node.fields.iter().any(|field| field.name == "name");
            let group_name = if has_name {
                let Some(name) = string_field(node, "name").and_then(decode_quoted) else {
                    diagnostics.push(Diagnostic::error(
                        "LM-DEF-036",
                        "syntax group name must be a valid quoted string",
                        source,
                        node.span,
                    ));
                    return None;
                };
                Some(name)
            } else {
                None
            };

            let parts = node
                .children
                .iter()
                .filter_map(|child| lower_pattern(child, source, diagnostics))
                .collect::<Vec<_>>();

            Some(match (node.name.as_str(), group_name) {
                ("optional", Some(name)) => PatternPart::optional_named(name, parts),
                ("optional", None) => PatternPart::optional(parts),
                ("zero_or_more", Some(name)) => PatternPart::zero_or_more_named(name, parts),
                ("zero_or_more", None) => PatternPart::zero_or_more(parts),
                ("one_or_more", Some(name)) => PatternPart::one_or_more_named(name, parts),
                ("one_or_more", None) => PatternPart::one_or_more(parts),
                _ => return None,
            })
        }
        _ => None,
    }
}

pub(super) fn lower_completion_kind(
    node: &ParsedConstruct,
    source: &SourceFile,
    diagnostics: &mut Vec<Diagnostic>,
    unknown_code: &'static str,
    unknown_suffix: &str,
) -> Option<CompletionKind> {
    let Some(value) = completion_kind_value(node) else {
        diagnostics.push(Diagnostic::error(
            "LM-DEF-030",
            "completion requires a valid kind name",
            source,
            node.span,
        ));
        return None;
    };
    let Some(kind) = parse_completion_kind(&value) else {
        diagnostics.push(Diagnostic::error(
            unknown_code,
            format!("unknown completion kind `{value}`{unknown_suffix}"),
            source,
            node.span,
        ));
        return None;
    };
    Some(kind)
}

fn parse_completion_kind(value: &str) -> Option<CompletionKind> {
    CompletionKind::from_name(value)
}

fn completion_kind_value(node: &ParsedConstruct) -> Option<String> {
    node.fields
        .iter()
        .find(|field| field.name == "kind")
        .and_then(|field| match &field.value {
            CapturedValue::String(value) => decode_quoted(value),
            CapturedValue::Token {
                text,
                kind: crate::TokenKind::Identifier,
            } => Some(text.clone()),
            _ => None,
        })
}
