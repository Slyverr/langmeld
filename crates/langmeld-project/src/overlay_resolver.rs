use std::{collections::HashSet, path::Path, sync::Arc};

use langmeld_core::SourceFile;
use langmeld_language::{DefinitionResolver, DefinitionSource};

use crate::{
    DirectoryDefinitionResolver, InMemoryDefinitionResolver, ProjectLoadError,
    definition_paths::resolve_import_key,
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

    /// Returns loaded disk sources and all overlay sources in stable name order.
    #[must_use]
    pub fn sources(&self) -> Vec<SourceFile> {
        let mut overlay_sources = self.overlay.sources();
        let overlay_names = overlay_sources
            .iter()
            .map(|source| source.name())
            .collect::<HashSet<_>>();
        let mut sources = self
            .directory
            .sources()
            .into_iter()
            .filter(|source| !overlay_names.contains(source.name()))
            .collect::<Vec<_>>();
        sources.append(&mut overlay_sources);
        sources.sort_by(|left, right| left.name().cmp(right.name()));
        sources
    }
}

impl DefinitionResolver for OverlayDefinitionResolver {
    fn resolve(&self, from_key: &str, import: &str) -> Result<DefinitionSource, String> {
        let key = resolve_import_key(from_key, import)
            .map_err(|error| crate::path_error(import, error).to_string())?;
        if let Some(source) = self
            .overlay
            .find_by_key(&key)
            .map_err(|error| error.to_string())?
        {
            return Ok(source);
        }
        self.directory.resolve(from_key, import)
    }
}
