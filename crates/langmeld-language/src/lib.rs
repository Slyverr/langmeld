//! Shared lexical contracts and resource limits for Langmeld language parsers.

mod analysis;
mod backend;
mod definition;
mod definition_extension;
mod definition_import;
mod definition_meta;
mod definition_tree;
mod formatting;
mod generated_artifacts;
mod host_scaffold;
mod lexer;
mod lexing;
mod model;
mod parser;
mod pipeline;
mod project;
mod rust_codegen;
mod syntax;
mod tooling;
mod tooling_capabilities;
mod tooling_highlighting;
mod tooling_reference;
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
    DefinitionLimits, DefinitionProjectDiagnostic, language_definition_source_paths,
    parse_definition, parse_definition_with_limits, parse_language_definition_modules,
    parse_language_definition_modules_with_limits, parse_language_definition_modules_with_meta,
    parse_language_definition_modules_with_meta_and_limits, parse_language_definition_project,
    parse_language_definition_project_with_limits, parse_language_definition_project_with_meta,
    parse_language_definition_project_with_meta_and_limits,
    parse_language_definition_project_with_overlays,
    parse_language_definition_project_with_overlays_and_diagnostics,
    parse_language_definition_project_with_overlays_and_limits,
    parse_language_definition_with_meta, parse_language_definition_with_meta_and_limits,
};
pub use definition_extension::LANGUAGE_DEFINITION_EXTENSION;
pub use definition_meta::{langmeld_definition_language, langmeld_definition_parser};
pub use formatting::{FormatOptions, format_parsed_source, format_source};
pub use generated_artifacts::{
    GeneratedArtifactWrite, check_generated_artifacts, write_generated_artifacts,
};
pub use host_scaffold::generate_host_scaffold;
pub use lexer::{
    CustomLexer, CustomLexerFn, LanguageToken, LanguageTokenKind, LexedSource, lex_source,
    lex_source_with_lexical_spec,
};
pub use model::{
    AstNode, BlockDelimiterSpan, CapturedValue, CompletionBlockContext, CompletionFrontier,
    CompletionPart, CustomConstructMatch, CustomConstructParser, CustomConstructParserFn,
    CustomExpressionAtomParserFn, ExpectedCapture, ExpressionAtomParserRegistration, HostData,
    MapEntry, ParseResult, Program, Value,
};
pub use parser::{
    ConfiguredLanguageParser, ParserRegistrationError, ParserRegistrationIssue,
    PatternCompileError, PreparedLanguage,
};
pub use pipeline::{PassDescriptor, PassPipeline, PipelineError, PipelinePlanError};
pub use project::{
    ExtensionDependency, ExtensionPackage, LockedDefinition, LockedExtension,
    LockedExtensionDependency, ManifestExtension, ProjectBuildConfig, ProjectLanguageSource,
    ProjectLockfile, ProjectManifest, ProjectSnapshot, compose_extensions, load_language,
    load_project, load_project_language, load_project_language_with_base_language,
    load_project_language_with_manifest_and_base_language, load_project_manifest,
    load_project_sources, load_project_sources_with_base_language, load_project_with_base_language,
    load_project_with_compiled_language, load_project_with_manifest,
    load_project_with_manifest_and_base_language, load_source_files,
    project_definition_source_paths, project_definition_source_paths_for_manifest,
    write_project_extensions, write_project_lockfile, write_project_manifest,
};
pub use rust_codegen::{generate_rust_ast, generate_rust_language_spec};
pub use syntax::{
    CLASSIFICATION_TYPES, Classification, SEMANTIC_TOKEN_MODIFIERS, SEMANTIC_TOKEN_TYPES,
    SyntaxElement, SyntaxNode, SyntaxNodeRole, SyntaxToken, SyntaxTokenKind, classify,
    classify_parse_result_prepared, classify_prepared, semantic_token_modifier_for_classification,
    semantic_token_type_for_classification, textmate_scope_for_classification,
    tree_sitter_capture_for_classification,
};
pub use tooling::{GeneratedArtifacts, generate_tooling, generate_tooling_capabilities};
pub use tooling_capabilities::{
    Capability, CapabilityStatus, EditorConfigurationAssumptions, ToolingCapabilities,
};
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
