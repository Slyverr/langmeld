//! Project files of a language's users that are open: their problems and keys.

use langmeld_syntax::{Diagnostic, SourceFile, TextSize};
use lsp_types::{CompletionItem, CompletionItemKind};
use serde::de::DeserializeOwned;

use crate::project::{ProjectManifest, keys};

/// A project file of a language's users: its name, and the type it is read into. See
/// [`Server::project_file`](super::Server::project_file).
#[derive(Clone, Debug)]
pub struct ProjectFileKind {
    name: String,
    check: fn(SourceFile) -> Option<Diagnostic>,
    keys: fn(&[&str]) -> Vec<&'static str>,
}

impl ProjectFileKind {
    /// The file named `name`, read into `T`.
    pub fn new<T: DeserializeOwned>(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            check: |file| {
                let problem = ProjectManifest::<T>::parse(file).err()?;
                Some(problem.diagnostic)
            },
            keys: keys::<T>,
        }
    }

    pub(super) fn name(&self) -> &str {
        &self.name
    }

    pub(super) fn check(&self, file: SourceFile) -> Option<Diagnostic> {
        (self.check)(file)
    }

    /// The keys that may be written at `offset`: those of the table there that it
    /// does not have yet.
    pub(super) fn completions(&self, file: &SourceFile, offset: TextSize) -> Vec<CompletionItem> {
        let text = file.text();
        let before = text.get(..usize::from(offset)).unwrap_or(text);
        let line = before.rsplit('\n').next().unwrap_or(before).trim_start();
        // Keys stand before `=`, outside table names.
        if line.contains('=') || line.starts_with('[') {
            return Vec::new();
        }
        let header = |line: &str| {
            let inner = line.trim().strip_prefix('[')?.trim_start_matches('[');
            Some(inner.split(']').next()?.trim().to_owned())
        };
        let table = before.lines().rev().find_map(header).unwrap_or_default();
        // The keys the table already has, anywhere in it.
        let mut inside = table.is_empty();
        let mut written = Vec::new();
        for (at, line) in text.split_inclusive('\n').enumerate() {
            if let Some(name) = header(line) {
                // A table inside this one is one of its keys.
                let below = match table.as_str() {
                    "" => Some(name.as_str()),
                    table => name
                        .strip_prefix(table)
                        .and_then(|rest| rest.strip_prefix('.')),
                };
                written.extend(
                    below
                        .and_then(|rest| rest.split('.').next())
                        .map(str::to_owned),
                );
                inside = name == table && at < before.lines().count();
            } else if inside && let Some((key, _)) = line.split_once('=') {
                written.push(key.trim().to_owned());
            }
        }
        let path: Vec<&str> = table
            .split('.')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .collect();
        (self.keys)(&path)
            .into_iter()
            .filter(|key| !written.iter().any(|written| written == key))
            .map(|key| CompletionItem {
                label: key.to_owned(),
                kind: Some(CompletionItemKind::PROPERTY),
                ..CompletionItem::default()
            })
            .collect()
    }
}
