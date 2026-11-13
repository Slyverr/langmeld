//! Source snapshots and stable identities.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
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
    name: String,
    /// Immutable UTF-8 source text shared by cloned snapshots.
    text: Arc<String>,
    identity: SourceIdentity,
    database_managed: bool,
}

static NEXT_SOURCE_NAMESPACE_ID: AtomicU64 = AtomicU64::new(1);

fn allocate_source_namespace_id() -> u64 {
    let mut current = NEXT_SOURCE_NAMESPACE_ID.load(Ordering::Relaxed);
    loop {
        let next = current
            .checked_add(1)
            .expect("source namespace identity space exhausted");
        match NEXT_SOURCE_NAMESPACE_ID.compare_exchange_weak(
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

impl SourceFile {
    /// Creates a standalone source snapshot with a unique process-local identity.
    #[must_use]
    pub fn new(name: impl Into<String>, text: impl Into<String>) -> Self {
        Self::with_identity(
            name,
            text,
            SourceIdentity {
                namespace_id: allocate_source_namespace_id(),
                id: SourceId(1),
                revision: 1,
            },
            false,
        )
    }

    fn with_identity(
        name: impl Into<String>,
        text: impl Into<String>,
        identity: SourceIdentity,
        database_managed: bool,
    ) -> Self {
        Self {
            name: name.into(),
            text: Arc::new(text.into()),
            identity,
            database_managed,
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

    /// Returns whether `offset` is an in-bounds UTF-8 character boundary in
    /// this snapshot. The end of the source is a valid offset.
    #[must_use]
    pub fn is_valid_offset(&self, offset: usize) -> bool {
        self.text.is_char_boundary(offset)
    }

    /// Returns the text selected by `span` in this snapshot, if it is an
    /// ordered, in-bounds UTF-8 byte range.
    #[must_use]
    pub fn slice(&self, span: Span) -> Option<&str> {
        self.text.get(span.start..span.end)
    }

    /// Returns whether `span` is an ordered, in-bounds UTF-8 byte range in this
    /// snapshot. Empty spans are valid when their offset is a character boundary.
    #[must_use]
    pub fn is_valid_span(&self, span: Span) -> bool {
        self.slice(span).is_some()
    }

    /// Returns this snapshot's stable identity and revision.
    #[must_use]
    pub const fn identity(&self) -> SourceIdentity {
        self.identity
    }

    /// Returns whether this snapshot's identity was allocated by a [`SourceDatabase`].
    #[must_use]
    pub const fn is_database_managed(&self) -> bool {
        self.database_managed
    }

    /// Builds a line index for converting between byte offsets and encoded
    /// line/character positions in this immutable snapshot.
    #[must_use]
    pub fn line_index(&self) -> LineIndex<'_> {
        LineIndex::new(self)
    }

    /// Converts a byte offset to one-based line and Unicode-scalar column.
    /// Lines are split on LF, and a CR before LF is excluded from the column.
    /// Offsets beyond the source are clamped to its end.
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
                let is_crlf_carriage_return =
                    character == '\r' && self.text.as_bytes().get(byte + 1) == Some(&b'\n');

                if !is_crlf_carriage_return {
                    column += 1;
                }
            }
        }
        (line, column)
    }
}

/// Stable identity for a source snapshot within a process-local namespace.
/// Database-managed sources share their database's namespace; each standalone
/// source receives a unique namespace.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SourceId(u64);

impl SourceId {
    /// Returns the numeric identity.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Identity and revision of one source snapshot.
///
/// These values are process-local validation tokens and are not persistent
/// identifiers for files or projects.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SourceIdentity {
    /// Process-local identity of the source namespace.
    namespace_id: u64,
    /// Stable source ID within its namespace.
    id: SourceId,
    /// Revision of the source text, starting at 1.
    revision: u64,
}

impl SourceIdentity {
    /// Returns the process-local identity of the source namespace.
    ///
    /// Database-managed sources share their database's namespace; a standalone
    /// source has its own namespace.
    #[must_use]
    pub const fn namespace_id(&self) -> u64 {
        self.namespace_id
    }

    /// Returns the stable source ID within the namespace.
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
