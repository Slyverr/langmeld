use std::{collections::HashSet, path::Path};

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
    directory: DirectoryDefinitionResolver,
    overlay: InMemoryDefinitionResolver,
}

impl OverlayDefinitionResolver {
    /// Creates a resolver for a project directory and bundled or virtual sources.
    pub fn new(
        root: impl AsRef<Path>,
        overlay: InMemoryDefinitionResolver,
    ) -> Result<Self, ProjectLoadError> {
        Ok(Self {
            directory: DirectoryDefinitionResolver::new(root)?,
            overlay,
        })
    }

    /// Loads the project root definition from disk.
    pub fn load(&self, relative_path: &str) -> Result<DefinitionSource, ProjectLoadError> {
        self.directory.load(relative_path)
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
