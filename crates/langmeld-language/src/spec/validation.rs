//! Validation rules for declarative language grammar specifications.

use std::collections::HashSet;

use super::{FieldKind, PatternPart, RepeatMinimum, SpecError};

pub(super) fn patterns_overlap(left: &[PatternPart], right: &[PatternPart]) -> bool {
    left.iter()
        .zip(right)
        .all(|(left, right)| parts_overlap(left, right))
}

pub(super) fn has_group(pattern: &[PatternPart]) -> bool {
    pattern.iter().any(|part| match part {
        PatternPart::Optional(_) | PatternPart::Repeat { .. } => true,
        PatternPart::Literal(_)
        | PatternPart::Field { .. }
        | PatternPart::Choice { .. }
        | PatternPart::Expression { .. } => false,
    })
}

fn parts_overlap(left: &PatternPart, right: &PatternPart) -> bool {
    match (left, right) {
        (PatternPart::Literal(left), PatternPart::Literal(right)) => left == right,
        (PatternPart::Field { kind: left, .. }, PatternPart::Field { kind: right, .. }) => {
            field_kinds_overlap(*left, *right)
        }
        (PatternPart::Choice { values, .. }, PatternPart::Choice { values: other, .. }) => {
            values.iter().any(|value| other.contains(value))
        }
        (PatternPart::Choice { values, .. }, PatternPart::Literal(literal))
        | (PatternPart::Literal(literal), PatternPart::Choice { values, .. }) => {
            values.iter().any(|value| value == literal)
        }
        (PatternPart::Choice { values, .. }, PatternPart::Field { kind, .. })
        | (PatternPart::Field { kind, .. }, PatternPart::Choice { values, .. }) => {
            *kind == FieldKind::Token
                || values
                    .iter()
                    .any(|value| literal_kind(value) == Some(*kind))
        }
        (PatternPart::Expression { .. }, _) | (_, PatternPart::Expression { .. }) => true,
        (PatternPart::Literal(literal), PatternPart::Field { kind, .. })
        | (PatternPart::Field { kind, .. }, PatternPart::Literal(literal)) => {
            *kind == FieldKind::Token || literal_kind(literal) == Some(*kind)
        }
        (PatternPart::Optional(_), _)
        | (PatternPart::Repeat { .. }, _)
        | (_, PatternPart::Optional(_))
        | (_, PatternPart::Repeat { .. }) => true,
    }
}

pub(super) fn validate_pattern(
    pattern: &[PatternPart],
    construct: &str,
    fields: &mut HashSet<String>,
) -> Result<(), SpecError> {
    for part in pattern {
        match part {
            PatternPart::Literal(literal) if literal.is_empty() => {
                return Err(SpecError::EmptyLiteral(construct.to_owned()));
            }
            PatternPart::Literal(literal) if !is_single_token(literal) => {
                return Err(SpecError::InvalidLiteral {
                    construct: construct.to_owned(),
                    literal: literal.clone(),
                });
            }
            PatternPart::Literal(_) => {}
            PatternPart::Field { name, .. } => {
                if name.trim().is_empty() {
                    return Err(SpecError::EmptyFieldName(construct.to_owned()));
                }
                if !fields.insert(name.clone()) {
                    return Err(SpecError::DuplicateField {
                        construct: construct.to_owned(),
                        field: name.clone(),
                    });
                }
            }
            PatternPart::Choice { name, values, .. } => {
                if name.trim().is_empty() {
                    return Err(SpecError::EmptyFieldName(construct.to_owned()));
                }
                if !fields.insert(name.clone()) {
                    return Err(SpecError::DuplicateField {
                        construct: construct.to_owned(),
                        field: name.clone(),
                    });
                }
                if values.is_empty() {
                    return Err(SpecError::EmptyChoiceValues(construct.to_owned()));
                }
                let mut seen = HashSet::new();
                for value in values {
                    if !is_single_token(value) {
                        return Err(SpecError::InvalidChoiceValue {
                            construct: construct.to_owned(),
                            value: value.clone(),
                        });
                    }
                    if !seen.insert(value) {
                        return Err(SpecError::DuplicateChoiceValue {
                            construct: construct.to_owned(),
                            value: value.clone(),
                        });
                    }
                }
            }
            PatternPart::Expression { name, terminator } => {
                if name.trim().is_empty() {
                    return Err(SpecError::EmptyFieldName(construct.to_owned()));
                }
                if !fields.insert(name.clone()) {
                    return Err(SpecError::DuplicateField {
                        construct: construct.to_owned(),
                        field: name.clone(),
                    });
                }
                if !is_single_token(terminator) {
                    return Err(SpecError::InvalidExpressionTerminator {
                        construct: construct.to_owned(),
                        terminator: terminator.clone(),
                    });
                }
            }
            PatternPart::Optional(parts) => {
                if parts.is_empty() {
                    return Err(SpecError::EmptyPatternGroup(construct.to_owned()));
                }
                if sequence_is_nullable(parts) {
                    return Err(SpecError::EmptyOptional(construct.to_owned()));
                }
                validate_pattern(parts, construct, fields)?;
            }
            PatternPart::Repeat { parts, .. } => {
                if parts.is_empty() {
                    return Err(SpecError::EmptyPatternGroup(construct.to_owned()));
                }
                if sequence_is_nullable(parts) {
                    return Err(SpecError::EmptyRepetition(construct.to_owned()));
                }
                validate_pattern(parts, construct, fields)?;
            }
        }
    }
    Ok(())
}

pub(super) fn sequence_is_nullable(parts: &[PatternPart]) -> bool {
    parts.iter().all(|part| match part {
        PatternPart::Literal(_)
        | PatternPart::Field { .. }
        | PatternPart::Choice { .. }
        | PatternPart::Expression { .. } => false,
        PatternPart::Optional(_) => true,
        PatternPart::Repeat { minimum, .. } => *minimum == RepeatMinimum::Zero,
    })
}

#[derive(Clone, Copy)]
enum FirstMatcher<'a> {
    Literal(&'a str),
    Field(FieldKind),
    Choice(&'a [String]),
}

fn first_set(pattern: &[PatternPart]) -> Vec<FirstMatcher<'_>> {
    let mut first = Vec::new();
    for part in pattern {
        match part {
            PatternPart::Literal(value) => {
                first.push(FirstMatcher::Literal(value));
                break;
            }
            PatternPart::Field { kind, .. } => {
                first.push(FirstMatcher::Field(*kind));
                break;
            }
            PatternPart::Choice { values, .. } => {
                first.push(FirstMatcher::Choice(values));
                break;
            }
            PatternPart::Expression { .. } => {
                first.push(FirstMatcher::Field(FieldKind::Token));
                break;
            }
            PatternPart::Optional(parts) => first.extend(first_set(parts)),
            PatternPart::Repeat { parts, minimum } => {
                first.extend(first_set(parts));
                if *minimum == RepeatMinimum::One {
                    break;
                }
            }
        }
    }
    first
}

fn first_matchers_overlap(left: FirstMatcher<'_>, right: FirstMatcher<'_>) -> bool {
    match (left, right) {
        (FirstMatcher::Literal(left), FirstMatcher::Literal(right)) => left == right,
        (FirstMatcher::Choice(left), FirstMatcher::Choice(right)) => {
            left.iter().any(|value| right.contains(value))
        }
        (FirstMatcher::Choice(values), FirstMatcher::Literal(literal))
        | (FirstMatcher::Literal(literal), FirstMatcher::Choice(values)) => {
            values.iter().any(|value| value == literal)
        }
        (FirstMatcher::Choice(values), FirstMatcher::Field(kind))
        | (FirstMatcher::Field(kind), FirstMatcher::Choice(values)) => {
            kind == FieldKind::Token || values.iter().any(|value| literal_kind(value) == Some(kind))
        }
        (FirstMatcher::Field(left), FirstMatcher::Field(right)) => field_kinds_overlap(left, right),
        (FirstMatcher::Literal(literal), FirstMatcher::Field(kind))
        | (FirstMatcher::Field(kind), FirstMatcher::Literal(literal)) => {
            kind == FieldKind::Token || literal_kind(literal) == Some(kind)
        }
    }
}

fn field_kinds_overlap(left: FieldKind, right: FieldKind) -> bool {
    left == right || left == FieldKind::Token || right == FieldKind::Token
}

pub(super) fn validate_decision_points(
    pattern: &[PatternPart],
    construct: &str,
) -> Result<(), SpecError> {
    for (index, part) in pattern.iter().enumerate() {
        let group = match part {
            PatternPart::Optional(parts) | PatternPart::Repeat { parts, .. } => Some(parts),
            PatternPart::Literal(_)
            | PatternPart::Field { .. }
            | PatternPart::Choice { .. }
            | PatternPart::Expression { .. } => None,
        };
        if let Some(group) = group {
            let group_first = first_set(group);
            let suffix_first = first_set(&pattern[index + 1..]);
            if group_first.iter().any(|left| {
                suffix_first
                    .iter()
                    .any(|right| first_matchers_overlap(*left, *right))
            }) {
                return Err(SpecError::AmbiguousPatternBoundary(construct.to_owned()));
            }
            validate_decision_points(group, construct)?;
        }
    }
    Ok(())
}

fn literal_kind(literal: &str) -> Option<FieldKind> {
    let mut characters = literal.chars();
    let first = characters.next()?;
    if first.is_alphabetic() || first == '_' {
        return characters
            .all(|character| character.is_alphanumeric() || character == '_')
            .then_some(FieldKind::Identifier);
    }
    if first.is_ascii_digit() {
        return literal
            .chars()
            .all(|character| character.is_ascii_digit())
            .then_some(FieldKind::Integer);
    }
    (literal.starts_with('"') && literal.ends_with('"')).then_some(FieldKind::String)
}

pub(super) fn is_single_token(literal: &str) -> bool {
    let Some(first) = literal.chars().next() else {
        return false;
    };
    if first == '"' {
        return is_quoted_token(literal);
    }
    if first.is_whitespace() || literal.chars().any(char::is_whitespace) {
        return false;
    }
    if first.is_alphabetic() || first == '_' {
        return literal
            .chars()
            .skip(1)
            .all(|character| character.is_alphanumeric() || character == '_');
    }
    if first.is_ascii_digit() {
        return literal.chars().all(|character| character.is_ascii_digit());
    }
    let is_operator = |character| {
        matches!(
            character,
            '+' | '-'
                | '*'
                | '/'
                | '='
                | '!'
                | '<'
                | '>'
                | '%'
                | '&'
                | '|'
                | '^'
                | '~'
                | '?'
                | ':'
                | '.'
        )
    };
    if is_operator(first) {
        return literal.chars().all(is_operator);
    }
    literal.chars().count() == 1
}

fn is_quoted_token(literal: &str) -> bool {
    if !literal.ends_with('"') || literal.len() < 2 {
        return false;
    }
    let mut escaped = false;
    for (index, character) in literal.char_indices().skip(1) {
        if index == literal.len() - 1 {
            return character == '"' && !escaped;
        }
        if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            return false;
        }
    }
    false
}
