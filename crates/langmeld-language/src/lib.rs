//! Shared lexical contracts and resource limits for Langmeld language parsers.

mod analysis;
mod backend;
mod definition;
mod definition_import;
mod definition_meta;
mod definition_tree;
mod formatting;
mod lexer;
mod lexing;
mod model;
mod parser;
mod pipeline;
mod project;
mod rust_codegen;
mod syntax;
mod tooling;
mod tree_sitter;

pub use analysis::*;
pub use backend::{
    ArtifactBackend, BackendDescriptor, BackendDescriptorIssue, BackendError, BackendFormat,
    BackendInputs, BackendOptions, BackendSetting, CompilationArtifact, SourceMapEntry,
    WorkspaceArtifactBackend, WorkspaceBackendInputs, compile_with_backend,
    compile_workspace_with_backend, resolve_backend_settings, validate_backend_descriptor,
    validate_backend_settings,
};
pub use definition::{
    DefinitionLimits, parse_definition, parse_definition_with_limits,
    parse_language_definition_modules, parse_language_definition_modules_with_limits,
    parse_language_definition_modules_with_meta,
    parse_language_definition_modules_with_meta_and_limits, parse_language_definition_project,
    parse_language_definition_project_with_limits, parse_language_definition_project_with_meta,
    parse_language_definition_project_with_meta_and_limits, parse_language_definition_with_meta,
    parse_language_definition_with_meta_and_limits,
};
pub use definition_meta::{langmeld_definition_language, langmeld_definition_parser};
pub use formatting::{FormatOptions, format_parsed_source, format_source};
pub use lexer::{
    CustomLexer, CustomLexerFn, LanguageToken, LanguageTokenKind, LexedSource, lex_source,
};
pub use model::{
    AstNode, CapturedValue, CustomConstructMatch, CustomConstructParser, CustomConstructParserFn,
    HostData, MapEntry, ParseResult, Program, Value,
};
pub use parser::{
    ConfiguredLanguageParser, ParserRegistrationError, ParserRegistrationIssue,
    PatternCompileError, PreparedLanguage,
};
pub use pipeline::{PassDescriptor, PassPipeline, PipelineError, PipelinePlanError};
pub use project::{
    ExtensionDependency, ExtensionPackage, LockedDefinition, LockedExtension,
    LockedExtensionDependency, ManifestExtension, ProjectBuildConfig, ProjectLockfile,
    ProjectManifest, ProjectSnapshot, compose_extensions, load_language, load_project,
    load_project_manifest, load_project_sources, load_project_with_manifest, load_source_files,
    write_project_extensions, write_project_lockfile, write_project_manifest,
};
pub use rust_codegen::generate_rust_ast;
pub use syntax::{
    CLASSIFICATION_TYPES, Classification, SyntaxElement, SyntaxNode, SyntaxNodeRole, SyntaxToken,
    SyntaxTokenKind, classify, classify_parse_result_prepared, classify_prepared,
};
pub use tooling::{GeneratedArtifacts, generate_tooling};
pub use tree_sitter::generate_tree_sitter;

/// Maximum nested construct bodies traversed by the recursive generic parser.
///
/// [`ParseLimits::max_nesting_depth`] can lower this bound for a parse. Values
/// above this ceiling are clamped to keep parser and callback-AST recursion safe.
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
    /// Requested maximum nested construct blocks, capped by [`MAX_NESTING_DEPTH`].
    pub max_nesting_depth: usize,
    /// Maximum recursive pattern matching and backtracking steps per parse.
    pub max_pattern_steps: usize,
}

impl Default for ParseLimits {
    fn default() -> Self {
        Self {
            max_source_bytes: 4 * 1024 * 1024,
            max_tokens: 250_000,
            max_ast_nodes: 250_000,
            max_nesting_depth: MAX_NESTING_DEPTH,
            max_pattern_steps: 1_000_000,
        }
    }
}
