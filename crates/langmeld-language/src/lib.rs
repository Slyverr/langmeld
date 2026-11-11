//! Declarative syntax models and a generic parser for Langmeld languages.
#![deny(missing_docs)]

mod analysis;
mod completion;
mod definition;
mod format;
mod handler;
mod host;
mod navigation;
mod parser;
mod spec;

pub use analysis::{AnalysisContext, LanguagePass};
pub use completion::{
    CompletionCandidate, CompletionKind, complete_at, complete_at_with_lexer, complete_with,
};
pub use definition::{
    DefinitionResolver, DefinitionResult, DefinitionSource, MAX_DEFINITION_IMPORT_DEPTH,
    language_definition_grammar, parse_language_definition, parse_language_definition_project,
};
pub use format::{DefinitionFormatOptions, format_language_definition};
pub use handler::{
    ConstructContext, ConstructHandler, ConstructHandlerRegistry, HandlerControl,
    HandlerRegistrationError,
};
pub use host::{
    AnalysisDocument, LanguageAnalyzer, LanguageCompleter, LanguageNavigator, LanguageParser,
    LanguageWorkspaceAnalyzer, ParsedDocument,
};
pub use navigation::{definition_at, references_at};
pub use parser::MAX_NESTING_DEPTH;
pub use parser::{
    CapturedField, CapturedValue, DefaultLexer, LexOutput, LexedToken, Lexer, ParseLimits,
    ParseResult, ParsedConstruct, TokenKind, parse, parse_with_lexer, parse_with_lexer_and_limits,
    parse_with_limits,
};
pub use spec::{
    BlockSpec, ConstructSpec, FieldKind, LanguageSpec, LexicalRules, PatternPart, RepeatMinimum,
    SpecError,
};
