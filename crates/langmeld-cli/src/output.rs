//! Writing to the terminal. A closed pipe ends output quietly.

use std::io::Write;

use langmeld_syntax::{Diagnostic, SourceFile};

/// Writes a line to the output.
pub fn line(text: &str) {
    let _ = writeln!(std::io::stdout().lock(), "{text}");
}

/// Writes a line to the error output.
pub fn error_line(text: &str) {
    let _ = writeln!(std::io::stderr().lock(), "{text}");
}

/// Shows diagnostics with the source they are about. `files` holds the files by the
/// number of their [`FileId`](langmeld_syntax::FileId).
pub fn report(files: &[SourceFile], diagnostics: &[Diagnostic]) {
    let _ = write!(
        std::io::stderr().lock(),
        "{}",
        langmeld_meta::render(files, diagnostics)
    );
}
