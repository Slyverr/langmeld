//! Syntax-template parsing and conflict analysis helpers.

use std::collections::BTreeSet;

use super::*;

pub(super) fn pattern_placeholders(pattern: &str) -> Result<Vec<(String, bool, usize)>, String> {
    fn scan(
        fragment: &str,
        whole_pattern: &str,
        inherited_optional: bool,
        repeat_depth: usize,
        placeholders: &mut Vec<(String, bool, usize)>,
    ) -> Result<bool, String> {
        let mut characters = fragment.char_indices().peekable();
        let mut has_content = false;
        while let Some((offset, character)) = characters.next() {
            match character {
                '\\' => {
                    let Some((_, escaped)) = characters.next() else {
                        return Err(format!(
                            "pattern `{whole_pattern}` ends with an incomplete escape"
                        ));
                    };
                    if !matches!(
                        escaped,
                        '[' | ']' | '(' | ')' | '|' | '{' | '}' | '\\' | '@'
                    ) {
                        return Err(format!(
                            "pattern `{whole_pattern}` uses unsupported escape `\\{escaped}`"
                        ));
                    }
                    has_content = true;
                }
                '[' => {
                    let end = optional_group_end(fragment, offset).ok_or_else(|| {
                        format!("pattern `{whole_pattern}` has an unclosed optional group")
                    })?;
                    let inner = &fragment[offset + 1..end];
                    if !scan(inner, whole_pattern, true, repeat_depth, placeholders)? {
                        return Err(format!(
                            "pattern `{whole_pattern}` has an empty optional group"
                        ));
                    }
                    while characters
                        .peek()
                        .is_some_and(|(position, _)| *position <= end)
                    {
                        characters.next();
                    }
                    has_content = true;
                }
                ']' => {
                    return Err(format!(
                        "pattern `{whole_pattern}` has an unmatched `]` at byte {offset}"
                    ));
                }
                '(' => {
                    if let Some((end, quantifier)) = repetition_group_end(fragment, offset) {
                        let before = placeholders.len();
                        let inner = &fragment[offset + 1..end];
                        if !scan(
                            inner,
                            whole_pattern,
                            inherited_optional || quantifier == '*',
                            repeat_depth + 1,
                            placeholders,
                        )? {
                            return Err(format!(
                                "pattern `{whole_pattern}` has an empty repetition group"
                            ));
                        }
                        let captures = &placeholders[before..];
                        let valid_record_captures = captures.len() > 1
                            && captures.iter().all(|(field, _, _)| field.contains('.'))
                            && captures
                                .iter()
                                .map(|(field, _, _)| field.split_once('.').unwrap().0)
                                .all(|root| {
                                    captures.iter().all(|(field, _, _)| {
                                        field.split_once('.').unwrap().0 == root
                                    })
                                })
                            && captures
                                .iter()
                                .map(|(field, _, _)| field.as_str())
                                .collect::<BTreeSet<_>>()
                                .len()
                                == captures.len();
                        if !captures.is_empty() && captures.len() != 1 && !valid_record_captures {
                            return Err(format!(
                                "repetition group in pattern `{whole_pattern}` may contain syntax only, one capture, or distinct properties of one repeated record"
                            ));
                        }
                        let quantifier_width = quantifier.len_utf8();
                        while characters
                            .peek()
                            .is_some_and(|(position, _)| *position <= end + quantifier_width)
                        {
                            characters.next();
                        }
                        has_content = true;
                    } else if let Some(end) = choice_group_end(fragment, offset) {
                        let inner = &fragment[offset + 1..end];
                        let mut seen_alternatives = BTreeSet::new();
                        let mut expected_captures = None;
                        let mut canonical_captures = Vec::new();
                        for alternative in split_choice_alternatives(inner) {
                            let alternative = alternative.trim();
                            if alternative.is_empty() {
                                return Err(format!(
                                    "choice group in pattern `{whole_pattern}` contains an empty alternative"
                                ));
                            }
                            if !seen_alternatives.insert(alternative) {
                                return Err(format!(
                                    "choice group in pattern `{whole_pattern}` repeats alternative `{alternative}`"
                                ));
                            }
                            let mut alternative_placeholders = Vec::new();
                            if !scan(
                                alternative,
                                whole_pattern,
                                inherited_optional,
                                repeat_depth,
                                &mut alternative_placeholders,
                            )? {
                                return Err(format!(
                                    "choice group in pattern `{whole_pattern}` contains an empty alternative"
                                ));
                            }
                            let mut branch_captures = BTreeSet::new();
                            if alternative_placeholders
                                .iter()
                                .any(|(field, _, _)| !branch_captures.insert(field.as_str()))
                            {
                                return Err(format!(
                                    "a choice alternative in pattern `{whole_pattern}` captures a field more than once"
                                ));
                            }
                            let mut capture_signature = alternative_placeholders.clone();
                            capture_signature.sort();
                            if let Some(expected) = &expected_captures {
                                if expected != &capture_signature {
                                    return Err(format!(
                                        "choice alternatives in pattern `{whole_pattern}` must capture the same fields"
                                    ));
                                }
                            } else {
                                expected_captures = Some(capture_signature);
                                canonical_captures = alternative_placeholders;
                            }
                        }
                        placeholders.extend(canonical_captures);
                        while characters
                            .peek()
                            .is_some_and(|(position, _)| *position <= end)
                        {
                            characters.next();
                        }
                        has_content = true;
                    } else {
                        has_content = true;
                    }
                }
                '{' => {
                    let Some((end, _)) = characters.find(|(_, next)| *next == '}') else {
                        if fragment[offset + 1..].trim().is_empty() {
                            has_content = true;
                            continue;
                        }
                        return Err(format!(
                            "pattern `{whole_pattern}` has an unclosed placeholder at byte {offset}"
                        ));
                    };
                    let field = &fragment[offset + 1..end];
                    if field.split('.').any(|part| {
                        part.is_empty()
                            || !part.chars().enumerate().all(|(index, item)| {
                                if index == 0 {
                                    item == '_' || item.is_alphabetic()
                                } else {
                                    item == '_' || item.is_alphanumeric()
                                }
                            })
                    }) {
                        return Err(format!(
                            "pattern `{whole_pattern}` has invalid placeholder `{field}`"
                        ));
                    }
                    placeholders.push((field.to_owned(), inherited_optional, repeat_depth));
                    has_content = true;
                }
                '}' => {
                    return Err(format!(
                        "pattern `{whole_pattern}` has an unmatched `}}` at byte {offset}"
                    ));
                }
                '"' | '#' => {
                    return Err(format!(
                        "pattern `{whole_pattern}` cannot match standalone `{character}` tokens"
                    ));
                }
                _ if !character.is_whitespace() => has_content = true,
                _ => {}
            }
        }
        Ok(has_content)
    }

    let mut placeholders = Vec::new();
    scan(pattern, pattern, false, 0, &mut placeholders)?;
    Ok(placeholders)
}

#[derive(Clone)]
pub(super) struct PatternConflictCandidate<'a> {
    pub(super) construct: &'a ConstructSpec,
    pub(super) pattern: &'a str,
    pub(super) captures: Vec<(String, bool, usize)>,
    pub(super) signature: String,
    pub(super) atoms: Option<Vec<PatternAtom>>,
    pub(super) has_repetition: bool,
    pub(super) automaton_supported: bool,
}

pub(super) const MAX_PATTERN_REALIZATIONS_PER_BLOCK: usize = 1024;
pub(super) const MAX_PATTERN_CONFLICT_COMPARISONS: usize = 4096;
pub(super) const MAX_PATTERN_AUTOMATON_WORK_PER_BLOCK: usize = 65_536;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PatternAtom {
    Literal(PatternLiteral),
    Capture(String, usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PatternLiteral {
    Identifier(String),
    Number(String),
    Arrow,
    Punctuation(char),
}

#[derive(Clone, Debug)]
enum PatternRegex {
    Empty,
    Atom(PatternAtom),
    Sequence(Vec<Self>),
    Choice(Vec<Self>),
    Optional(Box<Self>),
    Repeat {
        body: Box<Self>,
        minimum: usize,
        separator: Option<Box<Self>>,
    },
}

#[derive(Default)]
struct PatternAutomatonState {
    epsilon: Vec<usize>,
    transitions: Vec<(PatternAtom, usize)>,
}

pub(super) struct PatternTokenAutomaton {
    states: Vec<PatternAutomatonState>,
    start: usize,
    accept: usize,
    pub(super) has_repetition: bool,
}

pub(super) struct PatternAutomatonCandidate<'a> {
    pub(super) construct: &'a ConstructSpec,
    pub(super) pattern: &'a str,
    pub(super) automaton: PatternTokenAutomaton,
}

type PatternConflictKey = ((String, String), (String, String));

pub(super) fn pattern_conflict_key(
    left_construct: &ConstructSpec,
    left_pattern: &str,
    right_construct: &ConstructSpec,
    right_pattern: &str,
) -> PatternConflictKey {
    let left = (left_construct.id.clone(), left_pattern.to_owned());
    let right = (right_construct.id.clone(), right_pattern.to_owned());
    if left <= right {
        (left, right)
    } else {
        (right, left)
    }
}

fn parse_pattern_regex(pattern: &str, construct: &ConstructSpec) -> Option<(PatternRegex, bool)> {
    const MAX_GROUP_DEPTH: usize = 128;

    fn sequence(parts: Vec<PatternRegex>) -> PatternRegex {
        match parts.len() {
            0 => PatternRegex::Empty,
            1 => parts.into_iter().next().expect("one pattern part exists"),
            _ => PatternRegex::Sequence(parts),
        }
    }

    fn strip_literal_separator(regex: &mut PatternRegex) -> Option<Box<PatternRegex>> {
        fn is_literal_sequence(regex: &PatternRegex) -> bool {
            match regex {
                PatternRegex::Atom(PatternAtom::Literal(_)) => true,
                PatternRegex::Sequence(parts) => parts.iter().all(is_literal_sequence),
                _ => false,
            }
        }

        let PatternRegex::Sequence(parts) = regex else {
            return None;
        };
        let Some(PatternRegex::Optional(inner)) = parts.last() else {
            return None;
        };
        if !is_literal_sequence(inner) {
            return None;
        }
        let separator = inner.clone();
        parts.pop();
        Some(separator)
    }

    fn parse_fragment(
        fragment: &str,
        construct: &ConstructSpec,
        repeat_depth: usize,
        depth: usize,
    ) -> Option<(PatternRegex, bool)> {
        if depth > MAX_GROUP_DEPTH {
            return None;
        }

        let mut characters = fragment.char_indices().peekable();
        let mut parts = Vec::new();
        let mut has_repetition = false;
        while let Some((offset, character)) = characters.next() {
            if character.is_whitespace() {
                continue;
            }
            if character == '\\' {
                let (_, escaped) = characters.next()?;
                if !matches!(
                    escaped,
                    '[' | ']' | '(' | ')' | '|' | '{' | '}' | '\\' | '@'
                ) {
                    return None;
                }
                parts.push(PatternRegex::Atom(PatternAtom::Literal(
                    PatternLiteral::Punctuation(escaped),
                )));
                continue;
            }
            if character == '[' {
                let end = optional_group_end(fragment, offset)?;
                let (body, nested_repetition) = parse_fragment(
                    &fragment[offset + 1..end],
                    construct,
                    repeat_depth,
                    depth + 1,
                )?;
                has_repetition |= nested_repetition;
                parts.push(PatternRegex::Optional(Box::new(body)));
                while characters
                    .peek()
                    .is_some_and(|(position, _)| *position <= end)
                {
                    characters.next();
                }
                continue;
            }
            if character == ']' {
                return None;
            }
            if character == '(' {
                if let Some((end, quantifier)) = repetition_group_end(fragment, offset) {
                    let (mut body, _) = parse_fragment(
                        &fragment[offset + 1..end],
                        construct,
                        repeat_depth + 1,
                        depth + 1,
                    )?;
                    let separator = strip_literal_separator(&mut body);
                    has_repetition = true;
                    parts.push(PatternRegex::Repeat {
                        body: Box::new(body),
                        minimum: usize::from(quantifier == '+'),
                        separator,
                    });
                    while characters
                        .peek()
                        .is_some_and(|(position, _)| *position <= end + quantifier.len_utf8())
                    {
                        characters.next();
                    }
                    continue;
                }
                if let Some(end) = choice_group_end(fragment, offset) {
                    let mut alternatives = Vec::new();
                    for alternative in split_choice_alternatives(&fragment[offset + 1..end]) {
                        let (alternative, nested_repetition) =
                            parse_fragment(alternative.trim(), construct, repeat_depth, depth + 1)?;
                        has_repetition |= nested_repetition;
                        alternatives.push(alternative);
                    }
                    if alternatives.len() < 2 {
                        return None;
                    }
                    parts.push(PatternRegex::Choice(alternatives));
                    while characters
                        .peek()
                        .is_some_and(|(position, _)| *position <= end)
                    {
                        characters.next();
                    }
                    continue;
                }
                parts.push(PatternRegex::Atom(PatternAtom::Literal(
                    PatternLiteral::Punctuation(character),
                )));
                continue;
            }
            if character == '{' {
                let mut name = String::new();
                let mut closed = false;
                for (_, next) in characters.by_ref() {
                    if next == '}' {
                        closed = true;
                        break;
                    }
                    name.push(next);
                }
                if !closed
                    || pattern_capture_kind(construct, &(name.clone(), false, repeat_depth))
                        .is_none()
                {
                    return None;
                }
                parts.push(PatternRegex::Atom(PatternAtom::Capture(name, repeat_depth)));
                continue;
            }
            if character == '}' || character == '"' || character == '#' {
                return None;
            }
            if character == '-' && characters.peek().is_some_and(|(_, next)| *next == '>') {
                characters.next();
                parts.push(PatternRegex::Atom(PatternAtom::Literal(
                    PatternLiteral::Arrow,
                )));
                continue;
            }
            if character.is_ascii_digit()
                || (character == '-'
                    && characters
                        .peek()
                        .is_some_and(|(_, next)| next.is_ascii_digit()))
            {
                let mut number = String::from(character);
                let mut decimal = false;
                while let Some((_, next)) = characters.peek().copied() {
                    if next.is_ascii_digit() {
                        number.push(next);
                        characters.next();
                    } else if next == '.' && !decimal {
                        number.push(next);
                        characters.next();
                        decimal = true;
                    } else {
                        break;
                    }
                }
                parts.push(PatternRegex::Atom(PatternAtom::Literal(
                    PatternLiteral::Number(number),
                )));
                continue;
            }
            if character == '_' || character.is_alphabetic() {
                let mut identifier = String::from(character);
                while let Some((_, next)) = characters.peek().copied() {
                    if next == '_' || next.is_alphanumeric() {
                        identifier.push(next);
                        characters.next();
                    } else if next == '-' {
                        let mut lookahead = characters.clone();
                        lookahead.next();
                        if lookahead.peek().is_some_and(|(_, after)| *after == '>') {
                            break;
                        }
                        identifier.push(next);
                        characters.next();
                    } else {
                        break;
                    }
                }
                parts.push(PatternRegex::Atom(PatternAtom::Literal(
                    PatternLiteral::Identifier(identifier),
                )));
                continue;
            }
            if character.is_ascii_punctuation() {
                parts.push(PatternRegex::Atom(PatternAtom::Literal(
                    PatternLiteral::Punctuation(character),
                )));
                continue;
            }
            return None;
        }
        Some((sequence(parts), has_repetition))
    }

    parse_fragment(pattern, construct, 0, 0)
}

struct PatternAutomatonBuilder {
    states: Vec<PatternAutomatonState>,
}

impl PatternAutomatonBuilder {
    const MAX_STATES: usize = 512;

    fn new_state(&mut self) -> Option<usize> {
        if self.states.len() >= Self::MAX_STATES {
            return None;
        }
        let state = self.states.len();
        self.states.push(PatternAutomatonState::default());
        Some(state)
    }

    fn compile(&mut self, regex: &PatternRegex, depth: usize) -> Option<(usize, usize)> {
        if depth > 128 {
            return None;
        }
        match regex {
            PatternRegex::Empty => {
                let start = self.new_state()?;
                let accept = self.new_state()?;
                self.states[start].epsilon.push(accept);
                Some((start, accept))
            }
            PatternRegex::Atom(atom) => {
                let start = self.new_state()?;
                let accept = self.new_state()?;
                self.states[start].transitions.push((atom.clone(), accept));
                Some((start, accept))
            }
            PatternRegex::Sequence(parts) => {
                let mut parts = parts.iter();
                let (start, mut accept) = self.compile(parts.next()?, depth)?;
                for part in parts {
                    let (next_start, next_accept) = self.compile(part, depth)?;
                    self.states[accept].epsilon.push(next_start);
                    accept = next_accept;
                }
                Some((start, accept))
            }
            PatternRegex::Choice(alternatives) => {
                let start = self.new_state()?;
                let accept = self.new_state()?;
                for alternative in alternatives {
                    let (branch_start, branch_accept) = self.compile(alternative, depth + 1)?;
                    self.states[start].epsilon.push(branch_start);
                    self.states[branch_accept].epsilon.push(accept);
                }
                Some((start, accept))
            }
            PatternRegex::Optional(body) => {
                let start = self.new_state()?;
                let accept = self.new_state()?;
                let (body_start, body_accept) = self.compile(body, depth + 1)?;
                self.states[start].epsilon.extend([accept, body_start]);
                self.states[body_accept].epsilon.push(accept);
                Some((start, accept))
            }
            PatternRegex::Repeat {
                body,
                minimum,
                separator,
            } => {
                if let Some(separator) = separator {
                    let repeated = PatternRegex::Sequence(vec![
                        separator.as_ref().clone(),
                        body.as_ref().clone(),
                    ]);
                    let list = PatternRegex::Sequence(vec![
                        if *minimum == 0 {
                            PatternRegex::Optional(body.clone())
                        } else {
                            body.as_ref().clone()
                        },
                        PatternRegex::Repeat {
                            body: Box::new(repeated),
                            minimum: 0,
                            separator: None,
                        },
                        PatternRegex::Optional(separator.clone()),
                    ]);
                    return self.compile(&list, depth + 1);
                }
                let start = self.new_state()?;
                let accept = self.new_state()?;
                let (body_start, body_accept) = self.compile(body, depth + 1)?;
                if *minimum == 0 {
                    self.states[start].epsilon.push(accept);
                }
                self.states[start].epsilon.push(body_start);
                self.states[body_accept]
                    .epsilon
                    .extend([body_start, accept]);
                Some((start, accept))
            }
        }
    }
}

pub(super) fn pattern_token_automaton(
    pattern: &str,
    construct: &ConstructSpec,
) -> Option<PatternTokenAutomaton> {
    let (regex, has_repetition) = parse_pattern_regex(pattern, construct)?;
    let mut builder = PatternAutomatonBuilder { states: Vec::new() };
    let (start, accept) = builder.compile(&regex, 0)?;
    Some(PatternTokenAutomaton {
        states: builder.states,
        start,
        accept,
        has_repetition,
    })
}

pub(super) fn pattern_automata_overlap(
    left_construct: &ConstructSpec,
    left: &PatternTokenAutomaton,
    right_construct: &ConstructSpec,
    right: &PatternTokenAutomaton,
    work_limit: usize,
) -> (Option<bool>, usize) {
    let mut pending = vec![(left.start, right.start)];
    let mut visited = BTreeSet::new();
    let mut cursor = 0;
    let mut work = 0;
    while let Some(&(left_state, right_state)) = pending.get(cursor) {
        cursor += 1;
        if !visited.insert((left_state, right_state)) {
            continue;
        }
        work += 1;
        if work > work_limit {
            return (None, work);
        }
        if left_state == left.accept && right_state == right.accept {
            return (Some(true), work);
        }
        let left_node = &left.states[left_state];
        let right_node = &right.states[right_state];
        for next in &left_node.epsilon {
            work += 1;
            if work > work_limit {
                return (None, work);
            }
            pending.push((*next, right_state));
        }
        for next in &right_node.epsilon {
            work += 1;
            if work > work_limit {
                return (None, work);
            }
            pending.push((left_state, *next));
        }
        for (left_atom, left_next) in &left_node.transitions {
            for (right_atom, right_next) in &right_node.transitions {
                work += 1;
                if work > work_limit {
                    return (None, work);
                }
                if pattern_atoms_can_overlap(
                    left_construct,
                    std::slice::from_ref(left_atom),
                    right_construct,
                    std::slice::from_ref(right_atom),
                ) {
                    pending.push((*left_next, *right_next));
                }
            }
        }
    }
    (Some(false), work)
}

pub(super) fn pattern_realizations(pattern: &str) -> Option<Vec<String>> {
    // Keep validation bounded; callers fall back to comparing the original template.
    // Repetitions use representative lengths and longer overlaps remain runtime-checked.
    const MAX_REALIZATIONS: usize = 64;
    const MAX_REPEAT_INSTANCES: usize = 3;

    fn expand(fragment: &str) -> Option<Vec<String>> {
        let mut partial = vec![String::new()];
        let mut offset = 0;
        while offset < fragment.len() {
            let character = fragment[offset..].chars().next()?;
            if character == '\\' {
                let escaped_offset = offset + character.len_utf8();
                let escaped = fragment[escaped_offset..].chars().next()?;
                for prefix in &mut partial {
                    prefix.push(character);
                    prefix.push(escaped);
                }
                offset = escaped_offset + escaped.len_utf8();
                continue;
            }
            if character == '[' {
                let end = optional_group_end(fragment, offset)?;
                let included = expand(&fragment[offset + 1..end])?;
                let mut next = partial.clone();
                for prefix in &partial {
                    for suffix in &included {
                        let mut realization = prefix.clone();
                        realization.push_str(suffix);
                        next.push(realization);
                        if next.len() > MAX_REALIZATIONS {
                            return None;
                        }
                    }
                }
                partial = next;
                offset = end + 1;
                continue;
            }
            if character == '(' {
                if let Some((end, quantifier)) = repetition_group_end(fragment, offset) {
                    let included = expand(&fragment[offset + 1..end])?;
                    let minimum = usize::from(quantifier == '+');
                    let mut repeated = vec![String::new()];
                    let mut next = Vec::new();
                    for count in 0..=MAX_REPEAT_INSTANCES {
                        if count >= minimum {
                            for prefix in &partial {
                                for suffix in &repeated {
                                    let mut realization = prefix.clone();
                                    realization.push_str(suffix);
                                    next.push(realization);
                                    if next.len() > MAX_REALIZATIONS {
                                        return None;
                                    }
                                }
                            }
                        }
                        if count < MAX_REPEAT_INSTANCES {
                            let mut more = Vec::new();
                            for prefix in &repeated {
                                for suffix in &included {
                                    let mut realization = prefix.clone();
                                    realization.push_str(suffix);
                                    more.push(realization);
                                    if more.len() > MAX_REALIZATIONS {
                                        return None;
                                    }
                                }
                            }
                            repeated = more;
                        }
                    }
                    partial = next;
                    offset = end + 1 + quantifier.len_utf8();
                    continue;
                }
                if let Some(end) = choice_group_end(fragment, offset) {
                    let alternatives = split_choice_alternatives(&fragment[offset + 1..end]);
                    let mut next = Vec::new();
                    for alternative in alternatives {
                        for suffix in expand(alternative.trim())? {
                            for prefix in &partial {
                                let mut realization = prefix.clone();
                                realization.push_str(&suffix);
                                next.push(realization);
                                if next.len() > MAX_REALIZATIONS {
                                    return None;
                                }
                            }
                        }
                    }
                    partial = next;
                    offset = end + 1;
                    continue;
                }
            }
            for prefix in &mut partial {
                prefix.push(character);
            }
            offset += character.len_utf8();
        }
        Some(partial)
    }

    expand(pattern).map(|patterns| {
        patterns
            .into_iter()
            .map(|pattern| normalize_pattern_whitespace(&pattern))
            .collect()
    })
}

pub(super) fn normalize_pattern_whitespace(pattern: &str) -> String {
    pattern.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(super) fn normalize_pattern_captures(pattern: &str) -> String {
    let mut normalized = String::new();
    let mut offset = 0;
    while offset < pattern.len() {
        let character = pattern[offset..]
            .chars()
            .next()
            .expect("offset is a character boundary");
        if character == '\\' {
            let escaped_offset = offset + character.len_utf8();
            let Some(escaped) = pattern[escaped_offset..].chars().next() else {
                normalized.push(character);
                break;
            };
            normalized.push(character);
            normalized.push(escaped);
            offset = escaped_offset + escaped.len_utf8();
            continue;
        }
        if character == '{'
            && let Some((end, _)) = pattern[offset + 1..]
                .char_indices()
                .find(|(_, candidate)| *candidate == '}')
        {
            normalized.push_str("{}");
            offset += 1 + end + 1;
            continue;
        }
        normalized.push(character);
        offset += character.len_utf8();
    }
    normalize_pattern_whitespace(&normalized)
}

pub(super) fn pattern_captures_can_overlap(
    left_construct: &ConstructSpec,
    left: &[(String, bool, usize)],
    right_construct: &ConstructSpec,
    right: &[(String, bool, usize)],
) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            let Some(left_kind) = pattern_capture_kind(left_construct, left) else {
                return false;
            };
            let Some(right_kind) = pattern_capture_kind(right_construct, right) else {
                return false;
            };
            capture_kinds_can_overlap(left_kind, right_kind)
        })
}

pub(super) fn pattern_capture_kind<'a>(
    construct: &'a ConstructSpec,
    capture: &(String, bool, usize),
) -> Option<&'a FieldKind> {
    let (root, property) = capture
        .0
        .split_once('.')
        .map_or((capture.0.as_str(), None), |(root, property)| {
            (root, Some(property))
        });
    let field = construct.fields.iter().find(|field| field.name == root)?;
    match (property, capture.2, &field.kind) {
        (Some(property), 1, FieldKind::List { item }) => {
            let FieldKind::Properties { fields, .. } = item.as_ref() else {
                return None;
            };
            fields
                .iter()
                .find(|field| field.name == property)
                .map(|field| &field.kind)
        }
        (Some(_), _, _) => None,
        (None, repeat_depth, kind) => {
            let mut captured = kind;
            for _ in 0..repeat_depth {
                let FieldKind::List { item } = captured else {
                    return None;
                };
                captured = item;
            }
            Some(captured)
        }
    }
}

fn capture_kinds_can_overlap(left: &FieldKind, right: &FieldKind) -> bool {
    match (enum_values(left), enum_values(right)) {
        (Some(left_values), Some(right_values)) => {
            return left_values.iter().any(|value| right_values.contains(value));
        }
        (Some(values), None)
            if matches!(right, FieldKind::Identifier | FieldKind::Reference { .. }) =>
        {
            return !values.is_empty();
        }
        (None, Some(values))
            if matches!(left, FieldKind::Identifier | FieldKind::Reference { .. }) =>
        {
            return !values.is_empty();
        }
        _ => {}
    }
    matches!(
        (left, right),
        (
            FieldKind::Identifier | FieldKind::Reference { .. },
            FieldKind::Identifier
        ) | (FieldKind::Identifier, FieldKind::Reference { .. })
            | (FieldKind::Reference { .. }, FieldKind::Reference { .. })
            | (FieldKind::Number, FieldKind::Number)
            | (FieldKind::Text, FieldKind::Text)
    )
}

fn enum_values(kind: &FieldKind) -> Option<&[String]> {
    match kind {
        FieldKind::Enum { values } | FieldKind::NamedEnum { values, .. } => Some(values),
        _ => None,
    }
}

pub(super) fn pattern_atoms(pattern: &str) -> Option<Vec<PatternAtom>> {
    let mut characters = pattern.char_indices().peekable();
    let mut atoms = Vec::new();
    while let Some((offset, character)) = characters.next() {
        if character.is_whitespace() {
            continue;
        }
        if character == '\\' {
            let (_, escaped) = characters.next()?;
            if !matches!(
                escaped,
                '[' | ']' | '(' | ')' | '|' | '{' | '}' | '\\' | '@'
            ) {
                return None;
            }
            atoms.push(PatternAtom::Literal(PatternLiteral::Punctuation(escaped)));
            continue;
        }
        if character == '[' || character == ']' {
            return None;
        }
        if character == '(' {
            if repetition_group_end(pattern, offset).is_some()
                || choice_group_end(pattern, offset).is_some()
            {
                return None;
            }
            atoms.push(PatternAtom::Literal(PatternLiteral::Punctuation(character)));
            continue;
        }
        if character == '{' {
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
                return None;
            }
            atoms.push(PatternAtom::Capture(name, 0));
            continue;
        }
        if character == '}' || character == '"' || character == '#' {
            return None;
        }
        if character == '-' && characters.peek().is_some_and(|(_, next)| *next == '>') {
            characters.next();
            atoms.push(PatternAtom::Literal(PatternLiteral::Arrow));
            continue;
        }
        if character.is_ascii_digit()
            || (character == '-'
                && characters
                    .peek()
                    .is_some_and(|(_, next)| next.is_ascii_digit()))
        {
            let mut number = String::from(character);
            let mut decimal = false;
            while let Some((_, next)) = characters.peek().copied() {
                if next.is_ascii_digit() {
                    number.push(next);
                    characters.next();
                } else if next == '.' && !decimal {
                    number.push(next);
                    characters.next();
                    decimal = true;
                } else {
                    break;
                }
            }
            atoms.push(PatternAtom::Literal(PatternLiteral::Number(number)));
            continue;
        }
        if character == '_' || character.is_alphabetic() {
            let mut identifier = String::from(character);
            while let Some((_, next)) = characters.peek().copied() {
                if next == '_' || next.is_alphanumeric() {
                    identifier.push(next);
                    characters.next();
                } else if next == '-' {
                    let mut lookahead = characters.clone();
                    lookahead.next();
                    if lookahead.peek().is_some_and(|(_, after)| *after == '>') {
                        break;
                    }
                    identifier.push(next);
                    characters.next();
                } else {
                    break;
                }
            }
            atoms.push(PatternAtom::Literal(PatternLiteral::Identifier(identifier)));
            continue;
        }
        if character.is_ascii_punctuation() {
            atoms.push(PatternAtom::Literal(PatternLiteral::Punctuation(character)));
            continue;
        }
        return None;
    }
    (!atoms.is_empty()).then_some(atoms)
}

pub(super) fn pattern_atoms_can_overlap(
    left_construct: &ConstructSpec,
    left: &[PatternAtom],
    right_construct: &ConstructSpec,
    right: &[PatternAtom],
) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| match (left, right) {
                (PatternAtom::Literal(left), PatternAtom::Literal(right)) => left == right,
                (
                    PatternAtom::Capture(left, left_repeated),
                    PatternAtom::Capture(right, right_repeated),
                ) => {
                    let left_capture = (left.clone(), false, *left_repeated);
                    let right_capture = (right.clone(), false, *right_repeated);
                    let Some(left_kind) = pattern_capture_kind(left_construct, &left_capture)
                    else {
                        return false;
                    };
                    let Some(right_kind) = pattern_capture_kind(right_construct, &right_capture)
                    else {
                        return false;
                    };
                    capture_kinds_can_overlap(left_kind, right_kind)
                }
                (PatternAtom::Literal(literal), PatternAtom::Capture(capture, repeated)) => {
                    let capture = (capture.clone(), false, *repeated);
                    pattern_capture_kind(right_construct, &capture)
                        .is_some_and(|kind| literal_matches_capture(literal, kind))
                }
                (PatternAtom::Capture(capture, repeated), PatternAtom::Literal(literal)) => {
                    let capture = (capture.clone(), false, *repeated);
                    pattern_capture_kind(left_construct, &capture)
                        .is_some_and(|kind| literal_matches_capture(literal, kind))
                }
            })
}

fn literal_matches_capture(literal: &PatternLiteral, kind: &FieldKind) -> bool {
    match (literal, kind) {
        (PatternLiteral::Identifier(_), FieldKind::Identifier | FieldKind::Reference { .. })
        | (PatternLiteral::Number(_), FieldKind::Number) => true,
        (
            PatternLiteral::Identifier(value),
            FieldKind::Enum { values } | FieldKind::NamedEnum { values, .. },
        ) => values.contains(value),
        _ => false,
    }
}

fn repetition_group_end(pattern: &str, opening: usize) -> Option<(usize, char)> {
    let mut depth = 0;
    let mut escaped = false;
    for (offset, character) in pattern[opening..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' {
            escaped = true;
            continue;
        }
        match character {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    let end = opening + offset;
                    let (quantifier_offset, quantifier) =
                        pattern[end + 1..].char_indices().next()?;
                    if matches!(quantifier, '*' | '+') {
                        return Some((end, quantifier));
                    }
                    let _ = quantifier_offset;
                    return None;
                }
            }
            _ => {}
        }
    }
    None
}

fn choice_group_end(pattern: &str, opening: usize) -> Option<usize> {
    let mut depth = 0;
    let mut has_choice = false;
    let mut escaped = false;
    for (offset, character) in pattern[opening..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' {
            escaped = true;
            continue;
        }
        match character {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
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

fn split_choice_alternatives(pattern: &str) -> Vec<&str> {
    let mut alternatives = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    let mut escaped = false;
    for (offset, character) in pattern.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' {
            escaped = true;
            continue;
        }
        match character {
            '(' => depth += 1,
            ')' => depth -= 1,
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

fn optional_group_end(pattern: &str, opening: usize) -> Option<usize> {
    let mut depth = 0;
    let mut escaped = false;
    for (offset, character) in pattern[opening + 1..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' {
            escaped = true;
            continue;
        }
        match character {
            '[' => depth += 1,
            ']' if depth == 0 => return Some(opening + 1 + offset),
            ']' => depth -= 1,
            _ => {}
        }
    }
    None
}
