//! Filesystem-backed loading for modular Langmeld language projects.
//!
//! This crate keeps filesystem policy outside the language parser. Its
//! [`DirectoryDefinitionResolver`] confines imports to one canonical project
//! root and retains loaded source snapshots for diagnostic rendering.

mod definition_project;
mod manifest;

pub use definition_project::{
    DefinitionProjectError, LoadedDefinitionProject, load_definition_project,
    load_definition_project_from_manifest,
};
pub use manifest::{
    PROJECT_MANIFEST_FILE, ProjectManifest, ProjectManifestError, read_project_manifest,
    read_project_manifest_file,
};

use std::{
    collections::BTreeMap,
    error::Error,
    fmt, fs, io,
    path::{Component, Path, PathBuf},
    sync::Mutex,
};

use langmeld_core::SourceFile;
use langmeld_language::{DefinitionResolver, DefinitionSource};

/// Filesystem failure while loading a language project definition.
#[derive(Debug)]
pub enum ProjectLoadError {
    /// The configured root could not be resolved or is not a directory.
    RootIo { path: PathBuf, source: io::Error },
    /// The configured root exists but is not a directory.
    RootNotDirectory(PathBuf),
    /// The requested path is not a portable relative definition path.
    InvalidPath(String),
    /// A definition path resolves outside the configured root.
    OutsideRoot(String),
    /// A definition could not be read as a regular UTF-8 file.
    Read { path: PathBuf, source: io::Error },
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
                write!(formatter, "invalid relative definition path `{path}`")
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
        }
    }
}

impl Error for ProjectLoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::RootIo { source, .. } => Some(source),
            Self::Read { source, .. } => Some(source),
            Self::RootNotDirectory(_) | Self::InvalidPath(_) | Self::OutsideRoot(_) => None,
        }
    }
}

/// Resolves `.langmeld` imports from files under a project directory.
///
/// Imports are interpreted relative to the importing file. Parent-directory
/// segments are allowed when their canonical target remains inside the
/// project root. Absolute paths, platform-specific backslash separators, and
/// symlink escapes are rejected. The root itself must already exist.
pub struct DirectoryDefinitionResolver {
    root: PathBuf,
    sources: Mutex<BTreeMap<String, SourceFile>>,
}

impl DirectoryDefinitionResolver {
    /// Creates a resolver rooted at an existing directory.
    pub fn new(root: impl AsRef<Path>) -> Result<Self, ProjectLoadError> {
        let requested_root = root.as_ref();
        let root = requested_root
            .canonicalize()
            .map_err(|source| ProjectLoadError::RootIo {
                path: requested_root.to_path_buf(),
                source,
            })?;
        if !root.is_dir() {
            return Err(ProjectLoadError::RootNotDirectory(root));
        }

        Ok(Self {
            root,
            sources: Mutex::new(BTreeMap::new()),
        })
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    /// Loads a project-relative root definition file.
    pub fn load(&self, relative_path: &str) -> Result<DefinitionSource, ProjectLoadError> {
        validate_root_key(relative_path)?;
        let path = self.root.join(relative_path);
        self.load_path(path, relative_path.to_owned())
    }

    /// Returns loaded snapshots in stable path order for diagnostic display.
    #[must_use]
    pub fn sources(&self) -> Vec<SourceFile> {
        self.sources
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
            .cloned()
            .collect()
    }

    fn load_path(
        &self,
        requested: PathBuf,
        displayed_path: String,
    ) -> Result<DefinitionSource, ProjectLoadError> {
        let canonical = requested
            .canonicalize()
            .map_err(|source| ProjectLoadError::Read {
                path: requested,
                source,
            })?;
        if !canonical.starts_with(&self.root) {
            return Err(ProjectLoadError::OutsideRoot(displayed_path));
        }
        if !canonical.is_file() {
            return Err(ProjectLoadError::Read {
                path: canonical,
                source: io::Error::new(io::ErrorKind::InvalidInput, "expected a regular file"),
            });
        }
        let key = canonical
            .strip_prefix(&self.root)
            .map_err(|_| ProjectLoadError::OutsideRoot(displayed_path.clone()))?
            .to_str()
            .ok_or_else(|| ProjectLoadError::InvalidPath(displayed_path.clone()))?
            .replace('\\', "/");
        let text = fs::read_to_string(&canonical).map_err(|source| ProjectLoadError::Read {
            path: canonical,
            source,
        })?;
        let source = SourceFile::new(key.clone(), text);
        self.sources
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(key.clone(), source.clone());
        Ok(DefinitionSource::new(key, source))
    }
}

impl DefinitionResolver for DirectoryDefinitionResolver {
    fn resolve(&self, from_key: &str, import: &str) -> Result<DefinitionSource, String> {
        if !is_normal_relative_path(from_key) || !is_relative_import(import) {
            return Err(ProjectLoadError::InvalidPath(import.to_owned()).to_string());
        }
        let importer = self.root.join(from_key).canonicalize().map_err(|error| {
            format!("cannot resolve importing definition `{from_key}`: {error}")
        })?;
        if !importer.starts_with(&self.root) {
            return Err(ProjectLoadError::OutsideRoot(from_key.to_owned()).to_string());
        }
        let requested = importer.parent().unwrap_or(&self.root).join(import);
        self.load_path(requested, import.to_owned())
            .map_err(|error| error.to_string())
    }
}

fn validate_root_key(path: &str) -> Result<(), ProjectLoadError> {
    if is_normal_relative_path(path) {
        Ok(())
    } else {
        Err(ProjectLoadError::InvalidPath(path.to_owned()))
    }
}

fn is_normal_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && !has_windows_drive_prefix(path)
        && Path::new(path)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn is_relative_import(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && !has_windows_drive_prefix(path)
        && Path::new(path).components().all(|component| {
            matches!(
                component,
                Component::Normal(_) | Component::CurDir | Component::ParentDir
            )
        })
}

fn has_windows_drive_prefix(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}
