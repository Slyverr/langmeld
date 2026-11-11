use std::{fs, path::Path};

use langmeld_core::{SourceFile, Span};
use langmeld_language::{
    CapturedField, CompletionCandidate, CompletionKind, ParseResult, ParsedConstruct,
    quote_definition_string,
};
use url::Url;

use crate::definition_index::DefinitionIndex;

const MAX_DIRECTORY_SUGGESTIONS: usize = 200;
const IGNORED_DIRECTORIES: &[&str] = &[".git", "node_modules", "target"];

#[derive(Clone, Copy)]
enum FieldKind {
    Construct,
    Category,
    ImportPath,
}

pub(crate) fn complete_definition_field(
    root: &Path,
    source: &SourceFile,
    parsed: &ParseResult,
    offset: usize,
    index: Option<&DefinitionIndex>,
) -> Option<Vec<CompletionCandidate>> {
    let (field, kind) = find_definition_field(&parsed.constructs, offset)?;
    Some(match kind {
        FieldKind::ImportPath => complete_import_path(root, source, field.span, offset),
        FieldKind::Construct => {
            complete_values(index?.constructs(), field.span, source, offset, "Construct")
        }
        FieldKind::Category => {
            complete_values(index?.categories(), field.span, source, offset, "Category")
        }
    })
}

fn find_definition_field(
    constructs: &[ParsedConstruct],
    offset: usize,
) -> Option<(&CapturedField, FieldKind)> {
    for construct in constructs {
        if let Some(field) = construct
            .fields
            .iter()
            .find(|field| field.span.start <= offset && offset <= field.span.end)
        {
            let kind = match (construct.name.as_str(), field.name.as_str()) {
                ("root" | "allow", "name") => FieldKind::Construct,
                ("implements" | "allow_category", "category") => FieldKind::Category,
                ("import", "path") => FieldKind::ImportPath,
                _ => return None,
            };
            return Some((field, kind));
        }
        if let Some(found) = find_definition_field(&construct.children, offset) {
            return Some(found);
        }
    }
    None
}

fn complete_values(
    values: &[String],
    replace: Span,
    source: &SourceFile,
    offset: usize,
    detail: &str,
) -> Vec<CompletionCandidate> {
    let prefix = field_prefix(source, replace, offset);
    values
        .iter()
        .filter(|value| prefix.is_empty() || value.starts_with(&prefix))
        .map(|value| {
            CompletionCandidate::new(quote_definition_string(value), replace)
                .with_label(value)
                .with_filter_text(value)
                .with_kind(CompletionKind::Reference)
                .with_detail(detail)
        })
        .collect()
}

fn complete_import_path(
    root: &Path,
    source: &SourceFile,
    replace: Span,
    offset: usize,
) -> Vec<CompletionCandidate> {
    let prefix = field_prefix(source, replace, offset);
    if prefix.starts_with('/')
        || has_windows_drive_prefix(&prefix)
        || prefix.contains('\\')
        || prefix.contains('\0')
    {
        return Vec::new();
    }

    let Some(document_directory) = document_directory(root, source) else {
        return Vec::new();
    };
    let (directory_prefix, name_prefix) = prefix
        .rsplit_once('/')
        .map_or(("", prefix.as_str()), |(directory, name)| (directory, name));
    let requested_directory = document_directory.join(directory_prefix);
    let Ok(directory) = requested_directory.canonicalize() else {
        return Vec::new();
    };
    if !directory.starts_with(root) || !directory.is_dir() {
        return Vec::new();
    }

    let path_prefix = if directory_prefix.is_empty() {
        String::new()
    } else {
        format!("{}/", directory_prefix.trim_end_matches('/'))
    };
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut entries = entries.flatten().collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());
    let mut candidates = Vec::new();
    for entry in entries {
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !name.starts_with(name_prefix)
            || (name.starts_with('.') && !name_prefix.starts_with('.'))
            || IGNORED_DIRECTORIES.contains(&name.as_str())
        {
            continue;
        }
        let Ok(target) = entry.path().canonicalize() else {
            continue;
        };
        if !target.starts_with(root) {
            continue;
        }
        let is_directory = target.is_dir();
        if !is_directory
            && (!target.is_file() || target.extension().is_none_or(|ext| ext != "langmeld"))
        {
            continue;
        }

        let label = format!("{path_prefix}{name}{}", if is_directory { "/" } else { "" });
        let kind = if is_directory {
            CompletionKind::Folder
        } else {
            CompletionKind::File
        };
        let detail = if is_directory {
            "Folder"
        } else {
            ".langmeld definition"
        };
        candidates.push(
            CompletionCandidate::new(quote_definition_string(&label), replace)
                .with_label(&label)
                .with_filter_text(&label)
                .with_kind(kind)
                .with_detail(detail),
        );
        if candidates.len() >= MAX_DIRECTORY_SUGGESTIONS {
            break;
        }
    }
    candidates.sort_by(|left, right| left.label.cmp(&right.label));
    candidates
}

fn has_windows_drive_prefix(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn document_directory(root: &Path, source: &SourceFile) -> Option<std::path::PathBuf> {
    let uri = Url::parse(&source.name).ok()?;
    let path = uri.to_file_path().ok()?;
    let directory = path.parent()?.canonicalize().ok()?;
    directory.starts_with(root).then_some(directory)
}

fn field_prefix(source: &SourceFile, span: Span, offset: usize) -> String {
    let start = span.start.saturating_add(1);
    let end = offset.min(span.end.saturating_sub(1));
    if start > end || end > source.text.len() {
        return String::new();
    }
    source.text.get(start..end).unwrap_or_default().to_owned()
}
