//! Shared source and diagnostic primitives for Langmeld.
#![deny(missing_docs)]

mod diagnostic;
mod source;

pub use diagnostic::{Diagnostic, Severity, SourceLocation};
pub use source::{SourceDatabase, SourceDatabaseError, SourceFile, SourceId, SourceIdentity, Span};
