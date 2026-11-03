//! Rust host entry points for the @NAME@ language.

use langmeld::core::{Diagnostic, SourceFile, ValidatedLanguageSpec};
use langmeld::language::{
    ConfiguredLanguageParser, ParseResult, Program, ProjectSnapshot, WorkspaceAnalysis,
    analyze_workspace_with_host_callbacks,
};
use std::path::{Path, PathBuf};

pub mod host;

/// Loads the modular language definition from `language/`.
pub fn load_language() -> Result<ValidatedLanguageSpec, Vec<Diagnostic>> {
    langmeld::language::parse_language_definition_project(&language_definition_path())
}

/// Loads the declarative project definition and configured program sources.
pub fn load_project() -> Result<ProjectSnapshot, String> {
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
pub fn run_sources(_sources: &[SourceFile]) -> Result<(), String> {
    Err("runtime behavior is not implemented yet; add it in src/lib.rs".to_owned())
}

fn language_definition_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("language/language.lm")
}
