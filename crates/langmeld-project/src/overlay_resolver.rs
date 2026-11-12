use std::{path::Path, sync::Arc};

use langmeld_core::SourceFile;
use langmeld_language::{DefinitionResolver, DefinitionSource};

use crate::{
    DirectoryDefinitionResolver, InMemoryDefinitionResolver, ProjectLoadError,
    definition_paths::resolve_import_key, error::path_error,
};

/// Resolves a language project from disk with an in-memory source overlay.
///
/// Overlay sources take precedence when their project-relative keys match an
/// import. All other imports fall back to the directory resolver, which
/// retains its canonical-root and symlink protections.
pub struct OverlayDefinitionResolver {
    directory: Arc<DirectoryDefinitionResolver>,
    overlay: InMemoryDefinitionResolver,
}

impl OverlayDefinitionResolver {
    /// Creates a resolver for a project directory and bundled or virtual sources.
    pub fn new(
        root: impl AsRef<Path>,
        overlay: InMemoryDefinitionResolver,
    ) -> Result<Self, ProjectLoadError> {
        Ok(Self {
            directory: Arc::new(DirectoryDefinitionResolver::new(root)?),
            overlay,
        })
    }

    /// Returns a resolver that shares the project directory and uses a new
    /// in-memory overlay.
    #[must_use]
    pub fn with_overlay(&self, overlay: InMemoryDefinitionResolver) -> Self {
        Self {
            directory: Arc::clone(&self.directory),
            overlay,
        }
    }

    /// Loads the project root definition, preferring an overlay snapshot.
    pub fn load(&self, relative_path: &str) -> Result<DefinitionSource, ProjectLoadError> {
        match self.overlay.load(relative_path) {
            Ok(source) => Ok(source),
            Err(ProjectLoadError::MissingDefinition(_)) => self.directory.load(relative_path),
            Err(error) => Err(error),
        }
    }

    /// Maps a filesystem path to a project key, including paths that do not
    /// exist yet.
    pub fn key_for_path(&self, path: impl AsRef<Path>) -> Result<String, ProjectLoadError> {
        self.directory.key_for_path(path)
    }

    /// Returns loaded snapshots in stable display-name order, with overlay
    /// snapshots replacing disk sources that share the same project key.
    #[must_use]
    pub fn sources(&self) -> Vec<SourceFile> {
        let mut sources = self.directory.sources_by_key();
        sources.extend(
            self.overlay
                .sources_by_key()
                .iter()
                .map(|(key, source)| (key.clone(), source.clone())),
        );

        let mut sources = sources.into_values().collect::<Vec<_>>();
        sources.sort_by(|left, right| left.name().cmp(right.name()));
        sources
    }
}

impl DefinitionResolver for OverlayDefinitionResolver {
    type Error = ProjectLoadError;

    fn resolve(&self, from_key: &str, import: &str) -> Result<DefinitionSource, Self::Error> {
        let key =
            resolve_import_key(from_key, import).map_err(|error| path_error(import, error))?;
        if let Some(source) = self.overlay.find_by_key(&key)? {
            return Ok(source);
        }
        self.directory.resolve(from_key, import)
    }
}
