//! Minimal validated grammar model.

use std::collections::HashSet;

/// The token category accepted by a captured syntax field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
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
}

impl PatternPart {
    /// Creates an exact token pattern.
    #[must_use]
    pub fn literal(value: impl Into<String>) -> Self {
        Self::Literal(value.into())
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
}

/// A named syntax rule expressed as a sequence of token patterns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstructSpec {
    /// Stable construct name returned in parsed nodes.
    pub(crate) name: String,
    /// Ordered tokens and captures that make up this construct.
    pub(crate) pattern: Vec<PatternPart>,
    /// Optional delimited child-construct body.
    pub(crate) block: Option<BlockSpec>,
}

impl ConstructSpec {
    /// Creates a construct from its name and ordered pattern.
    #[must_use]
    pub fn new(name: impl Into<String>, pattern: impl IntoIterator<Item = PatternPart>) -> Self {
        Self {
            name: name.into(),
            pattern: pattern.into_iter().collect(),
            block: None,
        }
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
        });
        self
    }

    /// Returns this construct's name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the ordered syntax pattern.
    #[must_use]
    pub fn pattern(&self) -> &[PatternPart] {
        &self.pattern
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
}

/// A language's currently supported declarative grammar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguageSpec {
    /// Human-readable language name.
    pub(crate) name: String,
    /// Syntax constructs accepted by this language.
    pub(crate) constructs: Vec<ConstructSpec>,
}

impl LanguageSpec {
    /// Validates and creates a language grammar.
    pub fn new(
        name: impl Into<String>,
        constructs: impl IntoIterator<Item = ConstructSpec>,
    ) -> Result<Self, SpecError> {
        let name = name.into();
        let constructs: Vec<_> = constructs.into_iter().collect();
        if name.trim().is_empty() {
            return Err(SpecError::EmptyLanguageName);
        }
        if constructs.is_empty() {
            return Err(SpecError::NoConstructs);
        }
        let mut names = HashSet::new();
        for construct in &constructs {
            if construct.name.trim().is_empty() {
                return Err(SpecError::EmptyConstructName);
            }
            if !names.insert(construct.name.as_str()) {
                return Err(SpecError::DuplicateConstruct(construct.name.clone()));
            }
            if construct.pattern.is_empty() {
                return Err(SpecError::EmptyPattern(construct.name.clone()));
            }
            let mut fields = HashSet::new();
            for part in &construct.pattern {
                match part {
                    PatternPart::Literal(value) if value.is_empty() => {
                        return Err(SpecError::EmptyLiteral(construct.name.clone()));
                    }
                    PatternPart::Literal(value) if !is_single_token(value) => {
                        return Err(SpecError::InvalidLiteral {
                            construct: construct.name.clone(),
                            literal: value.clone(),
                        });
                    }
                    PatternPart::Field { name, .. } if name.trim().is_empty() => {
                        return Err(SpecError::EmptyFieldName(construct.name.clone()));
                    }
                    PatternPart::Field { name, .. } if !fields.insert(name.as_str()) => {
                        return Err(SpecError::DuplicateField {
                            construct: construct.name.clone(),
                            field: name.clone(),
                        });
                    }
                    _ => {}
                }
            }
        }
        for construct in &constructs {
            if let Some(block) = &construct.block {
                if block.open.trim().is_empty()
                    || block.close.trim().is_empty()
                    || block.open == block.close
                    || !is_single_token(&block.open)
                    || !is_single_token(&block.close)
                {
                    return Err(SpecError::InvalidBlockDelimiter(construct.name.clone()));
                }
                let mut children = HashSet::new();
                for child in &block.constructs {
                    if !names.contains(child.as_str()) {
                        return Err(SpecError::UnknownChildConstruct {
                            construct: construct.name.clone(),
                            child: child.clone(),
                        });
                    }
                    if !children.insert(child.as_str()) {
                        return Err(SpecError::DuplicateChildConstruct {
                            construct: construct.name.clone(),
                            child: child.clone(),
                        });
                    }
                }
            }
        }
        for (index, left) in constructs.iter().enumerate() {
            for right in &constructs[index + 1..] {
                if patterns_overlap(&left.pattern, &right.pattern) {
                    return Err(SpecError::AmbiguousConstructs(
                        left.name.clone(),
                        right.name.clone(),
                    ));
                }
            }
        }
        Ok(Self { name, constructs })
    }

    /// Returns the human-readable language name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the validated syntax constructs.
    #[must_use]
    pub fn constructs(&self) -> &[ConstructSpec] {
        &self.constructs
    }
}

/// An invalid declarative grammar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpecError {
    /// The language name is blank.
    EmptyLanguageName,
    /// At least one construct is required.
    NoConstructs,
    /// A construct name is blank.
    EmptyConstructName,
    /// Construct names must be unique.
    DuplicateConstruct(String),
    /// A construct pattern cannot be empty.
    EmptyPattern(String),
    /// A literal pattern must contain one token.
    EmptyLiteral(String),
    /// A literal contains whitespace or more than one lexical token.
    InvalidLiteral {
        /// Construct containing the invalid token pattern.
        construct: String,
        /// Literal value that is not a single token.
        literal: String,
    },
    /// A captured field name is blank.
    EmptyFieldName(String),
    /// A construct cannot capture a field twice.
    DuplicateField {
        /// Construct that contains the repeated capture.
        construct: String,
        /// Repeated field name.
        field: String,
    },
    /// Two constructs can match the same token sequence prefix.
    AmbiguousConstructs(String, String),
    /// A block opening or closing token is empty.
    InvalidBlockDelimiter(String),
    /// A block refers to a construct absent from the language.
    UnknownChildConstruct {
        /// Construct with the invalid block declaration.
        construct: String,
        /// Missing child construct name.
        child: String,
    },
    /// A child construct is listed more than once in a block declaration.
    DuplicateChildConstruct {
        /// Construct with the repeated block entry.
        construct: String,
        /// Repeated child construct name.
        child: String,
    },
}

fn patterns_overlap(left: &[PatternPart], right: &[PatternPart]) -> bool {
    left.iter()
        .zip(right)
        .all(|(left, right)| parts_overlap(left, right))
}

fn parts_overlap(left: &PatternPart, right: &PatternPart) -> bool {
    match (left, right) {
        (PatternPart::Literal(left), PatternPart::Literal(right)) => left == right,
        (PatternPart::Field { kind: left, .. }, PatternPart::Field { kind: right, .. }) => {
            left == right
        }
        (PatternPart::Literal(literal), PatternPart::Field { kind, .. })
        | (PatternPart::Field { kind, .. }, PatternPart::Literal(literal)) => {
            literal_kind(literal) == Some(*kind)
        }
    }
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

fn is_single_token(literal: &str) -> bool {
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

impl std::fmt::Display for SpecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyLanguageName => f.write_str("language name cannot be empty"),
            Self::NoConstructs => f.write_str("language must declare at least one construct"),
            Self::EmptyConstructName => f.write_str("construct name cannot be empty"),
            Self::DuplicateConstruct(name) => {
                write!(f, "construct `{name}` is declared more than once")
            }
            Self::EmptyPattern(name) => write!(f, "construct `{name}` has an empty syntax pattern"),
            Self::EmptyLiteral(name) => write!(f, "construct `{name}` has an empty literal token"),
            Self::InvalidLiteral { construct, literal } => write!(
                f,
                "construct `{construct}` literal `{literal}` is not one lexer token"
            ),
            Self::EmptyFieldName(name) => write!(f, "construct `{name}` has an empty field name"),
            Self::DuplicateField { construct, field } => write!(
                f,
                "construct `{construct}` captures field `{field}` more than once"
            ),
            Self::AmbiguousConstructs(left, right) => write!(
                f,
                "constructs `{left}` and `{right}` have overlapping syntax patterns"
            ),
            Self::InvalidBlockDelimiter(construct) => {
                write!(f, "construct `{construct}` has an empty block delimiter")
            }
            Self::UnknownChildConstruct { construct, child } => write!(
                f,
                "block for `{construct}` references unknown construct `{child}`"
            ),
            Self::DuplicateChildConstruct { construct, child } => write!(
                f,
                "block for `{construct}` lists child `{child}` more than once"
            ),
        }
    }
}

impl std::error::Error for SpecError {}
