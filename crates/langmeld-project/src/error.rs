use std::{error::Error, fmt, io, path::PathBuf};

use crate::definition_paths::DefinitionPathError;

/// Failure while loading a language project definition.
#[derive(Debug)]
pub enum ProjectLoadError {
    /// The configured root could not be resolved or is not a directory.
    RootIo {
        /// Requested project root path.
        path: PathBuf,
        /// Underlying filesystem error.
        source: io::Error,
    },
    /// The configured root exists but is not a directory.
    RootNotDirectory(PathBuf),
    /// The requested path is not a portable relative definition path.
    InvalidPath(String),
    /// A definition path resolves outside the configured root.
    OutsideRoot(String),
    /// A definition could not be read as a regular UTF-8 file.
    Read {
        /// Path of the definition that could not be read.
        path: PathBuf,
        /// Underlying filesystem or UTF-8 decoding error.
        source: io::Error,
    },
    /// A definition is absent from an in-memory source collection.
    MissingDefinition(String),
}

pub(crate) fn path_error(path: &str, error: DefinitionPathError) -> ProjectLoadError {
    match error {
        DefinitionPathError::Invalid => ProjectLoadError::InvalidPath(path.to_owned()),
        DefinitionPathError::OutsideRoot => ProjectLoadError::OutsideRoot(path.to_owned()),
    }
}

impl fmt::Display for ProjectLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RootIo { path, source } => {
                write!(
                    formatter,
                    "cannot resolve project root `{}`: {source}",
                    path.display()
                )
            }
            Self::RootNotDirectory(path) => {
                write!(
                    formatter,
                    "project root `{}` is not a directory",
                    path.display()
                )
            }
            Self::InvalidPath(path) => {
                write!(
                    formatter,
                    "invalid or non-portable definition path `{path}`; use `/` separators and Windows-compatible filename components"
                )
            }
            Self::OutsideRoot(path) => write!(
                formatter,
                "definition `{path}` resolves outside the project root"
            ),
            Self::Read { path, source } => {
                write!(
                    formatter,
                    "cannot read definition `{}`: {source}",
                    path.display()
                )
            }
            Self::MissingDefinition(path) => {
                write!(
                    formatter,
                    "definition `{path}` is not in the source collection"
                )
            }
        }
    }
}

impl Error for ProjectLoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::RootIo { source, .. } | Self::Read { source, .. } => Some(source),
            Self::RootNotDirectory(_)
            | Self::InvalidPath(_)
            | Self::OutsideRoot(_)
            | Self::MissingDefinition(_) => None,
        }
    }
}
