use std::sync::Arc;

use crate::{LineIndex, TextTooLarge};

/// Identifies one source file.
///
/// Whoever loads files assigns the numbers; this crate only compares them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileId(u32);

impl FileId {
    /// The identifier with the given number.
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    /// The number this identifier was made from.
    pub const fn index(self) -> u32 {
        self.0
    }
}

/// The text of one source file, together with its identity.
///
/// The text is kept byte for byte, so every offset into it stays valid. Cloning
/// shares the text.
///
/// ```
/// use langmeld_syntax::{FileId, SourceFile};
///
/// let file = SourceFile::new(FileId::new(0), "main.calc", "let x = 1;\n")?;
/// assert_eq!(file.name(), "main.calc");
/// assert_eq!(file.line_index().line_count(), 2);
/// # Ok::<(), langmeld_syntax::TextTooLarge>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceFile {
    id: FileId,
    name: Arc<str>,
    text: Arc<str>,
    line_index: Arc<LineIndex>,
}

impl SourceFile {
    /// Creates a file. `name` is shown to people; it is never opened or parsed.
    ///
    /// Fails if the text is too large to address.
    pub fn new(
        id: FileId,
        name: impl Into<Arc<str>>,
        text: impl Into<Arc<str>>,
    ) -> Result<Self, TextTooLarge> {
        let text = text.into();
        let line_index = Arc::new(LineIndex::new(&text)?);
        Ok(Self {
            id,
            name: name.into(),
            text,
            line_index,
        })
    }

    /// The identifier of this file.
    pub fn id(&self) -> FileId {
        self.id
    }

    /// The name shown to people.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The whole text.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Line and column conversions for this text.
    pub fn line_index(&self) -> &LineIndex {
        &self.line_index
    }
}
