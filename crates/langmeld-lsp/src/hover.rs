use langmeld_core::Span;

/// The presentation format for host-provided hover content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HoverContent {
    /// Text displayed without markup interpretation.
    PlainText(String),
    /// Markdown-formatted text.
    Markdown(String),
}

/// Content returned by a host for an editor hover request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HoverInfo {
    /// Text and presentation format shown by the editor.
    pub content: HoverContent,
    /// Optional UTF-8 byte range to highlight as the hover target.
    pub range: Option<Span>,
}

impl HoverInfo {
    /// Creates a hover containing plain text.
    #[must_use]
    pub fn plain_text(value: impl Into<String>) -> Self {
        Self {
            content: HoverContent::PlainText(value.into()),
            range: None,
        }
    }

    /// Creates a hover containing Markdown.
    #[must_use]
    pub fn markdown(value: impl Into<String>) -> Self {
        Self {
            content: HoverContent::Markdown(value.into()),
            range: None,
        }
    }

    /// Associates a source range with this hover.
    #[must_use]
    pub fn with_range(mut self, range: Span) -> Self {
        self.range = Some(range);
        self
    }
}
