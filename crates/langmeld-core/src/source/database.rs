use std::{collections::HashMap, sync::Arc};

use super::{SourceFile, SourceId, SourceIdentity};

/// An error while inserting or updating a source snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceDatabaseError {
    /// A display name is already assigned to another source.
    DuplicateName(String),
    /// The identity does not exist in the database.
    UnknownSource(SourceId),
    /// No more identities can be allocated.
    IdExhausted,
    /// The source revision cannot be advanced.
    RevisionExhausted(SourceId),
}

impl std::fmt::Display for SourceDatabaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateName(name) => write!(f, "source `{name}` already exists"),
            Self::UnknownSource(id) => write!(f, "unknown source ID {}", id.get()),
            Self::IdExhausted => f.write_str("source ID space exhausted"),
            Self::RevisionExhausted(id) => {
                write!(f, "revision counter exhausted for source ID {}", id.get())
            }
        }
    }
}

impl std::error::Error for SourceDatabaseError {}

/// Stores source snapshots with stable IDs and monotonically increasing revisions.
///
/// Cloning a database creates a distinct namespace identity while preserving
/// each source's local ID and revision.
#[derive(Debug)]
pub struct SourceDatabase {
    sources: HashMap<SourceId, SourceFile>,
    ids_by_name: HashMap<String, SourceId>,
    next_id: u64,
    namespace_id: u64,
}

impl Default for SourceDatabase {
    fn default() -> Self {
        Self {
            sources: HashMap::new(),
            ids_by_name: HashMap::new(),
            next_id: 0,
            namespace_id: super::allocate_source_namespace_id(),
        }
    }
}

impl Clone for SourceDatabase {
    fn clone(&self) -> Self {
        let namespace_id = super::allocate_source_namespace_id();
        let mut sources = self.sources.clone();
        for source in sources.values_mut() {
            source.identity = SourceIdentity {
                namespace_id,
                ..source.identity
            };
        }
        Self {
            sources,
            ids_by_name: self.ids_by_name.clone(),
            next_id: self.next_id,
            namespace_id,
        }
    }
}

impl SourceDatabase {
    /// Inserts a new named source at revision 1.
    ///
    /// # Errors
    ///
    /// Returns [`SourceDatabaseError::DuplicateName`] when the display name is
    /// already present, or [`SourceDatabaseError::IdExhausted`] when no source
    /// IDs remain.
    pub fn insert(
        &mut self,
        name: impl Into<String>,
        text: impl Into<String>,
    ) -> Result<SourceId, SourceDatabaseError> {
        let name = name.into();
        if self.ids_by_name.contains_key(&name) {
            return Err(SourceDatabaseError::DuplicateName(name));
        }
        let id_value = if self.next_id == 0 { 1 } else { self.next_id };
        let following_id = id_value
            .checked_add(1)
            .ok_or(SourceDatabaseError::IdExhausted)?;
        let id = SourceId(id_value);
        let source = SourceFile::with_identity(
            name.clone(),
            text,
            SourceIdentity {
                namespace_id: self.namespace_id,
                id,
                revision: 1,
            },
            true,
        );
        self.sources.insert(id, source);
        self.ids_by_name.insert(name, id);
        self.next_id = following_id;
        Ok(id)
    }

    /// Inserts a source or updates the snapshot with the same display name.
    /// An unchanged text leaves the revision unchanged.
    ///
    /// # Errors
    ///
    /// Returns [`SourceDatabaseError::IdExhausted`] when inserting a new source
    /// would exceed the ID space, or [`SourceDatabaseError::RevisionExhausted`]
    /// when updating an existing source would exceed its revision space.
    pub fn upsert(
        &mut self,
        name: impl Into<String>,
        text: impl Into<String>,
    ) -> Result<SourceId, SourceDatabaseError> {
        let name = name.into();
        let text = text.into();
        if let Some(&id) = self.ids_by_name.get(&name) {
            self.update(id, text)?;
            Ok(id)
        } else {
            self.insert(name, text)
        }
    }

    /// Replaces a source's text and returns its revision.
    ///
    /// # Errors
    ///
    /// Returns [`SourceDatabaseError::UnknownSource`] when `id` is not in this
    /// database, or [`SourceDatabaseError::RevisionExhausted`] when the source
    /// revision cannot be advanced.
    pub fn update(
        &mut self,
        id: SourceId,
        text: impl Into<String>,
    ) -> Result<u64, SourceDatabaseError> {
        let text = text.into();
        let source = self
            .sources
            .get_mut(&id)
            .ok_or(SourceDatabaseError::UnknownSource(id))?;
        if source.text.as_str() != text {
            let identity = source.identity;
            let revision = identity
                .revision
                .checked_add(1)
                .ok_or(SourceDatabaseError::RevisionExhausted(id))?;
            source.text = Arc::new(text);
            source.identity = SourceIdentity {
                revision,
                ..identity
            };
        }
        Ok(source.identity.revision)
    }

    /// Removes a source and returns its most recent snapshot.
    pub fn remove(&mut self, id: SourceId) -> Option<SourceFile> {
        let source = self.sources.remove(&id)?;
        self.ids_by_name.remove(&source.name);
        Some(source)
    }

    /// Returns a source snapshot by stable identity.
    #[must_use]
    pub fn get(&self, id: SourceId) -> Option<&SourceFile> {
        self.sources.get(&id)
    }

    /// Finds a source identity by display name.
    #[must_use]
    pub fn id_by_name(&self, name: &str) -> Option<SourceId> {
        self.ids_by_name.get(name).copied()
    }

    /// Returns a source's current revision, starting at 1.
    #[must_use]
    pub fn revision(&self, id: SourceId) -> Option<u64> {
        self.sources.get(&id).map(|source| source.identity.revision)
    }
}
