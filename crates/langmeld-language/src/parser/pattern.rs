//! Compiler for the shared declarative syntax-pattern subset.

use std::collections::BTreeSet;

use langmeld_core::{ConstructSpec, FieldKind};

use crate::LanguageTokenKind as TokenKind;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PatternPart {
    Literal(TokenKind),
    Capture {
        field: String,
        kind: FieldKind,
    },
    Optional(Vec<PatternPart>),
    Choice(Vec<Vec<PatternPart>>),
    ZeroOrMore {
        parts: Vec<PatternPart>,
        capture_fields: BTreeSet<String>,
        separator: Option<Vec<PatternPart>>,
        nested: bool,
    },
    OneOrMore {
        parts: Vec<PatternPart>,
        capture_fields: BTreeSet<String>,
        separator: Option<Vec<PatternPart>>,
        nested: bool,
    },
}

pub(super) fn compile_pattern(
    pattern: &str,
    construct: &ConstructSpec,
) -> Option<Vec<PatternPart>> {
    compile_at_depth(pattern, construct, 0)
}

fn compile_at_depth(
    pattern: &str,
    construct: &ConstructSpec,
    repeat_depth: usize,
) -> Option<Vec<PatternPart>> {
    if repeat_depth > 128 {
        return None;
    }

    let mut characters = pattern.char_indices().peekable();
    let mut parts = Vec::new();
    while let Some((offset, character)) = characters.next() {
        if character.is_whitespace() {
            continue;
        }
        match character {
            '\\' => {
                let (_, escaped) = characters.next()?;
                if !matches!(
                    escaped,
                    '[' | ']' | '(' | ')' | '|' | '{' | '}' | '\\' | '@'
                ) {
                    return None;
                }
                parts.push(PatternPart::Literal(TokenKind::Punctuation(escaped)));
            }
            '[' => {
                let end = optional_end(pattern, offset)?;
                let inner = compile_at_depth(&pattern[offset + 1..end], construct, repeat_depth)?;
                if inner.is_empty() {
                    return None;
                }
                parts.push(PatternPart::Optional(inner));
                while characters
                    .peek()
                    .is_some_and(|(position, _)| *position <= end)
                {
                    characters.next();
                }
            }
            '(' if let Some((end, quantifier)) = repetition_end(pattern, offset) => {
                let mut repeated =
                    compile_at_depth(&pattern[offset + 1..end], construct, repeat_depth + 1)?;
                let separator = repeated.last().and_then(|part| {
                    let PatternPart::Optional(optional) = part else {
                        return None;
                    };
                    optional
                        .iter()
                        .all(|part| matches!(part, PatternPart::Literal(_)))
                        .then(|| optional.clone())
                });
                if separator.is_some() {
                    repeated.pop();
                }
                let capture_fields = capture_fields(&repeated);
                let nested = repeat_depth > 0;
                let repeated = if quantifier == '+' {
                    PatternPart::OneOrMore {
                        parts: repeated,
                        capture_fields,
                        separator,
                        nested,
                    }
                } else {
                    PatternPart::ZeroOrMore {
                        parts: repeated,
                        capture_fields,
                        separator,
                        nested,
                    }
                };
                parts.push(repeated);
                let after = end + 1 + quantifier.len_utf8();
                characters = pattern.char_indices().peekable();
                while characters
                    .peek()
                    .is_some_and(|(position, _)| *position < after)
                {
                    characters.next();
                }
            }
            '(' if let Some(end) = choice_end(pattern, offset) => {
                let mut alternatives = Vec::new();
                for alternative in split_alternatives(&pattern[offset + 1..end]) {
                    let compiled = compile_at_depth(alternative.trim(), construct, repeat_depth)?;
                    if compiled.is_empty() {
                        return None;
                    }
                    alternatives.push(compiled);
                }
                if alternatives.len() < 2 {
                    return None;
                }
                parts.push(PatternPart::Choice(alternatives));
                while characters
                    .peek()
                    .is_some_and(|(position, _)| *position <= end)
                {
                    characters.next();
                }
            }
            '(' | ']' | '}' => return None,
            '{' => {
                let mut name = String::new();
                let mut closed = false;
                for (_, next) in characters.by_ref() {
                    if next == '}' {
                        closed = true;
                        break;
                    }
                    name.push(next);
                }
                if !closed {
                    if pattern[offset + 1..].trim().is_empty() {
                        parts.push(PatternPart::Literal(TokenKind::Punctuation('{')));
                        continue;
                    }
                    return None;
                }
                let (root, property) = name
                    .split_once('.')
                    .map_or((name.as_str(), None), |(root, property)| {
                        (root, Some(property))
                    });
                let root_field = construct.fields.iter().find(|field| field.name == root)?;
                let kind = if let Some(property) = property {
                    if repeat_depth != 1 {
                        return None;
                    }
                    let FieldKind::List { item } = &root_field.kind else {
                        return None;
                    };
                    let FieldKind::Properties { fields, .. } = item.as_ref() else {
                        return None;
                    };
                    fields
                        .iter()
                        .find(|field| field.name == property)?
                        .kind
                        .clone()
                } else {
                    capture_kind(&root_field.kind, repeat_depth)?.clone()
                };
                parts.push(PatternPart::Capture { field: name, kind });
            }
            '-' if characters.peek().is_some_and(|(_, next)| *next == '>') => {
                characters.next();
                parts.push(PatternPart::Literal(TokenKind::Arrow));
            }
            value
                if value.is_ascii_digit()
                    || (value == '-'
                        && characters
                            .peek()
                            .is_some_and(|(_, next)| next.is_ascii_digit())) =>
            {
                let mut literal = String::from(value);
                let mut decimal_seen = false;
                while let Some((_, next)) = characters.peek().copied() {
                    if next.is_ascii_digit() {
                        literal.push(next);
                        characters.next();
                    } else if next == '.' && !decimal_seen {
                        decimal_seen = true;
                        literal.push(next);
                        characters.next();
                    } else {
                        break;
                    }
                }
                parts.push(PatternPart::Literal(TokenKind::Number(literal)));
            }
            value if value == '_' || value.is_alphabetic() => {
                let mut literal = String::from(value);
                while let Some((_, next)) = characters.peek().copied() {
                    if next == '_' || next.is_alphanumeric() {
                        literal.push(next);
                        characters.next();
                    } else if next == '-' {
                        let mut lookahead = characters.clone();
                        lookahead.next();
                        if lookahead.peek().is_some_and(|(_, after)| *after == '>') {
                            break;
                        }
                        literal.push(next);
                        characters.next();
                    } else {
                        break;
                    }
                }
                parts.push(PatternPart::Literal(TokenKind::Identifier(literal)));
            }
            value if value.is_ascii_punctuation() => {
                parts.push(PatternPart::Literal(TokenKind::Punctuation(value)));
            }
            _ => return None,
        }
    }
    (!parts.is_empty()).then_some(parts)
}

fn capture_kind(kind: &FieldKind, repeat_depth: usize) -> Option<&FieldKind> {
    let mut current = kind;
    for _ in 0..repeat_depth {
        let FieldKind::List { item } = current else {
            return None;
        };
        current = item;
    }
    Some(current)
}

fn capture_fields(parts: &[PatternPart]) -> BTreeSet<String> {
    fn collect(parts: &[PatternPart], fields: &mut BTreeSet<String>) {
        for part in parts {
            match part {
                PatternPart::Capture { field, .. } => {
                    fields.insert(field.clone());
                }
                PatternPart::Optional(inner)
                | PatternPart::ZeroOrMore { parts: inner, .. }
                | PatternPart::OneOrMore { parts: inner, .. } => collect(inner, fields),
                PatternPart::Choice(alternatives) => {
                    if let Some(first) = alternatives.first() {
                        collect(first, fields);
                    }
                }
                PatternPart::Literal(_) => {}
            }
        }
    }

    let mut fields = BTreeSet::new();
    collect(parts, &mut fields);
    fields
}

fn repetition_end(pattern: &str, opening: usize) -> Option<(usize, char)> {
    let mut depth = 0_usize;
    let mut escaped = false;
    for (offset, character) in pattern[opening..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' => escaped = true,
            '(' => depth += 1,
            ')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    let end = opening + offset;
                    let quantifier = pattern[end + 1..].chars().next()?;
                    return matches!(quantifier, '*' | '+').then_some((end, quantifier));
                }
            }
            _ => {}
        }
    }
    None
}

fn choice_end(pattern: &str, opening: usize) -> Option<usize> {
    let mut depth = 0_usize;
    let mut has_choice = false;
    let mut escaped = false;
    for (offset, character) in pattern[opening..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' => escaped = true,
            '(' => depth += 1,
            ')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return has_choice.then_some(opening + offset);
                }
            }
            '|' if depth == 1 => has_choice = true,
            _ => {}
        }
    }
    None
}

fn split_alternatives(pattern: &str) -> Vec<&str> {
    let mut alternatives = Vec::new();
    let mut depth = 0_usize;
    let mut start = 0;
    let mut escaped = false;
    for (offset, character) in pattern.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' => escaped = true,
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            '|' if depth == 0 => {
                alternatives.push(&pattern[start..offset]);
                start = offset + character.len_utf8();
            }
            _ => {}
        }
    }
    alternatives.push(&pattern[start..]);
    alternatives
}

fn optional_end(pattern: &str, opening: usize) -> Option<usize> {
    let mut depth = 0_usize;
    let mut escaped = false;
    for (offset, character) in pattern[opening + 1..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' => escaped = true,
            '[' => depth += 1,
            ']' if depth == 0 => return Some(opening + 1 + offset),
            ']' => depth -= 1,
            _ => {}
        }
    }
    None
}

pub(super) fn collect_keywords(parts: &[PatternPart], output: &mut BTreeSet<String>) {
    for part in parts {
        match part {
            PatternPart::Literal(TokenKind::Identifier(word)) => {
                output.insert(word.clone());
            }
            PatternPart::Optional(inner) => collect_keywords(inner, output),
            PatternPart::Choice(alternatives) => {
                for alternative in alternatives {
                    collect_keywords(alternative, output);
                }
            }
            PatternPart::ZeroOrMore {
                parts, separator, ..
            }
            | PatternPart::OneOrMore {
                parts, separator, ..
            } => {
                collect_keywords(parts, output);
                if let Some(separator) = separator {
                    collect_keywords(separator, output);
                }
            }
            PatternPart::Literal(_) | PatternPart::Capture { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use langmeld_core::{FieldSpec, MapKeyKind};

    use super::*;

    fn construct() -> ConstructSpec {
        ConstructSpec {
            id: "binding".to_owned(),
            implements: Vec::new(),
            custom_parser: None,
            syntax: Vec::new(),
            syntax_ids: Vec::new(),
            fields: vec![
                FieldSpec::required("name", FieldKind::Identifier),
                FieldSpec::optional("value", FieldKind::Number),
                FieldSpec::optional("items", FieldKind::list(FieldKind::Identifier)),
                FieldSpec::optional(
                    "properties",
                    FieldKind::list(FieldKind::properties([
                        FieldSpec::required("key", FieldKind::Identifier),
                        FieldSpec::required("value", FieldKind::Text),
                    ])),
                ),
            ],
            block: None,
            symbol: None,
        }
    }

    #[test]
    fn compiler_preserves_captures_and_pattern_operators() {
        let definition = construct();
        let parts = compile_pattern(
            r"let {name} [= {value}] ({items}[,])* ({properties.key} => {properties.value})+",
            &definition,
        )
        .expect("valid shared pattern compiles");

        assert!(
            matches!(parts[0], PatternPart::Literal(TokenKind::Identifier(ref word)) if word == "let")
        );
        assert!(
            matches!(parts[1], PatternPart::Capture { ref field, kind: FieldKind::Identifier } if field == "name")
        );
        assert!(matches!(parts[2], PatternPart::Optional(_)));
        assert!(matches!(
            parts[3],
            PatternPart::ZeroOrMore { ref parts, separator: Some(ref separator), .. }
                if matches!(parts.as_slice(), [PatternPart::Capture { field, kind: FieldKind::Identifier }] if field == "items")
                    && matches!(separator.as_slice(), [PatternPart::Literal(TokenKind::Punctuation(','))])
        ));
        assert!(matches!(parts[4], PatternPart::OneOrMore { .. }));
    }

    #[test]
    fn compiler_rejects_unresolved_fields_and_unsupported_escapes() {
        let definition = construct();

        assert!(compile_pattern("let {missing}", &definition).is_none());
        assert!(compile_pattern(r"let {name} \q", &definition).is_none());
    }

    #[test]
    fn keyword_collection_visits_optional_choice_and_repeat_branches() {
        let definition = construct();
        let parts = compile_pattern("let {name} [mut] (true|false) ({items}[,])*", &definition)
            .expect("valid pattern compiles");
        let mut keywords = BTreeSet::new();
        collect_keywords(&parts, &mut keywords);

        assert_eq!(
            keywords.into_iter().collect::<Vec<_>>(),
            ["false", "let", "mut", "true"]
        );
    }

    #[test]
    fn map_fields_are_not_accidentally_resolved_as_repeat_lists() {
        let mut definition = construct();
        definition.fields.push(FieldSpec::optional(
            "map",
            FieldKind::map(MapKeyKind::Identifier, FieldKind::Number),
        ));
        assert!(compile_pattern("let {name} ({map})*", &definition).is_none());
    }
}
