use crate::completion_kind::CompletionKind;

use super::PatternPart;

/// A named syntax rule expressed as a sequence of token patterns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstructSpec {
    /// Stable construct name returned in parsed nodes.
    pub(crate) name: String,
    /// Ordered syntax alternatives that make up this construct.
    pub(crate) patterns: Vec<Vec<PatternPart>>,
    /// Optional stable labels corresponding to `patterns` by index.
    pub(crate) variant_names: Vec<Option<String>>,
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
            variant_names: vec![None],
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
        self.variant_names.push(None);
        self
    }

    /// Adds a syntax alternative with a stable name for generated projections.
    #[must_use]
    pub fn variant_named(
        mut self,
        name: impl Into<String>,
        pattern: impl IntoIterator<Item = PatternPart>,
    ) -> Self {
        self.patterns.push(pattern.into_iter().collect());
        self.variant_names.push(Some(name.into()));
        self
    }

    /// Names the first syntax alternative for generated projections.
    #[must_use]
    pub fn with_first_variant_name(mut self, name: impl Into<String>) -> Self {
        self.variant_names[0] = Some(name.into());
        self
    }

    /// Adds a delimited block containing the named child constructs.
    #[must_use]
    pub fn block(
        self,
        open: impl Into<String>,
        close: impl Into<String>,
        constructs: &[&str],
    ) -> Self {
        self.block_names(
            open,
            close,
            constructs.iter().map(|name| (*name).to_owned()),
        )
    }

    /// Adds a delimited block that accepts named constructs and category members.
    #[must_use]
    pub fn block_with_categories(
        self,
        open: impl Into<String>,
        close: impl Into<String>,
        constructs: &[&str],
        categories: &[&str],
    ) -> Self {
        self.block_names_with_categories(
            open,
            close,
            constructs.iter().map(|name| (*name).to_owned()),
            categories.iter().map(|category| (*category).to_owned()),
        )
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
        self,
        open: impl Into<String>,
        close: impl Into<String>,
        constructs: impl IntoIterator<Item = String>,
    ) -> Self {
        self.block_names_with_categories(open, close, constructs, std::iter::empty())
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

    /// Returns the stable name assigned to one syntax alternative, if any.
    #[must_use]
    pub fn variant_name(&self, index: usize) -> Option<&str> {
        self.variant_names.get(index)?.as_deref()
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
