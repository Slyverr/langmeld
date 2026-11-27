//! Problems in files, told the way every diagnostic of the format is.

use std::fmt::Display;

use langmeld_syntax::{Diagnostic, FileId, TextRange};

/// A problem in a `.langmeld` file, with the note that says how to solve it.
pub(crate) fn problem(
    code: &'static str,
    file: FileId,
    range: TextRange,
    headline: impl Into<String>,
    fix: impl Display,
) -> Diagnostic {
    Diagnostic::error(code, file, range, headline).note(format!("To fix this, {fix}."))
}
