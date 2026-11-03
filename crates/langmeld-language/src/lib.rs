//! Shared lexical contracts and resource limits for Langmeld language parsers.

mod lexer;
mod lexing;

pub use lexer::{
    CustomLexer, CustomLexerFn, LanguageToken, LanguageTokenKind, LexedSource, lex_source,
};

/// Maximum nested construct bodies accepted by default by the generic parser.
pub const MAX_NESTING_DEPTH: usize = 128;

/// Work and input bounds used when parsing untrusted source text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseLimits {
    /// Maximum UTF-8 source size in bytes.
    pub max_source_bytes: usize,
    /// Maximum lexical tokens, including the end-of-file sentinel.
    pub max_tokens: usize,
    /// Maximum AST nodes, including nodes returned by custom parser callbacks.
    pub max_ast_nodes: usize,
    /// Maximum nested construct blocks.
    pub max_nesting_depth: usize,
}

impl Default for ParseLimits {
    fn default() -> Self {
        Self {
            max_source_bytes: 4 * 1024 * 1024,
            max_tokens: 250_000,
            max_ast_nodes: 250_000,
            max_nesting_depth: MAX_NESTING_DEPTH,
        }
    }
}
