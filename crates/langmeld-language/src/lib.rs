//! Declarative syntax models and a generic parser for Langmeld languages.
#![deny(missing_docs)]

mod definition;
mod parser;
mod spec;

pub use definition::{DefinitionResult, parse_language_definition};
pub use parser::MAX_NESTING_DEPTH;
pub use parser::{CapturedField, CapturedValue, ParseResult, ParsedConstruct, parse};
pub use spec::{
    BlockSpec, ConstructSpec, FieldKind, LanguageSpec, PatternPart, RepeatMinimum, SpecError,
};
