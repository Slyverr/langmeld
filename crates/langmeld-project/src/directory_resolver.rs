use std::{
    collections::BTreeMap,
    fs, io,
    path::{Component, Path, PathBuf},
    sync::Mutex,
};

use langmeld_core::SourceFile;
use langmeld_language::{DefinitionResolver, DefinitionSource};

use crate::{
    ProjectLoadError,
    definition_paths::{normalize_source_key, resolve_import_key},
    error::path_error,
};

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
        let key = normalize_source_key(relative_path)
            .map_err(|_| ProjectLoadError::InvalidPath(relative_path.to_owned()))?;
        let path = self.root.join(key);
        self.load_path(path, relative_path.to_owned())
    }

    /// Maps an absolute or project-relative filesystem path to a project key.
    ///
    /// The file does not need to exist yet, which allows editor buffers for
    /// newly created definitions to participate in project analysis. Existing
    /// path components are canonicalized so symlinks outside the project root
    /// are rejected.
    pub fn key_for_path(&self, path: impl AsRef<Path>) -> Result<String, ProjectLoadError> {
        let requested = path.as_ref();
        let absolute = if requested.is_absolute() {
            requested.to_path_buf()
        } else {
            self.root.join(requested)
        };
        let mut ancestor = absolute.as_path();
        let mut missing_components = Vec::new();

        loop {
            match ancestor.canonicalize() {
                Ok(canonical) => {
                    if !canonical.starts_with(&self.root) {
                        return Err(ProjectLoadError::OutsideRoot(
                            requested.display().to_string(),
                        ));
                    }

                    let mut relative = canonical
                        .strip_prefix(&self.root)
                        .map_err(|_| {
                            ProjectLoadError::OutsideRoot(requested.display().to_string())
                        })?
                        .to_path_buf();
                    for component in missing_components.iter().rev() {
                        relative.push(component);
                    }

                    let key = relative
                        .to_str()
                        .ok_or_else(|| {
                            ProjectLoadError::InvalidPath(requested.display().to_string())
                        })?
                        .replace('\\', "/");
                    return normalize_source_key(&key)
                        .map_err(|_| ProjectLoadError::InvalidPath(key));
                }
                Err(source) => match fs::symlink_metadata(ancestor) {
                    Ok(_) => {
                        return Err(ProjectLoadError::Read {
                            path: ancestor.to_path_buf(),
                            source,
                        });
                    }
                    Err(metadata_error) if metadata_error.kind() == io::ErrorKind::NotFound => {
                        let Some(component) = ancestor.file_name() else {
                            return Err(ProjectLoadError::InvalidPath(
                                requested.display().to_string(),
                            ));
                        };
                        if !matches!(
                            Path::new(component).components().next(),
                            Some(Component::Normal(_))
                        ) {
                            return Err(ProjectLoadError::InvalidPath(
                                requested.display().to_string(),
                            ));
                        }
                        missing_components.push(component.to_os_string());
                        let Some(parent) = ancestor.parent() else {
                            return Err(ProjectLoadError::InvalidPath(
                                requested.display().to_string(),
                            ));
                        };
                        ancestor = parent;
                    }
                    Err(metadata_error) => {
                        return Err(ProjectLoadError::Read {
                            path: ancestor.to_path_buf(),
                            source: metadata_error,
                        });
                    }
                },
            }
        }
    }

    /// Returns loaded snapshots in stable path order for diagnostic display.
    #[must_use]
    pub fn sources(&self) -> Vec<SourceFile> {
        self.sources_by_key().into_values().collect()
    }

    pub(super) fn sources_by_key(&self) -> BTreeMap<String, SourceFile> {
        self.sources
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
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
        let key = resolve_import_key(from_key, import)
            .map_err(|error| path_error(import, error).to_string())?;
        let requested = self.root.join(key);
        self.load_path(requested, import.to_owned())
            .map_err(|error| error.to_string())
    }
}
