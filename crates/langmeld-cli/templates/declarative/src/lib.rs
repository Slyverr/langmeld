mod constructs;
mod runtime;

use std::path::Path;

use langmeld_core::{SourceFile, format_diagnostics};
use langmeld_language::{LanguageSpec, ParseResult, parse};
use langmeld_project::load_definition_project;

use crate::runtime::RuntimeState;

/// A loaded language definition.
///
/// Reuse this value to parse or run multiple source files without reloading
/// the project's definition for each file. Each run creates fresh runtime
/// state and handler instances.
pub struct Language {
    specification: LanguageSpec,
}

impl Language {
    /// Returns the validated grammar used by this language instance.
    #[must_use]
    pub fn specification(&self) -> &LanguageSpec {
        &self.specification
    }

    /// Parses one source file with this language's validated grammar.
    #[must_use]
    pub fn parse(&self, source: &SourceFile) -> ParseResult {
        parse(source, &self.specification)
    }

    /// Parses and runs one source file with fresh runtime state.
    pub fn run(&self, source: &SourceFile) -> Result<String, String> {
        let parsed = self.parse(source);

        if !parsed.diagnostics().is_empty() {
            let formatted = format_diagnostics(std::slice::from_ref(source), parsed.diagnostics());

            if !parsed.is_valid() {
                return Err(formatted);
            }

            eprintln!("{formatted}");
        }

        if parsed.constructs().is_empty() {
            return Err("program did not produce a root construct".to_owned());
        }

        let mut state = RuntimeState::default();
        let handlers = constructs::registry().map_err(|error| error.to_string())?;

        handlers
            .dispatch_top_level_children(source, &parsed, &mut state)
            .map_err(|diagnostic| {
                format_diagnostics(std::slice::from_ref(source), &[diagnostic])
            })?;

        Ok(state.output)
    }
}

/// Loads and validates the language definition from this Cargo project.
pub fn load() -> Result<Language, String> {
    let project_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let project = load_definition_project(project_root).map_err(|error| error.to_string())?;
    let (_, _, definition, sources) = project.into_parts();

    if !definition.is_valid() {
        return Err(format_diagnostics(&sources, &definition.diagnostics));
    }

    let specification = definition
        .language
        .ok_or_else(|| "validated definition did not produce a language".to_owned())?;

    Ok(Language { specification })
}
