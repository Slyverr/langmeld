/// Failure to construct a valid TextMate grammar or serialize its JSON.
#[derive(Debug)]
pub enum TextMateError {
    /// A scope name must start with `source.` or `text.` and contain valid dot-separated segments.
    InvalidScopeName(String),
    /// A file type must be a non-empty extension without a leading dot or path separators.
    InvalidFileType(String),
    /// JSON serialization failed.
    Serialize(serde_json::Error),
}

impl std::fmt::Display for TextMateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidScopeName(scope) => {
                write!(formatter, "invalid TextMate scope name `{scope}`")
            }
            Self::InvalidFileType(file_type) => {
                write!(formatter, "invalid TextMate file type `{file_type}`")
            }
            Self::Serialize(error) => {
                write!(formatter, "cannot serialize TextMate grammar: {error}")
            }
        }
    }
}

impl std::error::Error for TextMateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Serialize(error) => Some(error),
            Self::InvalidScopeName(_) | Self::InvalidFileType(_) => None,
        }
    }
}
