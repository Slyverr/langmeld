//! Minimal validated grammar model.

use std::collections::{HashMap, HashSet};

use crate::completion::CompletionKind;

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

    fn validate(&self) -> Result<(), SpecError> {
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

/// A language's currently supported declarative grammar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguageSpec {
    /// Human-readable language name.
    pub(crate) name: String,
    /// Syntax constructs accepted by this language.
    pub(crate) constructs: Vec<ConstructSpec>,
    /// Comment delimiters used by the built-in lexer.
    pub(crate) lexical_rules: LexicalRules,
    /// Indices of constructs allowed at the source root.
    pub(crate) root_constructs: Vec<usize>,
}

impl LanguageSpec {
    /// Validates and creates a language grammar.
    pub fn new(
        name: impl Into<String>,
        constructs: impl IntoIterator<Item = ConstructSpec>,
    ) -> Result<Self, SpecError> {
        let name = name.into();
        let mut constructs: Vec<_> = constructs.into_iter().collect();
        if name.trim().is_empty() {
            return Err(SpecError::EmptyLanguageName);
        }
        if constructs.is_empty() {
            return Err(SpecError::NoConstructs);
        }
        let mut names = HashSet::new();
        let mut category_members: HashMap<String, Vec<String>> = HashMap::new();
        for construct in &constructs {
            if construct.name.trim().is_empty() {
                return Err(SpecError::EmptyConstructName);
            }
            if !names.insert(construct.name.clone()) {
                return Err(SpecError::DuplicateConstruct(construct.name.clone()));
            }
            let mut categories = HashSet::new();
            for category in &construct.categories {
                if category.trim().is_empty() {
                    return Err(SpecError::EmptyCategoryName(construct.name.clone()));
                }
                if !categories.insert(category.as_str()) {
                    return Err(SpecError::DuplicateImplementedCategory {
                        construct: construct.name.clone(),
                        category: category.clone(),
                    });
                }
                category_members
                    .entry(category.clone())
                    .or_default()
                    .push(construct.name.clone());
            }
            if construct.patterns.is_empty() {
                return Err(SpecError::EmptyPattern(construct.name.clone()));
            }
            for (index, pattern) in construct.patterns.iter().enumerate() {
                if pattern.is_empty() || sequence_is_nullable(pattern) {
                    return Err(SpecError::EmptyPattern(construct.name.clone()));
                }
                let mut fields = HashSet::new();
                validate_pattern(pattern, &construct.name, &mut fields)?;
                validate_decision_points(pattern, &construct.name)?;
                if construct.patterns[..index].iter().any(|previous| {
                    !has_group(pattern)
                        && !has_group(previous)
                        && patterns_overlap(previous, pattern)
                }) {
                    return Err(SpecError::AmbiguousVariants(construct.name.clone()));
                }
            }
        }
        for construct in &mut constructs {
            if let Some(block) = &mut construct.block {
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
                    if !children.insert(child.clone()) {
                        return Err(SpecError::DuplicateChildConstruct {
                            construct: construct.name.clone(),
                            child: child.clone(),
                        });
                    }
                }
                let mut categories = HashSet::new();
                let mut expanded_children = block.constructs.clone();
                for category in &block.categories {
                    if category.trim().is_empty() {
                        return Err(SpecError::EmptyCategoryName(construct.name.clone()));
                    }
                    if !categories.insert(category.as_str()) {
                        return Err(SpecError::DuplicateBlockCategory {
                            construct: construct.name.clone(),
                            category: category.clone(),
                        });
                    }
                    let Some(members) = category_members.get(category) else {
                        return Err(SpecError::UnknownBlockCategory {
                            construct: construct.name.clone(),
                            category: category.clone(),
                        });
                    };
                    for member in members {
                        if children.insert(member.clone()) {
                            expanded_children.push(member.clone());
                        }
                    }
                }
                block.constructs = expanded_children;
            }
        }
        for (index, left) in constructs.iter().enumerate() {
            for right in &constructs[index + 1..] {
                for left_pattern in &left.patterns {
                    for right_pattern in &right.patterns {
                        if !has_group(left_pattern)
                            && !has_group(right_pattern)
                            && patterns_overlap(left_pattern, right_pattern)
                        {
                            return Err(SpecError::AmbiguousConstructs(
                                left.name.clone(),
                                right.name.clone(),
                            ));
                        }
                    }
                }
            }
        }
        let root_constructs = (0..constructs.len()).collect();
        Ok(Self {
            name,
            constructs,
            lexical_rules: LexicalRules::default(),
            root_constructs,
        })
    }

    /// Configures comment delimiters for the built-in lexer.
    pub fn with_lexical_rules(mut self, rules: LexicalRules) -> Result<Self, SpecError> {
        rules.validate()?;
        self.lexical_rules = rules;
        Ok(self)
    }

    /// Returns the comment delimiters used by the built-in lexer.
    #[must_use]
    pub fn lexical_rules(&self) -> &LexicalRules {
        &self.lexical_rules
    }

    /// Restricts which constructs may appear at the source root.
    ///
    /// By default every construct is allowed at the root. Constructs allowed
    /// inside delimited blocks are configured separately with
    /// [`ConstructSpec::block`].
    pub fn with_root_constructs(mut self, names: &[&str]) -> Result<Self, SpecError> {
        let mut root_constructs = vec![false; self.constructs.len()];
        for name in names {
            let Some(index) = self
                .constructs
                .iter()
                .position(|construct| construct.name == *name)
            else {
                return Err(SpecError::UnknownRootConstruct((*name).to_owned()));
            };
            if std::mem::replace(&mut root_constructs[index], true) {
                return Err(SpecError::DuplicateRootConstruct((*name).to_owned()));
            }
        }
        self.root_constructs = root_constructs
            .into_iter()
            .enumerate()
            .filter_map(|(index, allowed)| allowed.then_some(index))
            .collect();
        Ok(self)
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

    /// Returns constructs allowed at the source root in declaration order.
    pub fn root_constructs(&self) -> impl Iterator<Item = &ConstructSpec> {
        self.root_constructs
            .iter()
            .map(|index| &self.constructs[*index])
    }
}

/// An invalid declarative grammar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpecError {
    /// The language name is blank.
    EmptyLanguageName,
    /// At least one construct is required.
    NoConstructs,
    /// A root construct name does not exist in the language.
    UnknownRootConstruct(String),
    /// A root construct is listed more than once.
    DuplicateRootConstruct(String),
    /// A construct name is blank.
    EmptyConstructName,
    /// Construct names must be unique.
    DuplicateConstruct(String),
    /// A construct pattern cannot be empty.
    EmptyPattern(String),
    /// Two alternatives of one construct have overlapping syntax.
    AmbiguousVariants(String),
    /// A literal pattern must contain one token.
    EmptyLiteral(String),
    /// A literal contains whitespace or more than one lexical token.
    InvalidLiteral {
        /// Construct containing the invalid token pattern.
        construct: String,
        /// Literal value that is not a single token.
        literal: String,
    },
    /// An expression terminator is empty or not a single lexer token.
    InvalidExpressionTerminator {
        /// Construct containing the invalid terminator.
        construct: String,
        /// Invalid terminator text.
        terminator: String,
    },
    /// A finite-choice field has no values.
    EmptyChoiceValues(String),
    /// A finite-choice field contains text that is not one lexer token.
    InvalidChoiceValue {
        /// Construct containing the invalid choice field.
        construct: String,
        /// Invalid token value.
        value: String,
    },
    /// A finite-choice field repeats a token.
    DuplicateChoiceValue {
        /// Construct containing the repeated choice field.
        construct: String,
        /// Repeated token value.
        value: String,
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
    /// An optional or repeated group contains no syntax parts.
    EmptyPatternGroup(String),
    /// An optional group can match without consuming a token.
    EmptyOptional(String),
    /// A repeated group can match without consuming a token.
    EmptyRepetition(String),
    /// An optional or repeated group overlaps with the syntax that follows it.
    AmbiguousPatternBoundary(String),
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
    /// A construct or block category name is blank.
    EmptyCategoryName(String),
    /// A construct lists the same category more than once.
    DuplicateImplementedCategory {
        /// Construct that repeats the category.
        construct: String,
        /// Repeated category name.
        category: String,
    },
    /// A block lists the same category more than once.
    DuplicateBlockCategory {
        /// Construct with the repeated block category.
        construct: String,
        /// Repeated category name.
        category: String,
    },
    /// A block refers to a category with no implementing constructs.
    UnknownBlockCategory {
        /// Construct with the invalid block declaration.
        construct: String,
        /// Missing category name.
        category: String,
    },
    /// A line-comment marker cannot be recognized by the built-in lexer.
    InvalidLineComment(String),
    /// A block-comment marker is invalid for the built-in lexer.
    InvalidBlockComment {
        /// Opening marker that failed validation.
        opening: String,
        /// Closing marker that failed validation.
        closing: String,
    },
    /// Two comment rules use the same opening marker.
    DuplicateCommentStart(String),
}

fn patterns_overlap(left: &[PatternPart], right: &[PatternPart]) -> bool {
    left.iter()
        .zip(right)
        .all(|(left, right)| parts_overlap(left, right))
}

fn has_group(pattern: &[PatternPart]) -> bool {
    pattern.iter().any(|part| match part {
        PatternPart::Optional(_) | PatternPart::Repeat { .. } => true,
        PatternPart::Literal(_)
        | PatternPart::Field { .. }
        | PatternPart::Choice { .. }
        | PatternPart::Expression { .. } => false,
    })
}

fn parts_overlap(left: &PatternPart, right: &PatternPart) -> bool {
    match (left, right) {
        (PatternPart::Literal(left), PatternPart::Literal(right)) => left == right,
        (PatternPart::Field { kind: left, .. }, PatternPart::Field { kind: right, .. }) => {
            field_kinds_overlap(*left, *right)
        }
        (PatternPart::Choice { values, .. }, PatternPart::Choice { values: other, .. }) => {
            values.iter().any(|value| other.contains(value))
        }
        (PatternPart::Choice { values, .. }, PatternPart::Literal(literal))
        | (PatternPart::Literal(literal), PatternPart::Choice { values, .. }) => {
            values.iter().any(|value| value == literal)
        }
        (PatternPart::Choice { values, .. }, PatternPart::Field { kind, .. })
        | (PatternPart::Field { kind, .. }, PatternPart::Choice { values, .. }) => {
            *kind == FieldKind::Token
                || values
                    .iter()
                    .any(|value| literal_kind(value) == Some(*kind))
        }
        (PatternPart::Expression { .. }, _) | (_, PatternPart::Expression { .. }) => true,
        (PatternPart::Literal(literal), PatternPart::Field { kind, .. })
        | (PatternPart::Field { kind, .. }, PatternPart::Literal(literal)) => {
            *kind == FieldKind::Token || literal_kind(literal) == Some(*kind)
        }
        (PatternPart::Optional(_), _)
        | (PatternPart::Repeat { .. }, _)
        | (_, PatternPart::Optional(_))
        | (_, PatternPart::Repeat { .. }) => true,
    }
}

fn validate_pattern(
    pattern: &[PatternPart],
    construct: &str,
    fields: &mut HashSet<String>,
) -> Result<(), SpecError> {
    for part in pattern {
        match part {
            PatternPart::Literal(literal) if literal.is_empty() => {
                return Err(SpecError::EmptyLiteral(construct.to_owned()));
            }
            PatternPart::Literal(literal) if !is_single_token(literal) => {
                return Err(SpecError::InvalidLiteral {
                    construct: construct.to_owned(),
                    literal: literal.clone(),
                });
            }
            PatternPart::Literal(_) => {}
            PatternPart::Field { name, .. } => {
                if name.trim().is_empty() {
                    return Err(SpecError::EmptyFieldName(construct.to_owned()));
                }
                if !fields.insert(name.clone()) {
                    return Err(SpecError::DuplicateField {
                        construct: construct.to_owned(),
                        field: name.clone(),
                    });
                }
            }
            PatternPart::Choice { name, values, .. } => {
                if name.trim().is_empty() {
                    return Err(SpecError::EmptyFieldName(construct.to_owned()));
                }
                if !fields.insert(name.clone()) {
                    return Err(SpecError::DuplicateField {
                        construct: construct.to_owned(),
                        field: name.clone(),
                    });
                }
                if values.is_empty() {
                    return Err(SpecError::EmptyChoiceValues(construct.to_owned()));
                }
                let mut seen = HashSet::new();
                for value in values {
                    if !is_single_token(value) {
                        return Err(SpecError::InvalidChoiceValue {
                            construct: construct.to_owned(),
                            value: value.clone(),
                        });
                    }
                    if !seen.insert(value) {
                        return Err(SpecError::DuplicateChoiceValue {
                            construct: construct.to_owned(),
                            value: value.clone(),
                        });
                    }
                }
            }
            PatternPart::Expression { name, terminator } => {
                if name.trim().is_empty() {
                    return Err(SpecError::EmptyFieldName(construct.to_owned()));
                }
                if !fields.insert(name.clone()) {
                    return Err(SpecError::DuplicateField {
                        construct: construct.to_owned(),
                        field: name.clone(),
                    });
                }
                if !is_single_token(terminator) {
                    return Err(SpecError::InvalidExpressionTerminator {
                        construct: construct.to_owned(),
                        terminator: terminator.clone(),
                    });
                }
            }
            PatternPart::Optional(parts) => {
                if parts.is_empty() {
                    return Err(SpecError::EmptyPatternGroup(construct.to_owned()));
                }
                if sequence_is_nullable(parts) {
                    return Err(SpecError::EmptyOptional(construct.to_owned()));
                }
                validate_pattern(parts, construct, fields)?;
            }
            PatternPart::Repeat { parts, .. } => {
                if parts.is_empty() {
                    return Err(SpecError::EmptyPatternGroup(construct.to_owned()));
                }
                if sequence_is_nullable(parts) {
                    return Err(SpecError::EmptyRepetition(construct.to_owned()));
                }
                validate_pattern(parts, construct, fields)?;
            }
        }
    }
    Ok(())
}

fn sequence_is_nullable(parts: &[PatternPart]) -> bool {
    parts.iter().all(|part| match part {
        PatternPart::Literal(_)
        | PatternPart::Field { .. }
        | PatternPart::Choice { .. }
        | PatternPart::Expression { .. } => false,
        PatternPart::Optional(_) => true,
        PatternPart::Repeat { minimum, .. } => *minimum == RepeatMinimum::Zero,
    })
}

#[derive(Clone, Copy)]
enum FirstMatcher<'a> {
    Literal(&'a str),
    Field(FieldKind),
    Choice(&'a [String]),
}

fn first_set(pattern: &[PatternPart]) -> Vec<FirstMatcher<'_>> {
    let mut first = Vec::new();
    for part in pattern {
        match part {
            PatternPart::Literal(value) => {
                first.push(FirstMatcher::Literal(value));
                break;
            }
            PatternPart::Field { kind, .. } => {
                first.push(FirstMatcher::Field(*kind));
                break;
            }
            PatternPart::Choice { values, .. } => {
                first.push(FirstMatcher::Choice(values));
                break;
            }
            PatternPart::Expression { .. } => {
                first.push(FirstMatcher::Field(FieldKind::Token));
                break;
            }
            PatternPart::Optional(parts) => first.extend(first_set(parts)),
            PatternPart::Repeat { parts, minimum } => {
                first.extend(first_set(parts));
                if *minimum == RepeatMinimum::One {
                    break;
                }
            }
        }
    }
    first
}

fn first_matchers_overlap(left: FirstMatcher<'_>, right: FirstMatcher<'_>) -> bool {
    match (left, right) {
        (FirstMatcher::Literal(left), FirstMatcher::Literal(right)) => left == right,
        (FirstMatcher::Choice(left), FirstMatcher::Choice(right)) => {
            left.iter().any(|value| right.contains(value))
        }
        (FirstMatcher::Choice(values), FirstMatcher::Literal(literal))
        | (FirstMatcher::Literal(literal), FirstMatcher::Choice(values)) => {
            values.iter().any(|value| value == literal)
        }
        (FirstMatcher::Choice(values), FirstMatcher::Field(kind))
        | (FirstMatcher::Field(kind), FirstMatcher::Choice(values)) => {
            kind == FieldKind::Token || values.iter().any(|value| literal_kind(value) == Some(kind))
        }
        (FirstMatcher::Field(left), FirstMatcher::Field(right)) => field_kinds_overlap(left, right),
        (FirstMatcher::Literal(literal), FirstMatcher::Field(kind))
        | (FirstMatcher::Field(kind), FirstMatcher::Literal(literal)) => {
            kind == FieldKind::Token || literal_kind(literal) == Some(kind)
        }
    }
}

fn field_kinds_overlap(left: FieldKind, right: FieldKind) -> bool {
    left == right || left == FieldKind::Token || right == FieldKind::Token
}

fn validate_decision_points(pattern: &[PatternPart], construct: &str) -> Result<(), SpecError> {
    for (index, part) in pattern.iter().enumerate() {
        let group = match part {
            PatternPart::Optional(parts) | PatternPart::Repeat { parts, .. } => Some(parts),
            PatternPart::Literal(_)
            | PatternPart::Field { .. }
            | PatternPart::Choice { .. }
            | PatternPart::Expression { .. } => None,
        };
        if let Some(group) = group {
            let group_first = first_set(group);
            let suffix_first = first_set(&pattern[index + 1..]);
            if group_first.iter().any(|left| {
                suffix_first
                    .iter()
                    .any(|right| first_matchers_overlap(*left, *right))
            }) {
                return Err(SpecError::AmbiguousPatternBoundary(construct.to_owned()));
            }
            validate_decision_points(group, construct)?;
        }
    }
    Ok(())
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
            Self::UnknownRootConstruct(name) => {
                write!(f, "root scope references unknown construct `{name}`")
            }
            Self::DuplicateRootConstruct(name) => {
                write!(f, "root scope lists construct `{name}` more than once")
            }
            Self::EmptyConstructName => f.write_str("construct name cannot be empty"),
            Self::DuplicateConstruct(name) => {
                write!(f, "construct `{name}` is declared more than once")
            }
            Self::EmptyPattern(name) => write!(f, "construct `{name}` has an empty syntax pattern"),
            Self::AmbiguousVariants(construct) => write!(
                f,
                "construct `{construct}` has syntax alternatives with overlapping patterns"
            ),
            Self::EmptyLiteral(name) => write!(f, "construct `{name}` has an empty literal token"),
            Self::InvalidLiteral { construct, literal } => write!(
                f,
                "construct `{construct}` literal `{literal}` is not one lexer token"
            ),
            Self::InvalidExpressionTerminator {
                construct,
                terminator,
            } => write!(
                f,
                "construct `{construct}` expression terminator `{terminator}` is not one lexer token"
            ),
            Self::EmptyChoiceValues(construct) => {
                write!(
                    f,
                    "construct `{construct}` has a choice field with no values"
                )
            }
            Self::InvalidChoiceValue { construct, value } => write!(
                f,
                "construct `{construct}` choice value `{value}` is not one lexer token"
            ),
            Self::DuplicateChoiceValue { construct, value } => write!(
                f,
                "construct `{construct}` choice field repeats token `{value}`"
            ),
            Self::EmptyFieldName(name) => write!(f, "construct `{name}` has an empty field name"),
            Self::DuplicateField { construct, field } => write!(
                f,
                "construct `{construct}` captures field `{field}` more than once"
            ),
            Self::EmptyPatternGroup(construct) => {
                write!(
                    f,
                    "construct `{construct}` contains an empty optional or repeated group"
                )
            }
            Self::EmptyOptional(construct) => write!(
                f,
                "construct `{construct}` contains an optional group that can match no tokens"
            ),
            Self::EmptyRepetition(construct) => {
                write!(
                    f,
                    "construct `{construct}` repeats a group that can match no tokens"
                )
            }
            Self::AmbiguousPatternBoundary(construct) => write!(
                f,
                "construct `{construct}` has an optional or repeated group that overlaps following syntax"
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
            Self::EmptyCategoryName(construct) => {
                write!(f, "construct `{construct}` uses an empty category name")
            }
            Self::DuplicateImplementedCategory {
                construct,
                category,
            } => write!(
                f,
                "construct `{construct}` implements category `{category}` more than once"
            ),
            Self::DuplicateBlockCategory {
                construct,
                category,
            } => write!(
                f,
                "block for `{construct}` allows category `{category}` more than once"
            ),
            Self::UnknownBlockCategory {
                construct,
                category,
            } => write!(
                f,
                "block for `{construct}` references category `{category}` with no members"
            ),
            Self::InvalidLineComment(marker) => {
                write!(
                    f,
                    "line-comment marker `{marker}` must not start with whitespace or a quote, and must not contain a line break"
                )
            }
            Self::InvalidBlockComment { opening, closing } => write!(
                f,
                "block-comment markers `{opening}` and `{closing}` must be non-empty, line-local, and different"
            ),
            Self::DuplicateCommentStart(marker) => {
                write!(f, "comment marker `{marker}` is configured more than once")
            }
        }
    }
}

impl std::error::Error for SpecError {}
