//! CPython syntax adapted to Langmeld's generic language model and editor tools.

mod adapter;
mod frontend;
mod project;

use langmeld::{
    core::{Diagnostic, Severity, SourceFile, Span, ValidatedLanguageSpec},
    language::{
        ConfiguredLanguageParser, CustomLexer, LanguageToken, LanguageTokenKind, LexedSource,
        ParseResult, PreparedLanguage, Program, SyntaxNode, SyntaxNodeRole, WorkspaceAnalysis,
        WorkspaceDiagnostic, analyze, analyze_workspace_with_resolutions, parse_definition,
    },
};

/// Result of parsing one Python module through CPython and Langmeld.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceAnalysis {
    /// Parsed generic syntax tree and source-spanned nodes.
    pub parsed: ParseResult,
    /// CPython parse diagnostics and Langmeld semantic diagnostics.
    pub diagnostics: Vec<Diagnostic>,
}

/// Loads and validates the language model used by this source index.
pub fn validated_language_spec() -> Result<ValidatedLanguageSpec, Vec<Diagnostic>> {
    let definition = SourceFile::new("python-source.lm", include_str!("../python-source.lm"));
    parse_definition(&definition)?.validate().map_err(|errors| {
        errors
            .into_iter()
            .map(|error| Diagnostic::spec_error(error.code, error.message))
            .collect()
    })
}

/// Builds a parser configured with one persistent isolated CPython worker.
pub fn configured_parser() -> Result<ConfiguredLanguageParser, Vec<Diagnostic>> {
    let spec = validated_language_spec()?;
    let callbacks = adapter::parser_callbacks();
    let parser = PreparedLanguage::new(spec.clone())
        .map_err(|error| vec![Diagnostic::spec_error("PS1099", error.to_string())])?
        .with_custom_parsers(callbacks)
        .map_err(|error| vec![Diagnostic::spec_error("PS1099", error.to_string())])?;
    Ok(parser.with_custom_lexer(opaque_python_lexer()))
}

/// Parses a Python source file and resolves same-file function references.
#[must_use]
pub fn analyze_source(source: &SourceFile) -> SourceAnalysis {
    let spec = match validated_language_spec() {
        Ok(spec) => spec,
        Err(diagnostics) => return empty_analysis(source, diagnostics),
    };
    let parser = match configured_parser() {
        Ok(parser) => parser,
        Err(diagnostics) => return empty_analysis(source, diagnostics),
    };
    let mut parsed = parser.parse(source);
    let mut diagnostics = parsed.diagnostics.clone();
    diagnostics.extend(analyze(&spec, &parsed.program, source));
    parsed.diagnostics = diagnostics.clone();
    SourceAnalysis {
        parsed,
        diagnostics,
    }
}

/// Parses Python modules together, resolving direct relative sibling imports.
#[must_use]
pub fn analyze_sources(sources: &[SourceFile]) -> WorkspaceAnalysis {
    if sources.is_empty() {
        return WorkspaceAnalysis::default();
    }
    let spec = match validated_language_spec() {
        Ok(spec) => spec,
        Err(diagnostics) => {
            return WorkspaceAnalysis {
                diagnostics: diagnostics
                    .into_iter()
                    .map(|diagnostic| workspace_diagnostic(0, diagnostic))
                    .collect(),
                ..WorkspaceAnalysis::default()
            };
        }
    };
    let parser = match configured_parser() {
        Ok(parser) => parser,
        Err(diagnostics) => {
            return WorkspaceAnalysis {
                diagnostics: diagnostics
                    .into_iter()
                    .map(|diagnostic| workspace_diagnostic(0, diagnostic))
                    .collect(),
                ..WorkspaceAnalysis::default()
            };
        }
    };
    let parsed = sources
        .iter()
        .map(|source| parser.parse(source))
        .collect::<Vec<_>>();
    let files = sources
        .iter()
        .zip(&parsed)
        .map(|(source, result)| (source, &result.program))
        .collect::<Vec<_>>();
    let mut analysis = analyze_workspace_with_resolutions(&spec, &files);
    for (source_index, result) in parsed.iter().enumerate() {
        analysis.diagnostics.extend(
            result
                .diagnostics
                .iter()
                .cloned()
                .map(|diagnostic| workspace_diagnostic(source_index, diagnostic)),
        );
    }
    project::analyze_project_imports(&files, &mut analysis.diagnostics);
    analysis
}

/// Creates the workspace hook for checking relative module imports and exports.
#[must_use]
pub fn project_import_analysis_hook() -> langmeld::language::WorkspaceTypeAnalysisHook {
    project::project_import_analysis_hook()
}

fn opaque_python_lexer() -> CustomLexer {
    CustomLexer::new(|source| {
        if source.text.is_empty() {
            return LexedSource::default();
        }
        LexedSource::new(
            [LanguageToken::new(
                LanguageTokenKind::Opaque,
                Span::new(0, source.text.len()),
            )],
            [],
        )
    })
}

fn empty_analysis(source: &SourceFile, diagnostics: Vec<Diagnostic>) -> SourceAnalysis {
    SourceAnalysis {
        parsed: ParseResult {
            syntax_tree: SyntaxNode {
                role: SyntaxNodeRole::SourceFile,
                construct: None,
                syntax_variant: None,
                field: None,
                index: None,
                span: Span::new(0, source.text.len()),
                children: Vec::new(),
            },
            program: Program { nodes: Vec::new() },
            diagnostics: diagnostics.clone(),
            syntax_tokens: Vec::new(),
            completion_blocks: Vec::new(),
            completion_frontiers: Vec::new(),
            custom_parser_spans: Vec::new(),
            resource_limited: false,
        },
        diagnostics,
    }
}

fn workspace_diagnostic(source_index: usize, diagnostic: Diagnostic) -> WorkspaceDiagnostic {
    WorkspaceDiagnostic {
        source_index,
        source_id: None,
        source_revision: None,
        diagnostic,
    }
}

/// Generated typed views for the Python source-index definition.
pub mod generated {
    include!(concat!(env!("OUT_DIR"), "/langmeld_ast.rs"));
}

/// Reports whether parsing or generic analysis found an error.
#[must_use]
pub fn has_errors(analysis: &SourceAnalysis) -> bool {
    analysis
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
}
