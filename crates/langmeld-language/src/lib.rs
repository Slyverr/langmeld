//! Declarative syntax models and a generic parser for Langmeld languages.
#![deny(missing_docs)]

mod analysis;
mod completion;
mod completion_kind;
mod definition;
mod format;
mod handler;
mod host;
mod lexer;
mod navigation;
mod parser;
mod spec;
mod syntax;

pub use analysis::{AnalysisContext, LanguagePass};
pub use completion::{CompletionCandidate, complete_at, complete_at_with_lexer, complete_with};
pub use completion_kind::CompletionKind;
pub use definition::{
    DefinitionResolver, DefinitionResult, DefinitionSource, MAX_DEFINITION_IMPORT_DEPTH,
    decode_definition_string, language_definition_grammar, parse_language_definition,
    parse_language_definition_project, quote_definition_string,
};
pub use format::{DefinitionFormatOptions, format_language_definition};
pub use handler::{
    CapturedIdentifier, ConstructContext, ConstructHandler, ConstructHandlerRegistry,
    HandlerControl, HandlerRegistrationError,
};
pub use host::{
    AnalysisDocument, LanguageAnalyzer, LanguageCompleter, LanguageNavigator, LanguageParser,
    LanguageWorkspaceAnalyzer, ParsedDocument,
};
pub use lexer::{DefaultLexer, LexOutput, LexedToken, Lexer, TokenKind};
pub use navigation::{definition_at, references_at};
pub use parser::MAX_NESTING_DEPTH;
pub use parser::{
    CapturedField, CapturedValue, ParseLimits, ParseResult, ParseResultParts, ParsedConstruct,
    parse, parse_with_lexer, parse_with_lexer_and_limits, parse_with_limits,
};
pub use spec::{
    BlockSpec, ConstructSpec, FieldKind, LanguageSpec, LanguageSpecBuilder, LexicalRules,
    PatternPart, RepeatMinimum, SpecError,
};
pub use syntax::{
    SyntaxElement, SyntaxNode, SyntaxNodeKind, SyntaxToken, SyntaxTokenKind, SyntaxTree,
};
