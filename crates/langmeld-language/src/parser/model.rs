use std::ops::Range;

use langmeld_core::Span;

use crate::completion_kind::CompletionKind;
use crate::lexer::TokenKind;

/// A field value captured by a syntax pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapturedValue {
    /// Text and lexical category captured without narrowing its token type.
    Token {
        /// Exact source text of the token.
        text: String,
        /// Lexical category provided by the lexer.
        kind: TokenKind,
    },
    /// Identifier text without transformation.
    Identifier(String),
    /// Decimal integer text preserved as written.
    Integer(String),
    /// Quoted string text preserved as written.
    String(String),
    /// Expression source captured before a top-level pattern terminator.
    ///
    /// The corresponding [`CapturedField::span`] identifies the exact source
    /// range. Use that range with an expression parser chosen by the host.
    Expression,
}

/// A named source value captured by a construct.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedField {
    /// Field name from the language specification.
    pub name: String,
    /// Captured lexical value.
    pub value: CapturedValue,
    /// Source range of the captured value, excluding surrounding syntax.
    pub span: Span,
}

/// One construct recognized in the source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedConstruct {
    /// Name of the matching construct specification.
    pub name: String,
    /// Zero-based index of the matching syntax alternative.
    pub variant: usize,
    /// Complete source range consumed by the construct.
    pub span: Span,
    /// Values captured by named fields.
    pub fields: Vec<CapturedField>,
    /// Optional and repeated capture groups from the selected syntax variant.
    ///
    /// Each occurrence refers to a range in this construct's flattened
    /// [`Self::fields`] collection. Nested groups retain their own occurrences.
    pub capture_groups: Vec<CaptureGroup>,
    /// Child constructs parsed from this construct's delimited block.
    pub children: Vec<ParsedConstruct>,
}

/// Kind of a structural group in a matched syntax pattern.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureGroupKind {
    /// A sequence that matched zero or one time.
    Optional,
    /// A sequence that matched zero or more times, subject to its grammar minimum.
    Repeat,
}

/// A matched optional or repeated group in a construct's syntax pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptureGroup {
    /// Whether this group was optional or repeated in the grammar.
    pub kind: CaptureGroupKind,
    /// Source occurrences of this group, in source order.
    ///
    /// An omitted optional group and a zero-match repetition have no
    /// occurrences. A matched optional group has one occurrence.
    pub occurrences: Vec<CaptureGroupOccurrence>,
}

/// One matched occurrence of an optional or repeated syntax group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptureGroupOccurrence {
    /// Source range consumed by the occurrence, excluding surrounding syntax.
    pub span: Span,
    /// Groups nested inside this occurrence, in grammar order.
    pub groups: Vec<CaptureGroup>,
    /// Range into the containing [`ParsedConstruct::fields`] collection.
    pub(crate) field_range: Range<usize>,
}

impl CaptureGroupOccurrence {
    /// Returns the flattened captured fields belonging to this occurrence.
    ///
    /// The range is local to the parsed construct that owns this group.
    /// Returns `None` if the range is outside the supplied construct's field
    /// list. The caller must pass the construct that produced this occurrence.
    #[must_use]
    pub fn captures<'a>(&self, construct: &'a ParsedConstruct) -> Option<&'a [CapturedField]> {
        construct.fields.get(self.field_range.clone())
    }
}

/// Parser scope accepted at a source range, used to derive completions.
pub(crate) struct ParseContext {
    pub(crate) span: Span,
    pub(crate) constructs: Vec<String>,
    pub(crate) close: Option<String>,
    pub(crate) values: Vec<String>,
    pub(crate) value_kind: CompletionKind,
    pub(crate) blocked: bool,
}
