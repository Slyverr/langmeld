mod constructs;
mod runtime;

use std::path::Path;

use langmeld_core::{SourceFile, format_diagnostics};
use langmeld_language::{LanguageSpec, parse};
use langmeld_project::load_definition_project;

use crate::runtime::RuntimeState;

/// Loads the language definition from this Cargo project.
pub fn load() -> Result<LanguageSpec, String> {
    let project_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let project = load_definition_project(project_root).map_err(|error| error.to_string())?;

    if !project.is_valid() {
        return Err(format_diagnostics(
            &project.sources,
            &project.definition.diagnostics,
        ));
    }

    project
        .definition
        .language
        .ok_or_else(|| "validated definition did not produce a language".to_owned())
}

/// Parses and runs one source file, returning the language's output.
pub fn run(source: &SourceFile) -> Result<String, String> {
    let language = load()?;
    let parsed = parse(source, &language);

    if !parsed.diagnostics.is_empty() {
        return Err(format_diagnostics(
            std::slice::from_ref(source),
            &parsed.diagnostics,
        ));
    }

    let root = parsed
        .constructs
        .first()
        .ok_or_else(|| "program did not produce a root construct".to_owned())?;
    let mut state = RuntimeState::default();
    let handlers = constructs::registry().map_err(|error| error.to_string())?;

    handlers
        .dispatch_block(source, root, &mut state)
        .map_err(|diagnostic| format_diagnostics(std::slice::from_ref(source), &[diagnostic]))?;

    Ok(state.output)
}
