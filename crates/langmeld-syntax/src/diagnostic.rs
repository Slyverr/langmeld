use text_size::TextRange;

use crate::FileId;

/// How serious a [`Diagnostic`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Severity {
    /// The source is wrong.
    Error,
    /// The source is accepted, but something is likely a mistake.
    Warning,
    /// Information that is not a problem.
    Info,
}

/// One problem found in source text.
///
/// Build one with [`Diagnostic::error`], [`Diagnostic::warning`] or
/// [`Diagnostic::info`], then add detail with [`label`](Diagnostic::label),
/// [`note`](Diagnostic::note) and [`fix`](Diagnostic::fix).
///
/// ```
/// use langmeld_syntax::{Diagnostic, FileId, Fix, Severity, TextEdit, TextRange, TextSize};
///
/// let file = FileId::new(0);
/// let range = TextRange::new(TextSize::new(4), TextSize::new(7));
/// let diagnostic = Diagnostic::error(
///     "CALC-001",
///     file,
///     range,
///     "`let` is a keyword and can't be used as a name",
/// )
/// .note("choose another name")
/// .fix(Fix::new("rename to `value`", vec![TextEdit::new(file, range, "value")]));
///
/// assert_eq!(diagnostic.severity, Severity::Error);
/// assert_eq!(diagnostic.notes, ["choose another name"]);
/// assert_eq!(diagnostic.fixes[0].edits[0].replacement, "value");
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Diagnostic {
    /// Stable identifier of this kind of problem. A code keeps its meaning
    /// between releases.
    pub code: &'static str,
    /// How serious the problem is.
    pub severity: Severity,
    /// The file that `range` is in.
    pub file: FileId,
    /// The bytes the problem is about.
    pub range: TextRange,
    /// What is wrong, in plain language.
    pub message: String,
    /// Other places that help explain the problem.
    pub labels: Vec<Label>,
    /// Extra explanation that is not tied to a place.
    pub notes: Vec<String>,
    /// Edits that would solve the problem.
    pub fixes: Vec<Fix>,
}

impl Diagnostic {
    /// A diagnostic with no labels, notes or fixes.
    pub fn new(
        severity: Severity,
        code: &'static str,
        file: FileId,
        range: TextRange,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            severity,
            file,
            range,
            message: message.into(),
            labels: Vec::new(),
            notes: Vec::new(),
            fixes: Vec::new(),
        }
    }

    /// An [`Severity::Error`] diagnostic.
    pub fn error(
        code: &'static str,
        file: FileId,
        range: TextRange,
        message: impl Into<String>,
    ) -> Self {
        Self::new(Severity::Error, code, file, range, message)
    }

    /// A [`Severity::Warning`] diagnostic.
    pub fn warning(
        code: &'static str,
        file: FileId,
        range: TextRange,
        message: impl Into<String>,
    ) -> Self {
        Self::new(Severity::Warning, code, file, range, message)
    }

    /// A [`Severity::Info`] diagnostic.
    pub fn info(
        code: &'static str,
        file: FileId,
        range: TextRange,
        message: impl Into<String>,
    ) -> Self {
        Self::new(Severity::Info, code, file, range, message)
    }

    /// Adds a related place.
    #[must_use]
    pub fn label(mut self, label: Label) -> Self {
        self.labels.push(label);
        self
    }

    /// Adds an explanation that is not tied to a place.
    #[must_use]
    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    /// Adds a suggested solution.
    #[must_use]
    pub fn fix(mut self, fix: Fix) -> Self {
        self.fixes.push(fix);
        self
    }
}

/// A related place, with a short explanation.
///
/// ```
/// use langmeld_syntax::{FileId, Label, TextRange, TextSize};
///
/// let range = TextRange::new(TextSize::new(8), TextSize::new(9));
/// let label = Label::new(FileId::new(2), range, "first definition is here");
/// assert_eq!(label.file, FileId::new(2));
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Label {
    /// The file that `range` is in.
    pub file: FileId,
    /// The bytes being pointed at.
    pub range: TextRange,
    /// Why this place matters.
    pub message: String,
}

impl Label {
    /// A label for `range` of `file`.
    pub fn new(file: FileId, range: TextRange, message: impl Into<String>) -> Self {
        Self {
            file,
            range,
            message: message.into(),
        }
    }
}

/// A suggested solution: edits that are applied together.
///
/// ```
/// use langmeld_syntax::{FileId, Fix, TextEdit, TextRange, TextSize};
///
/// let range = TextRange::new(TextSize::new(0), TextSize::new(1));
/// let fix = Fix::new("rename to `x2`", vec![TextEdit::new(FileId::new(1), range, "x2")]);
/// assert_eq!(fix.edits.len(), 1);
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Fix {
    /// What applying the fix does, such as ``rename to `x2` ``.
    pub title: String,
    /// The changes to make.
    pub edits: Vec<TextEdit>,
}

impl Fix {
    /// A fix that applies `edits` together.
    pub fn new(title: impl Into<String>, edits: Vec<TextEdit>) -> Self {
        Self {
            title: title.into(),
            edits,
        }
    }
}

/// Replaces one range of one file.
///
/// ```
/// use langmeld_syntax::{FileId, TextEdit, TextRange, TextSize};
///
/// let at = TextSize::new(3);
/// let insert = TextEdit::new(FileId::new(0), TextRange::empty(at), ";");
/// assert!(insert.range.is_empty());
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct TextEdit {
    /// The file to change.
    pub file: FileId,
    /// The bytes to replace. An empty range inserts.
    pub range: TextRange,
    /// The new text. Empty text deletes.
    pub replacement: String,
}

impl TextEdit {
    /// An edit that replaces `range` of `file` with `replacement`.
    pub fn new(file: FileId, range: TextRange, replacement: impl Into<String>) -> Self {
        Self {
            file,
            range,
            replacement: replacement.into(),
        }
    }
}
