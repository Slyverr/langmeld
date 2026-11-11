use crate::completion_kind::CompletionKind;

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
