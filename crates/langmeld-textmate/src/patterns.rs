use std::collections::HashSet;

use langmeld_language::PatternPart;

use crate::model::{MatchPattern, StringPattern};

const GROUPED_OPERATOR_CHARACTERS: &str = "+-*/=!<>%&|^~?:.";
const GROUPED_OPERATOR_CLASS: &str = r"[+*/=!<>%&|^~?:.-]";

pub(super) fn collect_literals(
    parts: &[PatternPart],
    output: &mut Vec<String>,
    seen: &mut HashSet<String>,
) {
    for part in parts {
        match part {
            PatternPart::Literal(literal) => {
                if seen.insert(literal.clone()) {
                    output.push(literal.clone());
                }
            }
            PatternPart::Choice { values, .. } => {
                for value in values {
                    if seen.insert(value.clone()) {
                        output.push(value.clone());
                    }
                }
            }
            PatternPart::Field { .. } | PatternPart::Expression { .. } => {}
            PatternPart::Optional { parts, .. } | PatternPart::Repeat { parts, .. } => {
                collect_literals(parts, output, seen);
            }
        }
    }
}

pub(super) fn is_identifier_literal(literal: &str) -> bool {
    let mut characters = literal.chars();
    characters
        .next()
        .is_some_and(|character| character == '_' || character.is_alphabetic())
        && characters.all(|character| character == '_' || character.is_alphanumeric())
}

pub(super) fn punctuation_regex(literals: Vec<String>) -> String {
    let (mut grouped_operators, single_punctuation): (Vec<_>, Vec<_>) = literals
        .into_iter()
        .partition(|literal| is_grouped_operator_literal(literal));
    grouped_operators.sort_by_key(|literal| std::cmp::Reverse(literal.len()));

    let mut alternatives = Vec::new();
    if !grouped_operators.is_empty() {
        let operators = grouped_operators
            .iter()
            .map(|literal| regex_escape(literal))
            .collect::<Vec<_>>()
            .join("|");
        alternatives.push(format!(
            "(?<!{GROUPED_OPERATOR_CLASS})(?:{operators})(?!{GROUPED_OPERATOR_CLASS})"
        ));
    }
    if !single_punctuation.is_empty() {
        alternatives.push(format!(
            "(?:{})",
            single_punctuation
                .iter()
                .map(|literal| regex_escape(literal))
                .collect::<Vec<_>>()
                .join("|")
        ));
    }

    format!("(?:{})", alternatives.join("|"))
}

fn is_grouped_operator_literal(literal: &str) -> bool {
    literal
        .chars()
        .all(|character| GROUPED_OPERATOR_CHARACTERS.contains(character))
}

pub(super) fn regex_escape(literal: &str) -> String {
    let mut escaped = String::with_capacity(literal.len());
    for character in literal.chars() {
        if r"\.^$|?*+()[]{}".contains(character) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

pub(super) fn string_pattern(suffix: &str) -> StringPattern {
    StringPattern {
        name: format!("string.quoted.double.{suffix}"),
        begin: "\"".to_owned(),
        end: "\"".to_owned(),
        patterns: vec![MatchPattern {
            name: Some(format!("constant.character.escape.{suffix}")),
            regex: r"\\(?:.|$)".to_owned(),
        }],
    }
}
