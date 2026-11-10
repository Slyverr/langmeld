//! Shared source and diagnostic primitives for Langmeld.
#![deny(missing_docs)]

mod diagnostic;
mod position;
mod source;

pub use diagnostic::{Diagnostic, Severity, SourceLocation, format_diagnostics};
pub use position::{LineIndex, PositionEncoding, SourcePosition};
pub use source::{SourceDatabase, SourceDatabaseError, SourceFile, SourceId, SourceIdentity, Span};
