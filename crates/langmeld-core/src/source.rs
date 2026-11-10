//! Source snapshots and stable identities.

use std::{
    collections::HashMap,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::LineIndex;

/// A UTF-8 byte range in a source snapshot. The end offset is exclusive.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Span {
    /// First byte in the range.
    pub start: usize,
    /// First byte after the range.
    pub end: usize,
}

impl Span {
    /// Creates a byte range. Callers should ensure `start <= end` and that
    /// both offsets are UTF-8 character boundaries in the associated source.
    #[must_use]
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Returns the number of bytes in this range.
    #[must_use]
    pub const fn len(self) -> usize {
        self.end.saturating_sub(self.start)
    }

    /// Returns whether this range is empty.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start >= self.end
    }
}

/// An immutable named source snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceFile {
    /// Display name used by diagnostics. It is not interpreted as a path.
    pub name: String,
    /// UTF-8 source text.
    pub text: String,
    identity: Option<SourceIdentity>,
}

impl SourceFile {
    /// Creates a source snapshot.
    #[must_use]
    pub fn new(name: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            text: text.into(),
            identity: None,
        }
    }

    /// Returns this snapshot's stable database identity and revision, if it is
    /// managed by a [`SourceDatabase`]. Detached snapshots have no identity.
    #[must_use]
    pub const fn identity(&self) -> Option<SourceIdentity> {
        self.identity
    }

    /// Builds a line index for converting between byte offsets and encoded
    /// line/character positions in this immutable snapshot.
    #[must_use]
    pub fn line_index(&self) -> LineIndex<'_> {
        LineIndex::new(self)
    }

    /// Converts a byte offset to one-based line and Unicode-scalar column.
    /// Offsets inside a UTF-8 character are clamped to that character's start.
    #[must_use]
    pub fn line_column(&self, offset: usize) -> (usize, usize) {
        let mut offset = offset.min(self.text.len());
        while !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        let mut line = 1;
        let mut column = 1;
        for (byte, character) in self.text.char_indices() {
            if byte >= offset {
                break;
            }
            if character == '\n' {
                line += 1;
                column = 1;
            } else {
                column += 1;
            }
        }
        (line, column)
    }
}

/// Stable identity for a source in one [`SourceDatabase`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SourceId(u64);

impl SourceId {
    /// Returns the numeric identity.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Identity and revision of one managed source snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SourceIdentity {
    /// Process-local identity of the database that owns this source.
    pub database_id: u64,
    /// Stable source ID within its database.
    pub id: SourceId,
    /// Revision of the source text, starting at 1.
    pub revision: u64,
}

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

#[derive(Clone, Debug)]
struct SourceRecord {
    source: SourceFile,
}

/// Stores source snapshots with stable IDs and monotonically increasing revisions.
///
/// Cloning a database creates a distinct database identity while preserving
/// each source's local ID and revision.
#[derive(Debug)]
pub struct SourceDatabase {
    records: HashMap<SourceId, SourceRecord>,
    ids_by_name: HashMap<String, SourceId>,
    next_id: u64,
    database_id: u64,
}

static NEXT_DATABASE_ID: AtomicU64 = AtomicU64::new(1);

fn allocate_database_id() -> u64 {
    let mut current = NEXT_DATABASE_ID.load(Ordering::Relaxed);
    loop {
        let next = current
            .checked_add(1)
            .expect("source database identity space exhausted");
        match NEXT_DATABASE_ID.compare_exchange_weak(
            current,
            next,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return current,
            Err(observed) => current = observed,
        }
    }
}

impl Default for SourceDatabase {
    fn default() -> Self {
        Self {
            records: HashMap::new(),
            ids_by_name: HashMap::new(),
            next_id: 0,
            database_id: allocate_database_id(),
        }
    }
}

impl Clone for SourceDatabase {
    fn clone(&self) -> Self {
        let database_id = allocate_database_id();
        let mut records = self.records.clone();
        for record in records.values_mut() {
            let identity = record
                .source
                .identity
                .expect("database sources carry an identity");
            record.source.identity = Some(SourceIdentity {
                database_id,
                ..identity
            });
        }
        Self {
            records,
            ids_by_name: self.ids_by_name.clone(),
            next_id: self.next_id,
            database_id,
        }
    }
}

impl SourceDatabase {
    /// Inserts a new named source at revision 1.
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
        let mut source = SourceFile::new(name.clone(), text);
        source.identity = Some(SourceIdentity {
            database_id: self.database_id,
            id,
            revision: 1,
        });
        self.records.insert(id, SourceRecord { source });
        self.ids_by_name.insert(name, id);
        self.next_id = following_id;
        Ok(id)
    }

    /// Inserts a source or updates the snapshot with the same display name.
    /// An unchanged text leaves the revision unchanged.
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
    pub fn update(
        &mut self,
        id: SourceId,
        text: impl Into<String>,
    ) -> Result<u64, SourceDatabaseError> {
        let text = text.into();
        let record = self
            .records
            .get_mut(&id)
            .ok_or(SourceDatabaseError::UnknownSource(id))?;
        if record.source.text != text {
            let identity = record
                .source
                .identity
                .expect("database sources carry an identity");
            let revision = identity
                .revision
                .checked_add(1)
                .ok_or(SourceDatabaseError::RevisionExhausted(id))?;
            record.source.text = text;
            record.source.identity = Some(SourceIdentity {
                revision,
                ..identity
            });
        }
        Ok(record
            .source
            .identity
            .expect("database sources carry an identity")
            .revision)
    }

    /// Removes a source and returns its most recent snapshot.
    pub fn remove(&mut self, id: SourceId) -> Option<SourceFile> {
        let record = self.records.remove(&id)?;
        self.ids_by_name.remove(&record.source.name);
        Some(record.source)
    }

    /// Returns a source snapshot by stable identity.
    #[must_use]
    pub fn get(&self, id: SourceId) -> Option<&SourceFile> {
        self.records.get(&id).map(|r| &r.source)
    }

    /// Finds a source identity by display name.
    #[must_use]
    pub fn id_by_name(&self, name: &str) -> Option<SourceId> {
        self.ids_by_name.get(name).copied()
    }

    /// Returns a source's current revision, starting at 1.
    #[must_use]
    pub fn revision(&self, id: SourceId) -> Option<u64> {
        self.records
            .get(&id)
            .and_then(|r| r.source.identity)
            .map(|identity| identity.revision)
    }
}
