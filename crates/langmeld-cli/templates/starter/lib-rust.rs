//! Rust host entry points for the @NAME@ language.

use langmeld::core::{Diagnostic, SourceFile, ValidatedLanguageSpec};
use langmeld::language::{
    ConfiguredLanguageParser, ParseResult, Program, ProjectSnapshot, WorkspaceAnalysis,
    analyze_workspace_with_host_callbacks,
};
use std::path::Path;

pub mod host;
mod language;

/// Loads and validates the Rust-authored base language model.
pub fn load_language() -> Result<ValidatedLanguageSpec, Vec<Diagnostic>> {
    language::load_language()
}

/// Loads the project using this host's base model and declared `.lm` extensions.
pub fn load_project() -> Result<ProjectSnapshot, String> {
    let language = load_language()
        .map_err(|errors| format_language_errors(errors))?;
    langmeld::language::load_project_with_base_language(Path::new("langmeld.json"), &language)
}

/// Prepares the declarative parser for the base language model.
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
pub fn run_sources(_sources: &[SourceFile]) -> Result<(), String> {
    Err("runtime behavior is not implemented yet; add it in src/lib.rs".to_owned())
}

fn format_language_errors(errors: Vec<Diagnostic>) -> String {
    errors
        .into_iter()
        .map(|error| format!("{}: {}", error.code, error.message))
        .collect::<Vec<_>>()
        .join("\n")
}
