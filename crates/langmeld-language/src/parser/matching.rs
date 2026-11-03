//! Ordered matching and typed captures for prepared syntax patterns.

use std::collections::{BTreeMap, BTreeSet};

use langmeld_core::{
    ConstructSpec, Diagnostic, FieldKind, FieldSpec, MapKeyKind, SourceFile, Span,
};

use crate::{
    CapturedValue, LanguageToken, LanguageTokenKind as TokenKind, MapEntry, Value, ValueType,
};

use super::pattern::PatternPart;

#[derive(Debug)]
pub(super) struct PatternMatch {
    pub(super) fields: BTreeMap<String, CapturedValue>,
    pub(super) end_position: usize,
}

#[derive(Clone, Debug)]
pub(super) struct CaptureAttempt {
    pub(super) field: String,
    pub(super) kind: FieldKind,
}

#[derive(Clone, Debug)]
pub(super) struct PatternProgress {
    pub(super) token_position: usize,
    pub(super) span: Span,
    pub(super) captures: Vec<CaptureAttempt>,
    pub(super) has_non_capture: bool,
}

impl PatternMatch {
    /// Combines dotted repeated property captures into their declared list of records.
    pub(super) fn normalize_record_captures(
        &mut self,
        construct: &ConstructSpec,
    ) -> Result<(), Diagnostic> {
        let dotted = self
            .fields
            .keys()
            .filter_map(|name| {
                name.split_once('.')
                    .map(|(root, property)| (name.clone(), root.to_owned(), property.to_owned()))
            })
            .collect::<Vec<_>>();
        let mut grouped = BTreeMap::<String, Vec<(String, CapturedValue)>>::new();
        for (name, root, property) in dotted {
            if let Some(value) = self.fields.remove(&name) {
                grouped.entry(root).or_default().push((property, value));
            }
        }
        for (root, captures) in grouped {
            let field = construct.fields.iter().find(|field| field.name == root);
            let property_specs = field.and_then(|field| match &field.kind {
                FieldKind::List { item } => match item.as_ref() {
                    FieldKind::Properties { fields, .. } => Some(fields),
                    _ => None,
                },
                _ => None,
            });
            let Some(property_specs) = property_specs else {
                return Err(Diagnostic::error(
                    "LM1236",
                    format!("dotted captures for `{root}` require a list of property records"),
                    captures[0].1.span,
                ));
            };
            let columns = captures
                .into_iter()
                .map(|(property, capture)| {
                    let Value::List(values) = capture.value else {
                        return Err(Diagnostic::error(
                            "LM1236",
                            format!("repeated property `{root}.{property}` did not produce a list"),
                            capture.span,
                        ));
                    };
                    Ok((property, values))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let length = columns.first().map_or(0, |(_, values)| values.len());
            if let Some((_, values)) = columns.iter().find(|(_, values)| values.len() != length) {
                return Err(Diagnostic::error(
                    "LM1236",
                    format!("repeated properties for `{root}` must have equal item counts"),
                    values.first().map_or(Span::default(), |value| value.span),
                ));
            }
            let mut records = Vec::with_capacity(length);
            for index in 0..length {
                let mut properties = BTreeMap::new();
                let mut start = usize::MAX;
                let mut end = 0;
                for (property, values) in &columns {
                    let value = values[index].clone();
                    start = start.min(value.span.start);
                    end = end.max(value.span.end);
                    properties.insert(property.clone(), value);
                }
                for property in property_specs {
                    if properties.contains_key(&property.name) {
                        continue;
                    }
                    if let Some(default) = &property.default {
                        let value = default_value(&property.kind, default).ok_or_else(|| {
                            Diagnostic::error(
                                "LM1236",
                                format!("default for `{}` cannot be materialized", property.name),
                                Span::new(end, end),
                            )
                        })?;
                        properties.insert(
                            property.name.clone(),
                            CapturedValue::new(value, Span::new(end, end)),
                        );
                    }
                }
                records.push(CapturedValue::new(
                    Value::Properties(properties),
                    Span::new(start, end),
                ));
            }
            let span = records.first().map_or(Span::default(), |first| {
                Span::new(
                    first.span.start,
                    records.last().map_or(first.span.end, |last| last.span.end),
                )
            });
            self.fields
                .insert(root, CapturedValue::new(Value::List(records), span));
        }
        Ok(())
    }
}

fn record_expected_part(progress: &mut PatternProgress, part: Option<&PatternPart>) {
    match part {
        Some(PatternPart::Capture { field, kind }) => {
            if !progress
                .captures
                .iter()
                .any(|attempt| attempt.field == *field && attempt.kind == *kind)
            {
                progress.captures.push(CaptureAttempt {
                    field: field.clone(),
                    kind: kind.clone(),
                });
            }
        }
        Some(PatternPart::Literal(_)) => progress.has_non_capture = true,
        Some(
            PatternPart::Optional(_)
            | PatternPart::Choice(_)
            | PatternPart::ZeroOrMore { .. }
            | PatternPart::OneOrMore { .. },
        )
        | None => {}
    }
}

pub(super) struct PatternMatcher<'source, 'tokens, 'budget> {
    source: &'source SourceFile,
    tokens: &'tokens [LanguageToken],
    remaining_steps: &'budget mut usize,
    best_error: Option<Diagnostic>,
    progress: Option<PatternProgress>,
}

impl<'source, 'tokens, 'budget> PatternMatcher<'source, 'tokens, 'budget> {
    pub(super) fn new(
        source: &'source SourceFile,
        tokens: &'tokens [LanguageToken],
        remaining_steps: &'budget mut usize,
    ) -> Self {
        Self {
            source,
            tokens,
            remaining_steps,
            best_error: None,
            progress: None,
        }
    }

    pub(super) fn match_pattern(
        &mut self,
        parts: &[PatternPart],
        position: usize,
    ) -> Result<Option<PatternMatch>, Diagnostic> {
        self.best_error = None;
        self.progress = None;
        let mut fields = BTreeMap::new();
        let end = self.match_parts(parts, position, &mut fields, &[])?;
        if let Some(end_position) = end {
            Ok(Some(PatternMatch {
                fields,
                end_position,
            }))
        } else if let Some(error) = self.best_error.take() {
            Err(error)
        } else {
            Ok(None)
        }
    }

    pub(super) fn progress(&self) -> Option<&PatternProgress> {
        self.progress.as_ref()
    }

    fn match_parts(
        &mut self,
        parts: &[PatternPart],
        position: usize,
        fields: &mut BTreeMap<String, CapturedValue>,
        continuation: &[PatternPart],
    ) -> Result<Option<usize>, Diagnostic> {
        self.step(position)?;
        let Some((part, rest)) = parts.split_first() else {
            self.observe_progress(position, None);
            return if continuation.is_empty() {
                Ok(Some(position))
            } else {
                self.match_parts(continuation, position, fields, &[])
            };
        };
        self.observe_progress(position, Some(part));
        match part {
            PatternPart::Literal(expected) => {
                if self
                    .tokens
                    .get(position)
                    .is_some_and(|token| token_matches(expected, &token.kind))
                {
                    self.match_parts(rest, position + 1, fields, continuation)
                } else {
                    Ok(None)
                }
            }
            PatternPart::Capture { field, kind } => {
                let Some(captured) = self.match_field(position, kind, field)? else {
                    return Ok(None);
                };
                let mut branch = fields.clone();
                branch.insert(field.clone(), captured.value);
                if let Some(end) =
                    self.match_parts(rest, captured.end, &mut branch, continuation)?
                {
                    *fields = branch;
                    Ok(Some(end))
                } else {
                    Ok(None)
                }
            }
            PatternPart::Optional(optional) => {
                let tail = concat(rest, continuation);
                let mut included = fields.clone();
                if let Some(end) = self.match_parts(optional, position, &mut included, &tail)? {
                    *fields = included;
                    return Ok(Some(end));
                }
                self.match_parts(rest, position, fields, continuation)
            }
            PatternPart::Choice(alternatives) => {
                let tail = concat(rest, continuation);
                for alternative in alternatives {
                    let mut branch = fields.clone();
                    if let Some(end) =
                        self.match_parts(alternative, position, &mut branch, &tail)?
                    {
                        *fields = branch;
                        return Ok(Some(end));
                    }
                }
                Ok(None)
            }
            PatternPart::ZeroOrMore {
                parts,
                capture_fields,
                separator,
                nested,
            } => self.match_repeat(
                parts,
                capture_fields,
                separator.as_deref(),
                false,
                *nested,
                rest,
                position,
                fields,
                continuation,
            ),
            PatternPart::OneOrMore {
                parts,
                capture_fields,
                separator,
                nested,
            } => self.match_repeat(
                parts,
                capture_fields,
                separator.as_deref(),
                true,
                *nested,
                rest,
                position,
                fields,
                continuation,
            ),
        }
    }

    fn observe_progress(&mut self, position: usize, part: Option<&PatternPart>) {
        let Some(token) = self.tokens.get(position) else {
            return;
        };
        match self.progress.as_mut() {
            Some(progress) if position < progress.token_position => {}
            Some(progress) if position == progress.token_position => {
                record_expected_part(progress, part);
            }
            _ => {
                let mut progress = PatternProgress {
                    token_position: position,
                    span: token.span,
                    captures: Vec::new(),
                    has_non_capture: false,
                };
                record_expected_part(&mut progress, part);
                self.progress = Some(progress);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn match_repeat(
        &mut self,
        repeated: &[PatternPart],
        capture_fields: &BTreeSet<String>,
        separator: Option<&[PatternPart]>,
        require_one: bool,
        nested: bool,
        rest: &[PatternPart],
        position: usize,
        fields: &mut BTreeMap<String, CapturedValue>,
        continuation: &[PatternPart],
    ) -> Result<Option<usize>, Diagnostic> {
        let mut states = vec![(position, 0_usize)];
        let mut cursor = position;
        let mut iterations = Vec::new();
        loop {
            self.step(cursor)?;
            let item_position = if !iterations.is_empty() {
                if let Some(separator) = separator {
                    let Some(after) = self.match_literal_sequence(separator, cursor)? else {
                        break;
                    };
                    after
                } else {
                    cursor
                }
            } else {
                cursor
            };
            let mut item_fields = BTreeMap::new();
            let Some(next) = self.match_parts(repeated, item_position, &mut item_fields, &[])?
            else {
                break;
            };
            if next <= item_position {
                break;
            }
            iterations.push(capture_subset(&item_fields, capture_fields));
            cursor = next;
            states.push((cursor, iterations.len()));
        }

        let minimum = usize::from(require_one);
        let tail = concat(rest, continuation);
        for (end, count) in states.iter().rev() {
            let count = *count;
            if count < minimum {
                continue;
            }
            let mut branch = fields.clone();
            for name in capture_fields {
                let values = iterations[..count]
                    .iter()
                    .filter_map(|iteration| iteration.get(name).cloned())
                    .collect::<Vec<_>>();
                if values.is_empty() && count == 0 && !nested {
                    continue;
                }
                let start = values
                    .first()
                    .map_or_else(|| self.offset_at(position), |value| value.span.start);
                let finish = values.last().map_or(start, |value| value.span.end);
                branch.insert(
                    name.clone(),
                    CapturedValue::new(Value::List(values), Span::new(start, finish)),
                );
            }
            if let Some(separator) = separator
                && let Some(after_separator) = self.match_literal_sequence(separator, *end)?
                && let Some(next) =
                    self.match_parts(&tail, after_separator, &mut branch.clone(), &[])?
            {
                *fields = branch;
                return Ok(Some(next));
            }
            if let Some(next) = self.match_parts(&tail, *end, &mut branch.clone(), &[])? {
                *fields = branch;
                return Ok(Some(next));
            }
        }
        Ok(None)
    }

    fn match_literal_sequence(
        &mut self,
        parts: &[PatternPart],
        mut position: usize,
    ) -> Result<Option<usize>, Diagnostic> {
        for part in parts {
            self.step(position)?;
            let PatternPart::Literal(expected) = part else {
                return Ok(None);
            };
            if !self
                .tokens
                .get(position)
                .is_some_and(|token| token_matches(expected, &token.kind))
            {
                return Ok(None);
            }
            position += 1;
        }
        Ok(Some(position))
    }

    fn match_field(
        &mut self,
        position: usize,
        kind: &FieldKind,
        field: &str,
    ) -> Result<Option<FieldMatch>, Diagnostic> {
        self.step(position)?;
        let Some(token) = self.tokens.get(position) else {
            return Ok(None);
        };
        match kind {
            FieldKind::Properties { fields, .. } => {
                self.match_properties(position, fields, field).map(|value| {
                    value.map(|(value, end)| FieldMatch {
                        value: CapturedValue::new(value, self.range(position, end)),
                        end,
                    })
                })
            }
            FieldKind::List { item } => self.match_list(position, item, field).map(|value| {
                value.map(|(value, end)| FieldMatch {
                    value: CapturedValue::new(value, self.range(position, end)),
                    end,
                })
            }),
            FieldKind::Map { key, value } => {
                self.match_map(position, *key, value, field).map(|value| {
                    value.map(|(value, end)| FieldMatch {
                        value: CapturedValue::new(value, self.range(position, end)),
                        end,
                    })
                })
            }
            _ => {
                let value = match (&token.kind, kind) {
                    (TokenKind::Identifier(v), FieldKind::Identifier) => {
                        Value::Identifier(v.clone())
                    }
                    (TokenKind::Number(v), FieldKind::Number) => match v.parse::<f64>() {
                        Ok(n) if n.is_finite() => Value::Number(n),
                        _ => return Ok(None),
                    },
                    (TokenKind::Text(v), FieldKind::Text) => Value::Text(v.clone()),
                    (
                        TokenKind::Identifier(v),
                        FieldKind::Enum { values } | FieldKind::NamedEnum { values, .. },
                    ) if values.contains(v) => Value::Enum(v.clone()),
                    (actual, FieldKind::Enum { values } | FieldKind::NamedEnum { values, .. }) => {
                        let shown = token_display(actual);
                        self.record_error(Diagnostic::error(
                            "LM1206",
                            format!(
                                "invalid value `{shown}` for enum field `{field}`; expected {}",
                                values.join(", ")
                            ),
                            token.span,
                        ));
                        return Ok(None);
                    }
                    (TokenKind::Identifier(v), FieldKind::Reference { .. }) => {
                        Value::Reference(v.clone())
                    }
                    _ => {
                        self.record_error(Diagnostic::error(
                            "LM1214",
                            format!("invalid value for field `{field}`"),
                            token.span,
                        ));
                        return Ok(None);
                    }
                };
                Ok(Some(FieldMatch {
                    value: CapturedValue::new(value, token.span),
                    end: position + 1,
                }))
            }
        }
    }

    fn match_list(
        &mut self,
        position: usize,
        item: &FieldKind,
        field: &str,
    ) -> Result<Option<(Value, usize)>, Diagnostic> {
        let Some(open) = self.tokens.get(position).filter(|t| is_punct(&t.kind, '[')) else {
            return Ok(None);
        };
        let mut values = Vec::new();
        let mut cursor = self.skip_trivia(position + 1);
        loop {
            if self
                .tokens
                .get(cursor)
                .is_some_and(|t| is_punct(&t.kind, ']'))
            {
                return Ok(Some((Value::List(values), cursor + 1)));
            }
            if self.at_end(cursor) {
                self.record_error(Diagnostic::error(
                    "LM1219",
                    "expected `]` before the end of the file",
                    self.token_span(cursor, open.span),
                ));
                return Ok(None);
            }
            let Some(captured) = self.match_field(cursor, item, field)? else {
                return Ok(None);
            };
            values.push(CapturedValue::new(
                captured.value.value,
                self.range(cursor, captured.end),
            ));
            cursor = self.skip_trivia(captured.end);
            if self
                .tokens
                .get(cursor)
                .is_some_and(|t| is_punct(&t.kind, ','))
            {
                cursor = self.skip_trivia(cursor + 1);
            } else if !self
                .tokens
                .get(cursor)
                .is_some_and(|t| is_punct(&t.kind, ']'))
            {
                let message = if self.at_end(cursor) {
                    "expected `]` before the end of the file"
                } else {
                    "expected `,` or `]` in list value"
                };
                let code = if self.at_end(cursor) {
                    "LM1219"
                } else {
                    "LM1218"
                };
                self.record_error(Diagnostic::error(
                    code,
                    message,
                    self.token_span(cursor, open.span),
                ));
                return Ok(None);
            }
        }
    }

    fn match_map(
        &mut self,
        position: usize,
        key_kind: MapKeyKind,
        value_kind: &FieldKind,
        field: &str,
    ) -> Result<Option<(Value, usize)>, Diagnostic> {
        let Some(open) = self.tokens.get(position).filter(|t| is_punct(&t.kind, '{')) else {
            return Ok(None);
        };
        let mut entries = Vec::new();
        let mut seen = BTreeSet::new();
        let mut cursor = position + 1;
        loop {
            cursor = self.skip_map_separators(cursor);
            let Some(token) = self.tokens.get(cursor) else {
                return Ok(None);
            };
            if is_punct(&token.kind, '}') {
                return Ok(Some((Value::Map(entries), cursor + 1)));
            }
            if matches!(token.kind, TokenKind::End) {
                self.record_error(Diagnostic::error(
                    "LM1223",
                    "expected `}` to close the map",
                    open.span,
                ));
                return Ok(None);
            }
            let (key, key_type) = match (&token.kind, key_kind) {
                (TokenKind::Identifier(v), MapKeyKind::Identifier) => {
                    (v.clone(), ValueType::Identifier)
                }
                (TokenKind::Text(v), MapKeyKind::Text) => (v.clone(), ValueType::Text),
                _ => {
                    self.record_error(Diagnostic::error(
                        "LM1220",
                        format!("invalid key for map field `{field}`"),
                        token.span,
                    ));
                    return Ok(None);
                }
            };
            if !seen.insert(key.clone()) {
                self.record_error(Diagnostic::error(
                    "LM1221",
                    format!("duplicate key `{key}` in map field `{field}`"),
                    token.span,
                ));
                return Ok(None);
            }
            let key_span = token.span;
            cursor += 1;
            if !self
                .tokens
                .get(cursor)
                .is_some_and(|t| is_punct(&t.kind, ':'))
            {
                self.record_error(Diagnostic::error(
                    "LM1222",
                    "expected `:` after map key",
                    self.token_span(cursor, key_span),
                ));
                return Ok(None);
            }
            let value_start = cursor + 1;
            let Some(captured) = self.match_field(value_start, value_kind, field)? else {
                return Ok(None);
            };
            entries.push(MapEntry {
                key,
                key_type,
                key_span,
                value: CapturedValue::new(
                    captured.value.value,
                    self.range(value_start, captured.end),
                ),
            });
            cursor = self.skip_trivia(captured.end);
            if self
                .tokens
                .get(cursor)
                .is_some_and(|t| is_punct(&t.kind, ',') || is_punct(&t.kind, ';'))
            {
                cursor += 1;
            } else if !self
                .tokens
                .get(cursor)
                .is_some_and(|t| is_punct(&t.kind, '}'))
            {
                self.record_error(Diagnostic::error(
                    "LM1224",
                    "expected `,` or `}` in map value",
                    self.token_span(cursor, open.span),
                ));
                return Ok(None);
            }
        }
    }

    fn match_properties(
        &mut self,
        position: usize,
        specs: &[FieldSpec],
        field: &str,
    ) -> Result<Option<(Value, usize)>, Diagnostic> {
        let Some(open) = self.tokens.get(position).filter(|t| is_punct(&t.kind, '{')) else {
            return Ok(None);
        };
        let mut values = BTreeMap::new();
        let mut cursor = position + 1;
        loop {
            cursor = self.skip_property_separators(cursor);
            let Some(token) = self.tokens.get(cursor) else {
                self.record_error(Diagnostic::error(
                    "LM1217",
                    "expected `}` to close the property block",
                    open.span,
                ));
                return Ok(None);
            };
            if is_punct(&token.kind, '}') {
                cursor += 1;
                break;
            }
            if matches!(token.kind, TokenKind::End) {
                self.record_error(Diagnostic::error(
                    "LM1217",
                    "expected `}` to close the property block",
                    open.span,
                ));
                return Ok(None);
            }
            let TokenKind::Identifier(name) = &token.kind else {
                self.record_error(Diagnostic::error(
                    "LM1210",
                    "expected a property name or `}`",
                    token.span,
                ));
                return Ok(None);
            };
            let Some(spec) = specs.iter().find(|candidate| candidate.name == *name) else {
                self.record_error(Diagnostic::error(
                    "LM1211",
                    format!("unknown property `{name}`"),
                    token.span,
                ));
                return Ok(None);
            };
            if values.contains_key(name) {
                self.record_error(Diagnostic::error(
                    "LM1212",
                    format!("property `{name}` is assigned more than once"),
                    token.span,
                ));
                return Ok(None);
            }
            cursor += 1;
            if !self
                .tokens
                .get(cursor)
                .is_some_and(|t| is_punct(&t.kind, ':'))
            {
                self.record_error(Diagnostic::error(
                    "LM1213",
                    "expected `:` after the property name",
                    self.token_span(cursor, token.span),
                ));
                return Ok(None);
            }
            let value_start = cursor + 1;
            let Some(captured) = self.match_field(value_start, &spec.kind, &spec.name)? else {
                return Ok(None);
            };
            values.insert(
                name.clone(),
                CapturedValue::new(captured.value.value, self.range(value_start, captured.end)),
            );
            cursor = captured.end;
            if self.tokens.get(cursor).is_some_and(|t| {
                is_punct(&t.kind, ';')
                    || is_punct(&t.kind, ',')
                    || matches!(t.kind, TokenKind::Newline)
            }) {
                cursor += 1;
            } else if !self
                .tokens
                .get(cursor)
                .is_some_and(|t| is_punct(&t.kind, '}'))
            {
                self.record_error(Diagnostic::error(
                    "LM1215",
                    "expected a newline, `;`, or `}` after the property value",
                    self.token_span(cursor, token.span),
                ));
                return Ok(None);
            }
        }
        for spec in specs {
            if values.contains_key(&spec.name) {
                continue;
            }
            if let Some(default) = &spec.default {
                if let Some(value) = default_value(&spec.kind, default) {
                    values.insert(
                        spec.name.clone(),
                        CapturedValue::new(value, Span::new(open.span.start, open.span.start)),
                    );
                }
            } else if spec.required {
                self.record_error(Diagnostic::error(
                    "LM1216",
                    format!("required property `{}` is missing", spec.name),
                    open.span,
                ));
                return Ok(None);
            }
        }
        let _ = field;
        Ok(Some((Value::Properties(values), cursor)))
    }

    fn skip_trivia(&self, mut position: usize) -> usize {
        while self
            .tokens
            .get(position)
            .is_some_and(|t| matches!(t.kind, TokenKind::Newline | TokenKind::Comment))
        {
            position += 1;
        }
        position
    }
    fn skip_map_separators(&self, mut position: usize) -> usize {
        while self.tokens.get(position).is_some_and(|t| {
            matches!(t.kind, TokenKind::Newline | TokenKind::Comment)
                || is_punct(&t.kind, ',')
                || is_punct(&t.kind, ';')
        }) {
            position += 1;
        }
        position
    }
    fn skip_property_separators(&self, mut position: usize) -> usize {
        while self.tokens.get(position).is_some_and(|t| {
            matches!(t.kind, TokenKind::Newline | TokenKind::Comment)
                || is_punct(&t.kind, ',')
                || is_punct(&t.kind, ';')
        }) {
            position += 1;
        }
        position
    }
    fn range(&self, start: usize, end: usize) -> Span {
        let start = self
            .tokens
            .get(start)
            .map_or(self.source.text.len(), |t| t.span.start);
        let end = end
            .checked_sub(1)
            .and_then(|i| self.tokens.get(i))
            .map_or(start, |t| t.span.end);
        Span::new(start, end)
    }
    fn token_span(&self, position: usize, fallback: Span) -> Span {
        self.tokens.get(position).map_or(fallback, |t| t.span)
    }
    fn at_end(&self, position: usize) -> bool {
        self.tokens
            .get(position)
            .is_none_or(|t| matches!(t.kind, TokenKind::End))
    }
    fn offset_at(&self, position: usize) -> usize {
        self.tokens
            .get(position)
            .map_or(self.source.text.len(), |t| t.span.start)
    }
    fn record_error(&mut self, error: Diagnostic) {
        if self.best_error.is_none() {
            self.best_error = Some(error);
        }
    }
    fn step(&mut self, position: usize) -> Result<(), Diagnostic> {
        if *self.remaining_steps == 0 {
            let offset = self.offset_at(position);
            return Err(Diagnostic::error(
                "LM1304",
                "pattern matching work limit exceeded",
                Span::new(offset, offset),
            ));
        }
        *self.remaining_steps -= 1;
        Ok(())
    }
}

struct FieldMatch {
    value: CapturedValue,
    end: usize,
}

fn concat(first: &[PatternPart], second: &[PatternPart]) -> Vec<PatternPart> {
    let mut out = Vec::with_capacity(first.len() + second.len());
    out.extend_from_slice(first);
    out.extend_from_slice(second);
    out
}
fn capture_subset(
    fields: &BTreeMap<String, CapturedValue>,
    names: &BTreeSet<String>,
) -> BTreeMap<String, CapturedValue> {
    names
        .iter()
        .filter_map(|name| fields.get(name).cloned().map(|value| (name.clone(), value)))
        .collect()
}
fn is_punct(kind: &TokenKind, expected: char) -> bool {
    matches!(kind, TokenKind::Punctuation(value) if *value == expected)
}
fn token_display(kind: &TokenKind) -> String {
    match kind {
        TokenKind::Identifier(value) | TokenKind::Number(value) => value.clone(),
        TokenKind::Text(value) => format!("\"{value}\""),
        TokenKind::Punctuation(value) | TokenKind::Unknown(value) => value.to_string(),
        TokenKind::Arrow => "->".to_owned(),
        TokenKind::Comment => "comment".to_owned(),
        TokenKind::Newline => "newline".to_owned(),
        TokenKind::End => "end of file".to_owned(),
    }
}
pub(super) fn token_matches(expected: &TokenKind, actual: &TokenKind) -> bool {
    match (expected, actual) {
        (TokenKind::Identifier(a), TokenKind::Identifier(b))
        | (TokenKind::Number(a), TokenKind::Number(b))
        | (TokenKind::Text(a), TokenKind::Text(b)) => a == b,
        (TokenKind::Punctuation(a), TokenKind::Punctuation(b)) => a == b,
        (TokenKind::Arrow, TokenKind::Arrow)
        | (TokenKind::Comment, TokenKind::Comment)
        | (TokenKind::Newline, TokenKind::Newline)
        | (TokenKind::End, TokenKind::End) => true,
        (TokenKind::Unknown(a), TokenKind::Unknown(b)) => a == b,
        _ => false,
    }
}
pub(super) fn default_value(kind: &FieldKind, text: &str) -> Option<Value> {
    Some(match kind {
        FieldKind::Identifier => Value::Identifier(text.to_owned()),
        FieldKind::Number => Value::Number(text.parse::<f64>().ok().filter(|v| v.is_finite())?),
        FieldKind::Text => Value::Text(text.to_owned()),
        FieldKind::Enum { values } | FieldKind::NamedEnum { values, .. }
            if values.iter().any(|value| value == text) =>
        {
            Value::Enum(text.to_owned())
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use langmeld_core::FieldSpec;
    use langmeld_core::{ConstructBuilder, LanguageBuilder};

    use super::*;
    use crate::lex_source;

    fn parts(tokens: impl IntoIterator<Item = PatternPart>) -> Vec<PatternPart> {
        tokens.into_iter().collect()
    }
    fn literal(ch: char) -> PatternPart {
        PatternPart::Literal(TokenKind::Punctuation(ch))
    }
    fn ident(field: &str) -> PatternPart {
        PatternPart::Capture {
            field: field.to_owned(),
            kind: FieldKind::Identifier,
        }
    }
    fn lex(text: &str) -> (SourceFile, Vec<LanguageToken>) {
        let source = SourceFile::new("test.lm", text);
        let mut tokens = lex_source(&source).tokens;
        tokens.push(LanguageToken::new(
            TokenKind::End,
            Span::new(text.len(), text.len()),
        ));
        (source, tokens)
    }

    #[test]
    fn choice_uses_following_pattern_before_accepting_an_alternative() {
        let (source, tokens) = lex("b;");
        let mut budget = 100;
        let mut matcher = PatternMatcher::new(&source, &tokens, &mut budget);
        let choice = PatternPart::Choice(vec![
            parts([ident("first"), literal(';')]),
            parts([ident("second")]),
        ]);
        let syntax = parts([choice, literal(';')]);
        let result = matcher
            .match_pattern(&syntax, 0)
            .expect("matching stays within budget")
            .expect("second branch matches");
        assert!(!result.fields.contains_key("first"));
        assert!(result.fields.contains_key("second"));
        assert_eq!(result.end_position, 2);
    }

    #[test]
    fn optional_inclusion_can_backtrack_to_the_skip_path() {
        let (source, tokens) = lex(";");
        let mut budget = 100;
        let mut matcher = PatternMatcher::new(&source, &tokens, &mut budget);
        let syntax = parts([PatternPart::Optional(parts([literal(',')])), literal(';')]);
        let result = matcher
            .match_pattern(&syntax, 0)
            .expect("matching stays within budget")
            .expect("optional can be skipped");
        assert_eq!(result.end_position, 1);
    }

    #[test]
    fn repetition_backtracks_and_accepts_a_trailing_separator() {
        let (source, tokens) = lex("a,b,;");
        let mut budget = 1_000;
        let mut matcher = PatternMatcher::new(&source, &tokens, &mut budget);
        let repeat = PatternPart::ZeroOrMore {
            parts: parts([ident("item")]),
            capture_fields: BTreeSet::from(["item".to_owned()]),
            separator: Some(parts([literal(',')])),
            nested: false,
        };
        let syntax = parts([repeat, literal(';')]);
        let result = matcher
            .match_pattern(&syntax, 0)
            .expect("matching stays within budget")
            .expect("repeat and continuation match");
        let Value::List(items) = &result.fields["item"].value else {
            panic!("repeated capture is a list")
        };
        assert_eq!(items.len(), 2);
        assert_eq!(result.end_position, 5);
    }

    #[test]
    fn nested_repetitions_preserve_nested_lists() {
        let (source, tokens) = lex("a,b;c,d");
        let mut budget = 1_000;
        let mut matcher = PatternMatcher::new(&source, &tokens, &mut budget);
        let inner = PatternPart::OneOrMore {
            parts: parts([ident("item")]),
            capture_fields: BTreeSet::from(["item".to_owned()]),
            separator: Some(parts([literal(',')])),
            nested: true,
        };
        let outer = PatternPart::OneOrMore {
            parts: parts([inner]),
            capture_fields: BTreeSet::from(["item".to_owned()]),
            separator: Some(parts([literal(';')])),
            nested: false,
        };
        let result = matcher
            .match_pattern(&parts([outer]), 0)
            .expect("matching stays within budget")
            .expect("nested groups match");
        let Value::List(groups) = &result.fields["item"].value else {
            panic!("outer repetition produces a list")
        };
        assert_eq!(groups.len(), 2);
        assert!(matches!(groups[0].value, Value::List(ref values) if values.len() == 2));
        assert!(matches!(groups[1].value, Value::List(ref values) if values.len() == 2));
    }

    #[test]
    fn typed_lists_maps_and_property_defaults_are_captured() {
        let (source, tokens) = lex("[1, 2,] {a: 3; b: 4} { name: demo }");
        let mut budget = 2_000;
        let mut matcher = PatternMatcher::new(&source, &tokens, &mut budget);
        let fields = vec![FieldSpec::with_default(
            "name",
            FieldKind::Identifier,
            "fallback",
        )];
        let list = matcher
            .match_field(0, &FieldKind::list(FieldKind::Number), "nums")
            .expect("budget")
            .expect("list parsed");
        let map = matcher
            .match_field(
                list.end,
                &FieldKind::map(MapKeyKind::Identifier, FieldKind::Number),
                "scores",
            )
            .expect("budget")
            .expect("map parsed");
        let props = matcher
            .match_field(map.end, &FieldKind::properties(fields), "record")
            .expect("budget")
            .expect("properties parsed");
        assert!(matches!(list.value.value, Value::List(ref values) if values.len() == 2));
        assert!(matches!(map.value.value, Value::Map(ref entries) if entries.len() == 2));
        assert!(
            matches!(props.value.value, Value::Properties(ref values) if matches!(values["name"].value, Value::Identifier(ref value) if value == "demo"))
        );
    }

    #[test]
    fn dotted_repeated_property_captures_form_records_with_defaults() {
        let spec = LanguageBuilder::new("Records")
            .version("0.1.0")
            .root("program")
            .construct(
                ConstructBuilder::new("program")
                    .syntax("program {")
                    .block(["record"]),
            )
            .construct(
                ConstructBuilder::new("record")
                    .syntax("record ({items.name})+")
                    .field(FieldSpec::required(
                        "items",
                        FieldKind::list(FieldKind::properties([
                            FieldSpec::required("name", FieldKind::Identifier),
                            FieldSpec::with_default(
                                "enabled",
                                FieldKind::Enum {
                                    values: vec!["yes".to_owned(), "no".to_owned()],
                                },
                                "yes",
                            ),
                        ])),
                    )),
            )
            .build()
            .expect("record definition is valid");
        let construct = &spec.spec().constructs[1];
        let (source, tokens) = lex("a b");
        let mut budget = 100;
        let mut matcher = PatternMatcher::new(&source, &tokens, &mut budget);
        let repeated = PatternPart::OneOrMore {
            parts: parts([ident("items.name")]),
            capture_fields: BTreeSet::from(["items.name".to_owned()]),
            separator: None,
            nested: false,
        };
        let mut result = matcher
            .match_pattern(&parts([repeated]), 0)
            .expect("matching stays within budget")
            .expect("both repeated records match");
        result
            .normalize_record_captures(construct)
            .expect("record columns have equal lengths");
        let Value::List(records) = &result.fields["items"].value else {
            panic!("dotted captures form a list")
        };
        assert_eq!(records.len(), 2);
        let Value::Properties(properties) = &records[0].value else {
            panic!("each item is a record")
        };
        assert!(matches!(properties["name"].value, Value::Identifier(ref value) if value == "a"));
        assert!(matches!(properties["enabled"].value, Value::Enum(ref value) if value == "yes"));
    }

    #[test]
    fn pattern_work_is_bounded() {
        let (source, tokens) = lex("a");
        let mut budget = 0;
        let mut matcher = PatternMatcher::new(&source, &tokens, &mut budget);
        let error = matcher
            .match_pattern(&parts([ident("name")]), 0)
            .expect_err("zero budget is rejected");
        assert_eq!(error.code, "LM1304");
    }

    #[test]
    fn invalid_enum_values_report_the_declared_type_diagnostic() {
        let (source, tokens) = lex("42");
        let mut budget = 100;
        let mut matcher = PatternMatcher::new(&source, &tokens, &mut budget);
        let kind = FieldKind::Enum {
            values: vec!["fast".to_owned(), "safe".to_owned()],
        };
        let result = matcher.match_field(0, &kind, "mode").expect("budget");
        let diagnostic = matcher.best_error.expect("invalid enum is diagnosed");
        assert_eq!(diagnostic.code, "LM1206");
        assert!(result.is_none());
    }
}
