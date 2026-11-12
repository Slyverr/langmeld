//! Errors returned when a definition cannot be exported to Tree-sitter.

/// An error while creating a Tree-sitter grammar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeSitterError {
    /// The grammar name does not satisfy Tree-sitter's identifier format.
    InvalidGrammarName(String),
    /// The language has no root constructs from which to build a source rule.
    NoRootConstructs,
    /// A host-parsed expression capture cannot be represented by the exporter.
    UnsupportedExpressionCapture {
        /// Construct containing the unsupported capture.
        construct: String,
        /// Name of the expression field.
        field: String,
    },
    /// A configured block-comment rule cannot be represented by this exporter.
    UnsupportedBlockComment {
        /// Comment opening marker.
        opening: String,
        /// Comment closing marker.
        closing: String,
    },
}

impl std::fmt::Display for TreeSitterError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidGrammarName(name) => write!(
                formatter,
                "invalid Tree-sitter grammar name `{name}`; use lowercase ASCII letters, digits, or underscores and start with a letter or underscore"
            ),
            Self::NoRootConstructs => {
                formatter.write_str("cannot export a grammar with no root constructs")
            }
            Self::UnsupportedExpressionCapture { construct, field } => write!(
                formatter,
                "cannot export expression field `{field}` in construct `{construct}`; use a host-authored Tree-sitter grammar for expression syntax"
            ),
            Self::UnsupportedBlockComment { opening, closing } => write!(
                formatter,
                "cannot export block comments `{opening}` ... `{closing}` to Tree-sitter"
            ),
        }
    }
}

impl std::error::Error for TreeSitterError {}
