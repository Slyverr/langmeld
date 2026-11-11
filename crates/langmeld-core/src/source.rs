//! Source snapshots and stable identities.

use std::sync::Arc;

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
    name: String,
    /// Immutable UTF-8 source text shared by cloned snapshots.
    text: Arc<String>,
    identity: Option<SourceIdentity>,
}

impl SourceFile {
    /// Creates a source snapshot.
    #[must_use]
    pub fn new(name: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            text: Arc::new(text.into()),
            identity: None,
        }
    }

    /// Returns the display name used by diagnostics.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the UTF-8 text in this immutable source snapshot.
    #[must_use]
    pub fn text(&self) -> &str {
        self.text.as_str()
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
    database_id: u64,
    /// Stable source ID within its database.
    id: SourceId,
    /// Revision of the source text, starting at 1.
    revision: u64,
}

impl SourceIdentity {
    /// Returns the process-local identity of the owning database.
    #[must_use]
    pub const fn database_id(&self) -> u64 {
        self.database_id
    }

    /// Returns the stable source ID within the owning database.
    #[must_use]
    pub const fn id(&self) -> SourceId {
        self.id
    }

    /// Returns the revision of the source text, starting at 1.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
}

mod database;
pub use database::{SourceDatabase, SourceDatabaseError};
