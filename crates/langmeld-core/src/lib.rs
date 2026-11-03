#![deny(missing_docs)]

//! Shared source contracts and diagnostics for Langmeld language tools.

mod diagnostic;
mod source;

pub use diagnostic::{Diagnostic, Severity};
pub use source::{SourceDatabase, SourceDatabaseError, SourceFile, SourceId, Span};
