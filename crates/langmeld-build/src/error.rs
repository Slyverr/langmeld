use std::{error::Error, fmt, io, path::PathBuf};

use langmeld_project::DefinitionProjectError;

/// Failure while loading a Langmeld project or generating Rust bindings.
#[derive(Debug)]
pub enum BuildError {
    /// Cargo did not provide a required build-script environment variable.
    MissingEnvironment(&'static str),
    /// The definition project could not be loaded.
    Project(DefinitionProjectError),
    /// The `.langmeld` definition is invalid.
    InvalidDefinition(String),
    /// The language grammar cannot be represented by generated Rust bindings.
    UnsupportedGrammar(String),
    /// Generated bindings could not be written to Cargo's output directory.
    WriteOutput {
        /// Path of the generated file.
        path: PathBuf,
        /// Underlying file-system error.
        source: io::Error,
    },
}

impl fmt::Display for BuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingEnvironment(name) => {
                write!(
                    formatter,
                    "Cargo did not set `{name}` for this build script"
                )
            }
            Self::Project(error) => write!(formatter, "cannot load Langmeld project: {error}"),
            Self::InvalidDefinition(message) => {
                write!(formatter, "invalid Langmeld definition:\n{message}")
            }
            Self::UnsupportedGrammar(message) => formatter.write_str(message),
            Self::WriteOutput { path, source } => write!(
                formatter,
                "cannot write generated bindings to `{}`: {source}",
                path.display()
            ),
        }
    }
}

impl Error for BuildError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Project(error) => Some(error),
            Self::WriteOutput { source, .. } => Some(source),
            Self::MissingEnvironment(_)
            | Self::InvalidDefinition(_)
            | Self::UnsupportedGrammar(_) => None,
        }
    }
}

impl From<DefinitionProjectError> for BuildError {
    fn from(error: DefinitionProjectError) -> Self {
        Self::Project(error)
    }
}
