use std::collections::BTreeMap;

use langmeld_core::SourceFile;
use langmeld_language::{DefinitionResolver, DefinitionSource};

use crate::{ProjectLoadError, definition_paths::normalize_source_key, resolve_import_key};

/// Resolves imports from an in-memory collection of `.langmeld` sources.
///
/// Use this for embedded definitions, virtual projects, or editor-owned source
/// snapshots. Keys are portable project-relative paths; imports are resolved
/// against the importing source's key. Inserting an existing key replaces its
/// source, which supports updating editor snapshots between analyses.
#[derive(Clone, Debug, Default)]
pub struct InMemoryDefinitionResolver {
    sources: BTreeMap<String, SourceFile>,
}

impl InMemoryDefinitionResolver {
    /// Creates an empty in-memory source collection.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds or replaces a source at a portable project-relative path.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectLoadError::InvalidPath`] when `key` is not a valid
    /// portable project-relative path.
    pub fn insert(
        &mut self,
        key: &str,
        source: SourceFile,
    ) -> Result<Option<SourceFile>, ProjectLoadError> {
        let key =
            normalize_source_key(key).map_err(|_| ProjectLoadError::InvalidPath(key.to_owned()))?;
        Ok(self.sources.insert(key, source))
    }

    /// Loads an in-memory root definition by project-relative path.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectLoadError::InvalidPath`] when `key` is invalid, or
    /// [`ProjectLoadError::MissingDefinition`] when no source is stored there.
    pub fn load(&self, key: &str) -> Result<DefinitionSource, ProjectLoadError> {
        self.find_by_key(key)?
            .ok_or_else(|| ProjectLoadError::MissingDefinition(key.to_owned()))
    }

    pub(super) fn find_by_key(
        &self,
        key: &str,
    ) -> Result<Option<DefinitionSource>, ProjectLoadError> {
        let key =
            normalize_source_key(key).map_err(|_| ProjectLoadError::InvalidPath(key.to_owned()))?;
        let source = self
            .sources
            .get(&key)
            .cloned()
            .map(|source| DefinitionSource::new(key, source));
        Ok(source)
    }

    /// Returns source snapshots in stable project-relative path order.
    #[must_use]
    pub fn sources(&self) -> Vec<SourceFile> {
        self.sources.values().cloned().collect()
    }

    pub(super) fn sources_by_key(&self) -> &BTreeMap<String, SourceFile> {
        &self.sources
    }
}

impl DefinitionResolver for InMemoryDefinitionResolver {
    type Error = ProjectLoadError;

    fn resolve(&self, from_key: &str, import: &str) -> Result<DefinitionSource, Self::Error> {
        let key = resolve_import_key(from_key, import)?;
        self.find_by_key(&key)?
            .ok_or(ProjectLoadError::MissingDefinition(key))
    }
}
