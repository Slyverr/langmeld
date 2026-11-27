//! Diagnostics as text, with the line of the file they point at.

use std::fmt::Write;

use langmeld_syntax::{Diagnostic, Severity, SourceFile};

/// Where a byte offset is: the one-based line and column, and the text of the line.
fn find(text: &str, offset: usize) -> (usize, usize, &str, usize) {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    let start = text[..offset].rfind('\n').map_or(0, |at| at + 1);
    let end = text[offset..]
        .find('\n')
        .map_or(text.len(), |at| offset + at);
    let line = text[start..end].trim_end_matches('\r');
    let number = text[..start].matches('\n').count() + 1;
    let column = text[start..offset].chars().count();
    (number, column, line, offset - start)
}

/// `file:line:column` of a place in a file.
pub fn place(file: &SourceFile, offset: usize) -> String {
    let (line, column, _, _) = find(file.text(), offset);
    format!("{}:{line}:{}", file.name(), column + 1)
}

/// Writes each diagnostic as a headline, the place and line it points at, and its
/// notes, with a blank line between diagnostics.
pub fn render(files: &[SourceFile], diagnostics: &[Diagnostic]) -> String {
    let mut out = String::new();
    for diagnostic in diagnostics {
        let kind = match diagnostic.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            _ => "info",
        };
        writeln!(out, "{kind}[{}]: {}", diagnostic.code, diagnostic.message)
            .expect("writes to a string");
        let file = files.iter().find(|file| file.id() == diagnostic.file);
        let mut width = 1;
        if let Some(file) = file {
            let start = usize::try_from(u32::from(diagnostic.range.start())).unwrap_or(0);
            let end = usize::try_from(u32::from(diagnostic.range.end())).unwrap_or(start);
            let (number, column, line, byte) = find(file.text(), start);
            width = number.to_string().len();
            let shown = line.replace('\t', " ");
            let reach = end
                .saturating_sub(start)
                .min(line.len().saturating_sub(byte));
            let carets = line
                .get(byte..byte + reach)
                .map_or(1, |covered| covered.chars().count())
                .max(1);
            let gutter = " ".repeat(width);
            writeln!(
                out,
                "{gutter}--> {}:{number}:{}\n{gutter} |\n{number} | {shown}\n{gutter} | {}{}",
                file.name(),
                column + 1,
                " ".repeat(column),
                "^".repeat(carets)
            )
            .expect("writes to a string");
        }
        for note in &diagnostic.notes {
            writeln!(out, "{} = note: {note}", " ".repeat(width)).expect("writes to a string");
        }
        out.push('\n');
    }
    out
}
