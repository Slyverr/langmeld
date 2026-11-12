//! Lower declared construct patterns into Tree-sitter expressions.

use langmeld_language::{ConstructSpec, FieldKind, PatternPart, RepeatMinimum};

use super::{TreeSitterError, js_string};

pub(super) fn construct_rule_name(index: usize, name: &str) -> String {
    let mut fragment = String::new();
    let mut separator = false;
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            if separator && !fragment.is_empty() {
                fragment.push('_');
            }
            fragment.push(character.to_ascii_lowercase());
            separator = false;
        } else {
            separator = true;
        }
    }
    if fragment.is_empty() {
        fragment.push_str("item");
    }
    format!("construct_{index}_{fragment}")
}

pub(super) fn rule_reference<'a>(rules: &'a [(String, String)], name: &str) -> Option<&'a str> {
    rules
        .iter()
        .find(|(candidate, _)| candidate == name)
        .map(|(_, rule)| rule.as_str())
}

pub(super) fn construct_expression(
    construct: &ConstructSpec,
    rules: &[(String, String)],
) -> Result<String, TreeSitterError> {
    let variants = construct
        .variants()
        .iter()
        .map(|pattern| {
            let mut parts = vec![sequence(pattern, construct.name())?];
            if let Some(block) = construct.block_spec() {
                parts.push(js_string(block.open()));
                let children = block
                    .constructs()
                    .iter()
                    .map(|name| {
                        format!(
                            "$.{}",
                            rule_reference(rules, name)
                                .expect("validated block children exist in the language")
                        )
                    })
                    .collect::<Vec<_>>();
                if !children.is_empty() {
                    parts.push(format!("repeat({})", choice(&children)));
                }
                parts.push(js_string(block.close()));
            }
            Ok(sequence_strings(&parts))
        })
        .collect::<Result<Vec<_>, TreeSitterError>>()?;
    Ok(choice(&variants))
}

fn sequence(parts: &[PatternPart], construct: &str) -> Result<String, TreeSitterError> {
    let items = parts
        .iter()
        .map(|part| pattern_part(part, construct))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(sequence_strings(&items))
}

fn sequence_strings(parts: &[String]) -> String {
    match parts {
        [one] => one.clone(),
        _ => format!("seq({})", parts.join(", ")),
    }
}

fn pattern_part(part: &PatternPart, construct: &str) -> Result<String, TreeSitterError> {
    Ok(match part {
        PatternPart::Literal(literal) => js_string(literal),
        PatternPart::Field { name, kind } => {
            let symbol = match kind {
                FieldKind::Token => "token",
                FieldKind::Identifier => "identifier",
                FieldKind::Integer => "integer",
                FieldKind::String => "string",
            };
            format!("field({}, $.{symbol})", js_string(name))
        }
        PatternPart::Choice { name, values, .. } => {
            let alternatives = values
                .iter()
                .map(|value| js_string(value))
                .collect::<Vec<_>>();
            format!("field({}, {})", js_string(name), choice(&alternatives))
        }
        PatternPart::Expression { name, .. } => {
            return Err(TreeSitterError::UnsupportedExpressionCapture {
                construct: construct.to_owned(),
                field: name.clone(),
            });
        }
        PatternPart::Optional { parts, .. } => {
            format!("optional({})", sequence(parts, construct)?)
        }
        PatternPart::Repeat { parts, minimum, .. } => {
            let function = match minimum {
                RepeatMinimum::Zero => "repeat",
                RepeatMinimum::One => "repeat1",
            };
            format!("{function}({})", sequence(parts, construct)?)
        }
    })
}

pub(super) fn choice(rules: &[String]) -> String {
    match rules {
        [one] => one.clone(),
        _ => format!("choice({})", rules.join(", ")),
    }
}
