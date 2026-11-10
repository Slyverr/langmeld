//! Declarative syntax models and a generic parser for Langmeld languages.
#![deny(missing_docs)]

mod completion;
mod definition;
mod format;
mod host;
mod navigation;
mod parser;
mod spec;

pub use completion::{CompletionCandidate, complete_at, complete_at_with_lexer, complete_with};
pub use definition::{
    DefinitionResolver, DefinitionResult, DefinitionSource, MAX_DEFINITION_IMPORT_DEPTH,
    language_definition_grammar, parse_language_definition, parse_language_definition_project,
};
pub use format::{DefinitionFormatOptions, format_language_definition};
pub use host::{
    AnalysisDocument, LanguageAnalyzer, LanguageCompleter, LanguageNavigator, LanguageParser,
    ParsedDocument,
};
pub use navigation::{definition_at, references_at};
pub use parser::MAX_NESTING_DEPTH;
pub use parser::{
    CapturedField, CapturedValue, DefaultLexer, LexOutput, LexedToken, Lexer, ParseLimits,
    ParseResult, ParsedConstruct, TokenKind, parse, parse_with_lexer, parse_with_lexer_and_limits,
    parse_with_limits,
};
pub use spec::{
    BlockSpec, ConstructSpec, FieldKind, LanguageSpec, PatternPart, RepeatMinimum, SpecError,
};
