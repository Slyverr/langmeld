//! Structured diagnostics shared by language tooling.

use crate::{SourceFile, SourceIdentity, Span};

/// Severity assigned to a diagnostic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// The input cannot be accepted as valid.
    Error,
    /// The input is accepted but may be surprising.
    Warning,
    /// Additional information for a user or tool.
    Info,
}

/// A machine-readable finding, optionally located in source text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// Stable diagnostic code.
    pub code: String,
    /// Finding severity.
    pub severity: Severity,
    /// Human-readable explanation.
    pub message: String,
    /// Location in the source snapshot, when source text is involved.
    pub location: Option<SourceLocation>,
}

/// A source range tied to the identity and revision that were analyzed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceLocation {
    /// Display name of the source snapshot, including for detached sources.
    pub source_name: String,
    /// Source identity and revision, absent only for a detached source.
    pub identity: Option<SourceIdentity>,
    /// Byte range in the associated source snapshot.
    pub span: Span,
}

impl Diagnostic {
    /// Creates a source-located error.
    #[must_use]
    pub fn error(
        code: impl Into<String>,
        message: impl Into<String>,
        source: &SourceFile,
        span: Span,
    ) -> Self {
        Self {
            code: code.into(),
            severity: Severity::Error,
            message: message.into(),
            location: Some(SourceLocation {
                source_name: source.name.clone(),
                identity: source.identity(),
                span,
            }),
        }
    }

    /// Creates an error that is not tied to a source span.
    #[must_use]
    pub fn spec_error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            severity: Severity::Error,
            message: message.into(),
            location: None,
        }
    }
}

/// Formats diagnostics as plain text with source name, one-based line and
/// column, code, and message.
///
/// Locations are matched to `sources` by display name. If a source snapshot is
/// unavailable, the location is shown at line 1, column 1. Diagnostics without
/// a source location use `<project>` as their display name.
#[must_use]
pub fn format_diagnostics(sources: &[SourceFile], diagnostics: &[Diagnostic]) -> String {
    diagnostics
        .iter()
        .map(|diagnostic| {
            let location = diagnostic.location.as_ref();
            let source = location.and_then(|location| {
                sources
                    .iter()
                    .find(|source| source.name == location.source_name)
            });
            let (line, column) = match (source, location) {
                (Some(source), Some(location)) => source.line_column(location.span.start),
                _ => (1, 1),
            };
            let source_name = location
                .map(|location| location.source_name.as_str())
                .unwrap_or("<project>");
            format!(
                "{source_name}:{line}:{column}: {}: {}",
                diagnostic.code, diagnostic.message
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
