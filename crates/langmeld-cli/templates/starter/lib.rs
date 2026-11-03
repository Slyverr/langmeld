//! Rust host entry points for the @NAME@ language.

use langmeld::core::{Diagnostic, SourceFile, ValidatedLanguageSpec};
use langmeld::language::{
    ConfiguredLanguageParser, ParseResult, Program, ProjectSnapshot, WorkspaceAnalysis,
    analyze_workspace_with_host_callbacks,
};
use std::path::Path;

pub mod host;
mod language;

/// Parsed and analyzed inputs supplied to this language's execution hook.
pub struct RuntimeContext<'a> {
    /// Validated language model used to parse and analyze the inputs.
    pub language: &'a ValidatedLanguageSpec,
    /// Source snapshots in the order selected by the project.
    pub sources: &'a [SourceFile],
    /// Parsed programs corresponding to `sources` by index.
    pub programs: &'a [Program],
    /// Generic and host-provided analysis results for those programs.
    pub analysis: &'a WorkspaceAnalysis,
}

/// Loads the language model compiled by this crate's build script.
pub fn load_language() -> Result<ValidatedLanguageSpec, Vec<Diagnostic>> {
    language::load_language()
}

/// Loads the current project definitions dynamically from disk.
pub fn load_language_runtime() -> Result<ValidatedLanguageSpec, Vec<Diagnostic>> {
    language::load_language_runtime()
}

/// Loads the compiled language model and configured program sources.
pub fn load_project() -> Result<ProjectSnapshot, String> {
    let language = load_language().map_err(format_language_errors)?;
    langmeld::language::load_project_with_compiled_language(Path::new("langmeld.json"), &language)
}

/// Loads the current project definitions and sources dynamically from disk.
pub fn load_project_runtime() -> Result<ProjectSnapshot, String> {
    langmeld::language::load_project(Path::new("langmeld.json"))
}

/// Prepares the declarative parser for the language.
pub fn parser() -> Result<ConfiguredLanguageParser, Vec<Diagnostic>> {
    parser_for(load_language()?)
}

/// Prepares a validated language model with this project's custom callbacks.
pub fn parser_for(
    language: ValidatedLanguageSpec,
) -> Result<ConfiguredLanguageParser, Vec<Diagnostic>> {
    host::runtime::configured_parser(language).map_err(|error| {
        vec![Diagnostic::spec_error("LMSTARTER001", error)]
    })
}

/// Parses one source snapshot and returns its syntax tree and diagnostics.
pub fn parse_source(source: &SourceFile) -> Result<ParseResult, Vec<Diagnostic>> {
    Ok(parser()?.parse(source))
}

/// Runs the project's registered analysis callbacks over parsed source files.
pub fn analyze_sources(
    language: &ValidatedLanguageSpec,
    sources: &[SourceFile],
    programs: &[&Program],
) -> Result<WorkspaceAnalysis, String> {
    if sources.len() != programs.len() {
        return Err(format!(
            "source and program counts differ: {} sources, {} programs",
            sources.len(),
            programs.len()
        ));
    }
    let files: Vec<_> = sources.iter().zip(programs.iter().copied()).collect();
    let semantic_hooks = host::analysis::semantic_behavior_hooks(language)
        .map_err(|error| error.to_string())?;
    let resolver_hooks = host::analysis::resolver_behavior_hooks(language)
        .map_err(|error| error.to_string())?;
    let type_hooks = host::analysis::workspace_behavior_hooks(language)
        .map_err(|error| error.to_string())?;
    Ok(analyze_workspace_with_host_callbacks(
        language,
        &files,
        &[],
        &semantic_hooks,
        &resolver_hooks,
        &type_hooks,
    ))
}

/// Runs validated source files using this language's runtime implementation.
///
/// Replace the starter error with the interpreter or execution pipeline for
/// this language. Langmeld provides parsing and analysis; runtime semantics
/// belong to the project host.
pub fn run_sources(_context: &RuntimeContext<'_>) -> Result<(), String> {
    Err("runtime behavior is not implemented yet; add it in src/lib.rs".to_owned())
}

fn format_language_errors(errors: Vec<Diagnostic>) -> String {
    errors
        .into_iter()
        .map(|error| format!("{}: {}", error.code, error.message))
        .collect::<Vec<_>>()
        .join("\n")
}
