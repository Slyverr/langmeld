//! Checking the files of a language.

use std::path::PathBuf;

use langmeld_ide::Served;
use langmeld_ide::bind::{Project, Resolver};
use langmeld_ide::project::DiskFiles;
use langmeld_syntax::{Diagnostic, Diagnostics, FileId, Parse, Severity, SourceFile, TextRange};
use serde_json::{Value, json};

use crate::{Failure, files_in, output, read};

/// How problems are written.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Format {
    /// For people, with the source.
    #[default]
    Text,
    /// For programs: one JSON object on the output.
    Json,
}

/// A place as people count it: lines and characters from one.
fn position(file: &SourceFile, offset: u32) -> Value {
    let at = file.line_index().line_col(offset.into());
    let (line, column) = at.map_or((1, 1), |at| {
        let end = usize::try_from(offset).unwrap_or(0);
        let start = end.saturating_sub(usize::try_from(at.col).unwrap_or(0));
        let column = file.text().get(start..end).map_or(0, |t| t.chars().count()) + 1;
        (u64::from(at.line) + 1, column as u64)
    });
    json!({ "offset": offset, "line": line, "column": column })
}

fn place(files: &[SourceFile], id: FileId, range: TextRange) -> Value {
    let Some(file) = files.iter().find(|file| file.id() == id) else {
        return Value::Null;
    };
    json!({
        "file": file.name(),
        "start": position(file, range.start().into()),
        "end": position(file, range.end().into()),
    })
}

fn as_json(files: &[SourceFile], found: &Diagnostic) -> Value {
    let severity = match found.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        _ => "info",
    };
    json!({
        "code": found.code,
        "severity": severity,
        "message": found.message,
        "place": place(files, found.file, found.range),
        "labels": found.labels.iter().map(|label| json!({
            "message": label.message,
            "place": place(files, label.file, label.range),
        })).collect::<Vec<_>>(),
        "notes": found.notes,
        "fixes": found.fixes.iter().map(|fix| json!({
            "title": fix.title,
            "edits": fix.edits.iter().map(|edit| json!({
                "place": place(files, edit.file, edit.range),
                "replacement": edit.replacement,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

/// Parses the files that `paths` name, runs `host` on each that parsed without problems,
/// and shows every problem. Files are numbered in the order read. Fails if there are
/// errors.
///
/// For a language that binds names, names are resolved across these files and the files
/// they import, so a name that nothing defines is a problem. See [`check_files_with`] to
/// resolve names in a way of your own.
///
/// With [`Format::Json`] the output is one object:
///
/// ```json
/// {
///   "files": 1, "errors": 1, "warnings": 0,
///   "diagnostics": [{
///     "code": "LM-P001", "severity": "error", "message": "expected `;`, found `}`",
///     "place": { "file": "a.calc", "start": { "offset": 8, "line": 1, "column": 9 },
///                "end": { "offset": 9, "line": 1, "column": 10 } },
///     "labels": [], "notes": [], "fixes": []
///   }]
/// }
/// ```
pub fn check_files<'l>(
    language: impl Into<Served<'l>>,
    paths: &[PathBuf],
    format: Format,
    host: &dyn Fn(&SourceFile, &Parse) -> Vec<Diagnostic>,
) -> Result<(), Failure> {
    check_files_with(language, paths, format, host, None)
}

/// Like [`check_files`], resolving names with `resolver` where the declared rules are not
/// enough.
pub fn check_files_with<'l>(
    language: impl Into<Served<'l>>,
    paths: &[PathBuf],
    format: Format,
    host: &dyn Fn(&SourceFile, &Parse) -> Vec<Diagnostic>,
    resolver: Option<&dyn Resolver>,
) -> Result<(), Failure> {
    let served = language.into();
    let language = served.language();
    let mut project = Project::new(served);
    let mut checked: Vec<(FileId, String)> = Vec::new();
    for path in files_in(language, paths)? {
        let name = path.display().to_string().replace('\\', "/");
        if project.file_at(&name).is_some() {
            continue;
        }
        let id = project.set(&name, read(&path)?).ok_or_else(|| {
            Failure::Unable(format!(
                "{name}: the file is too large, or there are more files than a project holds"
            ))
        })?;
        checked.push((id, name));
    }
    let mut found = Diagnostics::new();
    // Names reach across the files checked and those they import.
    if !project.rules().rules.is_empty() || resolver.is_some() {
        for (_, name) in &checked {
            project.open(name, &DiskFiles);
        }
        let bindings = match resolver {
            Some(resolver) => resolver.resolve(&project),
            None => project.bind(),
        };
        let ours: Vec<FileId> = checked.iter().map(|&(id, _)| id).collect();
        let mine = bindings.diagnostics().iter();
        found.extend(mine.filter(|found| ours.contains(&found.file)).cloned());
    }
    for &(id, _) in &checked {
        let Some(file) = project.file(id) else {
            continue;
        };
        // The language's own checks expect a tree without errors.
        if file.parse().diagnostics.is_empty() {
            found.extend(host(file.source(), file.parse()));
        }
        found.extend(file.parse().diagnostics.iter().cloned());
    }
    // Imported files too, so that problems can point into them.
    let files: Vec<SourceFile> = project.files().map(|file| file.source().clone()).collect();
    let found = found.into_sorted();
    let errors = found
        .iter()
        .filter(|found| found.severity == Severity::Error)
        .count();
    let warnings = found.len() - errors;
    match format {
        Format::Json => {
            let value = json!({
                "files": checked.len(),
                "errors": errors,
                "warnings": warnings,
                "diagnostics": found.iter().map(|d| as_json(&files, d)).collect::<Vec<_>>(),
            });
            output::line(&value.to_string());
        }
        Format::Text => {
            output::report(&files, &found);
            let count =
                |n: usize, what: &str| format!("{n} {what}{}", if n == 1 { "" } else { "s" });
            output::line(&format!(
                "checked {}, {}, {}",
                count(checked.len(), "file"),
                count(errors, "error"),
                count(warnings, "warning")
            ));
        }
    }
    if errors > 0 {
        Err(Failure::Found)
    } else {
        Ok(())
    }
}
