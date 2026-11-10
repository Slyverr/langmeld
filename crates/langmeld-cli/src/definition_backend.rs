use std::{cell::Cell, error::Error, path::PathBuf};

use langmeld_core::{Diagnostic, SourceFile};
use langmeld_language::{
    DefinitionResolver, DefinitionSource, ParseResult, parse_language_definition_project,
};
use langmeld_lsp::{
    BackendCapabilities, BackendDocument, LanguageBackend, LanguageDefinitionBackend,
};
use langmeld_project::{DirectoryDefinitionResolver, ProjectManifest};
use url::Url;

use crate::project::DefinitionProjectContext;

pub(crate) struct ProjectDefinitionBackend {
    root: PathBuf,
    root_definition: String,
    resolver: DirectoryDefinitionResolver,
    syntax_backend: LanguageDefinitionBackend,
}

impl ProjectDefinitionBackend {
    pub(crate) fn new(manifest_path: impl Into<PathBuf>) -> Result<Self, Box<dyn Error>> {
        let project = DefinitionProjectContext::new(manifest_path.into())?;
        let root_definition = canonical_definition_key(&project.root, &project.manifest);
        Ok(Self {
            root: project.root,
            root_definition,
            resolver: project.resolver,
            syntax_backend: LanguageDefinitionBackend,
        })
    }

    fn project_diagnostics(&self, source: &SourceFile, key: &str) -> Option<Vec<Diagnostic>> {
        let result = if key == self.root_definition {
            parse_language_definition_project(
                DefinitionSource::new(key, source.clone()),
                &self.resolver,
            )
        } else {
            let root = self.resolver.load(&self.root_definition).ok()?;
            let overlay = OpenFileOverlayResolver {
                resolver: &self.resolver,
                key,
                source,
                used: Cell::new(false),
            };
            let result = parse_language_definition_project(root, &overlay);
            if !overlay.used.get() {
                return None;
            }
            result
        };

        Some(
            result
                .diagnostics
                .into_iter()
                .filter(|diagnostic| match diagnostic.location.as_ref() {
                    Some(location) => location.source_name == source.name,
                    None => key == self.root_definition,
                })
                .collect(),
        )
    }
}

impl LanguageBackend for ProjectDefinitionBackend {
    type Document = ParseResult;

    fn analyze(&self, source: &SourceFile) -> BackendDocument<Self::Document> {
        let syntax = self.syntax_backend.analyze(source);
        let diagnostics = self
            .source_key(source)
            .and_then(|key| self.project_diagnostics(source, &key))
            .unwrap_or(syntax.diagnostics);
        BackendDocument::new(source, syntax.state, diagnostics)
    }

    fn capabilities(&self) -> BackendCapabilities {
        self.syntax_backend.capabilities()
    }

    fn complete(
        &self,
        source: &SourceFile,
        document: &BackendDocument<Self::Document>,
        offset: usize,
    ) -> Vec<langmeld_language::CompletionCandidate> {
        self.syntax_backend.complete(source, document, offset)
    }

    fn format_document(
        &self,
        source: &SourceFile,
        document: &BackendDocument<Self::Document>,
        options: &lsp_types::FormattingOptions,
    ) -> Option<String> {
        self.syntax_backend
            .format_document(source, document, options)
    }
}

impl ProjectDefinitionBackend {
    fn source_key(&self, source: &SourceFile) -> Option<String> {
        let path = Url::parse(&source.name).ok()?.to_file_path().ok()?;
        let canonical = path.canonicalize().ok()?;
        let relative = canonical.strip_prefix(&self.root).ok()?;
        relative
            .to_str()
            .map(|path| path.replace(std::path::MAIN_SEPARATOR, "/"))
    }
}

fn canonical_definition_key(root: &std::path::Path, manifest: &ProjectManifest) -> String {
    let path = root.join(&manifest.definition);
    let canonical = path.canonicalize().unwrap_or(path);
    canonical
        .strip_prefix(root)
        .ok()
        .and_then(std::path::Path::to_str)
        .map(|path| path.replace(std::path::MAIN_SEPARATOR, "/"))
        .unwrap_or_else(|| manifest.definition.clone())
}

struct OpenFileOverlayResolver<'a> {
    resolver: &'a DirectoryDefinitionResolver,
    key: &'a str,
    source: &'a SourceFile,
    used: Cell<bool>,
}

impl DefinitionResolver for OpenFileOverlayResolver<'_> {
    fn resolve(&self, from_key: &str, import: &str) -> Result<DefinitionSource, String> {
        let resolved = self.resolver.resolve(from_key, import)?;
        if resolved.key == self.key {
            self.used.set(true);
            Ok(DefinitionSource::new(resolved.key, self.source.clone()))
        } else {
            Ok(resolved)
        }
    }
}
