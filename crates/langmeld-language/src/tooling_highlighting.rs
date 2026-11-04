use std::collections::BTreeSet;

use langmeld_core::{FieldKind, LanguageSpec, LexicalSpec};

use crate::textmate_scope_for_classification;

pub(crate) fn collect_enum_values(spec: &LanguageSpec) -> BTreeSet<String> {
    let mut values = BTreeSet::new();
    for enumeration in &spec.enums {
        values.extend(enumeration.values.iter().cloned());
    }
    for record in &spec.types {
        for field in &record.fields {
            collect_kind_values(&field.kind, &mut values);
        }
    }
    for construct in &spec.constructs {
        for field in &construct.fields {
            collect_kind_values(&field.kind, &mut values);
        }
    }
    values
}

fn collect_kind_values(kind: &FieldKind, output: &mut BTreeSet<String>) {
    match kind {
        FieldKind::Enum { values } | FieldKind::NamedEnum { values, .. } => {
            output.extend(values.iter().cloned());
        }
        FieldKind::Properties { fields, .. } => {
            for field in fields {
                collect_kind_values(&field.kind, output);
            }
        }
        FieldKind::List { item } => collect_kind_values(item, output),
        FieldKind::Map { value, .. } => collect_kind_values(value, output),
        FieldKind::Identifier
        | FieldKind::Number
        | FieldKind::Text
        | FieldKind::Reference { .. }
        | FieldKind::Expression { .. } => {}
    }
}

pub(crate) fn collect_property_keywords(kind: &FieldKind, output: &mut BTreeSet<String>) {
    match kind {
        FieldKind::Properties { fields, .. } => {
            for field in fields {
                output.insert(field.name.clone());
                collect_property_keywords(&field.kind, output);
            }
        }
        FieldKind::List { item } => collect_property_keywords(item, output),
        FieldKind::Map { value, .. } => collect_property_keywords(value, output),
        FieldKind::Identifier
        | FieldKind::Number
        | FieldKind::Text
        | FieldKind::Enum { .. }
        | FieldKind::NamedEnum { .. }
        | FieldKind::Reference { .. }
        | FieldKind::Expression { .. } => {}
    }
}

pub(crate) fn collect_operators(spec: &LanguageSpec) -> BTreeSet<String> {
    let mut operators = BTreeSet::new();
    for construct in &spec.constructs {
        for pattern in &construct.syntax {
            collect_pattern_operators(pattern, &mut operators);
        }
    }
    for rule in &spec.rules {
        collect_pattern_operators(&rule.pattern, &mut operators);
    }
    for expression in &spec.expressions {
        operators.extend(
            expression
                .operators
                .iter()
                .map(|operator| operator.token.clone()),
        );
    }
    operators
}

fn collect_pattern_operators(pattern: &str, output: &mut BTreeSet<String>) {
    let mut chars = pattern.chars().peekable();
    let mut token = String::new();
    let mut capture_depth = 0_usize;
    let mut quoted = false;
    while let Some(character) = chars.next() {
        if character == '\\' {
            if let Some(escaped) = chars.next()
                && capture_depth == 0
                && !quoted
                && !escaped.is_alphanumeric()
                && !escaped.is_whitespace()
            {
                token.push(escaped);
            }
            continue;
        }
        if character == '"' && capture_depth == 0 {
            flush_operator(&mut token, output);
            quoted = !quoted;
            continue;
        }
        if quoted {
            continue;
        }
        match character {
            '{' => {
                flush_operator(&mut token, output);
                capture_depth += 1;
            }
            '}' if capture_depth > 0 => {
                capture_depth -= 1;
            }
            '[' | ']' | '(' | ')' | '|' | '*' | '+' | '?' if capture_depth == 0 => {
                flush_operator(&mut token, output);
            }
            c if capture_depth > 0 => {
                let _ = c;
            }
            c if c.is_alphanumeric() || c == '_' || c.is_whitespace() => {
                flush_operator(&mut token, output)
            }
            c => token.push(c),
        }
    }
    flush_operator(&mut token, output);
}

fn flush_operator(token: &mut String, output: &mut BTreeSet<String>) {
    if !token.is_empty() {
        output.insert(std::mem::take(token));
    }
}

pub(crate) fn textmate_grammar(
    slug: &str,
    keywords: &BTreeSet<String>,
    operators: &BTreeSet<String>,
    enum_values: &BTreeSet<String>,
    lexical: &LexicalSpec,
) -> serde_json::Value {
    let mut patterns = Vec::new();
    if let Some(line_comment) = &lexical.line_comment
        && let Some(scope) = textmate_scope_for_classification("comment")
    {
        patterns.push(serde_json::json!({
            "name": format!("{scope}.line"),
            "match": format!("{}[^\\r\\n]*", regex_escape(line_comment))
        }));
    }
    if let Some(block_comment) = &lexical.block_comment
        && let Some(scope) = textmate_scope_for_classification("comment")
    {
        patterns.push(serde_json::json!({
            "name": format!("{scope}.block"),
            "begin": regex_escape(&block_comment.open),
            "end": regex_escape(&block_comment.close)
        }));
    }
    let string_scope = textmate_scope_for_classification("string").unwrap_or("string");
    let number_scope = textmate_scope_for_classification("number").unwrap_or("constant.numeric");
    patterns.extend([
        serde_json::json!({"name": string_scope, "begin": "\\\"", "end": "\\\""}),
        serde_json::json!({"name": number_scope, "match": "\\b-?[0-9]+(\\.[0-9]+)?\\b"}),
    ]);
    if !keywords.is_empty()
        && let Some(scope) = textmate_scope_for_classification("keyword")
    {
        patterns.push(serde_json::json!({"name": scope, "match": word_regex(keywords)}));
    }
    if !operators.is_empty()
        && let Some(scope) = textmate_scope_for_classification("operator")
    {
        patterns.push(serde_json::json!({"name": scope, "match": alternative_regex(operators)}));
    }
    if !enum_values.is_empty()
        && let Some(scope) = textmate_scope_for_classification("enumMember")
    {
        patterns.push(serde_json::json!({"name": scope, "match": word_regex(enum_values)}));
    }
    if let Some(scope) = textmate_scope_for_classification("identifier") {
        patterns.push(serde_json::json!({
            "name": scope,
            "match": "[_\\p{L}][_\\p{L}\\p{N}-]*"
        }));
    }
    serde_json::json!({"scopeName": format!("source.{slug}"), "patterns": patterns})
}

fn word_regex(values: &BTreeSet<String>) -> String {
    format!(
        "(?<![_\\p{{L}}\\p{{N}}-])(?:{})(?![_\\p{{L}}\\p{{N}}-])",
        alternative_regex(values)
    )
}

fn alternative_regex(values: &BTreeSet<String>) -> String {
    let mut ordered = values.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| right.len().cmp(&left.len()).then_with(|| left.cmp(right)));
    ordered
        .into_iter()
        .map(|value| regex_escape(value))
        .collect::<Vec<_>>()
        .join("|")
}

fn regex_escape(value: &str) -> String {
    let mut output = String::new();
    for character in value.chars() {
        if ".+*?^$()[]{}|\\".contains(character) {
            output.push('\\');
        }
        output.push(character);
    }
    output
}
