//! Index block children by pattern start token to avoid trying every construct.

use std::{collections::HashMap, sync::Arc};

use super::{pattern::PatternPart, prepared::CompiledConstruct};
use crate::LanguageTokenKind as TokenKind;
use langmeld_core::ConstructSpec;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum OtherStart {
    Arrow,
    Unknown(char),
    Comment,
    Newline,
    End,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
enum PatternStart {
    Word(String),
    Punctuation(char),
    Other(OtherStart),
}

#[derive(Clone, Debug)]
pub(super) struct BlockCandidateIndex {
    children: Arc<[String]>,
    words: HashMap<String, Vec<usize>>,
    punctuation: HashMap<char, Vec<usize>>,
    other: HashMap<OtherStart, Vec<usize>>,
    fallback: Vec<usize>,
}

impl BlockCandidateIndex {
    pub(super) fn new(
        children: Vec<String>,
        constructs: &[ConstructSpec],
        compiled: &HashMap<String, CompiledConstruct>,
    ) -> Self {
        let mut index = Self {
            children: Arc::from(children),
            words: HashMap::new(),
            punctuation: HashMap::new(),
            other: HashMap::new(),
            fallback: Vec::new(),
        };

        for (child_index, child_id) in index.children.iter().enumerate() {
            let Some(compiled_construct) = compiled.get(child_id) else {
                continue;
            };
            let Some(construct) = constructs.get(compiled_construct.construct_index) else {
                continue;
            };
            if construct.custom_parser.is_some() || compiled_construct.patterns.is_empty() {
                index.fallback.push(child_index);
                continue;
            }

            let mut starts = Vec::new();
            let mut can_filter = true;
            for pattern in &compiled_construct.patterns {
                let Some(start) = pattern.parts.first().and_then(pattern_start) else {
                    can_filter = false;
                    break;
                };
                if !starts.contains(&start) {
                    starts.push(start);
                }
            }
            if !can_filter {
                index.fallback.push(child_index);
                continue;
            }
            for start in starts {
                match start {
                    PatternStart::Word(word) => {
                        index.words.entry(word).or_default().push(child_index);
                    }
                    PatternStart::Punctuation(character) => {
                        index
                            .punctuation
                            .entry(character)
                            .or_default()
                            .push(child_index);
                    }
                    PatternStart::Other(token) => {
                        index.other.entry(token).or_default().push(child_index);
                    }
                }
            }
        }
        index
    }

    pub(super) fn candidates(&self, token: &TokenKind) -> CandidateIds<'_> {
        let exact = match token {
            TokenKind::Identifier(value) | TokenKind::Number(value) | TokenKind::Text(value) => {
                self.words
                    .get(value.as_str())
                    .map_or(&[] as &[usize], Vec::as_slice)
            }
            TokenKind::Punctuation(character) => self
                .punctuation
                .get(character)
                .map_or(&[] as &[usize], Vec::as_slice),
            TokenKind::Arrow => self
                .other
                .get(&OtherStart::Arrow)
                .map_or(&[] as &[usize], Vec::as_slice),
            TokenKind::Unknown(character) => self
                .other
                .get(&OtherStart::Unknown(*character))
                .map_or(&[] as &[usize], Vec::as_slice),
            TokenKind::Comment => self
                .other
                .get(&OtherStart::Comment)
                .map_or(&[] as &[usize], Vec::as_slice),
            TokenKind::Newline => self
                .other
                .get(&OtherStart::Newline)
                .map_or(&[] as &[usize], Vec::as_slice),
            TokenKind::End => self
                .other
                .get(&OtherStart::End)
                .map_or(&[] as &[usize], Vec::as_slice),
        };
        CandidateIds {
            children: &self.children,
            exact: exact.iter(),
            fallback: self.fallback.iter(),
        }
    }
}

pub(super) struct CandidateIds<'a> {
    children: &'a [String],
    exact: std::slice::Iter<'a, usize>,
    fallback: std::slice::Iter<'a, usize>,
}

impl<'a> Iterator for CandidateIds<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        let exact_index = self.exact.as_slice().first().copied();
        let fallback_index = self.fallback.as_slice().first().copied();
        let next = match (exact_index, fallback_index) {
            (Some(exact), Some(fallback)) if exact < fallback => self.exact.next().copied(),
            (Some(_), Some(_)) => self.fallback.next().copied(),
            (Some(_), None) => self.exact.next().copied(),
            (None, Some(_)) => self.fallback.next().copied(),
            (None, None) => return None,
        }?;
        self.children.get(next).map(String::as_str)
    }
}

fn pattern_start(part: &PatternPart) -> Option<PatternStart> {
    let PatternPart::Literal(token) = part else {
        return None;
    };
    match token {
        TokenKind::Identifier(value) | TokenKind::Number(value) | TokenKind::Text(value) => {
            Some(PatternStart::Word(value.clone()))
        }
        TokenKind::Punctuation(character) => Some(PatternStart::Punctuation(*character)),
        TokenKind::Arrow => Some(PatternStart::Other(OtherStart::Arrow)),
        TokenKind::Unknown(character) => Some(PatternStart::Other(OtherStart::Unknown(*character))),
        TokenKind::Comment => Some(PatternStart::Other(OtherStart::Comment)),
        TokenKind::Newline => Some(PatternStart::Other(OtherStart::Newline)),
        TokenKind::End => Some(PatternStart::Other(OtherStart::End)),
    }
}
