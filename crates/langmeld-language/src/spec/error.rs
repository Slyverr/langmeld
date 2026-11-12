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
    /// A syntax alternative's generated name is blank.
    EmptyVariantName(String),
    /// Syntax alternative names must be unique within a construct.
    DuplicateVariantName {
        /// Construct that contains the repeated name.
        construct: String,
        /// Repeated syntax alternative name.
        variant: String,
    },
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
    /// A named optional or repeated group has a blank name.
    EmptyPatternGroupName(String),
    /// Named fields and groups must be unique among siblings in a syntax pattern.
    DuplicatePatternName {
        /// Construct that contains the repeated name.
        construct: String,
        /// Repeated field or group name.
        name: String,
    },
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
            Self::EmptyVariantName(construct) => write!(
                f,
                "construct `{construct}` has a syntax alternative with an empty name"
            ),
            Self::DuplicateVariantName { construct, variant } => write!(
                f,
                "construct `{construct}` names syntax alternative `{variant}` more than once"
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
            Self::EmptyPatternGroupName(construct) => write!(
                f,
                "construct `{construct}` has an optional or repeated group with an empty name"
            ),
            Self::DuplicatePatternName { construct, name } => write!(
                f,
                "construct `{construct}` repeats field or group name `{name}` in the same syntax scope"
            ),
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
