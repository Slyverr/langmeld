//! Minimal validated grammar model.

mod builder;
mod construct;
mod error;
mod language;
mod lexical;
mod pattern;
mod validation;

pub use builder::LanguageSpecBuilder;
pub use construct::{BlockSpec, ConstructSpec};
pub use error::SpecError;
pub use language::LanguageSpec;
pub use lexical::LexicalRules;
pub use pattern::{FieldKind, PatternPart, RepeatMinimum};
