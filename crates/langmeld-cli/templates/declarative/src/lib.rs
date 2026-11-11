mod constructs;
mod runtime;

use std::path::Path;

use langmeld_core::{Diagnostic, SourceFile, Span, format_diagnostics};
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

/// Output and non-fatal diagnostics from a successful program run.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct RunReport {
    /// Text produced by the program.
    pub output: String,
    /// Non-fatal findings produced while parsing or running the program.
    pub diagnostics: Vec<Diagnostic>,
}

/// Diagnostics and any output produced before a program run failed.
#[derive(Debug, PartialEq, Eq)]
pub struct RunFailure {
    /// Output emitted before execution stopped. Hosts decide whether to keep it.
    pub partial_output: String,
    /// Diagnostics associated with the failed run.
    pub diagnostics: Vec<Diagnostic>,
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
    ///
    /// Diagnostics remain structured so the caller can choose how to display
    /// them. Output emitted before a handler fails is available in
    /// [`RunFailure::partial_output`].
    pub fn run(&self, source: &SourceFile) -> Result<RunReport, RunFailure> {
        let parsed = self.parse(source);
        let mut diagnostics = parsed.diagnostics().to_vec();

        if !parsed.is_valid() {
            if diagnostics.is_empty() {
                diagnostics.push(Diagnostic::error(
                    "EXAMPLE-004",
                    "the parser could not produce a complete valid result",
                    source,
                    Span::new(0, 0),
                ));
            }

            return Err(RunFailure {
                partial_output: String::new(),
                diagnostics,
            });
        }

        if parsed.constructs().is_empty() {
            diagnostics.push(Diagnostic::error(
                "EXAMPLE-003",
                "program did not produce a root construct",
                source,
                Span::new(0, 0),
            ));

            return Err(RunFailure {
                partial_output: String::new(),
                diagnostics,
            });
        }

        let mut state = RuntimeState::default();
        let handlers = match constructs::registry() {
            Ok(handlers) => handlers,
            Err(error) => {
                diagnostics.push(Diagnostic::spec_error("EXAMPLE-005", error.to_string()));

                return Err(RunFailure {
                    partial_output: state.output,
                    diagnostics,
                });
            }
        };

        if let Err(diagnostic) = handlers.dispatch_top_level_children(source, &parsed, &mut state) {
            diagnostics.push(diagnostic);

            return Err(RunFailure {
                partial_output: state.output,
                diagnostics,
            });
        }

        Ok(RunReport {
            output: state.output,
            diagnostics,
        })
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
