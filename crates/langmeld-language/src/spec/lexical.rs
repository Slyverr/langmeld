use std::collections::HashSet;

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
