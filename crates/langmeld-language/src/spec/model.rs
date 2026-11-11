use std::collections::HashSet;

use crate::completion::CompletionKind;

use super::SpecError;

/// Comment delimiters used by Langmeld's built-in lexer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LexicalRules {
    line_comments: Vec<String>,
    block_comments: Vec<(String, String)>,
}

impl LexicalRules {
    /// Adds a line-comment marker, such as `//` or `#`.
    #[must_use]
    pub fn line_comment(mut self, marker: impl Into<String>) -> Self {
        self.line_comments.push(marker.into());
        self
    }

    /// Adds a non-nested block-comment pair, such as `/*` and `*/`.
    #[must_use]
    pub fn block_comment(mut self, opening: impl Into<String>, closing: impl Into<String>) -> Self {
        self.block_comments.push((opening.into(), closing.into()));
        self
    }

    /// Returns the configured line-comment markers.
    #[must_use]
    pub fn line_comments(&self) -> &[String] {
        &self.line_comments
    }

    /// Returns configured block-comment opening and closing markers.
    #[must_use]
    pub fn block_comments(&self) -> &[(String, String)] {
        &self.block_comments
    }

    pub(super) fn validate(&self) -> Result<(), SpecError> {
        let mut openings = HashSet::new();
        for marker in &self.line_comments {
            if invalid_comment_marker(marker) {
                return Err(SpecError::InvalidLineComment(marker.clone()));
            }
            if !openings.insert(marker.as_str()) {
                return Err(SpecError::DuplicateCommentStart(marker.clone()));
            }
        }
        for (opening, closing) in &self.block_comments {
            if invalid_comment_marker(opening)
                || closing.trim().is_empty()
                || has_line_break(closing)
                || opening == closing
            {
                return Err(SpecError::InvalidBlockComment {
                    opening: opening.clone(),
                    closing: closing.clone(),
                });
            }
            if !openings.insert(opening.as_str()) {
                return Err(SpecError::DuplicateCommentStart(opening.clone()));
            }
        }
        Ok(())
    }
}

fn invalid_comment_marker(marker: &str) -> bool {
    marker.trim().is_empty()
        || marker.starts_with('"')
        || marker.chars().next().is_some_and(char::is_whitespace)
        || has_line_break(marker)
}

fn has_line_break(value: &str) -> bool {
    value
        .chars()
        .any(|character| matches!(character, '\n' | '\r'))
}

/// The token category accepted by a captured syntax field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    /// Any valid token, including punctuation and host-defined lexical forms.
    Token,
    /// A language identifier token.
    Identifier,
    /// A decimal integer token, preserved as written.
    Integer,
    /// A quoted string token, including its quotes.
    String,
}

/// One part of a construct's token pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatternPart {
    /// An exact token, such as a keyword, operator, or delimiter.
    Literal(String),
    /// A named value captured from a token of the selected category.
    Field {
        /// Name exposed in parsed construct fields.
        name: String,
        /// Required lexical category.
        kind: FieldKind,
    },
    /// Captures one exact token from a finite set of allowed values.
    /// Completion suggestions use the enum-member category by default.
    Choice {
        /// Name exposed in parsed construct fields.
        name: String,
        /// Exact tokens accepted by this field.
        values: Vec<String>,
        /// Editor category for suggestions from this field.
        completion_kind: CompletionKind,
    },
    /// Captures the source span before a top-level terminator token.
    ///
    /// The terminator is consumed by the parser but excluded from the captured
    /// span. Nested `()`, `[]`, and `{}` groups do not end the capture. The
    /// host can parse the captured span with its selected expression parser.
    Expression {
        /// Name exposed in parsed construct fields.
        name: String,
        /// Single token that ends the expression at delimiter depth zero.
        terminator: String,
    },
    /// A sequence that is included when it matches, and otherwise omitted.
    Optional(Vec<PatternPart>),
    /// A sequence matched repeatedly as one unit. Captured fields are appended
    /// in source order for every match, so a field inside this group can occur
    /// multiple times in a construct's capture list.
    Repeat {
        /// Sequence consumed by each repetition.
        parts: Vec<PatternPart>,
        /// Whether zero repetitions are accepted.
        minimum: RepeatMinimum,
    },
}

/// Minimum number of repetitions for a repeated pattern sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepeatMinimum {
    /// Accept zero or more repetitions.
    Zero,
    /// Require at least one repetition.
    One,
}

impl PatternPart {
    /// Creates an exact token pattern.
    #[must_use]
    pub fn literal(value: impl Into<String>) -> Self {
        Self::Literal(value.into())
    }

    /// Captures any valid token under a field name.
    #[must_use]
    pub fn token(name: impl Into<String>) -> Self {
        Self::Field {
            name: name.into(),
            kind: FieldKind::Token,
        }
    }

    /// Captures an identifier token under a field name.
    #[must_use]
    pub fn identifier(name: impl Into<String>) -> Self {
        Self::Field {
            name: name.into(),
            kind: FieldKind::Identifier,
        }
    }

    /// Captures a decimal integer token under a field name.
    #[must_use]
    pub fn integer(name: impl Into<String>) -> Self {
        Self::Field {
            name: name.into(),
            kind: FieldKind::Integer,
        }
    }

    /// Captures a quoted string token under a field name.
    #[must_use]
    pub fn string(name: impl Into<String>) -> Self {
        Self::Field {
            name: name.into(),
            kind: FieldKind::String,
        }
    }

    /// Captures one exact token from a finite set of allowed values.
    #[must_use]
    pub fn choice(
        name: impl Into<String>,
        values: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self::Choice {
            name: name.into(),
            values: values.into_iter().map(Into::into).collect(),
            completion_kind: CompletionKind::EnumMember,
        }
    }

    /// Captures one exact token from a finite set and sets its editor category.
    #[must_use]
    pub fn choice_with_completion_kind(
        name: impl Into<String>,
        values: impl IntoIterator<Item = impl Into<String>>,
        kind: CompletionKind,
    ) -> Self {
        Self::Choice {
            name: name.into(),
            values: values.into_iter().map(Into::into).collect(),
            completion_kind: kind,
        }
    }

    /// Captures tokens before a top-level terminator as an expression span.
    ///
    /// The terminator is consumed but excluded from the captured source range.
    /// The generic parser does not parse or type-check the expression.
    #[must_use]
    pub fn expression(name: impl Into<String>, terminator: impl Into<String>) -> Self {
        Self::Expression {
            name: name.into(),
            terminator: terminator.into(),
        }
    }

    /// Creates an optional sequence of syntax parts.
    #[must_use]
    pub fn optional(parts: impl IntoIterator<Item = PatternPart>) -> Self {
        Self::Optional(parts.into_iter().collect())
    }

    /// Creates a sequence that may repeat zero or more times.
    #[must_use]
    pub fn zero_or_more(parts: impl IntoIterator<Item = PatternPart>) -> Self {
        Self::Repeat {
            parts: parts.into_iter().collect(),
            minimum: RepeatMinimum::Zero,
        }
    }

    /// Creates a sequence that must repeat at least once.
    #[must_use]
    pub fn one_or_more(parts: impl IntoIterator<Item = PatternPart>) -> Self {
        Self::Repeat {
            parts: parts.into_iter().collect(),
            minimum: RepeatMinimum::One,
        }
    }
}

/// A named syntax rule expressed as a sequence of token patterns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstructSpec {
    /// Stable construct name returned in parsed nodes.
    pub(crate) name: String,
    /// Ordered syntax alternatives that make up this construct.
    pub(crate) patterns: Vec<Vec<PatternPart>>,
    /// Named structural groups this construct belongs to.
    pub(crate) categories: Vec<String>,
    /// Optional semantic kind for construct-start completion candidates.
    pub(crate) completion_kind: Option<CompletionKind>,
    /// Optional delimited child-construct body.
    pub(crate) block: Option<BlockSpec>,
}

impl ConstructSpec {
    /// Creates a construct from its name and ordered pattern.
    #[must_use]
    pub fn new(name: impl Into<String>, pattern: impl IntoIterator<Item = PatternPart>) -> Self {
        Self {
            name: name.into(),
            patterns: vec![pattern.into_iter().collect()],
            categories: Vec::new(),
            completion_kind: None,
            block: None,
        }
    }

    /// Marks this construct as a member of the named structural groups.
    ///
    /// Categories only group constructs for block allowlists. They do not
    /// assign semantics, scopes, or runtime behavior.
    #[must_use]
    pub fn implements(mut self, categories: &[&str]) -> Self {
        self.categories
            .extend(categories.iter().map(|category| (*category).to_owned()));
        self
    }

    /// Sets the semantic kind used for completion candidates that start this construct.
    #[must_use]
    pub fn with_completion_kind(mut self, kind: CompletionKind) -> Self {
        self.completion_kind = Some(kind);
        self
    }

    /// Adds another syntax alternative for this construct.
    #[must_use]
    pub fn variant(mut self, pattern: impl IntoIterator<Item = PatternPart>) -> Self {
        self.patterns.push(pattern.into_iter().collect());
        self
    }

    /// Adds a delimited block containing the named child constructs.
    #[must_use]
    pub fn block(
        mut self,
        open: impl Into<String>,
        close: impl Into<String>,
        constructs: &[&str],
    ) -> Self {
        self.block = Some(BlockSpec {
            open: open.into(),
            close: close.into(),
            constructs: constructs.iter().map(|name| (*name).to_owned()).collect(),
            categories: Vec::new(),
        });
        self
    }

    /// Adds a delimited block that accepts named constructs and category members.
    #[must_use]
    pub fn block_with_categories(
        mut self,
        open: impl Into<String>,
        close: impl Into<String>,
        constructs: &[&str],
        categories: &[&str],
    ) -> Self {
        self.block = Some(BlockSpec {
            open: open.into(),
            close: close.into(),
            constructs: constructs.iter().map(|name| (*name).to_owned()).collect(),
            categories: categories
                .iter()
                .map(|category| (*category).to_owned())
                .collect(),
        });
        self
    }

    /// Adds a delimited block from dynamically collected child names and categories.
    #[must_use]
    pub fn block_names_with_categories(
        mut self,
        open: impl Into<String>,
        close: impl Into<String>,
        constructs: impl IntoIterator<Item = String>,
        categories: impl IntoIterator<Item = String>,
    ) -> Self {
        self.block = Some(BlockSpec {
            open: open.into(),
            close: close.into(),
            constructs: constructs.into_iter().collect(),
            categories: categories.into_iter().collect(),
        });
        self
    }

    /// Adds a delimited block from dynamically collected child names.
    #[must_use]
    pub fn block_names(
        mut self,
        open: impl Into<String>,
        close: impl Into<String>,
        constructs: impl IntoIterator<Item = String>,
    ) -> Self {
        self.block = Some(BlockSpec {
            open: open.into(),
            close: close.into(),
            constructs: constructs.into_iter().collect(),
            categories: Vec::new(),
        });
        self
    }

    /// Returns this construct's name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the structural categories this construct implements.
    #[must_use]
    pub fn categories(&self) -> &[String] {
        &self.categories
    }

    /// Returns the optional kind used for this construct's completion candidates.
    #[must_use]
    pub fn completion_kind(&self) -> Option<CompletionKind> {
        self.completion_kind
    }

    /// Returns all syntax alternatives in declaration order.
    #[must_use]
    pub fn variants(&self) -> &[Vec<PatternPart>] {
        &self.patterns
    }

    /// Returns the first syntax alternative, or an empty slice if it is invalid.
    #[must_use]
    pub fn pattern(&self) -> &[PatternPart] {
        self.patterns.first().map_or(&[], Vec::as_slice)
    }

    /// Returns the optional delimited child block.
    #[must_use]
    pub fn block_spec(&self) -> Option<&BlockSpec> {
        self.block.as_ref()
    }
}

/// A construct body delimited by tokens and limited to named child constructs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockSpec {
    /// Opening token expected after the construct pattern.
    pub(crate) open: String,
    /// Closing token that ends the body.
    pub(crate) close: String,
    /// Child constructs accepted in this body.
    pub(crate) constructs: Vec<String>,
    /// Categories expanded into child constructs when the language is built.
    pub(crate) categories: Vec<String>,
}

impl BlockSpec {
    /// Returns the opening token.
    #[must_use]
    pub fn open(&self) -> &str {
        &self.open
    }

    /// Returns the closing token.
    #[must_use]
    pub fn close(&self) -> &str {
        &self.close
    }

    /// Returns accepted child construct names.
    #[must_use]
    pub fn constructs(&self) -> &[String] {
        &self.constructs
    }

    /// Returns category names used to expand this block's accepted child set.
    #[must_use]
    pub fn categories(&self) -> &[String] {
        &self.categories
    }
}
