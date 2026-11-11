use std::collections::BTreeMap;

use langmeld_core::SourceFile;
use langmeld_language::{DefinitionResolver, DefinitionSource};

use crate::{
    ProjectLoadError,
    definition_paths::{normalize_source_key, resolve_import_key},
    error::path_error,
};

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
}

impl DefinitionResolver for InMemoryDefinitionResolver {
    fn resolve(&self, from_key: &str, import: &str) -> Result<DefinitionSource, String> {
        let key = resolve_import_key(from_key, import)
            .map_err(|error| path_error(import, error).to_string())?;
        self.find_by_key(&key)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| ProjectLoadError::MissingDefinition(key).to_string())
    }
}
