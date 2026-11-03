//! Source snapshots, spans, and stable source database identities.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

/// A byte range in a source file, with an exclusive end offset.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct Span {
    /// First byte in the range.
    pub start: usize,
    /// First byte after the range.
    pub end: usize,
}

impl Span {
    /// Creates a source range from byte offsets.
    #[must_use]
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// An in-memory source snapshot with a display name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceFile {
    /// Name shown in diagnostics. The core does not interpret this as a path.
    pub name: String,
    /// Source contents.
    pub text: String,
}

impl SourceFile {
    /// Creates a source snapshot.
    #[must_use]
    pub fn new(name: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            text: text.into(),
        }
    }

    /// Converts a byte offset to a one-based line and Unicode-scalar column.
    ///
    /// Offsets beyond the source are clamped to its end. An offset inside a
    /// multibyte UTF-8 character is clamped to that character's first byte.
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

/// Stable identity for a source managed by one [`SourceDatabase`].
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct SourceId(u64);

impl SourceId {
    /// Returns the numeric identity value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Failure returned by source database mutations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceDatabaseError {
    /// A source with this display name is already present.
    DuplicateName(String),
    /// The requested source ID is not present in the database.
    UnknownSource(SourceId),
    /// No further source IDs can be allocated.
    IdExhausted,
    /// The source revision counter cannot be incremented.
    RevisionExhausted(SourceId),
}

impl std::fmt::Display for SourceDatabaseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateName(name) => write!(formatter, "source `{name}` already exists"),
            Self::UnknownSource(id) => write!(formatter, "unknown source ID {}", id.get()),
            Self::IdExhausted => formatter.write_str("source ID space exhausted"),
            Self::RevisionExhausted(id) => {
                write!(
                    formatter,
                    "revision counter exhausted for source ID {}",
                    id.get()
                )
            }
        }
    }
}

impl std::error::Error for SourceDatabaseError {}

#[derive(Clone, Debug)]
struct SourceRecord {
    source: SourceFile,
    revision: u64,
    line_starts: Vec<usize>,
}

/// Owns source snapshots, stable identities, revisions, and line-start indexes.
#[derive(Clone, Debug, Default)]
pub struct SourceDatabase {
    records: BTreeMap<SourceId, SourceRecord>,
    ids_by_name: HashMap<String, SourceId>,
    next_id: u64,
}

impl SourceDatabase {
    /// Inserts a new source with revision 1 and returns its stable identity.
    pub fn insert(
        &mut self,
        name: impl Into<String>,
        text: impl Into<String>,
    ) -> Result<SourceId, SourceDatabaseError> {
        let name = name.into();
        if self.ids_by_name.contains_key(&name) {
            return Err(SourceDatabaseError::DuplicateName(name));
        }
        let next_id = if self.next_id == 0 { 1 } else { self.next_id };
        let following_id = next_id
            .checked_add(1)
            .ok_or(SourceDatabaseError::IdExhausted)?;
        let id = SourceId(next_id);
        let source = SourceFile::new(name.clone(), text);
        self.records.insert(
            id,
            SourceRecord {
                line_starts: line_starts(&source.text),
                source,
                revision: 1,
            },
        );
        self.ids_by_name.insert(name, id);
        self.next_id = following_id;
        Ok(id)
    }

    /// Inserts a source or updates the existing source with the same display name.
    ///
    /// Repeated identical text does not advance its revision.
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

    /// Replaces source text and returns its current revision.
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
            record.revision = record
                .revision
                .checked_add(1)
                .ok_or(SourceDatabaseError::RevisionExhausted(id))?;
            record.line_starts = line_starts(&text);
            record.source.text = text;
        }
        Ok(record.revision)
    }

    /// Removes a source and returns its latest snapshot.
    pub fn remove(&mut self, id: SourceId) -> Option<SourceFile> {
        let record = self.records.remove(&id)?;
        self.ids_by_name.remove(&record.source.name);
        Some(record.source)
    }

    /// Looks up a source by stable identity.
    #[must_use]
    pub fn get(&self, id: SourceId) -> Option<&SourceFile> {
        self.records.get(&id).map(|record| &record.source)
    }

    /// Looks up a source identity by display name.
    #[must_use]
    pub fn id_by_name(&self, name: &str) -> Option<SourceId> {
        self.ids_by_name.get(name).copied()
    }

    /// Returns the current revision, starting at 1 when inserted.
    #[must_use]
    pub fn revision(&self, id: SourceId) -> Option<u64> {
        self.records.get(&id).map(|record| record.revision)
    }

    /// Converts a UTF-8 byte offset to a one-based line and Unicode-scalar column.
    #[must_use]
    pub fn line_column(&self, id: SourceId, offset: usize) -> Option<(usize, usize)> {
        let record = self.records.get(&id)?;
        let mut offset = offset.min(record.source.text.len());
        while !record.source.text.is_char_boundary(offset) {
            offset -= 1;
        }
        let line = record.line_starts.partition_point(|start| *start <= offset) - 1;
        let column = record.source.text[record.line_starts[line]..offset]
            .chars()
            .count()
            + 1;
        Some((line + 1, column))
    }

    /// Returns the number of sources in the database.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Returns whether the database contains no sources.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    starts.extend(
        text.bytes()
            .enumerate()
            .filter_map(|(index, byte)| (byte == b'\n').then_some(index + 1)),
    );
    starts
}
