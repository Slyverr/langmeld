//! Declarative syntax models and a generic parser for Langmeld languages.
#![deny(missing_docs)]

mod definition;
mod host;
mod parser;
mod spec;

pub use definition::{DefinitionResult, parse_language_definition};
pub use host::{LanguageParser, ParsedDocument};
pub use parser::MAX_NESTING_DEPTH;
pub use parser::{
    CapturedField, CapturedValue, DefaultLexer, LexOutput, LexedToken, Lexer, ParseResult,
    ParsedConstruct, TokenKind, parse, parse_with_lexer,
};
pub use spec::{
    BlockSpec, ConstructSpec, FieldKind, LanguageSpec, PatternPart, RepeatMinimum, SpecError,
};
