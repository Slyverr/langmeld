use std::collections::HashSet;

use langmeld_language::LanguageSpec;

use crate::{
    error::TextMateError,
    model::{MatchPattern, StringPattern, TextMateGrammar},
    patterns::{
        collect_literals, is_identifier_literal, punctuation_regex, regex_escape, string_pattern,
    },
};

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
/// reproduce Langmeld parsing, infer semantic roles, or account for host-defined
/// lexer rules. Configured line and block comments are emitted as TextMate
/// comment patterns.
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
    let mut comments = language
        .lexical_rules()
        .line_comments()
        .iter()
        .map(|opening| (opening.as_str(), None))
        .chain(
            language
                .lexical_rules()
                .block_comments()
                .iter()
                .map(|(opening, closing)| (opening.as_str(), Some(closing.as_str()))),
        )
        .collect::<Vec<_>>();
    comments.sort_by_key(|(opening, _)| std::cmp::Reverse(opening.len()));

    for (opening, closing) in comments {
        let pattern = match closing {
            Some(closing) => StringPattern {
                name: format!("comment.block.{suffix}"),
                begin: regex_escape(opening),
                end: regex_escape(closing),
                patterns: Vec::new(),
            }
            .into(),
            None => MatchPattern {
                name: Some(format!("comment.line.{suffix}")),
                regex: format!("{}[^\\r\\n]*", regex_escape(opening)),
            }
            .into(),
        };
        patterns.push(pattern);
    }
    if !keywords.is_empty() {
        patterns.push(
            MatchPattern {
                name: Some(format!("keyword.other.{suffix}")),
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

    // Consume identifiers as whole tokens so numeric rules cannot highlight their suffixes.
    patterns.push(
        MatchPattern {
            name: None,
            regex: r"(?:\p{Alphabetic}|_)(?:\p{Alphabetic}|\p{Number}|_)*".to_owned(),
        }
        .into(),
    );
    patterns.push(
        MatchPattern {
            name: Some(format!("constant.numeric.{suffix}")),
            regex: r"[0-9]+".to_owned(),
        }
        .into(),
    );
    if !punctuation.is_empty() {
        patterns.push(
            MatchPattern {
                name: Some(format!("punctuation.{suffix}")),
                regex: punctuation_regex(punctuation),
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
