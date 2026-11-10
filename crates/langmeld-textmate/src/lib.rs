//! Lexical TextMate grammar generation from Langmeld declarative languages.
#![deny(missing_docs)]

use std::collections::HashSet;

use langmeld_language::{LanguageSpec, PatternPart};
use serde::Serialize;

/// Metadata required to identify an exported TextMate grammar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextMateOptions {
    scope_name: String,
    file_types: Vec<String>,
}

impl TextMateOptions {
    /// Creates export metadata with a required TextMate scope name.
    ///
    /// Use a unique name such as `source.widget`; file types are optional.
    #[must_use]
    pub fn new(scope_name: impl Into<String>) -> Self {
        Self {
            scope_name: scope_name.into(),
            file_types: Vec::new(),
        }
    }

    /// Adds file extensions associated with the grammar, without leading dots.
    #[must_use]
    pub fn file_types(mut self, file_types: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.file_types = file_types.into_iter().map(Into::into).collect();
        self
    }

    /// Returns the TextMate grammar scope name.
    #[must_use]
    pub fn scope_name(&self) -> &str {
        &self.scope_name
    }

    /// Returns associated file extensions.
    #[must_use]
    pub fn file_types_list(&self) -> &[String] {
        &self.file_types
    }
}

/// Exports a lexical TextMate grammar as pretty-printed JSON.
///
/// The generated grammar highlights literal words as keywords, literal
/// punctuation, decimal integers, and double-quoted strings. It does not
/// reproduce Langmeld parsing, infer comments or semantic roles, or account
/// for host-defined lexer rules. Hosts can extend the emitted grammar for
/// language-specific lexical behavior.
pub fn generate(
    language: &LanguageSpec,
    options: &TextMateOptions,
) -> Result<String, TextMateError> {
    validate_options(options)?;
    let suffix = options
        .scope_name
        .rsplit('.')
        .next()
        .expect("validated scope name has a segment");
    let mut literals = Vec::new();
    let mut seen = HashSet::new();
    for construct in language.constructs() {
        for variant in construct.variants() {
            collect_literals(variant, &mut literals, &mut seen);
        }
        if let Some(block) = construct.block_spec() {
            for delimiter in [block.open(), block.close()] {
                if seen.insert(delimiter.to_owned()) {
                    literals.push(delimiter.to_owned());
                }
            }
        }
    }

    let (keywords, punctuation): (Vec<_>, Vec<_>) = literals
        .into_iter()
        .partition(|literal| is_identifier_literal(literal));
    let mut patterns = vec![string_pattern(suffix).into()];
    if !keywords.is_empty() {
        patterns.push(
            MatchPattern {
                name: format!("keyword.other.{suffix}"),
                regex: format!(
                    "\\b(?:{})\\b",
                    keywords
                        .iter()
                        .map(|word| regex_escape(word))
                        .collect::<Vec<_>>()
                        .join("|")
                ),
            }
            .into(),
        );
    }
    patterns.push(
        MatchPattern {
            name: format!("constant.numeric.{suffix}"),
            regex: r"\b[0-9]+\b".to_owned(),
        }
        .into(),
    );
    if !punctuation.is_empty() {
        patterns.push(
            MatchPattern {
                name: format!("punctuation.{suffix}"),
                regex: format!(
                    "(?:{})",
                    punctuation
                        .iter()
                        .map(|token| regex_escape(token))
                        .collect::<Vec<_>>()
                        .join("|")
                ),
            }
            .into(),
        );
    }

    let grammar = TextMateGrammar {
        name: language.name().to_owned(),
        scope_name: options.scope_name.clone(),
        file_types: options.file_types.clone(),
        patterns,
    };
    serde_json::to_string_pretty(&grammar).map_err(TextMateError::Serialize)
}

fn validate_options(options: &TextMateOptions) -> Result<(), TextMateError> {
    let segments: Vec<_> = options.scope_name.split('.').collect();
    if segments.len() < 2
        || !matches!(segments[0], "source" | "text")
        || segments.iter().any(|segment| {
            segment.is_empty()
                || !segment.chars().all(|character| {
                    character.is_ascii_alphanumeric() || character == '-' || character == '_'
                })
        })
    {
        return Err(TextMateError::InvalidScopeName(options.scope_name.clone()));
    }
    if let Some(file_type) = options.file_types.iter().find(|file_type| {
        file_type.is_empty()
            || file_type.starts_with('.')
            || file_type.contains('/')
            || file_type.contains('\\')
            || file_type.chars().any(char::is_whitespace)
    }) {
        return Err(TextMateError::InvalidFileType(file_type.clone()));
    }
    Ok(())
}

fn collect_literals(parts: &[PatternPart], output: &mut Vec<String>, seen: &mut HashSet<String>) {
    for part in parts {
        match part {
            PatternPart::Literal(literal) => {
                if seen.insert(literal.clone()) {
                    output.push(literal.clone());
                }
            }
            PatternPart::Field { .. } | PatternPart::Expression { .. } => {}
            PatternPart::Optional(parts) | PatternPart::Repeat { parts, .. } => {
                collect_literals(parts, output, seen);
            }
        }
    }
}

fn is_identifier_literal(literal: &str) -> bool {
    let mut characters = literal.chars();
    characters
        .next()
        .is_some_and(|character| character == '_' || character.is_alphabetic())
        && characters.all(|character| character == '_' || character.is_alphanumeric())
}

fn regex_escape(literal: &str) -> String {
    let mut escaped = String::with_capacity(literal.len());
    for character in literal.chars() {
        if r"\.^$|?*+()[]{}".contains(character) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

fn string_pattern(suffix: &str) -> StringPattern {
    StringPattern {
        name: format!("string.quoted.double.{suffix}"),
        begin: "\"".to_owned(),
        end: "\"".to_owned(),
        patterns: vec![MatchPattern {
            name: format!("constant.character.escape.{suffix}"),
            regex: r"\\.".to_owned(),
        }],
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TextMateGrammar {
    name: String,
    scope_name: String,
    file_types: Vec<String>,
    patterns: Vec<GrammarPattern>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum GrammarPattern {
    String(StringPattern),
    Match(MatchPattern),
}

#[derive(Serialize)]
struct StringPattern {
    name: String,
    begin: String,
    end: String,
    patterns: Vec<MatchPattern>,
}

#[derive(Serialize)]
struct MatchPattern {
    name: String,
    #[serde(rename = "match")]
    regex: String,
}

/// Failure to construct a valid TextMate grammar or serialize its JSON.
#[derive(Debug)]
pub enum TextMateError {
    /// A scope name must start with `source.` or `text.` and contain valid dot-separated segments.
    InvalidScopeName(String),
    /// A file type must be a non-empty extension without a leading dot or path separators.
    InvalidFileType(String),
    /// JSON serialization failed.
    Serialize(serde_json::Error),
}

impl std::fmt::Display for TextMateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidScopeName(scope) => {
                write!(formatter, "invalid TextMate scope name `{scope}`")
            }
            Self::InvalidFileType(file_type) => {
                write!(formatter, "invalid TextMate file type `{file_type}`")
            }
            Self::Serialize(error) => {
                write!(formatter, "cannot serialize TextMate grammar: {error}")
            }
        }
    }
}

impl std::error::Error for TextMateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Serialize(error) => Some(error),
            Self::InvalidScopeName(_) | Self::InvalidFileType(_) => None,
        }
    }
}

impl From<StringPattern> for GrammarPattern {
    fn from(pattern: StringPattern) -> Self {
        Self::String(pattern)
    }
}

impl From<MatchPattern> for GrammarPattern {
    fn from(pattern: MatchPattern) -> Self {
        Self::Match(pattern)
    }
}
