mod constructs;
mod runtime;

use std::{error::Error, fmt, path::Path};

use langmeld_core::{Diagnostic, SourceFile, Span};
use langmeld_language::{LanguageSpec, ParseResult, parse};
use langmeld_project::{DefinitionProjectError, load_definition_project};

use crate::runtime::RuntimeState;

/// A loaded WidgetScript definition.
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

/// Failure while loading the WidgetScript definition project.
#[derive(Debug)]
pub enum LanguageLoadError {
    /// The project manifest or definition files could not be loaded.
    Project(DefinitionProjectError),
    /// The loaded definition did not produce a valid language.
    InvalidDefinition {
        /// Diagnostics tied to the loaded definition source snapshots.
        diagnostics: Vec<Diagnostic>,
        /// Source snapshots used to resolve diagnostic locations.
        sources: Vec<SourceFile>,
    },
}

impl LanguageLoadError {
    fn invalid_definition(diagnostics: Vec<Diagnostic>, sources: Vec<SourceFile>) -> Self {
        Self::InvalidDefinition {
            diagnostics,
            sources,
        }
    }

    /// Returns definition diagnostics, or an empty slice for project I/O errors.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        match self {
            Self::Project(_) => &[],
            Self::InvalidDefinition { diagnostics, .. } => diagnostics,
        }
    }

    /// Returns source snapshots used to locate definition diagnostics.
    #[must_use]
    pub fn sources(&self) -> &[SourceFile] {
        match self {
            Self::Project(_) => &[],
            Self::InvalidDefinition { sources, .. } => sources,
        }
    }
}

impl fmt::Display for LanguageLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Project(error) => error.fmt(formatter),
            Self::InvalidDefinition { .. } => formatter.write_str("language definition is invalid"),
        }
    }
}

impl Error for LanguageLoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Project(error) => Some(error),
            Self::InvalidDefinition { .. } => None,
        }
    }
}

impl From<DefinitionProjectError> for LanguageLoadError {
    fn from(error: DefinitionProjectError) -> Self {
        Self::Project(error)
    }
}

impl Language {
    /// Returns the validated grammar used by this language instance.
    pub fn specification(&self) -> &LanguageSpec {
        &self.specification
    }

    /// Parses one source file with this language's validated grammar.
    pub fn parse(&self, source: &SourceFile) -> ParseResult {
        parse(source, &self.specification)
    }

    /// Parses and executes one WidgetScript program with fresh runtime state.
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
                    "TOY-007",
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
                "TOY-005",
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
                diagnostics.push(Diagnostic::spec_error("TOY-006", error.to_string()));

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

/// Loads and validates the WidgetScript definition.
pub fn load() -> Result<Language, LanguageLoadError> {
    load_from(Path::new(env!("CARGO_MANIFEST_DIR")))
}

/// Loads and validates a WidgetScript definition from a project root directory.
///
/// The directory must contain `langmeld.toml`; the manifest selects the root
/// `.langmeld` definition.
pub fn load_from(project_root: impl AsRef<Path>) -> Result<Language, LanguageLoadError> {
    let project = load_definition_project(project_root)?;
    let (_, _, definition, sources) = project.into_parts();

    if !definition.is_valid() {
        return Err(LanguageLoadError::invalid_definition(
            definition.diagnostics,
            sources,
        ));
    }

    let Some(specification) = definition.language else {
        return Err(LanguageLoadError::invalid_definition(Vec::new(), sources));
    };

    Ok(Language { specification })
}
