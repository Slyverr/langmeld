//! The stable project-level entry point for Langmeld CLI and host tooling.

use std::{
    error::Error,
    fmt, fs, io,
    path::{Component, Path},
};

/// Conventional manifest filename used by Langmeld projects.
pub const PROJECT_MANIFEST_FILE: &str = "langmeld.toml";

/// Current supported project manifest format.
pub const PROJECT_MANIFEST_FORMAT: i64 = 1;

/// Parsed project settings needed to locate the language definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectManifest {
    /// Project-relative path to the root `.lm` definition.
    pub definition: String,
}

impl ProjectManifest {
    /// Parses and validates a Langmeld project manifest.
    ///
    /// The current manifest has two required fields: `format = 1` and a
    /// project-relative `.lm` path in `definition`. Unknown fields are rejected
    /// so misspellings do not silently change project behavior.
    pub fn parse(source: &str) -> Result<Self, ProjectManifestError> {
        let document =
            toml::from_str::<toml::Value>(source).map_err(ProjectManifestError::Parse)?;
        let table = document
            .as_table()
            .ok_or(ProjectManifestError::ExpectedTable)?;

        for key in table.keys() {
            if !matches!(key.as_str(), "format" | "definition") {
                return Err(ProjectManifestError::UnknownField(key.clone()));
            }
        }

        let format = table
            .get("format")
            .and_then(toml::Value::as_integer)
            .ok_or(ProjectManifestError::MissingOrInvalidField("format"))?;
        if format != PROJECT_MANIFEST_FORMAT {
            return Err(ProjectManifestError::UnsupportedFormat(format));
        }

        let definition = table
            .get("definition")
            .and_then(toml::Value::as_str)
            .ok_or(ProjectManifestError::MissingOrInvalidField("definition"))?;
        if !is_definition_path(definition) {
            return Err(ProjectManifestError::InvalidDefinitionPath(
                definition.to_owned(),
            ));
        }

        Ok(Self {
            definition: definition.to_owned(),
        })
    }
}

/// Reads and validates `langmeld.toml` from a project root.
pub fn read_project_manifest(
    project_root: impl AsRef<Path>,
) -> Result<ProjectManifest, ProjectManifestError> {
    read_project_manifest_file(project_root.as_ref().join(PROJECT_MANIFEST_FILE))
}

/// Reads and validates a project manifest at an explicit path.
pub fn read_project_manifest_file(
    path: impl AsRef<Path>,
) -> Result<ProjectManifest, ProjectManifestError> {
    let path = path.as_ref().to_path_buf();
    let source = fs::read_to_string(&path).map_err(|source| ProjectManifestError::Read {
        path: path.clone(),
        source,
    })?;
    ProjectManifest::parse(&source)
}

/// Error encountered while reading or validating a Langmeld project manifest.
#[derive(Debug)]
pub enum ProjectManifestError {
    /// TOML syntax is malformed.
    Parse(toml::de::Error),
    /// The document root must be a TOML table.
    ExpectedTable,
    /// A required field is absent or has the wrong TOML type.
    MissingOrInvalidField(&'static str),
    /// The manifest format version is not supported.
    UnsupportedFormat(i64),
    /// A field not defined by this manifest format was found.
    UnknownField(String),
    /// The root definition path is absolute, non-portable, or not an `.lm` file.
    InvalidDefinitionPath(String),
    /// The manifest file could not be read.
    Read {
        path: std::path::PathBuf,
        source: io::Error,
    },
}

impl fmt::Display for ProjectManifestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => write!(formatter, "invalid {PROJECT_MANIFEST_FILE}: {error}"),
            Self::ExpectedTable => write!(
                formatter,
                "{PROJECT_MANIFEST_FILE} must contain a TOML table"
            ),
            Self::MissingOrInvalidField(field) => {
                write!(
                    formatter,
                    "{PROJECT_MANIFEST_FILE} requires a valid `{field}` field"
                )
            }
            Self::UnsupportedFormat(format) => write!(
                formatter,
                "unsupported {PROJECT_MANIFEST_FILE} format {format}; expected {PROJECT_MANIFEST_FORMAT}"
            ),
            Self::UnknownField(field) => {
                write!(
                    formatter,
                    "unknown `{field}` field in {PROJECT_MANIFEST_FILE}"
                )
            }
            Self::InvalidDefinitionPath(path) => write!(
                formatter,
                "`definition` must be a project-relative `.lm` path without parent segments, got `{path}`"
            ),
            Self::Read { path, source } => write!(
                formatter,
                "cannot read project manifest `{}`: {source}",
                path.display()
            ),
        }
    }
}

impl Error for ProjectManifestError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Parse(error) => Some(error),
            Self::Read { source, .. } => Some(source),
            Self::ExpectedTable
            | Self::MissingOrInvalidField(_)
            | Self::UnsupportedFormat(_)
            | Self::UnknownField(_)
            | Self::InvalidDefinitionPath(_) => None,
        }
    }
}

fn is_definition_path(path: &str) -> bool {
    !path.contains('\\')
        && !has_windows_drive_prefix(path)
        && path.ends_with(".lm")
        && !path.is_empty()
        && Path::new(path)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn has_windows_drive_prefix(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}
