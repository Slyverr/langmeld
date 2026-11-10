//! Declarative syntax models and a generic parser for Langmeld languages.
#![deny(missing_docs)]

mod definition;
mod host;
mod parser;
mod spec;

pub use definition::{
    DefinitionResolver, DefinitionResult, DefinitionSource, MAX_DEFINITION_IMPORT_DEPTH,
    parse_language_definition, parse_language_definition_project,
};
pub use host::{LanguageParser, ParsedDocument};
pub use parser::MAX_NESTING_DEPTH;
pub use parser::{
    CapturedField, CapturedValue, DefaultLexer, LexOutput, LexedToken, Lexer, ParseResult,
    ParsedConstruct, TokenKind, parse, parse_with_lexer,
};
pub use spec::{
    BlockSpec, ConstructSpec, FieldKind, LanguageSpec, PatternPart, RepeatMinimum, SpecError,
};
