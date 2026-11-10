//! Declarative syntax models and a generic parser for Langmeld languages.
#![deny(missing_docs)]

mod parser;
mod spec;

pub use parser::MAX_NESTING_DEPTH;
pub use parser::{CapturedField, CapturedValue, ParseResult, ParsedConstruct, parse};
pub use spec::{BlockSpec, ConstructSpec, FieldKind, LanguageSpec, PatternPart, SpecError};
