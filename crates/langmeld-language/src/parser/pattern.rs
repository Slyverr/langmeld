use langmeld_core::Span;

use crate::lexer::{LexedToken, TokenKind};
use crate::{FieldKind, PatternPart, RepeatMinimum};

use super::model::{
    CaptureGroup, CaptureGroupKind, CaptureGroupOccurrence, CapturedField, CapturedValue,
};

pub(super) struct MatchAttempt<'a> {
    pub(super) fields: Vec<CapturedField>,
    pub(super) groups: Vec<CaptureGroup>,
    pub(super) end: usize,
    pub(super) complete: bool,
    pub(super) expected: Option<&'a PatternPart>,
}

pub(super) fn any_construct_matches(
    constructs: &[&crate::ConstructSpec],
    tokens: &[LexedToken],
    start: usize,
    remaining_steps: &mut usize,
) -> Result<bool, ()> {
    let Some(token) = tokens.get(start) else {
        return Ok(false);
    };
    for construct in constructs {
        for pattern in construct.variants() {
            if !pattern_may_start_with(pattern, token) {
                continue;
            }
            let attempt = match_sequence(pattern, tokens, start, remaining_steps)?;
            if attempt.complete {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn pattern_may_start_with(pattern: &[PatternPart], token: &LexedToken) -> bool {
    for part in pattern {
        match part {
            PatternPart::Literal(literal) => {
                return token.kind != TokenKind::Invalid && token.text == *literal;
            }
            PatternPart::Field { kind, .. } => return accepts(*kind, token.kind),
            PatternPart::Choice { values, .. } => {
                return token.kind != TokenKind::Invalid
                    && values.iter().any(|value| value == &token.text);
            }
            PatternPart::Expression { .. } => return token.kind != TokenKind::Invalid,
            PatternPart::Optional(parts) => {
                if pattern_may_start_with(parts, token) {
                    return true;
                }
            }
            PatternPart::Repeat { parts, minimum } => {
                if pattern_may_start_with(parts, token) {
                    return true;
                }
                if *minimum == RepeatMinimum::One {
                    return false;
                }
            }
        }
    }
    false
}

pub(super) fn match_sequence<'a>(
    pattern: &'a [PatternPart],
    tokens: &[LexedToken],
    start: usize,
    remaining_steps: &mut usize,
) -> Result<MatchAttempt<'a>, ()> {
    let mut fields = Vec::new();
    let mut groups = Vec::new();
    let mut cursor = start;
    for part in pattern {
        if *remaining_steps == 0 {
            return Err(());
        }
        *remaining_steps -= 1;
        match part {
            PatternPart::Literal(literal) => {
                if tokens
                    .get(cursor)
                    .is_some_and(|token| token.kind != TokenKind::Invalid && token.text == *literal)
                {
                    cursor += 1;
                } else {
                    return Ok(MatchAttempt {
                        fields,
                        groups,
                        end: cursor,
                        complete: false,
                        expected: Some(part),
                    });
                }
            }
            PatternPart::Field { name, kind } => {
                let Some(token) = tokens
                    .get(cursor)
                    .filter(|token| accepts(*kind, token.kind))
                else {
                    return Ok(MatchAttempt {
                        fields,
                        groups,
                        end: cursor,
                        complete: false,
                        expected: Some(part),
                    });
                };
                fields.push(CapturedField {
                    name: name.clone(),
                    value: match kind {
                        FieldKind::Token => CapturedValue::Token {
                            text: token.text.clone(),
                            kind: token.kind,
                        },
                        FieldKind::Identifier => CapturedValue::Identifier(token.text.clone()),
                        FieldKind::Integer => CapturedValue::Integer(token.text.clone()),
                        FieldKind::String => CapturedValue::String(token.text.clone()),
                    },
                    span: token.span,
                });
                cursor += 1;
            }
            PatternPart::Choice { name, values, .. } => {
                let Some(token) = tokens.get(cursor).filter(|token| {
                    token.kind != TokenKind::Invalid
                        && values.iter().any(|value| value == &token.text)
                }) else {
                    return Ok(MatchAttempt {
                        fields,
                        groups,
                        end: cursor,
                        complete: false,
                        expected: Some(part),
                    });
                };
                fields.push(CapturedField {
                    name: name.clone(),
                    value: CapturedValue::Token {
                        text: token.text.clone(),
                        kind: token.kind,
                    },
                    span: token.span,
                });
                cursor += 1;
            }
            PatternPart::Expression { name, terminator } => {
                let Some(end) = expression_end(tokens, cursor, terminator, remaining_steps)? else {
                    return Ok(MatchAttempt {
                        fields,
                        groups,
                        end: tokens.len(),
                        complete: false,
                        expected: Some(part),
                    });
                };
                if end == cursor {
                    return Ok(MatchAttempt {
                        fields,
                        groups,
                        end: cursor,
                        complete: false,
                        expected: Some(part),
                    });
                }
                fields.push(CapturedField {
                    name: name.clone(),
                    value: CapturedValue::Expression,
                    span: Span::new(tokens[cursor].span.start, tokens[end - 1].span.end),
                });
                cursor = end + 1;
            }
            PatternPart::Optional(parts) => {
                let attempt = match_sequence(parts, tokens, cursor, remaining_steps)?;
                if attempt.complete {
                    let field_start = fields.len();
                    let field_end = field_start + attempt.fields.len();
                    let mut nested_groups = attempt.groups;
                    shift_group_ranges(&mut nested_groups, field_start);
                    fields.extend(attempt.fields);
                    groups.push(CaptureGroup {
                        kind: CaptureGroupKind::Optional,
                        occurrences: vec![CaptureGroupOccurrence {
                            span: matched_span(tokens, cursor, attempt.end),
                            field_range: field_start..field_end,
                            groups: nested_groups,
                        }],
                    });
                    cursor = attempt.end;
                } else if attempt.end > cursor {
                    return Ok(MatchAttempt {
                        fields,
                        groups,
                        end: attempt.end,
                        complete: false,
                        expected: attempt.expected,
                    });
                } else {
                    groups.push(CaptureGroup {
                        kind: CaptureGroupKind::Optional,
                        occurrences: Vec::new(),
                    });
                }
            }
            PatternPart::Repeat { parts, minimum } => {
                let mut repetitions = 0;
                let mut occurrences = Vec::new();
                loop {
                    let attempt = match_sequence(parts, tokens, cursor, remaining_steps)?;
                    if attempt.complete && attempt.end > cursor {
                        let field_start = fields.len();
                        let field_end = field_start + attempt.fields.len();
                        let mut nested_groups = attempt.groups;
                        shift_group_ranges(&mut nested_groups, field_start);
                        fields.extend(attempt.fields);
                        occurrences.push(CaptureGroupOccurrence {
                            span: matched_span(tokens, cursor, attempt.end),
                            field_range: field_start..field_end,
                            groups: nested_groups,
                        });
                        cursor = attempt.end;
                        repetitions += 1;
                    } else if attempt.end > cursor {
                        return Ok(MatchAttempt {
                            fields,
                            groups,
                            end: attempt.end,
                            complete: false,
                            expected: attempt.expected,
                        });
                    } else {
                        if *minimum == RepeatMinimum::One && repetitions == 0 {
                            return Ok(MatchAttempt {
                                fields,
                                groups,
                                end: cursor,
                                complete: false,
                                expected: parts.first(),
                            });
                        }
                        break;
                    }
                }
                groups.push(CaptureGroup {
                    kind: CaptureGroupKind::Repeat,
                    occurrences,
                });
            }
        }
    }
    Ok(MatchAttempt {
        fields,
        groups,
        end: cursor,
        complete: true,
        expected: None,
    })
}

fn shift_group_ranges(groups: &mut [CaptureGroup], offset: usize) {
    for group in groups {
        for occurrence in &mut group.occurrences {
            occurrence.field_range.start += offset;
            occurrence.field_range.end += offset;
            shift_group_ranges(&mut occurrence.groups, offset);
        }
    }
}

fn matched_span(tokens: &[LexedToken], start: usize, end: usize) -> Span {
    tokens
        .get(start)
        .zip(end.checked_sub(1).and_then(|index| tokens.get(index)))
        .map_or(Span::default(), |(first, last)| {
            Span::new(first.span.start, last.span.end)
        })
}

pub(super) fn describe_pattern(part: &PatternPart) -> String {
    match part {
        PatternPart::Literal(_) => "the expected token".to_owned(),
        PatternPart::Field {
            kind: FieldKind::Token,
            ..
        } => "a token".to_owned(),
        PatternPart::Field {
            kind: FieldKind::Identifier,
            ..
        } => "an identifier".to_owned(),
        PatternPart::Field {
            kind: FieldKind::Integer,
            ..
        } => "an integer".to_owned(),
        PatternPart::Field {
            kind: FieldKind::String,
            ..
        } => "a quoted string".to_owned(),
        PatternPart::Choice { values, .. } => format!(
            "one of {}",
            values
                .iter()
                .map(|value| format!("`{value}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        PatternPart::Expression { .. } => "an expression".to_owned(),
        PatternPart::Optional(_) | PatternPart::Repeat { .. } => {
            "the expected syntax sequence".to_owned()
        }
    }
}

fn expression_end(
    tokens: &[LexedToken],
    start: usize,
    terminator: &str,
    remaining_steps: &mut usize,
) -> Result<Option<usize>, ()> {
    let mut closing = Vec::new();
    let mut cursor = start;
    while let Some(token) = tokens.get(cursor) {
        if *remaining_steps == 0 {
            return Err(());
        }
        *remaining_steps -= 1;
        if closing.is_empty() && token.kind != TokenKind::Invalid && token.text == terminator {
            return Ok(Some(cursor));
        }
        match token.text.as_str() {
            "(" => closing.push(")"),
            "[" => closing.push("]"),
            "{" => closing.push("}"),
            ")" | "]" | "}" if closing.last() == Some(&token.text.as_str()) => {
                closing.pop();
            }
            _ => {}
        }
        cursor += 1;
    }
    Ok(None)
}

fn accepts(expected: FieldKind, actual: TokenKind) -> bool {
    if expected == FieldKind::Token {
        return actual != TokenKind::Invalid;
    }
    matches!(
        (expected, actual),
        (FieldKind::Identifier, TokenKind::Identifier)
            | (FieldKind::Integer, TokenKind::Integer)
            | (FieldKind::String, TokenKind::String)
    )
}
