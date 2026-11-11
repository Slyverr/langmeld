//! Minimal validated grammar model.

mod error;
mod language;
mod model;
mod validation;

pub use error::SpecError;
pub use language::LanguageSpec;
pub use model::{BlockSpec, ConstructSpec, FieldKind, LexicalRules, PatternPart, RepeatMinimum};
