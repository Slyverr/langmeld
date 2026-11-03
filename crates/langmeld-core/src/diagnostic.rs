//! Structured diagnostics shared by parsers, validators, and host tools.

use serde::{Deserialize, Serialize};

use crate::Span;

/// Severity assigned to a diagnostic.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// The input cannot be accepted as valid.
    Error,
    /// The input is accepted, but may be surprising.
    Warning,
    /// Additional information for an editor or user.
    Info,
}

/// A structured finding tied to a source span where possible.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    /// Stable, machine-readable diagnostic code.
    pub code: String,
    /// Diagnostic severity.
    pub severity: Severity,
    /// Human-readable explanation.
    pub message: String,
    /// Source range, omitted for errors in a language definition itself.
    pub span: Option<Span>,
}

impl Diagnostic {
    /// Creates an error diagnostic at a source location.
    #[must_use]
    pub fn error(code: impl Into<String>, message: impl Into<String>, span: Span) -> Self {
        Self {
            code: code.into(),
            severity: Severity::Error,
            message: message.into(),
            span: Some(span),
        }
    }

    /// Creates an error diagnostic without a source location.
    #[must_use]
    pub fn spec_error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            severity: Severity::Error,
            message: message.into(),
            span: None,
        }
    }
}
